// src/extension.ts
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

/* ────────────────────────────────────────────────────────────────────────── */
/*  Выбор бинарника                                                          */
/* ────────────────────────────────────────────────────────────────────────── */
function resolveServerPath(ctx: vscode.ExtensionContext): string {
    const env = process.env.GO_ANALYZER_PATH;
    if (env && fs.existsSync(env)) return env;

    const bin = process.platform === "win32"
        ? "go-analyzer-rs.exe"
        : "go-analyzer-rs";

    const primary = path.join(ctx.extensionPath, "server", bin);
    const secondary = path.join(
        ctx.extensionPath,
        "server",
        bin.endsWith(".exe") ? "go-analyzer-rs" : "go-analyzer-rs.exe",
    );

    if (fs.existsSync(primary)) return primary;
    if (fs.existsSync(secondary)) return secondary;

    throw new Error(
        `Go-Analyzer binary not found.\nTried:\n  ${primary}\n  ${secondary}`,
    );
}

/* ────────────────────────────────────────────────────────────────────────── */
/*  Глобальное состояние                                                     */
/* ────────────────────────────────────────────────────────────────────────── */
let client: LanguageClient | undefined;

const lastStatus = {
    variables: 0,
    functions: 0,
    channels: 0,
    goroutines: 0,
};

interface Decoration {
    range: vscode.Range;
    kind: "Declaration" | "Use" | "Pointer" | "Race";
    hoverMessage: string;
}

const ProgressNotification = new NotificationType<{ message: string }>(
    "goanalyzer/progress",
);
const IndexingStatusNotification = new NotificationType<{
    variables: number;
    functions: number;
    channels: number;
    goroutines: number;
}>("goanalyzer/indexingStatus");

/* ────────────────────────────────────────────────────────────────────────── */
/*  Активация                                                                */
/* ────────────────────────────────────────────────────────────────────────── */
export function activate(context: vscode.ExtensionContext) {
    /* -------- запуск сервера -------- */
    const serverModule = resolveServerPath(context);
    console.log(`Launching Go-Analyzer server: ${serverModule}`);

    const serverOptions: ServerOptions = {
        run: { command: serverModule, transport: TransportKind.stdio } as Executable,
        debug: { command: serverModule, transport: TransportKind.stdio } as Executable,
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "go" }],
        synchronize: { fileEvents: vscode.workspace.createFileSystemWatcher("**/*.go") },
        progressOnInitialization: true,
    };

    client = new LanguageClient("goAnalyzer", "Go Analyzer", serverOptions, clientOptions);

    /* -------- статус-бар -------- */
    const statusBar = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Right,
        100,
    );
    statusBar.text = "Go Analyzer";

    const updateTooltip = () => {
        statusBar.tooltip = [
            `Переменные: ${lastStatus.variables}`,
            `Функции: ${lastStatus.functions}`,
            `Каналы: ${lastStatus.channels}`,
            `Горутины: ${lastStatus.goroutines}`,
        ].join("\n");
    };
    updateTooltip();
    statusBar.show();
    context.subscriptions.push(statusBar);

    /* -------- уведомления от сервера -------- */
    client.onNotification(IndexingStatusNotification, p => {
        lastStatus.variables = p.variables;
        lastStatus.functions = p.functions;
        lastStatus.channels = p.channels;
        lastStatus.goroutines = p.goroutines;
        updateTooltip();
    });

    client.onNotification(ProgressNotification, p => {
        vscode.window.showInformationMessage(p.message);
    });

    /* -------- типы декораций -------- */
    const cfg = (key: string, def: string) =>
        vscode.workspace.getConfiguration("goAnalyzer").get<string>(key, def);

    const decorationTypes = {
        Declaration: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: cfg("declarationColor", "green"),
            overviewRulerColor: cfg("declarationColor", "green"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
        Use: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: cfg("useColor", "yellow"),
            overviewRulerColor: cfg("useColor", "yellow"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
        Pointer: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: cfg("pointerColor", "blue"),
            overviewRulerColor: cfg("pointerColor", "blue"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
        Race: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: cfg("raceColor", "red"),
            overviewRulerColor: cfg("raceColor", "red"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
    };

    /* -------- команда showLifecycle -------- */
    const lifecycleCmd = vscode.commands.registerCommand(
        "goanalyzer.showLifecycle",
        async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== "go") {
                vscode.window.showErrorMessage("No Go editor is active.");
                return;
            }

            const { selection, document } = editor;
            try {
                await vscode.window.withProgress(
                    {
                        location: vscode.ProgressLocation.Notification,
                        title: "Go Analyzer",
                        cancellable: false,
                    },
                    async progress => {
                        progress.report({ message: "Analyzing variable…" });

                        const resp: Decoration[] = await client!.sendRequest(
                            "workspace/executeCommand",
                            {
                                command: "goanalyzer/cursor",
                                arguments: [
                                    { textDocument: { uri: document.uri.toString() }, position: selection.active },
                                ],
                            },
                        );

                        /* очистка + применение декораций */
                        for (const key in decorationTypes) {
                            editor.setDecorations(decorationTypes[key as keyof typeof decorationTypes], []);
                        }

                        if (Array.isArray(resp)) {
                            const byType: Record<string, vscode.DecorationOptions[]> = {
                                Declaration: [],
                                Use: [],
                                Pointer: [],
                                Race: [],
                            };
                            for (const d of resp) {
                                const range = new vscode.Range(
                                    new vscode.Position(d.range.start.line, d.range.start.character),
                                    new vscode.Position(d.range.end.line, d.range.end.character),
                                );
                                byType[d.kind].push({ range, hoverMessage: d.hoverMessage });
                            }
                            for (const [k, decos] of Object.entries(byType)) {
                                editor.setDecorations(decorationTypes[k as keyof typeof decorationTypes], decos);
                            }
                        }

                        progress.report({ message: "Analysis complete" });
                    },
                );
            } catch (err) {
                vscode.window.showErrorMessage(`Go Analyzer error: ${err}`);
                console.error(err);
            }
        },
    );
    context.subscriptions.push(lifecycleCmd);

    /* -------- авто-анализ курсора -------- */
    let cursorDisp: vscode.Disposable | undefined;
    let lastPos: vscode.Position | undefined;
    let timeoutHandle: NodeJS.Timeout | undefined;

    const startCursorTracking = () => {
        cursorDisp?.dispose();

        cursorDisp = vscode.window.onDidChangeTextEditorSelection(evt => {
            const editor = evt.textEditor;
            if (editor.document.languageId !== "go") return;

            const enable = vscode.workspace.getConfiguration("goAnalyzer")
                .get<boolean>("enableAutoAnalysis", true);
            if (!enable) return;

            const pos = editor.selection.active;
            if (lastPos && pos.line === lastPos.line && pos.character === lastPos.character) return;
            lastPos = pos;

            clearTimeout(timeoutHandle as NodeJS.Timeout);

            const delay = vscode.workspace.getConfiguration("goAnalyzer")
                .get<number>("autoAnalysisDelay", 300);
            timeoutHandle = setTimeout(async () => {
                try {
                    const resp: Decoration[] = await client!.sendRequest(
                        "workspace/executeCommand",
                        {
                            command: "goanalyzer/cursor",
                            arguments: [
                                { textDocument: { uri: editor.document.uri.toString() }, position: pos },
                            ],
                        },
                    );

                    for (const key in decorationTypes) {
                        editor.setDecorations(decorationTypes[key as keyof typeof decorationTypes], []);
                    }

                    if (Array.isArray(resp)) {
                        const byType: Record<string, vscode.DecorationOptions[]> = {
                            Declaration: [],
                            Use: [],
                            Pointer: [],
                            Race: [],
                        };
                        for (const d of resp) {
                            const range = new vscode.Range(
                                new vscode.Position(d.range.start.line, d.range.start.character),
                                new vscode.Position(d.range.end.line, d.range.end.character),
                            );
                            byType[d.kind].push({ range, hoverMessage: d.hoverMessage });
                        }
                        for (const [k, decos] of Object.entries(byType)) {
                            editor.setDecorations(decorationTypes[k as keyof typeof decorationTypes], decos);
                        }
                    }
                } catch (err) {
                    console.error("Auto-analysis error:", err);
                }
            }, delay);
        });
    };

    startCursorTracking();
    if (cursorDisp) context.subscriptions.push(cursorDisp);

    context.subscriptions.push(
        vscode.window.onDidChangeActiveTextEditor(ed => {
            if (ed && ed.document.languageId === "go") startCursorTracking();
        }),
    );

    /* -------- hover provider -------- */
    vscode.languages.registerHoverProvider("go", {
        async provideHover(doc, pos, token) {
            try {
                const resp: any = await client!.sendRequest(
                    "textDocument/hover",
                    { textDocument: { uri: doc.uri.toString() }, position: pos },
                    token,
                );
                if (resp && resp.contents) {
                    return new vscode.Hover(resp.contents, resp.range);
                }
                return null;
            } catch (err) {
                vscode.window.showErrorMessage(`Hover error: ${err}`);
                console.error(err);
                return null;
            }
        },
    });

    /* -------- запуск клиента -------- */
    client.start()
        .then(() => vscode.window.showInformationMessage("Go Analyzer started"))
        .catch(err => {
            vscode.window.showErrorMessage(`Failed to start Go Analyzer: ${err}`);
            console.error(err);
        });
}

/* ────────────────────────────────────────────────────────────────────────── */
/*  Деактивация                                                              */
/* ────────────────────────────────────────────────────────────────────────── */
export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}