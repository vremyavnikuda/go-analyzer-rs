use crate::{types::*, util::node_to_range};
use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::{Node, Point, Tree};

/// Проверяет, есть ли синхронизация в том же блоке кода
pub fn has_synchronization_in_block(tree: &Tree, range: Range) -> bool {
    // Упрощенная проверка - всегда возвращаем false для демонстрации
    // В реальной реализации здесь была бы сложная логика анализа AST
    false
}

/// Определяет приоритет гонки данных на основе контекста
pub fn determine_race_severity(tree: &Tree, range: Range) -> RaceSeverity {
    if has_synchronization_in_block(tree, range) {
        RaceSeverity::Low
    } else {
        RaceSeverity::High
    }
}

pub fn find_variable_at_position(tree: &Tree, code: &str, pos: Position) -> Option<VariableInfo> {
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

        if node.kind() == "var_spec" || node.kind() == "short_var_declaration" {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.kind() == "identifier" {
                        let byte_range = child.byte_range();
                        if let Some(name) = code.get(byte_range.clone()) {
                            let decl_range = node_to_range(child);
                            let point = Point {
                                row: pos.line as usize,
                                column: pos.character as usize,
                            };
                            if child.start_position() <= point && point <= child.end_position() {
                                if var_info.is_none() {
                                    *var_info = Some(VariableInfo {
                                        name: name.to_string(),
                                        declaration: decl_range,
                                        uses: vec![],
                                        is_pointer: false,
                                        potential_race: false,
                                        race_severity: RaceSeverity::Medium,
                                        var_id: VarId {
                                            start_byte: byte_range.start,
                                            end_byte: byte_range.end,
                                        },
                                    });
                                    *found_variable_name = Some(name.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        if node.kind() == "identifier" {
            let byte_range = node.byte_range();
            if let Some(name) = code.get(byte_range.clone()) {
                let point = Point {
                    row: pos.line as usize,
                    column: pos.character as usize,
                };
                if node.start_position() <= point && point <= node.end_position() {
                    if var_info.is_none() {
                        *found_variable_name = Some(name.to_string());
                        *var_info = Some(VariableInfo {
                            name: name.to_string(),
                            declaration: Range::new(Position::new(0, 0), Position::new(0, 0)),
                            uses: vec![],
                            is_pointer: false,
                            potential_race: false,
                            race_severity: RaceSeverity::Medium,
                            var_id: VarId {
                                start_byte: byte_range.start,
                                end_byte: byte_range.end,
                            },
                        });
                    }
                }
                if let Some(ref mut info) = var_info {
                    if name == info.name {
                        let use_range = node_to_range(node);
                        if let Some(parent) = node.parent() {
                            if parent.kind() == "var_spec"
                                || parent.kind() == "short_var_declaration"
                            {
                                if info.declaration.start.line == 0
                                    && info.declaration.start.character == 0
                                {
                                    info.declaration = use_range;
                                }
                            } else {
                                info.uses.push(use_range);
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

pub fn is_in_goroutine(tree: &Tree, range: Range) -> bool {
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
        if node.kind() == "go_statement" {
            if node.start_position() <= target_point && target_point <= node.end_position() {
                return true;
            }
        }
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

pub fn count_entities(tree: &Tree) -> EntityCount {
    fn traverse(node: Node, counts: &mut EntityCount) {
        match node.kind() {
            "var_spec" | "short_var_declaration" => counts.variables += 1,
            "function_declaration" => counts.functions += 1,
            "go_statement" => counts.goroutines += 1,
            "channel_type" => counts.channels += 1,
            _ => {}
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                traverse(cursor.node(), counts);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
    let mut counts = EntityCount {
        variables: 0,
        functions: 0,
        channels: 0,
        goroutines: 0,
    };
    traverse(tree.root_node(), &mut counts);
    counts
}
