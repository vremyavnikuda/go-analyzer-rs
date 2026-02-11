use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::Node;

pub fn node_to_range(node: Node) -> Range {
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
