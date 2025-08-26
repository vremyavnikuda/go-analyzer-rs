use crate::analysis::{
    build_graph_data, count_entities, determine_race_severity, find_variable_at_position,
    find_variable_at_position_enhanced, is_in_goroutine,
};
use crate::types::{Decoration, DecorationType, ProgressNotification, RaceSeverity};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::{Parser, Tree};
use tree_sitter_go::language;

// Кастомный тип уведомления для статуса индексации
pub struct IndexingStatusNotification;
impl tower_lsp::lsp_types::notification::Notification for IndexingStatusNotification {
    // Имя метода для LSP-уведомления
    const METHOD: &'static str = "goanalyzer/indexingStatus";
    type Params = IndexingStatusParams;
}

// Параметры для уведомления о статусе индексации
#[derive(Serialize, Deserialize)]
pub struct IndexingStatusParams {
    // Количество переменных
    pub variables: usize,
    // Количество функций
    pub functions: usize,
    // Количество каналов
    pub channels: usize,
    // Количество горутин
    pub goroutines: usize,
}

// Константы для управления кэшами
const MAX_CACHED_TREES: usize = 20;
const MAX_CACHED_DOCUMENTS: usize = 50;
// 5 минут
const CACHE_TTL_SECONDS: u64 = 300;

// Структура для элемента кэша с TTL
#[derive(Clone)]
pub struct CacheEntry<T> {
    data: T,
    timestamp: SystemTime,
}

impl<T> CacheEntry<T> {
    fn new(data: T) -> Self {
        Self {
            data,
            timestamp: SystemTime::now(),
        }
    }

    fn is_expired(&self) -> bool {
        self.timestamp.elapsed().unwrap_or(Duration::from_secs(0))
            > Duration::from_secs(CACHE_TTL_SECONDS)
    }
}

// Основная структура Backend, реализующая сервер LSP
pub struct Backend {
    pub client: Client, // Клиент LSP для отправки уведомлений и сообщений
    pub documents: Mutex<HashMap<Url, CacheEntry<String>>>, // Кэш открытых документов с TTL
    pub parser: Mutex<Parser>, // Парсер tree-sitter для Go
    pub trees: Mutex<HashMap<Url, CacheEntry<Tree>>>, // Кэш синтаксических деревьев с TTL
}

impl Backend {
    // Конструктор Backend, инициализация парсера и кэшей
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

    /// Очистить истекшие элементы из кэша
    async fn cleanup_expired_cache(&self) {
        // Очистка кэша документов
        {
            let mut docs = self.documents.lock().await;
            docs.retain(|_, entry| !entry.is_expired());
        }

        // Очистка кэша деревьев
        {
            let mut trees = self.trees.lock().await;
            trees.retain(|_, entry| !entry.is_expired());
        }
    }

    /// Принудительно ограничить размер кэша по LRU принципу
    async fn enforce_cache_limits(&self) {
        // Ограничение размера кэша документов
        {
            let mut docs = self.documents.lock().await;
            if docs.len() > MAX_CACHED_DOCUMENTS {
                // Простая LRU: удаляем самые старые элементы
                let mut entries: Vec<_> =
                    docs.iter().map(|(k, v)| (k.clone(), v.timestamp)).collect();
                entries.sort_by_key(|(_, timestamp)| *timestamp);

                let to_remove = entries.len() - MAX_CACHED_DOCUMENTS;
                for (uri, _) in entries.into_iter().take(to_remove) {
                    docs.remove(&uri);
                }
            }
        }

        // Ограничение размера кэша деревьев
        {
            let mut trees = self.trees.lock().await;
            if trees.len() > MAX_CACHED_TREES {
                let mut entries: Vec<_> = trees
                    .iter()
                    .map(|(k, v)| (k.clone(), v.timestamp))
                    .collect();
                entries.sort_by_key(|(_, timestamp)| *timestamp);

                let to_remove = entries.len() - MAX_CACHED_TREES;
                for (uri, _) in entries.into_iter().take(to_remove) {
                    trees.remove(&uri);
                }
            }
        }
    }

    /// Получить или обновить дерево для документа (с кэшированием)
    pub async fn parse_document_with_cache(&self, uri: &Url, code: &str) -> Option<Tree> {
        // Периодическая очистка истекших элементов
        self.cleanup_expired_cache().await;

        let mut parser = self.parser.lock().await;
        let mut trees = self.trees.lock().await;

        let prev_tree = trees.get(uri).map(|entry| &entry.data);

        // Используем инкрементальный парсинг, если есть предыдущее дерево
        let new_tree = match if let Some(prev) = prev_tree {
            parser.parse(code, Some(prev))
        } else {
            parser.parse(code, None)
        } {
            Some(tree) => tree,
            None => {
                eprintln!("Failed to parse document: {}", uri);
                return None;
            }
        };

        // Кэшируем новое дерево с TTL
        trees.insert(uri.clone(), CacheEntry::new(new_tree.clone()));
        drop(trees);
        drop(parser);

        // Принудительно ограничиваем размер кэша
        self.enforce_cache_limits().await;

        Some(new_tree)
    }

    /// Получить дерево из кэша (если оно есть и не истекло)
    pub async fn get_tree_from_cache(&self, uri: &Url) -> Option<Tree> {
        let trees = self.trees.lock().await;
        if let Some(entry) = trees.get(uri) {
            if !entry.is_expired() {
                Some(entry.data.clone())
            } else {
                None // Истекший элемент будет удален при следующей очистке
            }
        } else {
            None
        }
    }

    /// Отправить клиенту статус индексации (количество сущностей в файле)
    pub async fn send_indexing_status(&self, uri: &Url) {
        let code = {
            let docs = self.documents.lock().await;
            match docs.get(uri) {
                Some(entry) if !entry.is_expired() => entry.data.clone(),
                _ => {
                    eprintln!("Document cache entry expired or missing for: {}", uri);
                    return;
                }
            }
        }; // docs lock is released here

        let tree = match self.parse_document_with_cache(uri, &code).await {
            Some(tree) => tree,
            None => {
                eprintln!("Failed to parse document for indexing status: {}", uri);
                return;
            }
        };

        let counts = match std::panic::catch_unwind(|| count_entities(&tree, &code)) {
            Ok(counts) => counts,
            Err(e) => {
                eprintln!("Panic occurred while counting entities: {:?}", e);
                return;
            }
        };

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

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    // Инициализация LSP-сервера: объявляем поддерживаемые возможности
    async fn initialize(
        &self,
        _: InitializeParams,
    ) -> tower_lsp::jsonrpc::Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)), // поддержка hover
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        "goanalyzer/cursor".to_string(),
                        "goanalyzer/graph".to_string(),
                    ], // поддерживаемые команды
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

    // Обработка события "initialized" — отправляем приветствие и уведомление о прогрессе
    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Go Analyzer initialized")
            .await;
        self.client
            .send_notification::<ProgressNotification>("Server initialized".to_string())
            .await;
    }

    // Завершение работы сервера - правильная очистка ресурсов
    async fn shutdown(&self) -> tower_lsp::jsonrpc::Result<()> {
        self.client
            .log_message(MessageType::INFO, "Go Analyzer server shutdown initiated")
            .await;

        // Очищаем все кэши и освобождаем ресурсы
        {
            let mut docs = self.documents.lock().await;
            let docs_count = docs.len();
            docs.clear();
            eprintln!("Cleared {} document cache entries", docs_count);
        }
        {
            let mut trees = self.trees.lock().await;
            let trees_count = trees.len();
            trees.clear();
            eprintln!("Cleared {} AST tree cache entries", trees_count);
        }

        // Освобождаем парсер
        {
            let _parser = self.parser.lock().await;
            eprintln!("Released tree-sitter parser resources");
        }

        self.client
            .log_message(MessageType::INFO, "Go Analyzer server shutdown completed")
            .await;

        // На Windows добавляем принудительный выход для предотвращения зависших процессов
        #[cfg(target_os = "windows")]
        {
            tokio::spawn(async {
                eprintln!("Windows: Initiating graceful shutdown in 100ms...");
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                eprintln!("Windows: Forcing process exit");
                std::process::exit(0);
            });
        }

        Ok(())
    }

    // Открытие документа: сохраняем текст, парсим дерево, отправляем статус индексации
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let mut docs = self.documents.lock().await;
        docs.insert(
            params.text_document.uri.clone(),
            CacheEntry::new(params.text_document.text.clone()),
        );
        drop(docs);

        // Принудительно ограничиваем размер кэша
        self.enforce_cache_limits().await;

        // Парсим и кэшируем дерево при открытии
        self.parse_document_with_cache(&params.text_document.uri, &params.text_document.text)
            .await;
        self.send_indexing_status(&params.text_document.uri).await;
    }

    // Изменение документа: обновляем текст, парсим дерево, отправляем статус индексации
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let mut docs = self.documents.lock().await;
        if let Some(doc) = docs.get_mut(&params.text_document.uri) {
            if let Some(change) = params.content_changes.into_iter().next_back() {
                // Обновляем запись с новым временным штампом
                *doc = CacheEntry::new(change.text.clone());
                let new_text = change.text.clone();
                drop(docs);

                // Инкрементальное обновление дерева
                self.parse_document_with_cache(&params.text_document.uri, &new_text)
                    .await;
                self.send_indexing_status(&params.text_document.uri).await;
                return;
            }
        }
        drop(docs);
    }

    // Hover-запрос: ищем переменную под курсором и возвращаем информацию о ней
    async fn hover(&self, params: HoverParams) -> tower_lsp::jsonrpc::Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.lock().await;

        let code = match docs.get(&uri) {
            Some(entry) if !entry.is_expired() => entry.data.clone(),
            _ => {
                return Ok(None);
            }
        };
        drop(docs); // Освобождаем блокировку раньше

        // Получаем дерево из кэша или парсим заново, если его нет
        let tree = match self.get_tree_from_cache(&uri).await {
            Some(tree) => tree,
            None => match self.parse_document_with_cache(&uri, &code).await {
                Some(tree) => tree,
                None => {
                    eprintln!("Failed to parse document for hover: {}", uri);
                    return Ok(None);
                }
            },
        };

        // Ищем переменную под курсором с улучшенным определением позиции
        let var_info = match std::panic::catch_unwind(|| {
            // Try enhanced detection first, fallback to standard
            find_variable_at_position_enhanced(&tree, &code, position)
                .or_else(|| find_variable_at_position(&tree, &code, position))
        }) {
            Ok(Some(var_info)) => var_info,
            Ok(None) => return Ok(None),
            Err(e) => {
                eprintln!("Panic occurred in find_variable_at_position: {:?}", e);
                return Ok(None);
            }
        };

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

        // Если есть потенциальная гонка данных — добавляем предупреждение
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
    }

    // Обработка команды goanalyzer/cursor: анализ переменной под курсором и отправка декораций
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

            // Десериализуем параметры команды (позиция курсора)
            if params.arguments.is_empty() {
                self.client
                    .send_notification::<ProgressNotification>("No arguments provided".to_string())
                    .await;
                return Ok(None);
            }

            let args: TextDocumentPositionParams =
                match serde_json::from_value(params.arguments[0].clone()) {
                    Ok(args) => args,
                    Err(e) => {
                        eprintln!("Failed to deserialize arguments: {}", e);
                        self.client
                            .send_notification::<ProgressNotification>(
                                "Invalid arguments".to_string(),
                            )
                            .await;
                        return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                            "Invalid arguments: {}",
                            e
                        )));
                    }
                };

            let uri = args.text_document.uri;
            let position = args.position;

            let code = {
                let docs = self.documents.lock().await;
                match docs.get(&uri) {
                    Some(entry) if !entry.is_expired() => entry.data.clone(),
                    _ => {
                        self.client
                            .send_notification::<ProgressNotification>(
                                "No document found or expired".to_string(),
                            )
                            .await;
                        return Ok(None);
                    }
                }
            };

            // Получаем дерево из кэша или парсим заново
            let tree = match self.get_tree_from_cache(&uri).await {
                Some(tree) => tree,
                None => match self.parse_document_with_cache(&uri, &code).await {
                    Some(tree) => tree,
                    None => {
                        self.client
                            .send_notification::<ProgressNotification>(
                                "Failed to parse document".to_string(),
                            )
                            .await;
                        return Ok(None);
                    }
                },
            };

            // Ищем переменную под курсором с улучшенным определением позиции
            let mut var_info = match std::panic::catch_unwind(|| {
                // First try the enhanced detection
                find_variable_at_position_enhanced(&tree, &code, position).or_else(|| {
                    // Fallback to standard detection
                    find_variable_at_position(&tree, &code, position)
                })
            }) {
                Ok(Some(var_info)) => var_info,
                Ok(None) => {
                    self.client
                        .send_notification::<ProgressNotification>("No variable found".to_string())
                        .await;
                    return Ok(None);
                }
                Err(e) => {
                    eprintln!("Panic occurred in find_variable_at_position: {:?}", e);
                    self.client
                        .send_notification::<ProgressNotification>("Analysis error".to_string())
                        .await;
                    return Ok(None);
                }
            };

            let mut decorations = vec![];

            // Декорация для объявления переменной
            decorations.push(Decoration {
                range: var_info.declaration,
                kind: DecorationType::Declaration,
                hover_text: format!("Declaration of `{}`", var_info.name),
            });

            // Декорации для всех использований переменной
            for use_range in var_info.uses.iter() {
                // По умолчанию: обычное использование или указатель
                let mut decoration_kind = if var_info.is_pointer {
                    DecorationType::Pointer
                } else {
                    DecorationType::Use
                };

                let mut hover_text = format!("Use of `{}`", var_info.name);

                // Check for variable reassignment
                let is_reassignment = match std::panic::catch_unwind(|| {
                    crate::analysis::is_variable_reassignment(
                        &tree,
                        &var_info.name,
                        *use_range,
                        &code,
                    )
                }) {
                    Ok(result) => result,
                    Err(e) => {
                        eprintln!("Panic occurred in is_variable_reassignment: {:?}", e);
                        false // Safe fallback
                    }
                };

                if is_reassignment {
                    decoration_kind = DecorationType::AliasReassigned;
                    hover_text = format!("Reassignment of `{}`", var_info.name);
                }
                // Check for variable capture in closure/goroutine
                else {
                    let is_captured = match std::panic::catch_unwind(|| {
                        crate::analysis::is_variable_captured(
                            &tree,
                            &var_info.name,
                            *use_range,
                            var_info.declaration,
                        )
                    }) {
                        Ok(result) => result,
                        Err(e) => {
                            eprintln!("Panic occurred in is_variable_captured: {:?}", e);
                            false // Safe fallback
                        }
                    };

                    if is_captured {
                        decoration_kind = DecorationType::AliasCaptured;
                        hover_text = format!("Captured `{}` in closure/goroutine", var_info.name);
                    }
                }

                // Если использование внутри горутины — определяем приоритет гонки
                // Only check for races if it's not already marked as reassignment or capture
                if !matches!(
                    decoration_kind,
                    DecorationType::AliasReassigned | DecorationType::AliasCaptured
                ) {
                    let is_in_goroutine_result =
                        match std::panic::catch_unwind(|| is_in_goroutine(&tree, *use_range)) {
                            Ok(result) => result,
                            Err(e) => {
                                eprintln!("Panic occurred in is_in_goroutine: {:?}", e);
                                false // Safe fallback
                            }
                        };

                    if is_in_goroutine_result {
                        // Определяем приоритет гонки на основе контекста
                        let race_severity = match std::panic::catch_unwind(|| {
                            determine_race_severity(&tree, *use_range, &code)
                        }) {
                            Ok(severity) => severity,
                            Err(e) => {
                                eprintln!("Panic occurred in determine_race_severity: {:?}", e);
                                RaceSeverity::Medium // Safe fallback
                            }
                        };

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
                }

                decorations.push(Decoration {
                    range: *use_range,
                    kind: decoration_kind,
                    hover_text,
                });
            }

            // Сериализуем декорации и отправляем клиенту
            let value = match serde_json::to_value(&decorations) {
                Ok(value) => value,
                Err(e) => {
                    eprintln!("Failed to serialize decorations: {}", e);
                    self.client
                        .send_notification::<ProgressNotification>(
                            "Serialization error".to_string(),
                        )
                        .await;
                    return Err(tower_lsp::jsonrpc::Error::internal_error());
                }
            };

            self.client
                .send_notification::<ProgressNotification>("Analysis complete".to_string())
                .await;
            return Ok(Some(value));
        }
        // Новый метод: goanalyzer/graph
        else if params.command == "goanalyzer/graph" {
            self.client
                .log_message(MessageType::INFO, "Executing goanalyzer/graph")
                .await;
            let args: TextDocumentIdentifier = serde_json::from_value(params.arguments[0].clone())
                .map_err(|e| {
                    tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid arguments: {}", e))
                })?;
            let uri = args.uri;
            let docs = self.documents.lock().await;
            let code = match docs.get(&uri) {
                Some(entry) if !entry.is_expired() => entry.data.clone(),
                _ => {
                    self.client
                        .send_notification::<ProgressNotification>(
                            "No document found or expired".to_string(),
                        )
                        .await;
                    return Ok(None);
                }
            };
            drop(docs); // Освобождаем блокировку раньше
            let tree = self.get_tree_from_cache(&uri).await.or_else(|| {
                futures::executor::block_on(self.parse_document_with_cache(&uri, &code))
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
            let graph = build_graph_data(&tree, &code);
            let value = serde_json::to_value(&graph)
                .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
            self.client
                .send_notification::<ProgressNotification>("Graph built".to_string())
                .await;
            return Ok(Some(value));
        }
        Ok(None)
    }
}
