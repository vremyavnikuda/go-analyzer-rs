use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::Range;

pub struct ProgressNotification;
impl tower_lsp::lsp_types::notification::Notification for ProgressNotification {
    const METHOD: &'static str = "goanalyzer/progress";
    type Params = String;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VariableInfo {
    pub name: String,
    pub declaration: Range,
    pub uses: Vec<Range>,
    pub is_pointer: bool,
    pub potential_race: bool,
    pub race_severity: RaceSeverity,
    pub var_id: VarId,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum DecorationType {
    Declaration,
    Use,
    Pointer,
    Race,
    RaceLow,
    AliasReassigned, // «x = …» :=
    AliasCaptured,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum RaceSeverity {
    High,
    Medium,
    Low,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Decoration {
    pub range: Range,
    pub kind: DecorationType,
    pub hover_text: String,
}

pub struct EntityCount {
    pub variables: usize,
    pub functions: usize,
    pub channels: usize,
    pub goroutines: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VarId {
    pub start_byte: usize,
    pub end_byte: usize,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CursorContext {
    pub target_node_kind: String,
    pub position: Range,
    pub context_type: CursorContextType,
    pub parent_context: Option<CursorContextType>,
    pub details: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum CursorContextType {
    VariableDeclaration,
    ParameterDeclaration,
    VariableUse,
    FieldAccess,
    ObjectAccess,
    StructField,
    FunctionName,
    FunctionDeclaration,
    FunctionCall,
    Assignment,
    GoroutineContext,
    GoroutineStatement,
    TypeReference,
    PackageReference,
    ChannelType,
    InterfaceType,
    StructType,
    Unknown,
}

pub const ATOMIC_FUNCS: &[&str] = &[
    "AddInt32",
    "AddInt64",
    "AddUint32",
    "AddUint64",
    "AddUintptr",
    "CompareAndSwapInt32",
    "CompareAndSwapInt64",
    "CompareAndSwapPointer",
    "CompareAndSwapUint32",
    "CompareAndSwapUint64",
    "LoadInt32",
    "LoadInt64",
    "LoadPointer",
    "LoadUint32",
    "LoadUint64",
    "StoreInt32",
    "StoreInt64",
    "StorePointer",
    "StoreUint32",
    "StoreUint64",
];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GraphEntityType {
    Variable,
    Function,
    Channel,
    Goroutine,
    SyncBlock,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum GraphEdgeType {
    Use,
    Call,
    Send,
    Receive,
    Spawn,
    Sync,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub entity_type: GraphEntityType,
    pub range: Range,
    pub extra: Option<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub edge_type: GraphEdgeType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}
