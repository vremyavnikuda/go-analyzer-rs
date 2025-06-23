use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::Mutex;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tree_sitter::{Parser, Point, Tree};
use tree_sitter_go::language;

struct ProgressNotification;
impl tower_lsp::lsp_types::notification::Notification for ProgressNotification {
    const METHOD: &'static str = "goanalyzer/progress";
    type Params = String;
}

#[derive(Serialize, Deserialize)]
struct VariableInfo {
    name: String,
    declaration: Range,
    uses: Vec<Range>,
    is_pointer: bool,
    potential_race: bool,
}

#[derive(Serialize, Deserialize)]
enum DecorationType {
    Declaration,
    Use,
    Pointer,
    Race,
}

#[derive(Serialize, Deserialize)]
struct Decoration {
    range: Range,
    kind: DecorationType,
    hover_text: String,
}

struct Backend {
    client: Client,
    documents: Mutex<HashMap<Url, String>>,
    parser: Mutex<Parser>,
}

impl Backend {
    fn new(client: Client) -> Self {
        let mut parser = Parser::new();
        parser
            .set_language(language())
            .expect("Failed to set Go language");
        Backend {
            client,
            documents: Mutex::new(HashMap::new()),
            parser: Mutex::new(parser),
        }
    }

    async fn parse_document(&self, code: &str) -> Option<Tree> {
        let mut parser = self.parser.lock().await;
        parser.parse(code, None)
    }

    async fn run_go_vet(&self, uri: &Url) -> Vec<(u32, String)> {
        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => {
                return vec![];
            }
        };
        let output = match std::process::Command::new("go")
            .arg("vet")
            .arg("-race")
            .arg(path)
            .output()
        {
            Ok(output) => output,
            Err(_) => {
                return vec![];
            }
        };
        let stderr = String::from_utf8_lossy(&output.stderr);
        let mut results = vec![];
        for line in stderr.lines() {
            if let Some(line_num) = Self::parse_line_number(line) {
                results.push((line_num, line.to_string()));
            }
        }
        results
    }

    fn parse_line_number(line: &str) -> Option<u32> {
        if let Some(colon_idx) = line.rfind(':') {
            if let Some(line_str) = line[..colon_idx].rsplit(':').next() {
                return line_str.parse().ok();
            }
        }
        None
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["goanalyzer/cursor".to_string()],
                    ..Default::default()
                }),
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Go Analyzer initialized")
            .await;
        self.client
            .send_notification::<ProgressNotification>("Server initialized".to_string())
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let mut docs = self.documents.lock().await;
        docs.insert(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let mut docs = self.documents.lock().await;
        if let Some(doc) = docs.get_mut(&params.text_document.uri) {
            if let Some(change) = params.content_changes.into_iter().last() {
                *doc = change.text;
            }
        }
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.documents.lock().await;
        let code = match docs.get(&uri) {
            Some(text) => text,
            None => {
                return Ok(None);
            }
        };
        let tree = match self.parse_document(code).await {
            Some(tree) => tree,
            None => {
                return Ok(None);
            }
        };

        if let Some(var_info) = find_variable_at_position(&tree, code, position) {
            let mut markdown = format!(
                "**Variable**: `{}`\n\n**Declared at**: line {}\n**Type**: {}\n**Uses**: {}\n",
                var_info.name,
                var_info.declaration.start.line + 1,
                if var_info.is_pointer {
                    "Pointer"
                } else {
                    "Value"
                },
                var_info.uses.len()
            );
            if var_info.potential_race {
                markdown.push_str("**Warning**: Potential data race detected!\n");
            }
            Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: markdown,
                }),
                range: Some(var_info.declaration),
            }))
        } else {
            Ok(None)
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        if params.command == "goanalyzer/cursor" {
            self.client
                .log_message(MessageType::INFO, "Executing goanalyzer/cursor")
                .await;
            self.client
                .send_notification::<ProgressNotification>("Starting analysis...".to_string())
                .await;
            let args: TextDocumentPositionParams =
                serde_json::from_value(params.arguments[0].clone()).map_err(|e| {
                    tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid arguments: {}", e))
                })?;
            let uri = args.text_document.uri;
            let position = args.position;
            let docs = self.documents.lock().await;
            let code = match docs.get(&uri) {
                Some(text) => text,
                None => {
                    self.client
                        .send_notification::<ProgressNotification>("No document found".to_string())
                        .await;
                    return Ok(None);
                }
            };
            let tree = match self.parse_document(code).await {
                Some(tree) => tree,
                None => {
                    self.client
                        .send_notification::<ProgressNotification>(
                            "Failed to parse document".to_string(),
                        )
                        .await;
                    return Ok(None);
                }
            };

            if let Some(mut var_info) = find_variable_at_position(&tree, code, position) {
                let mut decorations = vec![];
                decorations.push(Decoration {
                    range: var_info.declaration,
                    kind: DecorationType::Declaration,
                    hover_text: format!("Declaration of `{}`", var_info.name),
                });

                // Проверяем каждое использование на предмет race conditions
                for use_range in var_info.uses.iter() {
                    let mut decoration_kind = if var_info.is_pointer {
                        DecorationType::Pointer
                    } else {
                        DecorationType::Use
                    };

                    let mut hover_text = format!("Use of `{}`", var_info.name);

                    // Проверяем, находится ли использование в горутине
                    if is_in_goroutine(&tree, *use_range) {
                        decoration_kind = DecorationType::Race;
                        hover_text = format!(
                            "Use of `{}` in goroutine - potential data race!",
                            var_info.name
                        );
                        var_info.potential_race = true;
                    }

                    decorations.push(Decoration {
                        range: *use_range,
                        kind: decoration_kind,
                        hover_text,
                    });
                }

                let value = serde_json::to_value(&decorations)
                    .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
                self.client
                    .send_notification::<ProgressNotification>("Analysis complete".to_string())
                    .await;
                return Ok(Some(value));
            }
            self.client
                .send_notification::<ProgressNotification>("No variable found".to_string())
                .await;
        }
        Ok(None)
    }
}

fn find_variable_at_position(tree: &Tree, code: &str, pos: Position) -> Option<VariableInfo> {
    let mut cursor = tree.walk();
    let mut var_info: Option<VariableInfo> = None;
    let mut found_variable_name: Option<String> = None;

    fn traverse<'a>(
        cursor: &mut tree_sitter::TreeCursor<'a>,
        code: &str,
        pos: Position,
        var_info: &mut Option<VariableInfo>,
        found_variable_name: &mut Option<String>,
    ) {
        let node = cursor.node();
        eprintln!(
            "Visiting node: kind={}, range={:?}",
            node.kind(),
            node_to_range(node)
        );

        // Проверяем, является ли узел объявлением переменной
        if node.kind() == "var_spec" || node.kind() == "short_var_declaration" {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.kind() == "identifier" {
                        let byte_range = child.byte_range();
                        if let Some(name) = code.get(byte_range) {
                            let decl_range = node_to_range(child);
                            let point = Point {
                                row: pos.line as usize,
                                column: pos.character as usize,
                            };

                            // Если курсор находится на объявлении переменной
                            if child.start_position() <= point && point <= child.end_position() {
                                if var_info.is_none() {
                                    *var_info = Some(VariableInfo {
                                        name: name.to_string(),
                                        declaration: decl_range,
                                        uses: vec![],
                                        is_pointer: false,
                                        potential_race: false,
                                    });
                                    *found_variable_name = Some(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        // Проверяем использование переменной
        if node.kind() == "identifier" {
            let byte_range = node.byte_range();
            if let Some(name) = code.get(byte_range) {
                let point = Point {
                    row: pos.line as usize,
                    column: pos.character as usize,
                };

                // Если курсор находится на использовании переменной
                if node.start_position() <= point && point <= node.end_position() {
                    if var_info.is_none() {
                        // Находим объявление этой переменной
                        *found_variable_name = Some(name.to_string());
                        // Создаем временную структуру, объявление найдем позже
                        *var_info = Some(VariableInfo {
                            name: name.to_string(),
                            declaration: Range::new(Position::new(0, 0), Position::new(0, 0)), // временное
                            uses: vec![],
                            is_pointer: false,
                            potential_race: false,
                        });
                    }
                }

                // Если мы ищем конкретную переменную, собираем её использование
                if let Some(ref mut info) = var_info {
                    if name == info.name {
                        let use_range = node_to_range(node);

                        // Проверяем, является ли это объявлением
                        if let Some(parent) = node.parent() {
                            if parent.kind() == "var_spec"
                                || parent.kind() == "short_var_declaration"
                            {
                                // Это объявление, обновляем координаты
                                if info.declaration.start.line == 0
                                    && info.declaration.start.character == 0
                                {
                                    info.declaration = use_range;
                                }
                            } else {
                                // Это использование, добавляем в список
                                info.uses.push(use_range);

                                // Проверяем, является ли это указателем
                                if let Some(grand_parent) = parent.parent() {
                                    if parent.kind() == "unary_expression"
                                        || grand_parent.kind() == "pointer_type"
                                        || parent.kind() == "selector_expression"
                                    {
                                        info.is_pointer = true;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Рекурсивный обход дочерних узлов
        if cursor.goto_first_child() {
            loop {
                traverse(cursor, code, pos, var_info, found_variable_name);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }
    }

    traverse(
        &mut cursor,
        code,
        pos,
        &mut var_info,
        &mut found_variable_name,
    );
    var_info
}

fn node_to_range(node: tree_sitter::Node) -> Range {
    Range {
        start: Position::new(
            node.start_position().row as u32,
            node.start_position().column as u32,
        ),
        end: Position::new(
            node.end_position().row as u32,
            node.end_position().column as u32,
        ),
    }
}

fn is_in_goroutine(tree: &Tree, range: Range) -> bool {
    let mut cursor = tree.walk();
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };

    fn traverse_goroutine<'a>(
        cursor: &mut tree_sitter::TreeCursor<'a>,
        target_point: Point,
    ) -> bool {
        let node = cursor.node();

        // Проверяем, является ли узел горутиной
        if node.kind() == "go_statement" {
            // Проверяем, находится ли целевая точка внутри горутины
            if node.start_position() <= target_point && target_point <= node.end_position() {
                return true;
            }
        }

        // Рекурсивный обход дочерних узлов
        if cursor.goto_first_child() {
            loop {
                if traverse_goroutine(cursor, target_point) {
                    cursor.goto_parent();
                    return true;
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
            cursor.goto_parent();
        }

        false
    }

    traverse_goroutine(&mut cursor, target_point)
}

#[tokio::main]
async fn main() {
    eprintln!("Starting server...");
    let (stdin, stdout) = (tokio::io::stdin(), tokio::io::stdout());
    let (service, socket) = LspService::new(Backend::new);
    Server::new(stdin, stdout, socket).serve(service).await;
}
