use serde::{ Deserialize, Serialize };
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
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum DecorationType {
    Declaration,
    Use,
    Pointer,
    Race,
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
