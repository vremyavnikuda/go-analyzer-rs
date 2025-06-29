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
    //TODO:Id декларации, чтобы быстро искать в `MutabilityMap`
    pub var_id: VarId,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum DecorationType {
    Declaration,
    Use,
    Pointer,
    Race,
    RaceLow, // Пониженный приоритет для ложных гонок
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum RaceSeverity {
    High,   // Высокий приоритет - явная гонка данных
    Medium, // Средний приоритет - потенциальная гонка
    Low,    // Низкий приоритет - возможная ложная гонка (есть синхронизация)
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

// TODO: Уникальный ID декларации (байтовый диапазон узла `identifier`)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VarId {
    pub start_byte: usize,
    pub end_byte: usize,
}
// TODO: Статус изменяемости переменной
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mutability {
    Immutabile,
    Reassigned,
    Captured,
}
// TODO: Карта изменяемости переменных
pub type MutabilityMap = std::collections::HashMap<VarId, Mutability>;
