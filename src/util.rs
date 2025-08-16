use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::Node;

/// Преобразует узел дерева синтаксического разбора (tree-sitter Node)
/// в диапазон LSP (Range), который используется для выделения текста в редакторе.
/// Начальная и конечная позиции берутся из node.start_position() и node.end_position().
pub fn node_to_range(node: Node) -> Range {
    Range {
        // Начальная позиция диапазона (строка и столбец)
        start: Position::new(
            node.start_position().row as u32,
            node.start_position().column as u32,
        ),
        // Конечная позиция диапазона (строка и столбец)
        end: Position::new(
            node.end_position().row as u32,
            node.end_position().column as u32,
        ),
    }
}
