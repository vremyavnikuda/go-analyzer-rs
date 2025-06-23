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

interface Decoration {
    range: vscode.Range;
    kind: "Declaration" | "Use" | "Pointer" | "Race";
    hoverMessage: string;
}

const ProgressNotification = new NotificationType<{ message: string }>("goanalyzer/progress");

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
}

export function deactivate(): Promise<void> | undefined {
    if (!client) {
        return undefined;
    }
    return client.stop();
}