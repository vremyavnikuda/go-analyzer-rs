#![allow(clippy::collapsible_if)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::only_used_in_recursion)]

use crate::types::{GraphData, GraphEdge, GraphEdgeType, GraphEntityType, GraphNode};
use crate::{types::*, util::node_to_range};
use serde_json::json;
use std::collections::HashSet;
use tower_lsp::lsp_types::{Position, Range};
use tree_sitter::{Node, Point, Tree};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldTypeKind {
    Slice,
    String,
    Map,
    Other,
}

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
            if kind != "{" && kind != "}" && find_sync_in_node(node, code) {
                return true;
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
                return matches!(name, "Lock" | "Unlock" | "RLock" | "RUnlock" | "Wait");
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

pub fn determine_race_severity(
    tree: &Tree,
    range: Range,
    code: &str,
    is_write: bool,
    sync_funcs: &HashSet<String>,
) -> RaceSeverity {
    if is_access_synchronized(tree, range, code, sync_funcs) {
        RaceSeverity::Low
    } else if is_in_goroutine(tree, range) || is_write {
        RaceSeverity::High
    } else {
        RaceSeverity::Medium
    }
}

fn is_access_synchronized(
    tree: &Tree,
    range: Range,
    code: &str,
    sync_funcs: &HashSet<String>,
) -> bool {
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };
    let target_node = match find_node_at_position(tree.root_node(), target_point) {
        Some(node) => node,
        None => return has_synchronization_in_block(tree, range, code),
    };
    let mut current = Some(target_node);
    while let Some(candidate) = current {
        if candidate.kind() == "call_expression" {
            if is_mutex_call(candidate, code) || is_atomic_call(candidate, code) {
                return true;
            }
            if let Some(name) = call_expression_name(candidate, code) {
                if sync_funcs.contains(&name) {
                    return true;
                }
            }
        }
        current = candidate.parent();
    }
    current = Some(target_node);
    while let Some(candidate) = current {
        if candidate.kind() == "block" {
            return has_active_lock_for_target(candidate, target_node, code);
        }
        current = candidate.parent();
    }
    false
}

pub fn is_access_synchronized_at(
    tree: &Tree,
    range: Range,
    code: &str,
    sync_funcs: &HashSet<String>,
) -> bool {
    is_access_synchronized(tree, range, code, sync_funcs)
}

pub fn is_access_in_atomic_context(tree: &Tree, range: Range, code: &str) -> bool {
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };
    let mut current = find_node_at_position(tree.root_node(), target_point);
    while let Some(node) = current {
        if node.kind() == "call_expression" && is_atomic_call(node, code) {
            return true;
        }
        current = node.parent();
    }
    false
}

pub fn is_struct_field_declaration(tree: &Tree, range: Range) -> bool {
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };
    let mut current = find_node_at_position(tree.root_node(), target_point);
    while let Some(node) = current {
        if node.kind() == "field_identifier" {
            let mut parent = node.parent();
            while let Some(p) = parent {
                if p.kind() == "field_declaration" {
                    return true;
                }
                if p.kind() == "function_declaration" || p.kind() == "method_declaration" {
                    break;
                }
                parent = p.parent();
            }
        }
        current = node.parent();
    }
    false
}

pub fn is_heavy_work_in_call_context(tree: &Tree, range: Range, code: &str) -> bool {
    let heavy_full_names = [
        "fmt.Println",
        "fmt.Printf",
        "fmt.Sprintf",
        "sort.Slice",
        "sort.SliceStable",
    ];
    let heavy_simple_names = ["append", "copy"];
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };
    let mut current = find_node_at_position(tree.root_node(), target_point);
    while let Some(node) = current {
        if node.kind() == "call_expression" {
            if let Some(function) = node.child_by_field_name("function") {
                let name = text(code, function).trim().to_string();
                if heavy_full_names.iter().any(|n| *n == name)
                    || heavy_simple_names.iter().any(|n| *n == name)
                {
                    return true;
                }
            }
        }
        current = node.parent();
    }
    false
}

pub fn field_type_kind_at_declaration(tree: &Tree, range: Range, code: &str) -> FieldTypeKind {
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };
    let mut current = find_node_at_position(tree.root_node(), target_point);
    while let Some(node) = current {
        if node.kind() == "field_declaration" {
            if let Some(typ) = node.child_by_field_name("type") {
                return match typ.kind() {
                    "slice_type" => FieldTypeKind::Slice,
                    "map_type" => FieldTypeKind::Map,
                    "type_identifier" => {
                        if text(code, typ) == "string" {
                            FieldTypeKind::String
                        } else {
                            FieldTypeKind::Other
                        }
                    }
                    _ => FieldTypeKind::Other,
                };
            }
            return FieldTypeKind::Other;
        }
        current = node.parent();
    }
    FieldTypeKind::Other
}

pub fn detect_retention_pattern(
    tree: &Tree,
    range: Range,
    field_kind: FieldTypeKind,
) -> Option<&'static str> {
    if !matches!(
        field_kind,
        FieldTypeKind::Slice | FieldTypeKind::String | FieldTypeKind::Map
    ) {
        return None;
    }
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };
    let target_node = find_node_at_position(tree.root_node(), target_point)?;
    let rhs = assignment_rhs_for_target(target_node)?;
    match field_kind {
        FieldTypeKind::Slice | FieldTypeKind::String => {
            if rhs.kind() == "slice_expression" {
                return Some("Retention risk: sub-slice/sub-string may keep large backing memory");
            }
        }
        FieldTypeKind::Map => {
            if matches!(
                rhs.kind(),
                "identifier" | "selector_expression" | "index_expression"
            ) {
                return Some("Retention risk: map reference can keep large object graph alive");
            }
        }
        FieldTypeKind::Other => {}
    }
    None
}

pub fn is_value_copy_context(tree: &Tree, range: Range, code: &str) -> bool {
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };
    let target_node = match find_node_at_position(tree.root_node(), target_point) {
        Some(node) => node,
        None => return false,
    };

    if is_under_address_of(target_node, code) {
        return false;
    }
    let mut current = Some(target_node);
    while let Some(node) = current {
        let parent = match node.parent() {
            Some(p) => p,
            None => break,
        };
        match parent.kind() {
            "argument_list" | "expression_list" => {
                if let Some(grand) = parent.parent() {
                    if matches!(
                        grand.kind(),
                        "call_expression"
                            | "assignment_statement"
                            | "short_var_declaration"
                            | "return_statement"
                    ) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        current = Some(parent);
    }
    false
}

pub fn access_context_key(tree: &Tree, range: Range) -> Option<(u32, u32, u32, u32)> {
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };
    let mut current = find_node_at_position(tree.root_node(), target_point);
    while let Some(node) = current {
        if matches!(
            node.kind(),
            "function_declaration" | "method_declaration" | "function_literal"
        ) {
            let r = node_to_range(node);
            return Some((r.start.line, r.start.character, r.end.line, r.end.character));
        }
        current = node.parent();
    }
    None
}

fn assignment_rhs_for_target(target_node: Node) -> Option<Node> {
    let target_start = target_node.start_byte();
    let mut current = Some(target_node);
    while let Some(node) = current {
        if matches!(
            node.kind(),
            "assignment_statement" | "short_var_declaration"
        ) {
            let left = node.child_by_field_name("left")?;
            if left.kind() != "expression_list" {
                current = node.parent();
                continue;
            }

            let mut lhs_index: Option<usize> = None;
            for idx in 0..left.named_child_count() {
                if let Some(lhs) = left.named_child(idx) {
                    if lhs.start_byte() <= target_start && target_start <= lhs.end_byte() {
                        lhs_index = Some(idx);
                        break;
                    }
                }
            }
            let lhs_index = lhs_index?;
            let right = node.child_by_field_name("right")?;
            if right.kind() != "expression_list" {
                return Some(right);
            }
            if let Some(rhs) = right.named_child(lhs_index) {
                return Some(rhs);
            }
            return right.named_child(0);
        }
        current = node.parent();
    }
    None
}

fn is_under_address_of(node: Node, code: &str) -> bool {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if candidate.kind() == "unary_expression" {
            let txt = text(code, candidate).trim_start();
            if txt.starts_with('&') {
                return true;
            }
        }
        if matches!(
            candidate.kind(),
            "assignment_statement"
                | "short_var_declaration"
                | "return_statement"
                | "call_expression"
                | "function_declaration"
                | "method_declaration"
        ) {
            break;
        }
        current = candidate.parent();
    }
    false
}

pub fn collect_sync_functions(tree: &Tree, code: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "function_declaration" | "method_declaration" => {
                if let Some(body) = node.child_by_field_name("body") {
                    if find_sync_in_node(body, code) {
                        if let Some(name_node) = node.child_by_field_name("name") {
                            let name = text(code, name_node).to_string();
                            if !name.is_empty() {
                                names.insert(name);
                            }
                        }
                    }
                }
            }
            _ => {}
        }
        for i in (0..node.child_count()).rev() {
            if let Some(c) = node.child(i) {
                stack.push(c);
            }
        }
    }
    names
}

fn call_expression_name(call: Node, code: &str) -> Option<String> {
    let func = call.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => Some(text(code, func).to_string()),
        "selector_expression" => func
            .child_by_field_name("field")
            .map(|n| text(code, n).to_string()),
        _ => None,
    }
}

fn has_active_lock_for_target(block: Node, target_node: Node, code: &str) -> bool {
    let target_context = find_execution_context(target_node);
    let target_byte = target_node.start_byte();
    let mut calls = Vec::new();
    let mut stack = vec![block];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression" {
            calls.push(node);
        }
        for i in (0..node.child_count()).rev() {
            if let Some(child) = node.child(i) {
                stack.push(child);
            }
        }
    }
    calls.sort_by_key(|n| n.start_byte());
    use std::collections::HashMap;
    let mut lock_depths: HashMap<String, i32> = HashMap::new();
    for call in calls {
        if call.start_byte() > target_byte {
            break;
        }
        let same_context = match (target_context, find_execution_context(call)) {
            (Some(target), Some(current)) => same_scope(target, current),
            (None, None) => true,
            _ => false,
        };
        if !same_context {
            continue;
        }
        let (mutex_key, delta) = match lock_event(call, code) {
            Some(event) => event,
            None => continue,
        };
        let mut is_deferred = false;
        let mut current = Some(call);
        while let Some(candidate) = current {
            if candidate.kind() == "defer_statement" {
                is_deferred = true;
                break;
            }
            current = candidate.parent();
        }
        if delta < 0 && is_deferred {
            continue;
        }
        let depth = lock_depths.entry(mutex_key.clone()).or_insert(0);
        *depth += delta;
        if *depth <= 0 {
            lock_depths.remove(&mutex_key);
        }
    }
    lock_depths.values().any(|depth| *depth > 0)
}

fn find_execution_context(node: Node) -> Option<Node> {
    let mut current = Some(node);
    while let Some(candidate) = current {
        if matches!(
            candidate.kind(),
            "function_declaration" | "method_declaration" | "function_literal"
        ) {
            return Some(candidate);
        }
        current = candidate.parent();
    }
    None
}

fn lock_event(call: Node, code: &str) -> Option<(String, i32)> {
    let function = call.child_by_field_name("function")?;
    if function.kind() != "selector_expression" {
        return None;
    }
    let operand = function.child_by_field_name("operand")?;
    let field = function.child_by_field_name("field")?;
    let method = text(code, field);
    let delta = match method {
        "Lock" | "RLock" => 1,
        "Unlock" | "RUnlock" => -1,
        _ => return None,
    };
    let key = text(code, operand).trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), delta))
}

pub fn find_variable_at_position(tree: &Tree, code: &str, pos: Position) -> Option<VariableInfo> {
    let target_point = Point {
        row: pos.line as usize,
        column: pos.character as usize,
    };
    let target_node = find_node_at_position(tree.root_node(), target_point)?;
    if is_selector_call_symbol(target_node) {
        return None;
    }
    let var_name = extract_variable_name(target_node, code)?;
    if is_field_identifier_context(target_node, target_point) {
        return collect_field_info(tree, code, &var_name, target_point);
    }
    let function_scope = find_function_scope(tree.root_node(), target_point);
    collect_variable_info(tree, code, &var_name, function_scope, target_point)
}

fn find_node_at_position(node: tree_sitter::Node, target: Point) -> Option<tree_sitter::Node> {
    if !is_position_in_node_range(node, target) {
        return None;
    }
    let mut best_match = node;
    let mut best_size = node_size(node);
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Some(child_match) = find_node_at_position(child, target) {
                let child_size = node_size(child_match);
                if child_size < best_size && is_meaningful_node(child_match) {
                    best_match = child_match;
                    best_size = child_size;
                }
            }
        }
    }
    Some(best_match)
}

fn is_position_in_node_range(node: tree_sitter::Node, position: Point) -> bool {
    let start = node.start_position();
    let end = node.end_position();
    if start.row == end.row {
        return start.row == position.row
            && start.column <= position.column
            && position.column <= end.column;
    }
    if position.row < start.row || position.row > end.row {
        return false;
    }
    if position.row == start.row {
        return position.column >= start.column;
    }
    if position.row == end.row {
        return position.column <= end.column;
    }
    true
}

fn node_size(node: tree_sitter::Node) -> usize {
    let start = node.start_position();
    let end = node.end_position();
    if start.row == end.row {
        end.column - start.column
    } else {
        (end.row - start.row) * 1000 + end.column + start.column
    }
}

fn is_meaningful_node(node: tree_sitter::Node) -> bool {
    !matches!(
        node.kind(),
        "{" | "}"
            | "("
            | ")"
            | "["
            | "]"
            | ","
            | ";"
            | ":"
            | "."
            | "="
            | "+"
            | "-"
            | "*"
            | "/"
            | "%"
            | "<"
            | ">"
            | "!"
            | "&"
            | "|"
            | "^"
            | "~"
            | "?"
            | "comment"
            | "\n"
            | " "
    )
}

pub fn find_node_at_cursor_with_context(tree: &Tree, position: Position) -> Option<CursorContext> {
    let target_point = Point {
        row: position.line as usize,
        column: position.character as usize,
    };
    let node = find_node_at_position(tree.root_node(), target_point)?;
    Some(CursorContext {
        target_node_kind: node.kind().to_string(),
        position: node_to_range(node),
        context_type: determine_cursor_context(node),
        parent_context: node.parent().map(|p| determine_cursor_context(p)),
        details: Some(format!(
            "Node: {} at {}:{}",
            node.kind(),
            position.line,
            position.character
        )),
    })
}

fn determine_cursor_context(node: tree_sitter::Node) -> CursorContextType {
    match node.kind() {
        "identifier" => {
            if let Some(parent) = node.parent() {
                match parent.kind() {
                    "var_spec" | "short_var_declaration" => CursorContextType::VariableDeclaration,
                    "parameter_declaration" => CursorContextType::ParameterDeclaration,
                    "field_identifier" => CursorContextType::StructField,
                    "function_declaration" => CursorContextType::FunctionName,
                    "call_expression" => CursorContextType::FunctionCall,
                    "selector_expression" => {
                        if let Some(field_node) = parent.child_by_field_name("field") {
                            if field_node == node {
                                CursorContextType::FieldAccess
                            } else {
                                CursorContextType::ObjectAccess
                            }
                        } else {
                            CursorContextType::VariableUse
                        }
                    }
                    "go_statement" => CursorContextType::GoroutineContext,
                    "assignment_statement" => CursorContextType::Assignment,
                    _ => CursorContextType::VariableUse,
                }
            } else {
                CursorContextType::Unknown
            }
        }
        "field_identifier" => CursorContextType::FieldAccess,
        "type_identifier" => CursorContextType::TypeReference,
        "package_identifier" => CursorContextType::PackageReference,
        "function_declaration" => CursorContextType::FunctionDeclaration,
        "go_statement" => CursorContextType::GoroutineStatement,
        "channel_type" => CursorContextType::ChannelType,
        "interface_type" => CursorContextType::InterfaceType,
        "struct_type" => CursorContextType::StructType,
        _ => CursorContextType::Unknown,
    }
}

pub fn find_variable_at_position_enhanced(
    tree: &Tree,
    code: &str,
    pos: Position,
) -> Option<VariableInfo> {
    let cursor_context = find_node_at_cursor_with_context(tree, pos)?;
    match cursor_context.context_type {
        CursorContextType::VariableDeclaration
        | CursorContextType::ParameterDeclaration
        | CursorContextType::VariableUse
        | CursorContextType::FieldAccess
        | CursorContextType::ObjectAccess => find_variable_at_position(tree, code, pos),
        CursorContextType::FunctionCall => find_variable_at_position(tree, code, pos),
        _ => find_variable_at_position(tree, code, pos),
    }
}

fn extract_variable_name(node: tree_sitter::Node, code: &str) -> Option<String> {
    match node.kind() {
        "identifier" => {
            let byte_range = node.byte_range();
            code.get(byte_range).map(|s| s.to_string())
        }
        "field_identifier" => {
            let byte_range = node.byte_range();
            code.get(byte_range).map(|s| s.to_string())
        }
        "method_identifier" => {
            let byte_range = node.byte_range();
            code.get(byte_range).map(|s| s.to_string())
        }
        _ => {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if let Some(name) = extract_variable_name(child, code) {
                        return Some(name);
                    }
                }
            }
            None
        }
    }
}

fn is_field_identifier_context(node: tree_sitter::Node, target: Point) -> bool {
    if node.kind() == "field_identifier" {
        return true;
    }
    if node.kind() == "selector_expression" {
        if let Some(field) = node.child_by_field_name("field") {
            if field.start_position() <= target && target <= field.end_position() {
                return true;
            }
        }
    }
    false
}

fn is_selector_call_symbol(node: tree_sitter::Node) -> bool {
    let selector = match node.kind() {
        "selector_expression" => Some(node),
        "field_identifier" | "identifier" => {
            let parent = node.parent();
            match parent {
                Some(p) if p.kind() == "selector_expression" => Some(p),
                _ => None,
            }
        }
        _ => None,
    };
    let selector = match selector {
        Some(s) => s,
        None => return false,
    };
    let parent = match selector.parent() {
        Some(p) => p,
        None => return false,
    };
    if parent.kind() != "call_expression" {
        return false;
    }
    match parent.child_by_field_name("function") {
        Some(function_node) => function_node == selector,
        None => false,
    }
}

fn collect_field_info(
    tree: &Tree,
    code: &str,
    var_name: &str,
    target: Point,
) -> Option<VariableInfo> {
    let mut var_info = VariableInfo {
        name: var_name.to_string(),
        declaration: Range::new(Position::new(0, 0), Position::new(0, 0)),
        uses: vec![],
        is_pointer: false,
        potential_race: false,
        race_severity: RaceSeverity::Medium,
        var_id: VarId {
            start_byte: 0,
            end_byte: 0,
        },
    };
    let mut found_declaration = false;
    fn traverse_fields(
        node: tree_sitter::Node,
        code: &str,
        var_name: &str,
        target: Point,
        var_info: &mut VariableInfo,
        found_declaration: &mut bool,
    ) {
        if node.kind() == "field_declaration" {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if child.kind() == "field_identifier" {
                        let byte_range = child.byte_range();
                        if let Some(name) = code.get(byte_range.clone()) {
                            if name == var_name {
                                let decl_range = node_to_range(child);
                                if !*found_declaration
                                    || (child.start_position() <= target
                                        && target <= child.end_position())
                                {
                                    var_info.declaration = decl_range;
                                    var_info.var_id = VarId {
                                        start_byte: byte_range.start,
                                        end_byte: byte_range.end,
                                    };
                                    *found_declaration = true;
                                    if let Some(type_node) = node.child_by_field_name("type") {
                                        if type_node.kind() == "pointer_type" {
                                            var_info.is_pointer = true;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        if node.kind() == "selector_expression" {
            if let Some(field) = node.child_by_field_name("field") {
                if field.kind() == "field_identifier" {
                    let byte_range = field.byte_range();
                    if let Some(name) = code.get(byte_range.clone()) {
                        if name == var_name {
                            let use_range = node_to_range(field);
                            if !var_info.uses.contains(&use_range)
                                && use_range != var_info.declaration
                            {
                                var_info.uses.push(use_range);
                            }
                        }
                    }
                }
            }
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                traverse_fields(
                    cursor.node(),
                    code,
                    var_name,
                    target,
                    var_info,
                    found_declaration,
                );
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
    }
    traverse_fields(
        tree.root_node(),
        code,
        var_name,
        target,
        &mut var_info,
        &mut found_declaration,
    );
    if found_declaration {
        Some(var_info)
    } else {
        None
    }
}

fn find_function_scope(node: tree_sitter::Node, target: Point) -> Option<tree_sitter::Node> {
    if (node.kind() == "function_declaration" || node.kind() == "method_declaration")
        && node.start_position() <= target
        && target <= node.end_position()
    {
        return Some(node);
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Some(scope) = find_function_scope(child, target) {
                return Some(scope);
            }
        }
    }
    None
}

fn collect_variable_info(
    tree: &Tree,
    code: &str,
    var_name: &str,
    scope: Option<tree_sitter::Node>,
    target_point: Point,
) -> Option<VariableInfo> {
    let search_root = scope.unwrap_or(tree.root_node());
    let decl = resolve_decl_for_target(search_root, code, var_name, target_point)?;
    let mut var_info = VariableInfo {
        name: var_name.to_string(),
        declaration: decl.range,
        uses: vec![],
        is_pointer: decl.is_pointer,
        potential_race: false,
        race_severity: RaceSeverity::Medium,
        var_id: decl.var_id,
    };
    collect_uses_for_decl(search_root, code, var_name, decl, &mut var_info);
    Some(var_info)
}

#[derive(Clone, Copy)]
struct DeclInfo {
    range: Range,
    var_id: VarId,
    is_pointer: bool,
}

#[derive(Clone, Copy)]
struct ScopeEntry {
    decl: Option<DeclInfo>,
}

fn is_scope_node(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "method_declaration"
            | "function_literal"
            | "block"
            | "if_statement"
            | "for_statement"
            | "switch_statement"
            | "type_switch_statement"
            | "select_statement"
            | "case_clause"
    )
}

fn node_contains_point(node: tree_sitter::Node, target: Point) -> bool {
    node.start_position() <= target && target <= node.end_position()
}

fn range_contains_point(range: Range, target: Point) -> bool {
    let start = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };
    let end = Point {
        row: range.end.line as usize,
        column: range.end.character as usize,
    };
    start <= target && target <= end
}

fn resolve_current_decl(scope_stack: &[ScopeEntry]) -> Option<DeclInfo> {
    for entry in scope_stack.iter().rev() {
        if let Some(decl) = entry.decl {
            return Some(decl);
        }
    }
    None
}

fn current_scope_has_decl(scope_stack: &[ScopeEntry]) -> bool {
    scope_stack.last().and_then(|entry| entry.decl).is_some()
}

fn resolve_decl_for_target(
    root: tree_sitter::Node,
    code: &str,
    var_name: &str,
    target: Point,
) -> Option<DeclInfo> {
    fn traverse(
        node: tree_sitter::Node,
        code: &str,
        var_name: &str,
        target: Point,
        scope_stack: &mut Vec<ScopeEntry>,
    ) -> Option<DeclInfo> {
        let is_scope = is_scope_node(node.kind());
        if is_scope {
            scope_stack.push(ScopeEntry { decl: None });
        }
        if let Some(decl) =
            find_decl_in_node(node, code, var_name, current_scope_has_decl(scope_stack))
        {
            if let Some(top) = scope_stack.last_mut() {
                top.decl = Some(decl);
            }
            if range_contains_point(decl.range, target) {
                if is_scope {
                    scope_stack.pop();
                }
                return Some(decl);
            }
        }
        if node.kind() == "identifier" {
            if let Some(name) = code.get(node.byte_range()) {
                if name == var_name && node_contains_point(node, target) {
                    let found = resolve_current_decl(scope_stack);
                    if is_scope {
                        scope_stack.pop();
                    }
                    return found;
                }
            }
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                if let Some(found) = traverse(cursor.node(), code, var_name, target, scope_stack) {
                    if is_scope {
                        scope_stack.pop();
                    }
                    return Some(found);
                }
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        if is_scope {
            scope_stack.pop();
        }
        None
    }
    let mut scope_stack: Vec<ScopeEntry> = vec![ScopeEntry { decl: None }];
    traverse(root, code, var_name, target, &mut scope_stack)
}

fn collect_uses_for_decl(
    root: tree_sitter::Node,
    code: &str,
    var_name: &str,
    target_decl: DeclInfo,
    var_info: &mut VariableInfo,
) {
    fn decl_eq(a: DeclInfo, b: DeclInfo) -> bool {
        a.var_id.start_byte == b.var_id.start_byte && a.var_id.end_byte == b.var_id.end_byte
    }

    fn traverse(
        node: tree_sitter::Node,
        code: &str,
        var_name: &str,
        target_decl: DeclInfo,
        scope_stack: &mut Vec<ScopeEntry>,
        var_info: &mut VariableInfo,
    ) {
        let is_scope = is_scope_node(node.kind());
        if is_scope {
            scope_stack.push(ScopeEntry { decl: None });
        }
        if let Some(decl) =
            find_decl_in_node(node, code, var_name, current_scope_has_decl(scope_stack))
        {
            if let Some(top) = scope_stack.last_mut() {
                top.decl = Some(decl);
            }
        }
        if node.kind() == "identifier" {
            if let Some(name) = code.get(node.byte_range()) {
                if name == var_name {
                    if let Some(current) = resolve_current_decl(scope_stack) {
                        if decl_eq(current, target_decl) {
                            let use_range = node_to_range(node);
                            if use_range != var_info.declaration
                                && !var_info.uses.contains(&use_range)
                            {
                                if let Some(parent) = node.parent() {
                                    check_pointer_context(parent, code, var_info);
                                }
                                var_info.uses.push(use_range);
                            }
                        }
                    }
                }
            }
        }
        let mut cursor = node.walk();
        if cursor.goto_first_child() {
            loop {
                traverse(
                    cursor.node(),
                    code,
                    var_name,
                    target_decl,
                    scope_stack,
                    var_info,
                );
                if !cursor.goto_next_sibling() {
                    break;
                }
            }
        }
        if is_scope {
            scope_stack.pop();
        }
    }
    let mut scope_stack: Vec<ScopeEntry> = vec![ScopeEntry { decl: None }];
    traverse(
        root,
        code,
        var_name,
        target_decl,
        &mut scope_stack,
        var_info,
    );
}

fn find_decl_in_node(
    node: tree_sitter::Node,
    code: &str,
    var_name: &str,
    current_scope_has_decl: bool,
) -> Option<DeclInfo> {
    match node.kind() {
        "short_var_declaration" => {
            if current_scope_has_decl {
                return None;
            }
            let left = node.child_by_field_name("left")?;
            let ident = find_identifier_in_node(left, code, var_name)?;
            let mut is_pointer = false;
            if let Some(right) = node.child_by_field_name("right") {
                if contains_address_of(right, code) {
                    is_pointer = true;
                }
                if contains_reference_type(right) {
                    is_pointer = true;
                }
            }
            if let Some(value) = node.child_by_field_name("value") {
                if contains_address_of(value, code) {
                    is_pointer = true;
                }
                if contains_reference_type(value) {
                    is_pointer = true;
                }
            }
            let byte_range = ident.byte_range();
            return Some(DeclInfo {
                range: node_to_range(ident),
                var_id: VarId {
                    start_byte: byte_range.start,
                    end_byte: byte_range.end,
                },
                is_pointer,
            });
        }
        "var_spec" => {
            let ident = find_identifier_in_var_spec(node, code, var_name)?;
            let mut is_pointer = false;
            if let Some(type_node) = node.child_by_field_name("type") {
                if type_node.kind() == "pointer_type" || is_reference_type_kind(type_node.kind()) {
                    is_pointer = true;
                }
            }
            if let Some(value) = node.child_by_field_name("value") {
                if contains_address_of(value, code) {
                    is_pointer = true;
                }
                if contains_reference_type(value) {
                    is_pointer = true;
                }
            }
            let byte_range = ident.byte_range();
            return Some(DeclInfo {
                range: node_to_range(ident),
                var_id: VarId {
                    start_byte: byte_range.start,
                    end_byte: byte_range.end,
                },
                is_pointer,
            });
        }
        "parameter_declaration" => {
            let ident = find_identifier_in_param(node, code, var_name)?;
            let mut is_pointer = false;
            if let Some(type_node) = node.child_by_field_name("type") {
                if type_node.kind() == "pointer_type" || is_reference_type_kind(type_node.kind()) {
                    is_pointer = true;
                }
            }
            let byte_range = ident.byte_range();
            return Some(DeclInfo {
                range: node_to_range(ident),
                var_id: VarId {
                    start_byte: byte_range.start,
                    end_byte: byte_range.end,
                },
                is_pointer,
            });
        }
        "range_clause" => {
            if !range_clause_declares(node) {
                return None;
            }
            let left = node.child_by_field_name("left")?;
            let ident = find_identifier_in_node(left, code, var_name)?;
            let byte_range = ident.byte_range();
            return Some(DeclInfo {
                range: node_to_range(ident),
                var_id: VarId {
                    start_byte: byte_range.start,
                    end_byte: byte_range.end,
                },
                is_pointer: false,
            });
        }
        _ => {}
    }
    None
}

fn find_identifier_in_var_spec<'a>(
    node: tree_sitter::Node<'a>,
    code: &'a str,
    var_name: &'a str,
) -> Option<tree_sitter::Node<'a>> {
    if let Some(name_node) = node.child_by_field_name("name") {
        if let Some(found) = find_identifier_in_node(name_node, code, var_name) {
            return Some(found);
        }
    }
    if let Some(names_node) = node.child_by_field_name("names") {
        if let Some(found) = find_identifier_in_node(names_node, code, var_name) {
            return Some(found);
        }
    }
    None
}

fn find_identifier_in_param<'a>(
    node: tree_sitter::Node<'a>,
    code: &'a str,
    var_name: &'a str,
) -> Option<tree_sitter::Node<'a>> {
    if let Some(name_node) = node.child_by_field_name("name") {
        if let Some(found) = find_identifier_in_node(name_node, code, var_name) {
            return Some(found);
        }
    }
    if let Some(names_node) = node.child_by_field_name("names") {
        if let Some(found) = find_identifier_in_node(names_node, code, var_name) {
            return Some(found);
        }
    }
    None
}

fn find_identifier_in_node<'a>(
    node: tree_sitter::Node<'a>,
    code: &'a str,
    var_name: &'a str,
) -> Option<tree_sitter::Node<'a>> {
    if node.kind() == "identifier" {
        if let Some(name) = code.get(node.byte_range()) {
            if name == var_name {
                return Some(node);
            }
        }
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if let Some(found) = find_identifier_in_node(cursor.node(), code, var_name) {
                return Some(found);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    None
}

fn range_clause_declares(node: tree_sitter::Node) -> bool {
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let kind = cursor.node().kind();
            if kind == ":=" {
                return true;
            }
            if kind == "=" {
                return false;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}

fn check_pointer_context(node: tree_sitter::Node, code: &str, var_info: &mut VariableInfo) {
    match node.kind() {
        "unary_expression" => {
            // Check for & (address-of) or * (dereference)
            if let Some(operator) = node.child_by_field_name("operator") {
                let op_text = text(code, operator);
                if op_text == "&" || op_text == "*" {
                    var_info.is_pointer = true;
                }
            }
        }
        "pointer_type" => {
            var_info.is_pointer = true;
        }
        kind if is_reference_type_kind(kind) => {
            var_info.is_pointer = true;
        }
        _ => {
            if let Some(parent) = node.parent() {
                check_pointer_context(parent, code, var_info);
            }
        }
    }
}

fn contains_address_of(node: tree_sitter::Node, code: &str) -> bool {
    if node.kind() == "unary_expression" {
        if let Some(operator) = node.child_by_field_name("operator") {
            if text(code, operator) == "&" {
                return true;
            }
        }
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if contains_address_of(cursor.node(), code) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}

fn is_reference_type_kind(kind: &str) -> bool {
    matches!(
        kind,
        "slice_type" | "map_type" | "channel_type" | "function_type" | "interface_type"
    )
}

fn contains_reference_type(node: tree_sitter::Node) -> bool {
    if is_reference_type_kind(node.kind()) {
        return true;
    }
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            if contains_reference_type(cursor.node()) {
                return true;
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    false
}

pub fn is_variable_reassignment(tree: &Tree, var_name: &str, use_range: Range, code: &str) -> bool {
    let target_point = Point {
        row: use_range.start.line as usize,
        column: use_range.start.character as usize,
    };

    if let Some(node) = find_node_at_position(tree.root_node(), target_point) {
        if let Some(parent) = node.parent() {
            match parent.kind() {
                "assignment_statement" => {
                    // x = value
                    if let Some(left) = parent.child_by_field_name("left") {
                        if contains_variable_name(left, var_name, code) {
                            return true;
                        }
                    }
                }
                "inc_statement" | "dec_statement" => {
                    // x++ or x-- are reassignments
                    return true;
                }
                "short_var_declaration" => {
                    // For := declarations, check if this is a redeclaration
                    // In Go, x := can be reassignment if x already exists in scope
                    if let Some(left) = parent.child_by_field_name("left") {
                        if contains_variable_name(left, var_name, code) {
                            return false;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    false
}

fn contains_variable_name(node: tree_sitter::Node, var_name: &str, code: &str) -> bool {
    match node.kind() {
        "identifier" => {
            let node_text = tree_sitter_text(node, code);
            node_text == var_name
        }
        "expression_list" | "identifier_list" => {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if contains_variable_name(child, var_name, code) {
                        return true;
                    }
                }
            }
            false
        }
        _ => {
            for i in 0..node.child_count() {
                if let Some(child) = node.child(i) {
                    if contains_variable_name(child, var_name, code) {
                        return true;
                    }
                }
            }
            false
        }
    }
}

fn tree_sitter_text(node: tree_sitter::Node, code: &str) -> String {
    text(code, node).to_string()
}

pub fn is_variable_captured(
    tree: &Tree,
    var_name: &str,
    use_range: Range,
    declaration_range: Range,
) -> bool {
    let target_point = Point {
        row: use_range.start.line as usize,
        column: use_range.start.character as usize,
    };
    let decl_point = Point {
        row: declaration_range.start.line as usize,
        column: declaration_range.start.character as usize,
    };
    if let Some(use_node) = find_node_at_position(tree.root_node(), target_point) {
        if let Some(decl_node) = find_node_at_position(tree.root_node(), decl_point) {
            return is_captured_in_closure(use_node, decl_node, var_name);
        }
    }
    false
}

fn is_captured_in_closure(
    use_node: tree_sitter::Node,
    decl_node: tree_sitter::Node,
    _var_name: &str,
) -> bool {
    let use_closure = find_enclosing_closure_or_goroutine(use_node);
    if use_closure.is_none() {
        return false;
    }
    let decl_closure = find_enclosing_closure_or_goroutine(decl_node);
    match (use_closure, decl_closure) {
        (Some(use_closure), Some(decl_closure)) => !same_scope(use_closure, decl_closure),
        (Some(_), None) => true,
        (None, _) => false,
    }
}

fn same_scope(a: tree_sitter::Node, b: tree_sitter::Node) -> bool {
    a.kind() == b.kind() && a.start_byte() == b.start_byte() && a.end_byte() == b.end_byte()
}

fn find_enclosing_closure_or_goroutine(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut current = Some(node);
    while let Some(node) = current {
        match node.kind() {
            "function_literal" => {
                return Some(node);
            }
            "go_statement" => {
                return Some(node);
            }
            "function_declaration" => {
                return None;
            }
            _ => {
                current = node.parent();
            }
        }
    }
    None
}

pub fn is_in_goroutine(tree: &Tree, range: Range) -> bool {
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };
    find_goroutine_context(tree.root_node(), target_point).is_some()
}

fn find_goroutine_context(
    node: tree_sitter::Node,
    target_point: Point,
) -> Option<tree_sitter::Node> {
    if node.start_position() > target_point || target_point > node.end_position() {
        return None;
    }
    match node.kind() {
        "go_statement" => {
            // go func() {}
            if node.start_position() <= target_point && target_point <= node.end_position() {
                return Some(node);
            }
        }
        "function_literal" => {
            if let Some(parent) = node.parent() {
                if parent.kind() == "go_statement" {
                    if node.start_position() <= target_point && target_point <= node.end_position()
                    {
                        return Some(parent);
                    }
                }
            }
        }
        "call_expression" => {
            // go myFunc()
            if let Some(parent) = node.parent() {
                if parent.kind() == "go_statement" {
                    if node.start_position() <= target_point && target_point <= node.end_position()
                    {
                        return Some(parent);
                    }
                }
            }
        }
        _ => {}
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Some(goroutine_node) = find_goroutine_context(child, target_point) {
                return Some(goroutine_node);
            }
        }
    }
    None
}

pub fn count_entities(tree: &Tree, code: &str) -> EntityCount {
    fn traverse(node: Node, _code: &str, counts: &mut EntityCount) {
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
                traverse(cursor.node(), _code, counts);
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
    let bytes = code.as_bytes();
    if let Some(slice) = bytes.get(node.start_byte()..node.end_byte()) {
        unsafe { std::str::from_utf8_unchecked(slice) }
    } else {
        ""
    }
}

pub fn build_graph_data(tree: &Tree, code: &str) -> GraphData {
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    use std::collections::HashMap;
    let mut var_decl_ids = HashMap::new();

    fn make_id(kind: &str, name: &str, range: &Range) -> String {
        format!(
            "{}:{}:{}:{}:{}",
            kind, name, range.start.line, range.start.character, range.end.character
        )
    }

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
        if node.kind() == "identifier" {
            let name = crate::analysis::text(code, node);
            let range = crate::util::node_to_range(node);
            if let Some(parent) = node.parent() {
                if parent.kind() != "var_spec" && parent.kind() != "short_var_declaration" {
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
        if node.kind() == "call_expression" {
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
            let range = crate::util::node_to_range(node);
            let from_id = make_id("spawnsite", "go", &range);
            let to_id = make_id("go", "goroutine", &range);
            edges.push(GraphEdge {
                from: from_id,
                to: to_id,
                edge_type: GraphEdgeType::Spawn,
            });
        }
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
