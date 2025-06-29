use crate::analysis::{
    count_entities, determine_race_severity, find_variable_at_position, is_in_goroutine,
};
use crate::types::{Decoration, DecorationType, ProgressNotification};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::Mutex;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::{Parser, Tree};
use tree_sitter_go::language;

// Custom notification type for indexing status
pub struct IndexingStatusNotification;
impl tower_lsp::lsp_types::notification::Notification for IndexingStatusNotification {
    const METHOD: &'static str = "goanalyzer/indexingStatus";
    type Params = IndexingStatusParams;
}

#[derive(Serialize, Deserialize)]
pub struct IndexingStatusParams {
    pub variables: usize,
    pub functions: usize,
    pub channels: usize,
    pub goroutines: usize,
}

pub struct Backend {
    pub client: Client,
    pub documents: Mutex<HashMap<Url, String>>,
    pub parser: Mutex<Parser>,
    pub trees: Mutex<HashMap<Url, Tree>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        let mut parser = Parser::new();
        parser.set_language(language()).unwrap_or_else(|e| {
            eprintln!("Failed to set Go language: {:?}", e);
            std::process::exit(1);
        });
        Backend {
            client,
            documents: Mutex::new(HashMap::new()),
            parser: Mutex::new(parser),
            trees: Mutex::new(HashMap::new()),
        }
    }

    /// Получить или обновить дерево для документа
    pub async fn parse_document_with_cache(&self, uri: &Url, code: &str) -> Option<Tree> {
        let mut parser = self.parser.lock().await;
        let mut trees = self.trees.lock().await;
        let prev_tree = trees.get(uri);
        let new_tree = if let Some(prev) = prev_tree {
            parser.parse(code, Some(prev))
        } else {
            parser.parse(code, None)
        };
        if let Some(ref tree) = new_tree {
            trees.insert(uri.clone(), tree.clone());
        }
        new_tree
    }

    /// Получить дерево из кэша (если есть)
    pub async fn get_tree_from_cache(&self, uri: &Url) -> Option<Tree> {
        let trees = self.trees.lock().await;
        trees.get(uri).cloned()
    }

    pub async fn send_indexing_status(&self, uri: &Url) {
        let docs = self.documents.lock().await;
        if let Some(code) = docs.get(uri) {
            let tree = self.parse_document_with_cache(uri, code).await;
            if let Some(tree) = tree {
                let counts = count_entities(&tree);
                let params = IndexingStatusParams {
                    variables: counts.variables,
                    functions: counts.functions,
                    channels: counts.channels,
                    goroutines: counts.goroutines,
                };
                self.client
                    .send_notification::<IndexingStatusNotification>(params)
                    .await;
            }
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(
        &self,
        _: InitializeParams,
    ) -> tower_lsp::jsonrpc::Result<InitializeResult> {
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

    async fn shutdown(&self) -> tower_lsp::jsonrpc::Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let mut docs = self.documents.lock().await;
        docs.insert(
            params.text_document.uri.clone(),
            params.text_document.text.clone(),
        );
        drop(docs);
        // Парсим и кэшируем дерево при открытии
        self.parse_document_with_cache(&params.text_document.uri, &params.text_document.text)
            .await;
        self.send_indexing_status(&params.text_document.uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let mut docs = self.documents.lock().await;
        if let Some(doc) = docs.get_mut(&params.text_document.uri) {
            if let Some(change) = params.content_changes.into_iter().next_back() {
                *doc = change.text.clone();
                // Инкрементальное обновление дерева
                self.parse_document_with_cache(&params.text_document.uri, &change.text)
                    .await;
            }
        }
        drop(docs);
        self.send_indexing_status(&params.text_document.uri).await;
    }

    async fn hover(&self, params: HoverParams) -> tower_lsp::jsonrpc::Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let docs = self.documents.lock().await;
        let code = match docs.get(&uri) {
            Some(text) => text,
            None => {
                return Ok(None);
            }
        };
        let tree = self.get_tree_from_cache(&uri).await.or_else(|| {
            // Если нет в кэше, парсим и кэшируем
            futures::executor::block_on(self.parse_document_with_cache(&uri, code))
        });
        let tree = match tree {
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
    ) -> tower_lsp::jsonrpc::Result<Option<serde_json::Value>> {
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
            let tree = self.get_tree_from_cache(&uri).await.or_else(|| {
                futures::executor::block_on(self.parse_document_with_cache(&uri, code))
            });
            let tree = match tree {
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

                for use_range in var_info.uses.iter() {
                    let mut decoration_kind = if var_info.is_pointer {
                        DecorationType::Pointer
                    } else {
                        DecorationType::Use
                    };

                    let mut hover_text = format!("Use of `{}`", var_info.name);

                    if is_in_goroutine(&tree, *use_range) {
                        // Определяем приоритет гонки на основе контекста
                        let race_severity = determine_race_severity(&tree, *use_range);
                        var_info.race_severity = race_severity.clone();

                        match race_severity {
                            crate::types::RaceSeverity::High => {
                                decoration_kind = DecorationType::Race;
                                hover_text = format!(
                                    "Use of `{}` in goroutine - HIGH PRIORITY data race!",
                                    var_info.name
                                );
                            }
                            crate::types::RaceSeverity::Medium => {
                                decoration_kind = DecorationType::Race;
                                hover_text = format!(
                                    "Use of `{}` in goroutine - potential data race",
                                    var_info.name
                                );
                            }
                            crate::types::RaceSeverity::Low => {
                                decoration_kind = DecorationType::RaceLow;
                                hover_text = format!(
                                    "Use of `{}` in goroutine - LOW PRIORITY (sync detected)",
                                    var_info.name
                                );
                            }
                        }
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
