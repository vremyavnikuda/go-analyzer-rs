use crate::analysis::{
    access_context_key, build_graph_data, count_entities, detect_retention_pattern,
    determine_race_severity, field_type_kind_at_declaration, find_variable_at_position,
    find_variable_at_position_enhanced, is_access_in_atomic_context, is_access_synchronized_at,
    is_heavy_work_in_call_context, is_in_goroutine, is_struct_field_declaration,
    is_value_copy_context, FieldTypeKind,
};
use crate::semantic::{resolve_semantic_variable, SemanticConfig};
use crate::types::{
    Decoration, DecorationDiagnostic, DecorationDiagnosticSeverity, DecorationType,
    ProgressNotification, RaceSeverity,
};

fn decoration_label(kind: &DecorationType) -> &'static str {
    match kind {
        DecorationType::Declaration => "Declaration",
        DecorationType::Use => "Use",
        DecorationType::Pointer => "Pointer",
        DecorationType::Race => "Race",
        DecorationType::RaceLow => "RaceLow",
        DecorationType::AliasReassigned => "AliasReassigned",
        DecorationType::AliasCaptured => "AliasCaptured",
    }
}

fn decoration_color_key(kind: &DecorationType) -> &'static str {
    match kind {
        DecorationType::Declaration => "declarationColor",
        DecorationType::Use => "useColor",
        DecorationType::Pointer => "pointerColor",
        DecorationType::Race => "raceColor",
        DecorationType::RaceLow => "raceLowColor",
        DecorationType::AliasReassigned => "aliasReassignedColor",
        DecorationType::AliasCaptured => "aliasCapturedColor",
    }
}
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::time::Instant;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer};
use tree_sitter::{Parser, Point, Tree};
use tree_sitter_go::language;

pub struct IndexingStatusNotification;
impl tower_lsp::lsp_types::notification::Notification for IndexingStatusNotification {
    const METHOD: &'static str = "goanalyzer/indexingStatus";
    type Params = IndexingStatusParams;
}

#[derive(Serialize, Deserialize)]
pub struct IndexingStatusParams {
    pub uri: String,
    pub variables: usize,
    pub functions: usize,
    pub channels: usize,
    pub goroutines: usize,
}

pub struct ParseInfoNotification;
impl tower_lsp::lsp_types::notification::Notification for ParseInfoNotification {
    const METHOD: &'static str = "goanalyzer/parseInfo";
    type Params = ParseInfoParams;
}

#[derive(Serialize, Deserialize)]
pub struct ParseInfoParams {
    pub uri: String,
    pub source: Option<String>,
    pub cache_hit: bool,
    pub parse_ms: Option<u128>,
    pub code_len: usize,
}

pub struct LifecycleDumpNotification;
impl tower_lsp::lsp_types::notification::Notification for LifecycleDumpNotification {
    const METHOD: &'static str = "goanalyzer/lifecycleDump";
    type Params = LifecycleDumpParams;
}

#[derive(Serialize, Deserialize)]
pub struct LifecycleDumpParams {
    pub uri: String,
    pub points: Vec<LifecyclePoint>,
}

#[derive(Serialize, Deserialize)]
pub struct LifecyclePoint {
    pub name: String,
    pub file: String,
    pub pos: LifecyclePos,
    pub expected: LifecycleExpected,
}

#[derive(Serialize, Deserialize)]
pub struct LifecyclePos {
    pub line: u32,
    pub col: u32,
}

#[derive(Serialize, Deserialize)]
pub struct LifecycleExpected {
    pub var: String,
    pub kind: String,
    pub pointer: bool,
    pub reassign: bool,
    pub captured: bool,
    pub decoration: String,
    pub color_key: String,
}

#[derive(Clone)]
struct UseMeta {
    range: Range,
    reassign: bool,
    captured: bool,
}

fn make_diagnostic(
    severity: DecorationDiagnosticSeverity,
    code: &str,
    message: String,
) -> DecorationDiagnostic {
    DecorationDiagnostic {
        severity,
        code: code.to_string(),
        message,
    }
}

const MAX_CACHED_TREES: usize = 20;
const MAX_CACHED_DOCUMENTS: usize = 50;
const CACHE_TTL_SECONDS: u64 = 300;

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

    fn touch(&mut self) {
        self.timestamp = SystemTime::now();
    }

    fn is_expired(&self) -> bool {
        self.timestamp.elapsed().unwrap_or(Duration::from_secs(0))
            > Duration::from_secs(CACHE_TTL_SECONDS)
    }
}

pub struct Backend {
    pub client: Client,
    pub documents: Mutex<HashMap<Url, CacheEntry<String>>>,
    pub parser: Mutex<Parser>,
    pub trees: Mutex<HashMap<Url, CacheEntry<Tree>>>,
    pub semantic: SemanticConfig,
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
            semantic: SemanticConfig::from_env(),
        }
    }

    async fn cleanup_expired_cache(&self) {
        {
            let mut docs = self.documents.lock().await;
            docs.retain(|_, entry| !entry.is_expired());
        }

        {
            let mut trees = self.trees.lock().await;
            trees.retain(|_, entry| !entry.is_expired());
        }
    }

    async fn enforce_cache_limits(&self) {
        {
            let mut docs = self.documents.lock().await;
            if docs.len() > MAX_CACHED_DOCUMENTS {
                let mut entries: Vec<_> =
                    docs.iter().map(|(k, v)| (k.clone(), v.timestamp)).collect();
                entries.sort_by_key(|(_, timestamp)| *timestamp);
                let to_remove = entries.len() - MAX_CACHED_DOCUMENTS;
                for (uri, _) in entries.into_iter().take(to_remove) {
                    docs.remove(&uri);
                }
            }
        }
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

    pub async fn parse_document_with_cache(&self, uri: &Url, code: &str) -> Option<Tree> {
        self.cleanup_expired_cache().await;
        let mut parser = self.parser.lock().await;
        let mut trees = self.trees.lock().await;
        let prev_tree = trees.get(uri).map(|entry| &entry.data);
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
        trees.insert(uri.clone(), CacheEntry::new(new_tree.clone()));
        drop(trees);
        drop(parser);
        self.enforce_cache_limits().await;
        Some(new_tree)
    }

    pub async fn get_document(&self, uri: &Url) -> Option<String> {
        let mut docs = self.documents.lock().await;
        match docs.get_mut(uri) {
            Some(entry) if !entry.is_expired() => {
                entry.touch();
                Some(entry.data.clone())
            }
            _ => None,
        }
    }

    pub async fn get_tree_from_cache(&self, uri: &Url) -> Option<Tree> {
        let trees = self.trees.lock().await;
        if let Some(entry) = trees.get(uri) {
            if !entry.is_expired() {
                Some(entry.data.clone())
            } else {
                None
            }
        } else {
            None
        }
    }

    pub async fn send_indexing_status(&self, uri: &Url) {
        let code = match self.get_document(uri).await {
            Some(code) => code,
            None => {
                eprintln!("Document cache entry expired or missing for: {}", uri);
                return;
            }
        };
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
            uri: uri.to_string(),
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
    async fn initialize(
        &self,
        _: InitializeParams,
    ) -> tower_lsp::jsonrpc::Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec![
                        "goanalyzer/cursor".to_string(),
                        "goanalyzer/graph".to_string(),
                        "goanalyzer/ast".to_string(),
                    ],
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
        self.client
            .log_message(MessageType::INFO, "Go Analyzer server shutdown initiated")
            .await;

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

        {
            let _parser = self.parser.lock().await;
            eprintln!("Released tree-sitter parser resources");
        }

        self.client
            .log_message(MessageType::INFO, "Go Analyzer server shutdown completed")
            .await;

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

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let mut docs = self.documents.lock().await;
        docs.insert(
            params.text_document.uri.clone(),
            CacheEntry::new(params.text_document.text.clone()),
        );
        drop(docs);
        self.enforce_cache_limits().await;
        self.parse_document_with_cache(&params.text_document.uri, &params.text_document.text)
            .await;
        self.send_indexing_status(&params.text_document.uri).await;
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let mut docs = self.documents.lock().await;
        if let Some(doc) = docs.get_mut(&params.text_document.uri) {
            if let Some(change) = params.content_changes.into_iter().next_back() {
                *doc = CacheEntry::new(change.text.clone());
                let new_text = change.text.clone();
                drop(docs);
                self.parse_document_with_cache(&params.text_document.uri, &new_text)
                    .await;
                self.send_indexing_status(&params.text_document.uri).await;
                return;
            }
        }
        drop(docs);
    }

    async fn hover(&self, params: HoverParams) -> tower_lsp::jsonrpc::Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let code = match self.get_document(&uri).await {
            Some(code) => code,
            None => return Ok(None),
        };

        // go/types
        if let Some(semantic) =
            resolve_semantic_variable(&self.semantic, &uri, position, &code).await
        {
            let var_info = &semantic.info;
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: format!(
                        "**Variable**: `{}`\n\n**Declared at**: line {}\n**Type**: {}\n**Uses**: {}\n",
                        var_info.name,
                        var_info.declaration.start.line + 1,
                        if var_info.is_pointer { "Pointer" } else { "Value" },
                        var_info.uses.len()
                    ),
                }),
                range: Some(var_info.declaration),
            }));
        }
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
        let var_info = match std::panic::catch_unwind(|| {
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

            if params.arguments.is_empty() {
                self.client
                    .send_notification::<ProgressNotification>("No arguments provided".to_string())
                    .await;
                return Ok(None);
            }

            #[derive(Deserialize)]
            struct CursorCommandParams {
                #[serde(rename = "textDocument")]
                text_document: TextDocumentIdentifier,
                position: Position,
                source: Option<String>,
                dump_json: Option<bool>,
            }

            let args: CursorCommandParams = match params
                .arguments
                .first()
                .ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments".to_string())
                })
                .and_then(|arg| {
                    serde_json::from_value(arg.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!(
                            "Invalid arguments: {}",
                            e
                        ))
                    })
                }) {
                Ok(args) => args,
                Err(e) => {
                    self.client
                        .send_notification::<ProgressNotification>("Invalid arguments".to_string())
                        .await;
                    return Err(e);
                }
            };

            let uri = args.text_document.uri;
            let position = args.position;
            let source = args.source;
            let dump_json = args.dump_json.unwrap_or(false);
            let code = match self.get_document(&uri).await {
                Some(code) => code,
                None => {
                    self.client
                        .send_notification::<ProgressNotification>(
                            "No document found or expired".to_string(),
                        )
                        .await;
                    return Ok(None);
                }
            };

            let (tree, cache_hit, parse_ms) = match self.get_tree_from_cache(&uri).await {
                Some(tree) => (tree, true, None),
                None => {
                    let start = Instant::now();
                    let parsed = match self.parse_document_with_cache(&uri, &code).await {
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
                    (parsed, false, Some(start.elapsed().as_millis()))
                }
            };

            let _ = self
                .client
                .send_notification::<ParseInfoNotification>(ParseInfoParams {
                    uri: uri.to_string(),
                    source,
                    cache_hit,
                    parse_ms,
                    code_len: code.len(),
                })
                .await;

            let mut semantic_uses = None;
            let mut var_info = if let Some(semantic) =
                resolve_semantic_variable(&self.semantic, &uri, position, &code).await
            {
                semantic_uses = Some(semantic.uses);
                semantic.info
            } else {
                match std::panic::catch_unwind(|| {
                    find_variable_at_position_enhanced(&tree, &code, position)
                        .or_else(|| find_variable_at_position(&tree, &code, position))
                }) {
                    Ok(Some(var_info)) => var_info,
                    Ok(None) => {
                        self.client
                            .send_notification::<ProgressNotification>(
                                "No variable found".to_string(),
                            )
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
                }
            };

            let mut decorations = vec![];
            let mut lifecycle_points: Vec<LifecyclePoint> = Vec::new();
            let sync_funcs = crate::analysis::collect_sync_functions(&tree, &code);
            let is_decl_global = {
                let mut is_global = true;
                let decl_point = Point {
                    row: var_info.declaration.start.line as usize,
                    column: var_info.declaration.start.character as usize,
                };
                if let Some(mut node) = tree
                    .root_node()
                    .descendant_for_point_range(decl_point, decl_point)
                {
                    loop {
                        let kind = node.kind();
                        if kind == "function_declaration"
                            || kind == "method_declaration"
                            || kind == "func_literal"
                        {
                            is_global = false;
                            break;
                        }
                        if let Some(parent) = node.parent() {
                            node = parent;
                        } else {
                            break;
                        }
                    }
                }
                is_global
            };

            decorations.push(Decoration {
                range: var_info.declaration,
                kind: DecorationType::Declaration,
                hover_text: format!("Declaration of `{}`", var_info.name),
                diagnostic: None,
            });

            if dump_json {
                let decl_kind = DecorationType::Declaration;
                lifecycle_points.push(LifecyclePoint {
                    name: format!("{}_decl", var_info.name),
                    file: uri.to_string(),
                    pos: LifecyclePos {
                        line: var_info.declaration.start.line,
                        col: var_info.declaration.start.character,
                    },
                    expected: LifecycleExpected {
                        var: var_info.name.clone(),
                        kind: "decl".to_string(),
                        pointer: var_info.is_pointer,
                        reassign: false,
                        captured: false,
                        decoration: decoration_label(&decl_kind).to_string(),
                        color_key: decoration_color_key(&decl_kind).to_string(),
                    },
                });
            }

            let use_metas: Vec<UseMeta> = if let Some(uses) = semantic_uses.take() {
                uses.into_iter()
                    .map(|u| UseMeta {
                        range: u.range,
                        reassign: u.reassign,
                        captured: u.captured,
                    })
                    .collect()
            } else {
                var_info
                    .uses
                    .iter()
                    .map(|use_range| {
                        let reassign = match std::panic::catch_unwind(|| {
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
                                false
                            }
                        };
                        let captured = if reassign {
                            false
                        } else {
                            match std::panic::catch_unwind(|| {
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
                                    false
                                }
                            }
                        };
                        UseMeta {
                            range: *use_range,
                            reassign,
                            captured,
                        }
                    })
                    .collect()
            };

            let is_field_symbol = is_struct_field_declaration(&tree, var_info.declaration);
            let field_type_kind = if is_field_symbol {
                field_type_kind_at_declaration(&tree, var_info.declaration, &code)
            } else {
                FieldTypeKind::Other
            };
            let mut atomic_map: HashMap<String, bool> = HashMap::new();
            let mut sync_map: HashMap<String, bool> = HashMap::new();
            let mut heavy_map: HashMap<String, bool> = HashMap::new();
            let mut saw_atomic = false;
            let mut saw_non_atomic = false;
            let mut saw_sync = false;
            let mut saw_unsync = false;

            if is_field_symbol {
                for use_meta in &use_metas {
                    let key = format!(
                        "{}:{}:{}:{}",
                        use_meta.range.start.line,
                        use_meta.range.start.character,
                        use_meta.range.end.line,
                        use_meta.range.end.character
                    );
                    let in_atomic: bool = std::panic::catch_unwind(|| {
                        is_access_in_atomic_context(&tree, use_meta.range, &code)
                    })
                    .unwrap_or_default();
                    let in_sync: bool = std::panic::catch_unwind(|| {
                        is_access_synchronized_at(&tree, use_meta.range, &code, &sync_funcs)
                    })
                    .unwrap_or_default();
                    let heavy_under_lock = in_sync
                        && std::panic::catch_unwind(|| {
                            is_heavy_work_in_call_context(&tree, use_meta.range, &code)
                        })
                        .unwrap_or_default();

                    atomic_map.insert(key.clone(), in_atomic);
                    sync_map.insert(key.clone(), in_sync);
                    heavy_map.insert(key, heavy_under_lock);

                    if in_atomic {
                        saw_atomic = true;
                    } else {
                        saw_non_atomic = true;
                    }
                    if in_sync {
                        saw_sync = true;
                    } else {
                        saw_unsync = true;
                    }
                }
            }

            let has_mixed_atomic = is_field_symbol && saw_atomic && saw_non_atomic;
            let has_lock_coverage_violation = is_field_symbol && saw_sync && saw_unsync;
            let mut read_before_write_keys: HashSet<String> = HashSet::new();
            if is_field_symbol {
                let mut by_context: HashMap<(u32, u32, u32, u32), Vec<UseMeta>> = HashMap::new();
                for use_meta in &use_metas {
                    if let Some(ctx) = access_context_key(&tree, use_meta.range) {
                        by_context.entry(ctx).or_default().push(use_meta.clone());
                    }
                }
                for items in by_context.values_mut() {
                    items.sort_by_key(|u| (u.range.start.line, u.range.start.character));
                    let first_write_idx = items.iter().position(|u| u.reassign);
                    if let Some(write_idx) = first_write_idx {
                        if items.iter().any(|u| !u.reassign) {
                            for item in items.iter().take(write_idx) {
                                if !item.reassign {
                                    let key = format!(
                                        "{}:{}:{}:{}",
                                        item.range.start.line,
                                        item.range.start.character,
                                        item.range.end.line,
                                        item.range.end.character
                                    );
                                    read_before_write_keys.insert(key);
                                }
                            }
                        }
                    }
                }
            }
            let field_write_only =
                is_field_symbol && use_metas.len() >= 2 && use_metas.iter().all(|u| u.reassign);
            let has_read_before_write = !read_before_write_keys.is_empty();
            let is_struct_value_candidate = !is_field_symbol && !var_info.is_pointer;
            let mut emitted_mixed_atomic = false;
            let mut emitted_lock_coverage = false;
            let mut emitted_heavy_under_lock = false;
            let mut emitted_retention = false;
            let mut emitted_large_copy = false;
            let mut emitted_read_before_write = false;
            let mut emitted_write_only = false;
            for use_meta in use_metas {
                let use_range = use_meta.range;
                let is_reassignment = use_meta.reassign;
                let is_captured = use_meta.captured;
                let key = format!(
                    "{}:{}:{}:{}",
                    use_range.start.line,
                    use_range.start.character,
                    use_range.end.line,
                    use_range.end.character
                );
                let in_atomic = atomic_map.get(&key).copied().unwrap_or(false);
                let in_sync = sync_map.get(&key).copied().unwrap_or(false);
                let heavy_under_lock = heavy_map.get(&key).copied().unwrap_or(false);
                let mut decoration_kind = if var_info.is_pointer {
                    DecorationType::Pointer
                } else {
                    DecorationType::Use
                };
                let mut hover_text = format!("Use of `{}`", var_info.name);
                let mut diagnostic: Option<DecorationDiagnostic> = None;
                if is_reassignment {
                    decoration_kind = DecorationType::AliasReassigned;
                    hover_text = format!("Reassignment of `{}`", var_info.name);
                } else if is_captured {
                    decoration_kind = DecorationType::AliasCaptured;
                    hover_text = format!("Captured `{}` in closure/goroutine", var_info.name);
                }
                let is_in_goroutine_result: bool =
                    std::panic::catch_unwind(|| is_in_goroutine(&tree, use_range))
                        .unwrap_or_default();

                if !is_captured && is_in_goroutine_result && (is_decl_global || is_field_symbol) {
                    let race_access = if is_reassignment {
                        "write access"
                    } else {
                        "read access"
                    };
                    let race_severity = match std::panic::catch_unwind(|| {
                        determine_race_severity(
                            &tree,
                            use_range,
                            &code,
                            is_reassignment,
                            &sync_funcs,
                        )
                    }) {
                        Ok(severity) => severity,
                        Err(_) => RaceSeverity::Medium,
                    };
                    var_info.race_severity = race_severity.clone();
                    match race_severity {
                        crate::types::RaceSeverity::High => {
                            decoration_kind = DecorationType::Race;
                            hover_text = format!(
                                "Use of `{}` in goroutine - HIGH PRIORITY data race ({})",
                                var_info.name, race_access
                            );
                            diagnostic = Some(make_diagnostic(
                                DecorationDiagnosticSeverity::Warning,
                                "field-race-high",
                                format!(
                                    "Potential data race on `{}` in goroutine ({})",
                                    var_info.name, race_access
                                ),
                            ));
                        }
                        crate::types::RaceSeverity::Medium => {
                            decoration_kind = DecorationType::Race;
                            hover_text = format!(
                                "Use of `{}` in goroutine - potential data race ({})",
                                var_info.name, race_access
                            );
                        }
                        crate::types::RaceSeverity::Low => {
                            decoration_kind = DecorationType::RaceLow;
                            hover_text = format!(
                                "Use of `{}` in goroutine - LOW PRIORITY (sync detected, {})",
                                var_info.name, race_access
                            );
                        }
                    }
                    var_info.potential_race = true;
                }
                if is_field_symbol {
                    if has_mixed_atomic {
                        hover_text = format!(
                            "{} | mixed atomic/non-atomic access detected for field `{}`",
                            hover_text, var_info.name
                        );
                        if !in_atomic && !emitted_mixed_atomic && diagnostic.is_none() {
                            diagnostic = Some(make_diagnostic(
                                DecorationDiagnosticSeverity::Warning,
                                "field-mixed-atomic",
                                format!(
                                    "Field `{}` is accessed both atomically and non-atomically",
                                    var_info.name
                                ),
                            ));
                            emitted_mixed_atomic = true;
                        }
                    }
                    if has_lock_coverage_violation
                        && !in_sync
                        && !emitted_lock_coverage
                        && diagnostic.is_none()
                    {
                        hover_text = format!(
                            "{} | lock coverage violation for field `{}`",
                            hover_text, var_info.name
                        );
                        diagnostic = Some(make_diagnostic(
                            DecorationDiagnosticSeverity::Warning,
                            "field-lock-coverage",
                            format!(
                                "Field `{}` has mixed synchronized/unsynchronized access",
                                var_info.name
                            ),
                        ));
                        emitted_lock_coverage = true;
                    }
                    if heavy_under_lock && !emitted_heavy_under_lock && diagnostic.is_none() {
                        hover_text = format!(
                            "{} | heavy call under lock while touching `{}`",
                            hover_text, var_info.name
                        );
                        diagnostic = Some(make_diagnostic(
                            DecorationDiagnosticSeverity::Information,
                            "field-heavy-under-lock",
                            format!(
                                "Heavy operation under lock for field `{}` may hurt throughput",
                                var_info.name
                            ),
                        ));
                        emitted_heavy_under_lock = true;
                    }
                    if is_in_goroutine_result && !in_sync {
                        hover_text = format!(
                            "{} | captured field access in goroutine without active lock",
                            hover_text
                        );
                    }
                    if !emitted_retention {
                        if let Some(retention_msg) =
                            detect_retention_pattern(&tree, use_range, field_type_kind)
                        {
                            hover_text = format!("{} | {}", hover_text, retention_msg);
                            if diagnostic.is_none() {
                                diagnostic = Some(make_diagnostic(
                                    DecorationDiagnosticSeverity::Information,
                                    "field-retention",
                                    format!("{}: `{}`", retention_msg, var_info.name),
                                ));
                                emitted_retention = true;
                            }
                        }
                    }
                    if field_write_only {
                        hover_text = format!(
                            "{} | field appears write-only in current file scope",
                            hover_text
                        );
                        if !emitted_write_only && diagnostic.is_none() {
                            diagnostic = Some(make_diagnostic(
                                DecorationDiagnosticSeverity::Information,
                                "field-write-only",
                                format!("Field `{}` appears write-only", var_info.name),
                            ));
                            emitted_write_only = true;
                        }
                    } else if has_read_before_write
                        && read_before_write_keys.contains(&key)
                        && !is_reassignment
                    {
                        hover_text = format!(
                            "{} | read-before-write pattern detected in current file scope",
                            hover_text
                        );
                        if !emitted_read_before_write && diagnostic.is_none() {
                            diagnostic = Some(make_diagnostic(
                                DecorationDiagnosticSeverity::Warning,
                                "field-read-before-write",
                                format!(
                                    "Field `{}` is read before first write in this execution context",
                                    var_info.name
                                ),
                            ));
                            emitted_read_before_write = true;
                        }
                    }
                }
                if is_struct_value_candidate
                    && !is_reassignment
                    && !emitted_large_copy
                    && std::panic::catch_unwind(|| is_value_copy_context(&tree, use_range, &code))
                        .unwrap_or_default()
                {
                    hover_text = format!("{} | potential large struct copy by value", hover_text);
                    if diagnostic.is_none() {
                        diagnostic = Some(make_diagnostic(
                            DecorationDiagnosticSeverity::Information,
                            "struct-large-copy",
                            format!(
                                "Potential large struct copy by value for `{}`",
                                var_info.name
                            ),
                        ));
                        emitted_large_copy = true;
                    }
                }
                let decoration_label_text = decoration_label(&decoration_kind).to_string();
                let decoration_color = decoration_color_key(&decoration_kind).to_string();
                decorations.push(Decoration {
                    range: use_range,
                    kind: decoration_kind,
                    hover_text,
                    diagnostic,
                });
                if dump_json {
                    lifecycle_points.push(LifecyclePoint {
                        name: format!("{}_use_{}", var_info.name, lifecycle_points.len()),
                        file: uri.to_string(),
                        pos: LifecyclePos {
                            line: use_range.start.line,
                            col: use_range.start.character,
                        },
                        expected: LifecycleExpected {
                            var: var_info.name.clone(),
                            kind: "use".to_string(),
                            pointer: var_info.is_pointer,
                            reassign: is_reassignment,
                            captured: is_captured,
                            decoration: decoration_label_text,
                            color_key: decoration_color,
                        },
                    });
                }
            }
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
            if dump_json {
                let _ = self
                    .client
                    .send_notification::<LifecycleDumpNotification>(LifecycleDumpParams {
                        uri: uri.to_string(),
                        points: lifecycle_points,
                    })
                    .await;
            }
            return Ok(Some(value));
        } else if params.command == "goanalyzer/graph" {
            self.client
                .log_message(MessageType::INFO, "Executing goanalyzer/graph")
                .await;
            let args: TextDocumentIdentifier = params
                .arguments
                .first()
                .ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments".to_string())
                })
                .and_then(|arg| {
                    serde_json::from_value(arg.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!(
                            "Invalid arguments: {}",
                            e
                        ))
                    })
                })?;
            let uri = args.uri;
            let code = match self.get_document(&uri).await {
                Some(code) => code,
                None => {
                    self.client
                        .send_notification::<ProgressNotification>(
                            "No document found or expired".to_string(),
                        )
                        .await;
                    return Ok(None);
                }
            };
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
        } else if params.command == "goanalyzer/ast" {
            self.client
                .log_message(MessageType::INFO, "Executing goanalyzer/ast")
                .await;
            let args: TextDocumentIdentifier = params
                .arguments
                .first()
                .ok_or_else(|| {
                    tower_lsp::jsonrpc::Error::invalid_params("Missing arguments".to_string())
                })
                .and_then(|arg| {
                    serde_json::from_value(arg.clone()).map_err(|e| {
                        tower_lsp::jsonrpc::Error::invalid_params(format!(
                            "Invalid arguments: {}",
                            e
                        ))
                    })
                })?;
            let uri = args.uri;
            let code = match self.get_document(&uri).await {
                Some(code) => code,
                None => {
                    self.client
                        .send_notification::<ProgressNotification>(
                            "No document found or expired".to_string(),
                        )
                        .await;
                    return Ok(None);
                }
            };
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
            let sexp = tree.root_node().to_sexp();
            let value = serde_json::to_value(sexp)
                .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?;
            return Ok(Some(value));
        }
        Ok(None)
    }
}
