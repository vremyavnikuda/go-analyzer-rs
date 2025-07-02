use crate::types::{GraphData, GraphEdge, GraphEdgeType, GraphEntityType, GraphNode};
use crate::{types::*, util::node_to_range};
use serde_json::json;
use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::{Node, Point, Tree};

pub fn has_synchronization_in_block(tree: &Tree, range: Range, code: &str) -> bool {
    let target = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };

    let mut enclosing: Option<Node> = None;
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "block"
            && node.start_position() <= target
            && target <= node.end_position()
        {
            enclosing = Some(node);
            break;
        }
        for i in (0..node.child_count()).rev() {
            if let Some(c) = node.child(i) {
                stack.push(c);
            }
        }
    }
    let block = match enclosing {
        Some(b) => b,
        None => return false,
    };

    let mut cursor = block.walk();
    if cursor.goto_first_child() {
        loop {
            let node = cursor.node();
            let kind = node.kind();
            eprintln!(
                "[block_child] kind: {} bytes: {:?}",
                kind,
                node.byte_range()
            );
            if kind != "{" && kind != "}" {
                if find_sync_in_node(node, code) {
                    return true;
                }
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}

fn find_sync_in_node(node: Node, code: &str) -> bool {
    if node.kind() == "call_expression" {
        eprintln!("[has_sync] call_expression: {:?}", text(code, node));
        if is_mutex_call(node, code) || is_atomic_call(node, code) {
            return true;
        }
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if find_sync_in_node(cursor.node(), code) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}

#[inline]
fn is_mutex_call(call: Node, code: &str) -> bool {
    if let Some(sel) = call.child_by_field_name("function") {
        if sel.kind() == "selector_expression" {
            if let Some(field) = sel.child_by_field_name("field") {
                let name = text(code, field);
                return matches!(name, "Lock" | "Unlock" | "Wait");
            }
        }
    }
    false
}

#[inline]
fn is_atomic_call(call: Node, code: &str) -> bool {
    let func = match call.child_by_field_name("function") {
        Some(f) => f,
        None => return false,
    };
    if func.kind() == "selector_expression" {
        let pkg = func.child_by_field_name("operand").map(|n| text(code, n));
        let field = func.child_by_field_name("field").map(|n| text(code, n));
        if matches!(pkg, Some("atomic")) {
            if let Some(f) = field {
                return crate::types::ATOMIC_FUNCS.contains(&f);
            }
        }
    }
    false
}

pub fn determine_race_severity(tree: &Tree, range: Range, code: &str) -> RaceSeverity {
    if has_synchronization_in_block(tree, range, code) {
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

        // Если это просто идентификатор
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
                                let _expr_kinds = [
                                    "expression_list",
                                    "binary_expression",
                                    "assignment_statement",
                                ];
                                if info.declaration != use_range
                                    && !info.uses.contains(&use_range)
                                    && parent.kind() != "var_spec"
                                    && parent.kind() != "short_var_declaration"
                                {
                                    info.uses.push(use_range);
                                }
                                // Проверяем, является ли переменная указателем
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
        // Рекурсивно обходим детей
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

pub fn count_entities(tree: &Tree, code: &str) -> EntityCount {
    fn traverse(node: Node, code: &str, counts: &mut EntityCount) {
        match node.kind() {
            "var_spec" | "short_var_declaration" => {
                let mut cursor = node.walk();
                if cursor.goto_first_child() {
                    loop {
                        let child = cursor.node();
                        if child.kind() == "identifier" {
                            counts.variables += 1;
                        } else {
                            let mut sub_cursor = child.walk();
                            if sub_cursor.goto_first_child() {
                                loop {
                                    let sub_child = sub_cursor.node();
                                    if sub_child.kind() == "identifier" {
                                        counts.variables += 1;
                                    }
                                    if !sub_cursor.goto_next_sibling() {
                                        break;
                                    }
                                }
                            }
                        }
                        if !cursor.goto_next_sibling() {
                            break;
                        }
                    }
                }
            }
            "function_declaration" => counts.functions += 1,
            "go_statement" => counts.goroutines += 1,
            "channel_type" => counts.channels += 1,
            _ => {}
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                traverse(cursor.node(), code, counts);
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
    traverse(tree.root_node(), code, &mut counts);
    counts
}

#[inline]
fn text<'a>(code: &'a str, node: Node) -> &'a str {
    let bytes = &code.as_bytes()[node.start_byte()..node.end_byte()];
    unsafe { std::str::from_utf8_unchecked(bytes) }
}

/// Собирает граф сущностей Go-файла (переменные, функции, каналы, горутины и связи)
pub fn build_graph_data(tree: &Tree, code: &str) -> GraphData {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Вспомогательные мапы для уникальных id
    use std::collections::HashMap;
    let mut var_decl_ids = HashMap::new();

    // Вспомогательная функция для генерации id
    fn make_id(kind: &str, name: &str, range: &Range) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            kind, name, range.start.line, range.start.character, range.end.character
        )
    }

    // Рекурсивный обход AST
    fn traverse(
        node: Node,
        code: &str,
        nodes: &mut Vec<GraphNode>,
        edges: &mut Vec<GraphEdge>,
        var_decl_ids: &mut HashMap<String, String>,
    ) {
        match node.kind() {
            "var_spec" | "short_var_declaration" => {
                for i in 0..node.child_count() {
                    if let Some(child) = node.child(i) {
                        if child.kind() == "identifier" {
                            let name = crate::analysis::text(code, child);
                            let range = crate::util::node_to_range(child);
                            let id = make_id("var", name, &range);
                            var_decl_ids.insert(name.to_string(), id.clone());
                            let node_info = GraphNode {
                                id: id.clone(),
                                label: name.to_string(),
                                entity_type: GraphEntityType::Variable,
                                range: range.clone(),
                                extra: None,
                            };
                            nodes.push(node_info);
                        }
                    }
                }
            }
            "function_declaration" => {
                if let Some(ident) = node.child_by_field_name("name") {
                    let name = crate::analysis::text(code, ident);
                    let range = crate::util::node_to_range(ident);
                    let id = make_id("fn", name, &range);
                    let node_info = GraphNode {
                        id: id.clone(),
                        label: name.to_string(),
                        entity_type: GraphEntityType::Function,
                        range: range.clone(),
                        extra: None,
                    };
                    nodes.push(node_info);
                }
            }
            "go_statement" => {
                let range = crate::util::node_to_range(node);
                let id = make_id("go", "goroutine", &range);
                let node_info = GraphNode {
                    id: id.clone(),
                    label: "goroutine".to_string(),
                    entity_type: GraphEntityType::Goroutine,
                    range: range.clone(),
                    extra: None,
                };
                nodes.push(node_info);
            }
            "channel_type" => {
                let range = crate::util::node_to_range(node);
                let id = make_id("chan", "channel", &range);
                let node_info = GraphNode {
                    id: id.clone(),
                    label: "channel".to_string(),
                    entity_type: GraphEntityType::Channel,
                    range: range.clone(),
                    extra: None,
                };
                nodes.push(node_info);
            }
            _ => {}
        }
        // Связи: переменная используется (ищем идентификаторы)
        if node.kind() == "identifier" {
            let name = crate::analysis::text(code, node);
            let range = crate::util::node_to_range(node);
            if let Some(parent) = node.parent() {
                if parent.kind() != "var_spec" && parent.kind() != "short_var_declaration" {
                    // Это use, а не объявление
                    if let Some(decl_id) = var_decl_ids.get(name) {
                        let use_id = make_id("use", name, &range);
                        nodes.push(GraphNode {
                            id: use_id.clone(),
                            label: name.to_string(),
                            entity_type: GraphEntityType::Variable,
                            range: range.clone(),
                            extra: Some(json!({"use": true})),
                        });
                        edges.push(GraphEdge {
                            from: decl_id.clone(),
                            to: use_id,
                            edge_type: GraphEdgeType::Use,
                        });
                    }
                }
            }
        }
        // Новые типы рёбер
        if node.kind() == "call_expression" {
            // Call edge
            if let Some(func_node) = node.child_by_field_name("function") {
                let func_name = crate::analysis::text(code, func_node);
                let range = crate::util::node_to_range(func_node);
                let to_id = make_id("fn", func_name, &range);
                let from_id = make_id("callsite", func_name, &crate::util::node_to_range(node));
                edges.push(GraphEdge {
                    from: from_id,
                    to: to_id,
                    edge_type: GraphEdgeType::Call,
                });
            }
            // Sync edge
            if is_mutex_call(node, code) || is_atomic_call(node, code) {
                let sync_id = make_id("sync", "sync", &crate::util::node_to_range(node));
                let from_id = make_id("callsite", "sync", &crate::util::node_to_range(node));
                edges.push(GraphEdge {
                    from: from_id,
                    to: sync_id,
                    edge_type: GraphEdgeType::Sync,
                });
            }
        }
        if node.kind() == "send_statement" {
            // Send edge
            if let Some(chan_node) = node.child_by_field_name("channel") {
                let chan_name = crate::analysis::text(code, chan_node);
                let range = crate::util::node_to_range(chan_node);
                let to_id = make_id("chan", chan_name, &range);
                let from_id = make_id("send", chan_name, &crate::util::node_to_range(node));
                edges.push(GraphEdge {
                    from: from_id,
                    to: to_id,
                    edge_type: GraphEdgeType::Send,
                });
            }
        }
        if node.kind() == "unary_expression" && crate::analysis::text(code, node).starts_with("<-")
        {
            // Receive edge
            if let Some(chan_node) = node.child(0) {
                let chan_name = crate::analysis::text(code, chan_node);
                let range = crate::util::node_to_range(chan_node);
                let to_id = make_id("chan", chan_name, &range);
                let from_id = make_id("recv", chan_name, &crate::util::node_to_range(node));
                edges.push(GraphEdge {
                    from: from_id,
                    to: to_id,
                    edge_type: GraphEdgeType::Receive,
                });
            }
        }
        if node.kind() == "go_statement" {
            // Spawn edge
            let range = crate::util::node_to_range(node);
            let from_id = make_id("spawnsite", "go", &range);
            let to_id = make_id("go", "goroutine", &range);
            edges.push(GraphEdge {
                from: from_id,
                to: to_id,
                edge_type: GraphEdgeType::Spawn,
            });
        }
        // Рекурсивно обходим детей
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                traverse(cursor.node(), code, nodes, edges, var_decl_ids);
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }

    traverse(
        tree.root_node(),
        code,
        &mut nodes,
        &mut edges,
        &mut var_decl_ids,
    );
    GraphData { nodes, edges }
}
