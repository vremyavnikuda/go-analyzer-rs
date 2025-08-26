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

let client: LanguageClient | undefined;
let extensionActive = true; // Track extension active state

const lastStatus = {
    variables: 0,
    functions: 0,
    channels: 0,
    goroutines: 0,
};

interface Decoration {
    range: vscode.Range;
    kind: "Declaration" | "Use" | "Pointer" | "Race" | "RaceLow" | "AliasReassigned" | "AliasCaptured";
    // Changed from hoverMessage to match server
    hover_text: string;
}

const ProgressNotification = new NotificationType<string>(
    "goanalyzer/progress",
);
const IndexingStatusNotification = new NotificationType<{
    variables: number;
    functions: number;
    channels: number;
    goroutines: number;
}>("goanalyzer/indexingStatus");

export function activate(context: vscode.ExtensionContext) {
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

    /* -------- уведомления от сервера -------- */
    client.onNotification(IndexingStatusNotification, p => {
        lastStatus.variables = p.variables;
        lastStatus.functions = p.functions;
        lastStatus.channels = p.channels;
        lastStatus.goroutines = p.goroutines;
        updateStatusBar();
    });

    client.onNotification(ProgressNotification, message => {
        vscode.window.showInformationMessage(message);
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
        RaceLow: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: vscode.workspace.getConfiguration("goAnalyzer").get("raceLowColor", "orange"),
            overviewRulerColor: vscode.workspace.getConfiguration("goAnalyzer").get("raceLowColor", "orange"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
        AliasReassigned: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: cfg("aliasReassignedColor", "purple"),
            overviewRulerColor: cfg("aliasReassignedColor", "purple"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
        AliasCaptured: vscode.window.createTextEditorDecorationType({
            textDecoration: "underline",
            color: cfg("aliasCapturedColor", "magenta"),
            overviewRulerColor: cfg("aliasCapturedColor", "magenta"),
            overviewRulerLane: vscode.OverviewRulerLane.Right,
        }),
    };

    /* -------- команда showLifecycle -------- */
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
                                RaceLow: [],
                                AliasReassigned: [],
                                AliasCaptured: [],
                            };
                            for (const d of resp) {
                                const range = new vscode.Range(
                                    new vscode.Position(d.range.start.line, d.range.start.character),
                                    new vscode.Position(d.range.end.line, d.range.end.character),
                                );
                                byType[d.kind].push({ range, hoverMessage: d.hover_text });
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

    /* -------- команды активации/деактивации -------- */
    const activateCmd = vscode.commands.registerCommand(
        "goanalyzer.activate",
        () => {
            extensionActive = true;
            updateStatusBar();
            vscode.window.showInformationMessage("Go Analyzer: Extension activated (Расширение активировано)");
        }
    );
    context.subscriptions.push(activateCmd);

    const deactivateCmd = vscode.commands.registerCommand(
        "goanalyzer.deactivate",
        () => {
            extensionActive = false;
            updateStatusBar();
            // Clear any existing decorations
            const editor = vscode.window.activeTextEditor;
            if (editor && editor.document.languageId === "go") {
                for (const key in decorationTypes) {
                    editor.setDecorations(decorationTypes[key as keyof typeof decorationTypes], []);
                }
            }
            vscode.window.showInformationMessage("Go Analyzer: Extension deactivated (Расширение деактивировано)");
        }
    );
    context.subscriptions.push(deactivateCmd);

    /* -------- авто-анализ курсора -------- */
    let cursorDisp: vscode.Disposable | undefined;
    let lastPos: vscode.Position | undefined;
    let timeoutHandle: NodeJS.Timeout | undefined;

    const startCursorTracking = () => {
        cursorDisp?.dispose();

        cursorDisp = vscode.window.onDidChangeTextEditorSelection(evt => {
            const editor = evt.textEditor;
            if (editor.document.languageId !== "go") return;

            // Check if extension is active
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
                            RaceLow: [],
                            AliasReassigned: [],
                            AliasCaptured: [],
                        };
                        for (const d of resp) {
                            const range = new vscode.Range(
                                new vscode.Position(d.range.start.line, d.range.start.character),
                                new vscode.Position(d.range.end.line, d.range.end.character),
                            );
                            byType[d.kind].push({ range, hoverMessage: d.hover_text });
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
                console.error("Hover provider error:", err);
                // Don't show error messages for hover failures as they're too frequent
                return null;
            }
        },
    });

    /* -------- команда showGraph -------- */
    const showGraphCmd = vscode.commands.registerCommand(
        "goanalyzer.showGraph",
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
            const uri = editor.document.uri.toString();
            // Запрашиваем у сервера JSON-граф
            const graph: any = await client!.sendRequest(
                "workspace/executeCommand",
                {
                    command: "goanalyzer/graph",
                    arguments: [{ uri }],
                },
            );
            if (!graph || !graph.nodes) {
                vscode.window.showErrorMessage("No graph data received.");
                return;
            }
            // Логирование на стороне расширения
            console.log('GRAPH FROM SERVER', graph);
            // Добавить недостающие узлы (callsite, sync и т.д.) в graph.nodes
            const allNodeIds = new Set(graph.nodes.map((n: any) => n.id));
            for (const e of graph.edges) {
                if (!allNodeIds.has(e.from)) {
                    graph.nodes.push({
                        id: e.from,
                        label: e.from.split(":")[0],
                        entity_type: "CallSite",
                        range: null,
                        extra: null
                    });
                    allNodeIds.add(e.from);
                }
                if (!allNodeIds.has(e.to)) {
                    graph.nodes.push({
                        id: e.to,
                        label: e.to.split(":")[0],
                        entity_type: "CallSite",
                        range: null,
                        extra: null
                    });
                    allNodeIds.add(e.to);
                }
            }
            // Открываем webview с SVG-графом
            const panel = vscode.window.createWebviewPanel(
                "goanalyzer-graph",
                "Go Analyzer: Граф кода",
                vscode.ViewColumn.Beside,
                { enableScripts: true }
            );
            // Цвета для сущностей
            const colorMap: Record<string, string> = {
                Variable: "#4caf50",
                Function: "#2196f3",
                Channel: "#ff9800",
                Goroutine: "#9c27b0",
                SyncBlock: "#607d8b",
            };
            // SVG-узлы
            const svgNodes = graph.nodes.map((n: any, i: number) => {
                const color = colorMap[n.entity_type] || "#888";
                // Простая раскладка: по кругу
                const angle = (2 * Math.PI * i) / graph.nodes.length;
                const cx = 250 + 180 * Math.cos(angle);
                const cy = 250 + 180 * Math.sin(angle);
                return `<circle cx="${cx}" cy="${cy}" r="24" fill="${color}" stroke="#222" stroke-width="2"/>
                    <text x="${cx}" y="${cy + 5}" text-anchor="middle" font-size="13" fill="#fff">${n.label}</text>`;
            }).join("\n");
            // SVG-ребра
            const svgEdges = graph.edges.map((e: any) => {
                const fromIdx = graph.nodes.findIndex((n: any) => n.id === e.from);
                const toIdx = graph.nodes.findIndex((n: any) => n.id === e.to);
                if (fromIdx === -1 || toIdx === -1) return "";
                const angle1 = (2 * Math.PI * fromIdx) / graph.nodes.length;
                const angle2 = (2 * Math.PI * toIdx) / graph.nodes.length;
                const x1 = 250 + 180 * Math.cos(angle1);
                const y1 = 250 + 180 * Math.sin(angle1);
                const x2 = 250 + 180 * Math.cos(angle2);
                const y2 = 250 + 180 * Math.sin(angle2);
                return `<line x1="${x1}" y1="${y1}" x2="${x2}" y2="${y2}" stroke="#aaa" stroke-width="2" marker-end="url(#arrow)"/>`;
            }).join("\n");
            // Легенда
            const legend = Object.entries(colorMap).map(([k, v]) =>
                `<span style="display:inline-block;width:16px;height:16px;background:${v};margin-right:6px;border-radius:3px;"></span>${k}`
            ).join(" &nbsp; ");
            const html = `
                <!DOCTYPE html>
                <html lang="en">
                <head>
                  <meta charset="UTF-8">
                  <title>Go Analyzer Graph</title>
                  <script src="https://d3js.org/d3.v7.min.js"></script>
                  <style>
                    .node { cursor: pointer; stroke: #fff; stroke-width: 1.5px; }
                    .edge-use { stroke: #999; stroke-width: 2px; }
                    .edge-call { stroke: #1f77b4; stroke-width: 2.5px; }
                    .edge-send { stroke: #2ca02c; stroke-width: 2.5px; }
                    .edge-receive { stroke: #ff7f0e; stroke-width: 2.5px; }
                    .edge-spawn { stroke: #d62728; stroke-width: 2.5px; }
                    .edge-sync { stroke: #9467bd; stroke-width: 2.5px; }
                    .legend { font-size: 13px; }
                  </style>
                </head>
                <body>
                  <svg id="graph" width="1400" height="900"></svg>
                  <script>
                    window.__GRAPH_DATA__ = ${JSON.stringify(graph)};
                    console.log('GRAPH DATA', window.__GRAPH_DATA__);
                    console.log('NODES', window.__GRAPH_DATA__.nodes);
                    console.log('EDGES', window.__GRAPH_DATA__.edges);
                    console.log('D3', typeof d3);
                  </script>
                  <script>
                    const vscode = acquireVsCodeApi();
                    const graph = window.__GRAPH_DATA__;
                    // Преобразуем from/to в source/target для d3
                    const edges = graph.edges.map(e => ({ ...e, source: e.from, target: e.to }));
                    const width = 1400, height = 900;
                    const svg = d3.select("#graph")
                      .call(d3.zoom().on("zoom", (event) => {
                        svg.selectAll("g").attr("transform", event.transform);
                      }));
                    const simulation = d3.forceSimulation(graph.nodes)
                      .force("link", d3.forceLink(edges).id(d => d.id).distance(120))
                      .force("charge", d3.forceManyBody().strength(-200))
                      .force("center", d3.forceCenter(width / 2, height / 2));
                    // Edge color by type
                    function edgeClass(type) {
                      switch(type) {
                        case "Call": return "edge-call";
                        case "Send": return "edge-send";
                        case "Receive": return "edge-receive";
                        case "Spawn": return "edge-spawn";
                        case "Sync": return "edge-sync";
                        default: return "edge-use";
                      }
                    }
                    // Draw edges
                    const link = svg.append("g")
                      .attr("stroke", "#999").attr("stroke-opacity", 0.6)
                      .selectAll("line")
                      .data(edges)
                      .enter().append("line")
                      .attr("class", d => edgeClass(d.edge_type));
                    // Draw nodes
                    const node = svg.append("g")
                      .attr("stroke", "#fff").attr("stroke-width", 1.5)
                      .selectAll("circle")
                      .data(graph.nodes)
                      .enter().append("circle")
                      .attr("r", 18)
                      .attr("fill", d => {
                        switch(d.entity_type) {
                          case "Variable": return "#ffe066";
                          case "Function": return "#6ab0f3";
                          case "Channel": return "#b6e3a7";
                          case "Goroutine": return "#f7a6a6";
                          case "SyncBlock": return "#c3a6f7";
                          default: return "#ccc";
                        }
                      })
                      .on("click", (event, d) => {
                        vscode.postMessage({ type: "goto", id: d.id });
                      });
                    // Add labels
                    svg.append("g")
                      .selectAll("text")
                      .data(graph.nodes)
                      .enter().append("text")
                      .attr("text-anchor", "middle")
                      .attr("dy", 5)
                      .text(d => d.label);
                    // Add tooltips
                    node.append("title")
                      .text(function(d) {
                        return d.label + " (" + d.entity_type + ")" + (d.range ? " | Line: " + (d.range.start.line + 1) : "");
                      });
                    // Simulation tick
                    simulation.on("tick", () => {
                      link.attr("x1", d => d.source.x)
                          .attr("y1", d => d.source.y)
                          .attr("x2", d => d.target.x)
                          .attr("y2", d => d.target.y);
                      node.attr("cx", d => d.x)
                          .attr("cy", d => d.y);
                      svg.selectAll("text")
                        .attr("x", d => d.x)
                        .attr("y", d => d.y);
                    });
                    // Legend
                    svg.append("g").attr("class", "legend").attr("transform", "translate(10,10)")
                      .selectAll("text")
                      .data([
                        ["Variable", "#ffe066"],
                        ["Function", "#6ab0f3"],
                        ["Channel", "#b6e3a7"],
                        ["Goroutine", "#f7a6a6"],
                        ["SyncBlock", "#c3a6f7"]
                      ])
                      .enter().append("text")
                      .attr("y", (d,i) => i*20)
                      .attr("fill", d => d[1])
                      .text(d => d[0]);
                    svg.append("g").attr("class", "legend").attr("transform", "translate(150,10)")
                      .selectAll("text")
                      .data([
                        ["Use", "#999"],
                        ["Call", "#1f77b4"],
                        ["Send", "#2ca02c"],
                        ["Receive", "#ff7f0e"],
                        ["Spawn", "#d62728"],
                        ["Sync", "#9467bd"]
                      ])
                      .enter().append("text")
                      .attr("y", (d,i) => i*20)
                      .attr("fill", d => d[1])
                      .text(d => d[0]);
                  </script>
                </body>
                </html>
            `;
            panel.webview.html = html;
        }
    );
    context.subscriptions.push(showGraphCmd);

    /* -------- запуск клиента -------- */
    client.start()
        .then(() => vscode.window.showInformationMessage("Go Analyzer started"))
        .catch(err => {
            vscode.window.showErrorMessage(`Failed to start Go Analyzer: ${err}`);
            console.error(err);
        });
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}