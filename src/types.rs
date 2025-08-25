use serde::{Deserialize, Serialize};
use tower_lsp::lsp_types::Range;

/// Структура для отправки уведомлений о прогрессе от сервера к клиенту LSP.
/// Используется для передачи текстовых сообщений о ходе анализа или других длительных операций.
pub struct ProgressNotification;
/// Реализация LSP-уведомления для ProgressNotification.
/// Метод "goanalyzer/progress" будет использоваться для отправки уведомлений.
/// В качестве параметра передаётся строка (например, сообщение о прогрессе).
impl tower_lsp::lsp_types::notification::Notification for ProgressNotification {
    /// Имя метода уведомления, используемое клиентом и сервером LSP.
    const METHOD: &'static str = "goanalyzer/progress";
    /// Тип параметров, передаваемых с уведомлением (в данном случае — строка).
    type Params = String;
}

/// Информация о переменной, используемой в анализе кода.
/// Содержит имя, диапазон объявления, все использования, флаг указателя, информацию о гонках и уникальный идентификатор.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VariableInfo {
    /// Имя переменной (например, "x")
    pub name: String,
    /// Диапазон (позиции) объявления переменной в исходном коде
    pub declaration: Range,
    /// Все диапазоны (позиции) использований переменной в коде
    pub uses: Vec<Range>,
    /// Является ли переменная указателем (true для *x или &x)
    pub is_pointer: bool,
    /// Потенциальная гонка данных обнаружена для этой переменной
    pub potential_race: bool,
    /// Серьёзность обнаруженной гонки данных (если есть)
    pub race_severity: RaceSeverity,
    /// Уникальный идентификатор переменной (используется для быстрого поиска в MutabilityMap)
    // TODO: Id декларации, чтобы быстро искать в `MutabilityMap`
    pub var_id: VarId,
}

/// Тип декорации для подсветки переменных и других сущностей в редакторе.
/// Используется для визуального выделения объявлений, использований, указателей, гонок данных и других случаев.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum DecorationType {
    /// Объявление переменной (например, `let x = ...` или `var x = ...`)
    Declaration,
    /// Использование переменной (например, просто `x` в выражении)
    Use,
    /// Переменная является указателем (например, `*x` или `&x`)
    Pointer,
    /// Обнаружена гонка данных (race condition) высокой или средней серьёзности
    Race,
    /// Гонка данных низкой серьёзности (например, потенциальная или ложноположительная)
    RaceLow,
    /// Переменная была переопределена (например, `x = ...` или повторное `:=`)
    AliasReassigned, // «x = …» или повторное :=
    /// Переменная захвачена в замыкании (closure) или горутине (goroutine)
    AliasCaptured, // переменная захвачена в closure / goroutine
}

/// Уровень серьёзности гонки данных.
/// Используется для классификации найденных гонок по степени опасности.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum RaceSeverity {
    /// Высокий приоритет — явная гонка данных, требует немедленного внимания.
    High,
    /// Средний приоритет — потенциальная гонка, возможно, не всегда проявляется.
    Medium,
    /// Низкий приоритет — возможная ложная гонка (например, если есть синхронизация).
    Low,
}

/// Структура для хранения информации о декорации (подсветке) в редакторе.
/// Используется для выделения переменных, указателей, гонок данных и других сущностей.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Decoration {
    /// Диапазон, к которому применяется декорация (позиции в документе)
    pub range: Range,
    /// Тип декорации (например, объявление, использование, гонка и т.д.)
    pub kind: DecorationType,
    /// Текст, который отображается при наведении курсора (hover)
    pub hover_text: String,
}

/// Структура для хранения количества различных сущностей в исходном коде.
/// Используется для подсчёта переменных, функций, каналов и горутин.
pub struct EntityCount {
    /// Количество переменных (например, объявлений переменных)
    pub variables: usize,
    /// Количество функций (объявлений функций)
    pub functions: usize,
    /// Количество каналов (например, make(chan ...) в Go)
    pub channels: usize,
    /// Количество горутин (go func(){}() в Go)
    pub goroutines: usize,
}

/// Уникальный идентификатор декларации переменной.
/// Используется для различения переменных по их положению в исходном коде.
/// start_byte и end_byte — это байтовые диапазоны узла идентификатора в дереве разбора.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VarId {
    /// Начальная позиция идентификатора переменной в байтах
    pub start_byte: usize,
    /// Конечная позиция идентификатора переменной в байтах
    pub end_byte: usize,
}

/// Статус изменяемости переменной.
/// Показывает, может ли переменная быть изменена, была ли она переопределена,
/// взят ли у неё адрес или захвачена ли она в замыкании/горутине.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Mutability {
    /// Переменная неизменяема (например, объявлена как let x = ...)
    Immutable,
    /// Переменная была переопределена (например, x = ...)
    Reassigned,
    /// У переменной был взят адрес (например, &x)
    AddressTaken,
    /// Переменная была захвачена в замыкании или горутине
    Captured,
}

/// Information about goroutine usage of a variable
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GoroutineUsage {
    /// Range of the goroutine statement
    pub goroutine_range: Range,
    /// All variable accesses within this goroutine
    pub variable_accesses: Vec<VariableAccess>,
    /// Type of goroutine (function call, anonymous function, etc.)
    pub goroutine_type: GoroutineType,
    /// Calculated race severity level
    pub potential_race_level: RaceSeverity,
}

/// Type of goroutine construct
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum GoroutineType {
    /// go func() { ... }()
    AnonymousFunction,
    /// go myFunction()
    FunctionCall,
    /// go obj.method()
    MethodCall,
    /// Unknown or complex pattern
    Unknown,
}

/// Information about a variable access within a goroutine
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VariableAccess {
    /// Location of the access
    pub range: Range,
    /// Type of access (read, write, etc.)
    pub access_type: VariableAccessType,
    /// Context description
    pub context: String,
}

/// Type of variable access
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum VariableAccessType {
    /// Reading the variable value
    Read,
    /// Writing to the variable
    Write,
    /// Modifying the variable (++, --, +=, etc.)
    Modify,
    /// Taking address of variable (&var)
    AddressOf,
    /// Dereferencing variable (*var)
    Dereference,
}

/// Context information about cursor position
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct CursorContext {
    /// The node at the cursor position
    pub target_node_kind: String,
    /// The cursor position
    pub position: Range,
    /// Type of context
    pub context_type: CursorContextType,
    /// Parent context type (if available)
    pub parent_context: Option<CursorContextType>,
    /// Additional context information
    pub details: Option<String>,
}

/// Types of contexts where cursor can be positioned
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum CursorContextType {
    /// Cursor is on a variable declaration
    VariableDeclaration,
    /// Cursor is on a parameter declaration
    ParameterDeclaration,
    /// Cursor is on a variable usage
    VariableUse,
    /// Cursor is on a struct field access (obj.field)
    FieldAccess,
    /// Cursor is on the object part of field access
    ObjectAccess,
    /// Cursor is on a struct field definition
    StructField,
    /// Cursor is on a function name (declaration)
    FunctionName,
    /// Cursor is on a function declaration
    FunctionDeclaration,
    /// Cursor is on a function call
    FunctionCall,
    /// Cursor is on an assignment statement
    Assignment,
    /// Cursor is in a goroutine context
    GoroutineContext,
    /// Cursor is on a go statement
    GoroutineStatement,
    /// Cursor is on a type reference
    TypeReference,
    /// Cursor is on a package reference
    PackageReference,
    /// Cursor is on a channel type
    ChannelType,
    /// Cursor is on an interface type
    InterfaceType,
    /// Cursor is on a struct type
    StructType,
    /// Unknown or unsupported context
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
    Use,     // переменная используется
    Call,    // вызов функции
    Send,    // отправка в канал
    Receive, // получение из канала
    Spawn,   // запуск горутины
    Sync,    // синхронизация
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphNode {
    pub id: String,    // уникальный идентификатор (например, имя+позиция)
    pub label: String, // отображаемое имя
    pub entity_type: GraphEntityType,
    pub range: Range,
    pub extra: Option<serde_json::Value>, // для доп. информации (тип, гонка, и т.д.)
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphEdge {
    pub from: String, // id источника
    pub to: String,   // id назначения
    pub edge_type: GraphEdgeType,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct GraphData {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}
