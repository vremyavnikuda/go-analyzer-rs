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

function resolveServerPath(ctx: vscode.ExtensionContext): string {
    const env = process.env.GO_ANALYZER_PATH;
    if (env && fs.existsSync(env)) return env;
    const bin = process.platform === "win32"
        ? "go-analyzer.exe"
        : "go-analyzer";

    const bundled = path.join(ctx.extensionPath, "server", bin);
    if (fs.existsSync(bundled)) return bundled;
    const cargoHome = process.env.CARGO_HOME;
    let cargoBinPaths: string[] = [];
    if (cargoHome) {
        cargoBinPaths.push(path.join(cargoHome, "bin", bin));
    }
    if (process.platform === "win32") {
        const userProfile = process.env.USERPROFILE;
        if (userProfile) {
            cargoBinPaths.push(path.join(userProfile, ".cargo", "bin", bin));
        }
    } else {
        const home = process.env.HOME;
        if (home) {
            cargoBinPaths.push(path.join(home, ".cargo", "bin", bin));
        }
    }

    for (const cargoPath of cargoBinPaths) {
        if (fs.existsSync(cargoPath)) {
            return cargoPath;
        }
    }
    const allPaths = [bundled, ...cargoBinPaths];
    const pathList = allPaths.map(p => `  ${p}`).join('\n');
    throw new Error(
        `Go-Analyzer binary not found.\nTried:\n${pathList}\n\nTo install: cargo install go-analyzer\nOr set GO_ANALYZER_PATH environment variable.`
    );
}

function resolveSemanticHelperPath(ctx: vscode.ExtensionContext, serverModule: string): string | undefined {
    const env = process.env.GO_ANALYZER_SEMANTIC_PATH;
    if (env && fs.existsSync(env)) return env;
    const bin = process.platform === "win32"
        ? "goanalyzer-semantic.exe"
        : "goanalyzer-semantic";

    const nearServer = path.join(path.dirname(serverModule), bin);
    if (fs.existsSync(nearServer)) return nearServer;
    const bundled = path.join(ctx.extensionPath, "server", bin);
    if (fs.existsSync(bundled)) return bundled;
    return undefined;
}

let client: LanguageClient | undefined;
let extensionActive = true;
let output: vscode.OutputChannel | undefined;
let outputShown = false;
let disposables: vscode.Disposable[] = [];
let cursorDisp: vscode.Disposable | undefined;
let lastPos: vscode.Position | undefined;
let timeoutHandle: NodeJS.Timeout | undefined;

function cleanupEventHandlers() {
    console.log(`Cleaning up ${disposables.length} event handlers`);
    disposables.forEach(d => {
        try {
            d.dispose();
        } catch (error) {
            console.error("Error disposing event handler:", error);
        }
    });
    disposables = [];
    if (cursorDisp) {
        try {
            cursorDisp.dispose();
        } catch (error) {
            console.error("Error disposing cursor handler:", error);
        }
        cursorDisp = undefined;
    }
    if (timeoutHandle) {
        clearTimeout(timeoutHandle);
        timeoutHandle = undefined;
    }
    lastPos = undefined;
}

function log(message: string) {
    if (!output) return;
    const stamp = new Date().toISOString();
    output.appendLine(`[${stamp}] ${message}`);
    if (!outputShown) {
        output.show(true);
        outputShown = true;
    }
}

function logRaw(message: string) {
    if (!output) return;
    output.appendLine(message);
    if (!outputShown) {
        output.show(true);
        outputShown = true;
    }
}

function addDisposable(disposable: vscode.Disposable) {
    disposables.push(disposable);
    return disposable;
}

const lastStatus = {
    variables: 0,
    functions: 0,
    channels: 0,
    goroutines: 0,
};

interface Decoration {
    range: vscode.Range;
    kind:
    | "Declaration"
    | "Use"
    | "Pointer"
    | "Race"
    | "RaceLow"
    | "AliasReassigned"
    | "AliasCaptured";
    hover_text: string;
}

const ProgressNotification = new NotificationType<string>(
    "goanalyzer/progress",
);

const IndexingStatusNotification = new NotificationType<{
    uri?: string;
    variables: number;
    functions: number;
    channels: number;
    goroutines: number;
}>("goanalyzer/indexingStatus");

const ParseInfoNotification = new NotificationType<{
    uri: string;
    source?: string;
    cache_hit: boolean;
    parse_ms?: number;
    code_len: number;
}>("goanalyzer/parseInfo");

const LifecycleDumpNotification = new NotificationType<{
    uri: string;
    points: unknown[];
}>("goanalyzer/lifecycleDump");

function addColorValues(points: unknown[]): unknown[] {
    const cfg = vscode.workspace.getConfiguration("goAnalyzer");
    return (points as Array<Record<string, unknown>>).map(point => {
        if (!point || typeof point !== "object") return point;
        const expected = point["expected"];
        if (!expected || typeof expected !== "object") return point;
        const colorKey = (expected as Record<string, unknown>)["color_key"];
        if (typeof colorKey !== "string" || colorKey.length === 0) return point;
        const colorValue = cfg.get<string>(colorKey) ?? "";
        return {
            ...point,
            expected: {
                ...(expected as Record<string, unknown>),
                color_value: colorValue,
            },
        };
    });
}

async function dumpAstForDocument(uri: vscode.Uri) {
    if (!client) return;
    const maxChars = vscode.workspace.getConfiguration("goAnalyzer")
        .get<number>("astMaxChars", 20000);

    const sexp: string | null = await client.sendRequest(
        "workspace/executeCommand",
        {
            command: "goanalyzer/ast",
            arguments: [{ uri: uri.toString() }],
        },
    );
    if (!sexp) {
        log(`AST dump: no data for ${uri.toString()}`);
        return;
    }
    const clipped = sexp.length > maxChars
        ? `${sexp.slice(0, maxChars)}\n...[truncated ${sexp.length - maxChars} chars]`
        : sexp;
    const doc = await vscode.workspace.openTextDocument({
        content: clipped,
        language: "lisp",
    });
    await vscode.window.showTextDocument(doc, {
        preview: true,
        viewColumn: vscode.ViewColumn.Beside,
    });
    log(`AST opened in editor for ${uri.toString()}`);
}

export function activate(context: vscode.ExtensionContext) {
    cleanupEventHandlers();
    output = vscode.window.createOutputChannel("Go Analyzer");
    context.subscriptions.push(output);
    const serverModule = resolveServerPath(context);
    log(`Launching Go-Analyzer server: ${serverModule}`);
    const semanticEnable = vscode.workspace.getConfiguration("goAnalyzer")
        .get<boolean>("semanticEnable", true);
    const semanticHelperPathOverride = vscode.workspace.getConfiguration("goAnalyzer")
        .get<string>("semanticHelperPath", "").trim();
    const semanticHelperPath = semanticHelperPathOverride || resolveSemanticHelperPath(context, serverModule);
    const semanticEnabled = semanticEnable && !!semanticHelperPath;
    const semanticTimeoutMs = vscode.workspace.getConfiguration("goAnalyzer")
        .get<number>("semanticTimeoutMs", 2000);

    const semanticEnv = {
        ...process.env,
        GO_ANALYZER_SEMANTIC: semanticEnabled ? "1" : "0",
        GO_ANALYZER_SEMANTIC_PATH: semanticHelperPath ?? "",
        GO_ANALYZER_SEMANTIC_TIMEOUT_MS: String(semanticTimeoutMs),
    };
    log(`Semantic helper: ${semanticEnabled ? (semanticHelperPath ?? "enabled") : "disabled"}`);
    const serverOptions: ServerOptions = {
        run: {
            command: serverModule,
            transport: TransportKind.stdio,
            options: { env: semanticEnv },
        } as Executable,
        debug: {
            command: serverModule,
            transport: TransportKind.stdio,
            options: { env: semanticEnv },
        } as Executable,
    };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: "file", language: "go" }],
        synchronize: { fileEvents: vscode.workspace.createFileSystemWatcher("**/*.go") },
        progressOnInitialization: true,
        outputChannel: output,
    };
    client = new LanguageClient("goAnalyzer", "Go Analyzer", serverOptions, clientOptions);
    const statusBar = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Right,
        100,
    );
    const updateStatusBar = () => {
        statusBar.text = extensionActive ? "Go Analyzer ✅" : "Go Analyzer ❌";
        statusBar.tooltip = extensionActive
            ? `Active - Analysis enabled\n${getStatusTooltip()}`
            : `Inactive - Analysis disabled\n${getStatusTooltip()}`;
    };
    const getStatusTooltip = () => {
        return [
            `Переменные: ${lastStatus.variables}`,
            `Функции: ${lastStatus.functions}`,
            `Каналы: ${lastStatus.channels}`,
            `Горутины: ${lastStatus.goroutines}`,
        ].join("\n");
    };
    updateStatusBar();
    statusBar.show();
    context.subscriptions.push(statusBar);
    client.onNotification(IndexingStatusNotification, p => {
        lastStatus.variables = p.variables;
        lastStatus.functions = p.functions;
        lastStatus.channels = p.channels;
        lastStatus.goroutines = p.goroutines;
        updateStatusBar();
        const target = p.uri ? ` ${p.uri}` : "";
        log(`Indexing status:${target} vars=${p.variables}, funcs=${p.functions}, chans=${p.channels}, goroutines=${p.goroutines}`);
    });
    client.onNotification(ProgressNotification, message => {
        vscode.window.showInformationMessage(message);
        log(`Progress: ${message}`);
    });
    client.onNotification(ParseInfoNotification, p => {
        if (p.source !== "auto") return;
        const parseMs = p.parse_ms == null ? "cache" : `${p.parse_ms}ms`;
        log(`Parse info (auto): ${p.uri} cache_hit=${p.cache_hit} parse_ms=${parseMs} code_len=${p.code_len}`);
    });
    client.onNotification(LifecycleDumpNotification, p => {
        const maxChars = vscode.workspace.getConfiguration("goAnalyzer")
            .get<number>("lifecycleJsonMaxChars", 20000);
        const pointsWithColors = addColorValues(p.points);
        const json = JSON.stringify(pointsWithColors, null, 2);
        const clipped = json.length > maxChars
            ? `${json.slice(0, maxChars)}\n...[truncated ${json.length - maxChars} chars]`
            : json;
        logRaw(clipped);
    });
    const cfg = (key: string, def: string) =>
        vscode.workspace.getConfiguration("goAnalyzer").get<string>(key, def);
    const aliasReassignedColor = cfg("aliasReassignedColor", "purple");
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
        RaceLow: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: vscode.workspace.getConfiguration("goAnalyzer").get("raceLowColor", "orange"),
            overviewRulerColor: vscode.workspace.getConfiguration("goAnalyzer").get("raceLowColor", "orange"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
        AliasReassigned: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: aliasReassignedColor,
            overviewRulerColor: aliasReassignedColor,
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
        AliasCaptured: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: cfg("aliasCapturedColor", "magenta"),
            overviewRulerColor: cfg("aliasCapturedColor", "magenta"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
    };
    const lifecycleCmd = vscode.commands.registerCommand(
        "goanalyzer.showLifecycle",
        async () => {
            if (!extensionActive) {
                vscode.window.showWarningMessage("Go Analyzer is deactivated. Use Shift+Alt+S to activate.");
                return;
            }
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
                                    {
                                        textDocument: { uri: document.uri.toString() },
                                        position: selection.active,
                                        source: "manual",
                                        dump_json: vscode.workspace.getConfiguration("goAnalyzer")
                                            .get<boolean>("debugLifecycleJson", false),
                                    },
                                ],
                            },
                        );
                        log(`Manual analysis: ${document.uri.toString()} @ ${selection.active.line}:${selection.active.character}`);
                        const dumpAst = vscode.workspace.getConfiguration("goAnalyzer")
                            .get<boolean>("debugDumpAst", false);
                        if (dumpAst) {
                            await dumpAstForDocument(document.uri);
                        }
                        for (const key in decorationTypes) {
                            editor.setDecorations(decorationTypes[key as keyof typeof decorationTypes], []);
                        }
                        if (Array.isArray(resp)) {
                            const byType: Record<string, vscode.DecorationOptions[]> = {
                                Declaration: [],
                                Use: [],
                                Pointer: [],
                                Race: [],
                                RaceLow: [],
                                AliasReassigned: [],
                                AliasCaptured: [],
                            };
                            for (const d of resp) {
                                const range = new vscode.Range(
                                    new vscode.Position(d.range.start.line, d.range.start.character),
                                    new vscode.Position(d.range.end.line, d.range.end.character),
                                );
                                if (byType[d.kind]) {
                                    byType[d.kind].push({ range, hoverMessage: d.hover_text });
                                }
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
    const dumpAstCmd = vscode.commands.registerCommand(
        "goanalyzer.dumpAst",
        async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== "go") {
                vscode.window.showErrorMessage("No Go editor is active.");
                return;
            }
            await dumpAstForDocument(editor.document.uri);
        },
    );
    context.subscriptions.push(dumpAstCmd);
    const activateCmd = vscode.commands.registerCommand(
        "goanalyzer.activate",
        async () => {
            extensionActive = true;
            updateStatusBar();
            if (!client || client.state !== 2) {
                try {
                    log("Starting Go Analyzer LSP server...");
                    const serverModule = resolveServerPath(context);
                    const serverOptions: ServerOptions = {
                        run: { command: serverModule, transport: TransportKind.stdio } as Executable,
                        debug: { command: serverModule, transport: TransportKind.stdio } as Executable,
                    };
                    const clientOptions: LanguageClientOptions = {
                        documentSelector: [{ scheme: "file", language: "go" }],
                        synchronize: { fileEvents: vscode.workspace.createFileSystemWatcher("**/*.go") },
                        progressOnInitialization: true,
                        outputChannel: output,
                    };
                    client = new LanguageClient("goAnalyzer", "Go Analyzer", serverOptions, clientOptions);
                    client.onNotification(IndexingStatusNotification, p => {
                        lastStatus.variables = p.variables;
                        lastStatus.functions = p.functions;
                        lastStatus.channels = p.channels;
                        lastStatus.goroutines = p.goroutines;
                        updateStatusBar();
                        const target = p.uri ? ` ${p.uri}` : "";
                        log(`Indexing status:${target} vars=${p.variables}, funcs=${p.functions}, chans=${p.channels}, goroutines=${p.goroutines}`);
                    });
                    client.onNotification(ProgressNotification, message => {
                        vscode.window.showInformationMessage(message);
                        log(`Progress: ${message}`);
                    });
                    client.onNotification(ParseInfoNotification, p => {
                        if (p.source !== "auto") return;
                        const parseMs = p.parse_ms == null ? "cache" : `${p.parse_ms}ms`;
                        log(`Parse info (auto): ${p.uri} cache_hit=${p.cache_hit} parse_ms=${parseMs} code_len=${p.code_len}`);
                    });
                    client.onNotification(LifecycleDumpNotification, p => {
                        const maxChars = vscode.workspace.getConfiguration("goAnalyzer")
                            .get<number>("lifecycleJsonMaxChars", 20000);
                        const pointsWithColors = addColorValues(p.points);
                        const json = JSON.stringify(pointsWithColors, null, 2);
                        const clipped = json.length > maxChars
                            ? `${json.slice(0, maxChars)}\n...[truncated ${json.length - maxChars} chars]`
                            : json;
                        logRaw(clipped);
                    });
                    await client.start();
                    log("Go Analyzer LSP server restarted successfully");
                    startCursorTracking();
                } catch (error) {
                    console.error("Error restarting LSP server:", error);
                    log(`Error restarting LSP server: ${error}`);
                    vscode.window.showErrorMessage(`Failed to restart Go Analyzer: ${error}`);
                    return;
                }
            } else {
                startCursorTracking();
            }
            vscode.window.showInformationMessage("Go Analyzer: Extension activated - Ready for analysis (Расширение активировано - Готов к анализу)");
        }
    );
    context.subscriptions.push(activateCmd);
    const deactivateCmd = vscode.commands.registerCommand(
        "goanalyzer.deactivate",
        async () => {
            extensionActive = false;
            updateStatusBar();
            const editor = vscode.window.activeTextEditor;
            if (editor && editor.document.languageId === "go") {
                for (const key in decorationTypes) {
                    editor.setDecorations(decorationTypes[key as keyof typeof decorationTypes], []);
                }
            }
            cursorDisp?.dispose();
            clearTimeout(timeoutHandle as NodeJS.Timeout);
            if (client && client.state === 2) {
                try {
                    log("Stopping Go Analyzer LSP server...");
                    await client.stop();
                    log("Go Analyzer LSP server stopped successfully");
                } catch (error) {
                    console.error("Error stopping LSP server:", error);
                    log(`Error stopping LSP server: ${error}`);
                }
            }
            vscode.window.showInformationMessage("Go Analyzer: Extension deactivated - LSP server stopped (Расширение деактивировано - LSP сервер остановлен)");
        }
    );
    context.subscriptions.push(deactivateCmd);
    let cursorDisp: vscode.Disposable | undefined;
    let lastPos: vscode.Position | undefined;
    let timeoutHandle: NodeJS.Timeout | undefined;
    const startCursorTracking = () => {
        cursorDisp?.dispose();
        cursorDisp = vscode.window.onDidChangeTextEditorSelection(evt => {
            const editor = evt.textEditor;
            if (editor.document.languageId !== "go") return;
            if (!extensionActive) return;
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
                                {
                                    textDocument: { uri: editor.document.uri.toString() },
                                    position: pos,
                                    source: "auto",
                                    dump_json: vscode.workspace.getConfiguration("goAnalyzer")
                                        .get<boolean>("debugLifecycleJson", false),
                                },
                            ],
                        },
                    );
                    log(`Auto analysis: ${editor.document.uri.toString()} @ ${pos.line}:${pos.character}`);
                    const dumpAst = vscode.workspace.getConfiguration("goAnalyzer")
                        .get<boolean>("debugDumpAst", false);
                    if (dumpAst) {
                        await dumpAstForDocument(editor.document.uri);
                    }
                    for (const key in decorationTypes) {
                        editor.setDecorations(decorationTypes[key as keyof typeof decorationTypes], []);
                    }
                    if (Array.isArray(resp)) {
                        const byType: Record<string, vscode.DecorationOptions[]> = {
                            Declaration: [],
                            Use: [],
                            Pointer: [],
                            Race: [],
                            RaceLow: [],
                            AliasReassigned: [],
                            AliasCaptured: [],
                        };
                        for (const d of resp) {
                            const range = new vscode.Range(
                                new vscode.Position(d.range.start.line, d.range.start.character),
                                new vscode.Position(d.range.end.line, d.range.end.character),
                            );
                            if (byType[d.kind]) {
                                byType[d.kind].push({ range, hoverMessage: d.hover_text });
                            }
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
    const editorChangeDisposable = vscode.window.onDidChangeActiveTextEditor(ed => {
        if (ed && ed.document.languageId === "go") {
            startCursorTracking();
        }
    });
    addDisposable(editorChangeDisposable);
    const hoverProvider = vscode.languages.registerHoverProvider("go", {
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
                console.error("Hover provider error:", err);
                return null;
            }
        },
    });
    addDisposable(hoverProvider);
    client.start()
        .then(() => {
            vscode.window.showInformationMessage("Go Analyzer started");
            log("Go Analyzer started");
            disposables.forEach(d => context.subscriptions.push(d));
        })
        .catch(err => {
            vscode.window.showErrorMessage(`Failed to start Go Analyzer: ${err}`);
            console.error(err);
            log(`Failed to start Go Analyzer: ${err}`);
        });
}

export function deactivate(): Thenable<void> | undefined {
    console.log("Extension deactivation started");
    cleanupEventHandlers();
    if (client) {
        console.log("Stopping LSP client during deactivation");
        return client.stop().then(() => {
            console.log("LSP client stopped successfully");
        }).catch(error => {
            console.error("Error stopping LSP client:", error);
        });
    }
    return undefined;
}
