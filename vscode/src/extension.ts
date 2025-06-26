import * as vscode from "vscode";
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
    Executable,
    NotificationType,
} from "vscode-languageclient/node";
import * as path from "path";
import * as fs from "fs";

let client: LanguageClient;

let lastStatus = {
    variables: 0,
    functions: 0,
    channels: 0,
    goroutines: 0
};

interface Decoration {
    range: vscode.Range;
    kind: "Declaration" | "Use" | "Pointer" | "Race";
    hoverMessage: string;
}

const ProgressNotification = new NotificationType<{ message: string }>("goanalyzer/progress");
const IndexingStatusNotification = new NotificationType<{ variables: number, functions: number, channels: number, goroutines: number }>("goanalyzer/indexingStatus");

export function activate(context: vscode.ExtensionContext) {
    const serverModule = process.env.GO_ANALYZER_PATH || path.resolve(context.extensionPath, "server", "go-analyzer-rs.exe");
    console.log(`Attempting to launch server at: ${serverModule}`);

    if (!fs.existsSync(serverModule)) {
        vscode.window.showErrorMessage(`Server binary not found at: ${serverModule}`);
        return;
    }

    const serverOptions: ServerOptions = {
        run: { command: serverModule, transport: TransportKind.stdio } as Executable,
        debug: { command: serverModule, transport: TransportKind.stdio } as Executable,
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "go" }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher("**/*.go"),
        },
        progressOnInitialization: true,
    };

    client = new LanguageClient(
        "goAnalyzer",
        "Go Analyzer",
        serverOptions,
        clientOptions
    );

    // REVIEW: проверить работоспособность нового статус бара 
    const statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
    statusBarItem.text = "Go Analyzer";
    const updateTooltip = () => {
        const details = [
            `Переменные: ${lastStatus.variables}`,
            `Функции: ${lastStatus.functions}`,
            `Каналы: ${lastStatus.channels}`,
            `Горутины: ${lastStatus.goroutines}`
        ];
        statusBarItem.tooltip = details.join("\n");
    };
    updateTooltip();
    statusBarItem.command = undefined;
    statusBarItem.show();
    context.subscriptions.push(statusBarItem);

    // Обновление lastStatus и tooltip при получении IndexingStatusNotification
    client.onNotification(IndexingStatusNotification, (params) => {
        lastStatus.variables = params.variables;
        lastStatus.functions = params.functions;
        lastStatus.channels = params.channels;
        lastStatus.goroutines = params.goroutines;
        updateTooltip();
    });

    const decorationTypes = {
        Declaration: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: vscode.workspace.getConfiguration("goAnalyzer").get("declarationColor", "green"),
            overviewRulerColor: vscode.workspace.getConfiguration("goAnalyzer").get("declarationColor", "green"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
        Use: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: vscode.workspace.getConfiguration("goAnalyzer").get("useColor", "yellow"),
            overviewRulerColor: vscode.workspace.getConfiguration("goAnalyzer").get("useColor", "yellow"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
        Pointer: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: vscode.workspace.getConfiguration("goAnalyzer").get("pointerColor", "blue"),
            overviewRulerColor: vscode.workspace.getConfiguration("goAnalyzer").get("pointerColor", "blue"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
        Race: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: vscode.workspace.getConfiguration("goAnalyzer").get("raceColor", "red"),
            overviewRulerColor: vscode.workspace.getConfiguration("goAnalyzer").get("raceColor", "red"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
    };

    const disposable = vscode.commands.registerCommand(
        "goanalyzer.showLifecycle",
        async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor) {
                vscode.window.showErrorMessage("No active editor found.");
                return;
            }

            const position = editor.selection.active;
            const uri = editor.document.uri;

            try {
                await vscode.window.withProgress(
                    {
                        location: vscode.ProgressLocation.Notification,
                        title: "Go Analyzer",
                        cancellable: false,
                    },
                    async (progress) => {
                        progress.report({ message: "Analyzing variable..." });

                        const response: Decoration[] = await client.sendRequest(
                            "workspace/executeCommand",
                            {
                                command: "goanalyzer/cursor",
                                arguments: [{ textDocument: { uri: uri.toString() }, position }],
                            }
                        );

                        console.log("Server response:", JSON.stringify(response, null, 2));

                        for (const key in decorationTypes) {
                            editor.setDecorations(decorationTypes[key as keyof typeof decorationTypes], []);
                        }

                        if (response && Array.isArray(response)) {
                            const decorationsByType: { [key: string]: { range: vscode.Range; hoverMessage: string }[] } = {
                                Declaration: [],
                                Use: [],
                                Pointer: [],
                                Race: [],
                            };

                            for (const deco of response) {
                                const range = new vscode.Range(
                                    new vscode.Position(deco.range.start.line, deco.range.start.character),
                                    new vscode.Position(deco.range.end.line, deco.range.end.character)
                                );
                                decorationsByType[deco.kind].push({ range, hoverMessage: deco.hoverMessage });
                            }

                            for (const [kind, decorations] of Object.entries(decorationsByType)) {
                                editor.setDecorations(decorationTypes[kind as keyof typeof decorationTypes], decorations);
                            }
                        }

                        progress.report({ message: "Analysis complete" });
                    }
                );
            } catch (error) {
                vscode.window.showErrorMessage(`Go Analyzer error: ${error}`);
                console.error("Error executing command:", error);
            }
        }
    );

    // Автоматический анализ при изменении позиции курсора
    let cursorChangeDisposable: vscode.Disposable | undefined;
    let lastPosition: vscode.Position | undefined;
    let analysisTimeout: NodeJS.Timeout | undefined;

    const startCursorTracking = () => {
        if (cursorChangeDisposable) {
            cursorChangeDisposable.dispose();
        }

        cursorChangeDisposable = vscode.window.onDidChangeTextEditorSelection(async (event) => {
            const editor = event.textEditor;
            if (editor.document.languageId !== 'go') {
                return;
            }

            // Проверяем, включен ли автоматический анализ
            const enableAutoAnalysis = vscode.workspace.getConfiguration("goAnalyzer").get("enableAutoAnalysis", true);
            if (!enableAutoAnalysis) {
                return;
            }

            const position = editor.selection.active;

            // Проверяем, изменилась ли позиция курсора
            if (lastPosition &&
                lastPosition.line === position.line &&
                lastPosition.character === position.character) {
                return;
            }

            lastPosition = position;

            // Очищаем предыдущий таймаут
            if (analysisTimeout) {
                clearTimeout(analysisTimeout);
            }

            // Получаем задержку из настроек
            const delay = vscode.workspace.getConfiguration("goAnalyzer").get("autoAnalysisDelay", 300);

            // Запускаем анализ с небольшой задержкой для избежания частых запросов
            analysisTimeout = setTimeout(async () => {
                try {
                    const response: Decoration[] = await client.sendRequest(
                        "workspace/executeCommand",
                        {
                            command: "goanalyzer/cursor",
                            arguments: [{ textDocument: { uri: editor.document.uri.toString() }, position }],
                        }
                    );

                    // Очищаем предыдущие декорации
                    for (const key in decorationTypes) {
                        editor.setDecorations(decorationTypes[key as keyof typeof decorationTypes], []);
                    }

                    if (response && Array.isArray(response)) {
                        const decorationsByType: { [key: string]: { range: vscode.Range; hoverMessage: string }[] } = {
                            Declaration: [],
                            Use: [],
                            Pointer: [],
                            Race: [],
                        };

                        for (const deco of response) {
                            const range = new vscode.Range(
                                new vscode.Position(deco.range.start.line, deco.range.start.character),
                                new vscode.Position(deco.range.end.line, deco.range.end.character)
                            );
                            decorationsByType[deco.kind].push({ range, hoverMessage: deco.hoverMessage });
                        }

                        for (const [kind, decorations] of Object.entries(decorationsByType)) {
                            editor.setDecorations(decorationTypes[kind as keyof typeof decorationTypes], decorations);
                        }
                    }
                } catch (error) {
                    console.error("Auto-analysis error:", error);
                }
            }, delay);
        });
    };

    // Запускаем отслеживание курсора при активации расширения
    startCursorTracking();

    // Перезапускаем отслеживание при смене активного редактора
    const editorChangeDisposable = vscode.window.onDidChangeActiveTextEditor((editor) => {
        if (editor && editor.document.languageId === 'go') {
            startCursorTracking();
        }
    });

    vscode.languages.registerHoverProvider("go", {
        async provideHover(document: vscode.TextDocument, position: vscode.Position, token: vscode.CancellationToken) {
            try {
                const response: any = await client.sendRequest(
                    "textDocument/hover",
                    { textDocument: { uri: document.uri.toString() }, position },
                    token
                );
                console.log("Hover response:", JSON.stringify(response, null, 2));
                if (response && response.contents) {
                    return new vscode.Hover(response.contents, response.range);
                }
                return null;
            } catch (error) {
                vscode.window.showErrorMessage(`Hover error: ${error}`);
                console.error("Error in hover provider:", error);
                return null;
            }
        },
    });

    client.onNotification(ProgressNotification, (params: { message: string }) => {
        vscode.window.showInformationMessage(params.message);
    });

    client.start().then(() => {
        vscode.window.showInformationMessage("Go Analyzer started");
    }).catch((error) => {
        vscode.window.showErrorMessage(`Failed to start Go Analyzer: ${error}`);
        console.error("Failed to start client:", error);
    });

    context.subscriptions.push(disposable);
    context.subscriptions.push(editorChangeDisposable);

    // Очистка ресурсов при деактивации
    context.subscriptions.push({
        dispose: () => {
            if (cursorChangeDisposable) {
                cursorChangeDisposable.dispose();
            }
            if (analysisTimeout) {
                clearTimeout(analysisTimeout);
            }
        }
    });
}

export function deactivate(): Promise<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}