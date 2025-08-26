#![allow(clippy::collapsible_if)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::only_used_in_recursion)]

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
    // First, check if we're inside a goroutine
    let target_point = Point {
        row: range.start.line as usize,
        column: range.start.character as usize,
    };

    // Find the goroutine context if any
    if let Some(goroutine_node) = find_goroutine_context(tree.root_node(), target_point) {
        // Check for synchronization within the entire goroutine scope
        if has_synchronization_in_goroutine(goroutine_node, code) {
            RaceSeverity::Low
        } else {
            RaceSeverity::High
        }
    } else {
        // Not in goroutine, check local block synchronization
        if has_synchronization_in_block(tree, range, code) {
            RaceSeverity::Low
        } else {
            RaceSeverity::High
        }
    }
}

/// Check for synchronization within a goroutine scope
fn has_synchronization_in_goroutine(goroutine_node: tree_sitter::Node, code: &str) -> bool {
    // Look for synchronization primitives within the entire goroutine
    find_sync_in_node(goroutine_node, code)
}

pub fn find_variable_at_position(tree: &Tree, code: &str, pos: Position) -> Option<VariableInfo> {
    let target_point = Point {
        row: pos.line as usize,
        column: pos.character as usize,
    };

    // First, find the exact node at the cursor position
    let target_node = find_node_at_position(tree.root_node(), target_point)?;
    let var_name = extract_variable_name(target_node, code)?;

    // Find the function scope containing this position
    let function_scope = find_function_scope(tree.root_node(), target_point);

    // Collect all variable information within the scope
    collect_variable_info(tree, code, &var_name, function_scope)
}

/// Find the exact node at the given position with improved accuracy
fn find_node_at_position(node: tree_sitter::Node, target: Point) -> Option<tree_sitter::Node> {
    // Enhanced boundary checking
    if !is_position_in_node_range(node, target) {
        return None;
    }

    // Find the most specific child that contains the target position
    let mut best_match = node;
    let mut best_size = node_size(node);

    // Recursively check children to find the most specific match
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Some(child_match) = find_node_at_position(child, target) {
                let child_size = node_size(child_match);
                // Prefer smaller (more specific) nodes, but prioritize meaningful nodes
                if child_size < best_size && is_meaningful_node(child_match) {
                    best_match = child_match;
                    best_size = child_size;
                }
            }
        }
    }

    Some(best_match)
}

/// Check if a position is within a node's range with better boundary handling
fn is_position_in_node_range(node: tree_sitter::Node, position: Point) -> bool {
    let start = node.start_position();
    let end = node.end_position();

    // Handle single-line nodes
    if start.row == end.row {
        return start.row == position.row
            && start.column <= position.column
            && position.column <= end.column;
    }

    // Handle multi-line nodes
    if position.row < start.row || position.row > end.row {
        return false;
    }

    if position.row == start.row {
        return position.column >= start.column;
    }

    if position.row == end.row {
        return position.column <= end.column;
    }

    // Position is on a line between start and end
    true
}

/// Calculate the "size" of a node for specificity comparison
fn node_size(node: tree_sitter::Node) -> usize {
    let start = node.start_position();
    let end = node.end_position();

    if start.row == end.row {
        end.column - start.column
    } else {
        // For multi-line nodes, use a larger value but still comparable
        (end.row - start.row) * 1000 + end.column + start.column
    }
}

/// Check if a node is meaningful for cursor positioning (not just syntax)
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

/// Enhanced position-based node finding with better context awareness
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

/// Determine the type of context where the cursor is positioned
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
                        // Check if this is the field part of obj.field
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

/// Enhanced variable finding that uses improved cursor detection
pub fn find_variable_at_position_enhanced(
    tree: &Tree,
    code: &str,
    pos: Position,
) -> Option<VariableInfo> {
    // Get enhanced cursor context
    let cursor_context = find_node_at_cursor_with_context(tree, pos)?;

    // Use context to improve variable detection
    match cursor_context.context_type {
        CursorContextType::VariableDeclaration
        | CursorContextType::ParameterDeclaration
        | CursorContextType::VariableUse
        | CursorContextType::FieldAccess
        | CursorContextType::ObjectAccess => {
            // Use the standard detection for these contexts
            find_variable_at_position(tree, code, pos)
        }
        CursorContextType::FunctionCall => {
            // For function calls, we might want to analyze the function instead
            // For now, fall back to standard detection
            find_variable_at_position(tree, code, pos)
        }
        _ => {
            // For other contexts, try standard detection but may return None
            find_variable_at_position(tree, code, pos)
        }
    }
}

/// Extract variable name from a node, handling different Go constructs
fn extract_variable_name(node: tree_sitter::Node, code: &str) -> Option<String> {
    match node.kind() {
        "identifier" => {
            let byte_range = node.byte_range();
            code.get(byte_range).map(|s| s.to_string())
        }
        "field_identifier" => {
            // Handle struct field access like obj.field
            let byte_range = node.byte_range();
            code.get(byte_range).map(|s| s.to_string())
        }
        "method_identifier" => {
            // Handle interface method calls
            let byte_range = node.byte_range();
            code.get(byte_range).map(|s| s.to_string())
        }
        _ => {
            // Try to find identifier child
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

/// Find the function scope that contains the target position
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

/// Collect comprehensive variable information within a scope
fn collect_variable_info(
    tree: &Tree,
    code: &str,
    var_name: &str,
    scope: Option<tree_sitter::Node>,
) -> Option<VariableInfo> {
    let search_root = scope.unwrap_or(tree.root_node());

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

    fn traverse_for_variable(
        node: tree_sitter::Node,
        code: &str,
        var_name: &str,
        var_info: &mut VariableInfo,
        found_declaration: &mut bool,
    ) {
        match node.kind() {
            // Variable declarations
            "var_spec" | "short_var_declaration" => {
                handle_variable_declaration(node, code, var_name, var_info, found_declaration);
            }
            // Function parameters
            "parameter_declaration" => {
                handle_parameter_declaration(node, code, var_name, var_info, found_declaration);
            }
            // Range statements (for loops)
            "range_clause" => {
                handle_range_clause(node, code, var_name, var_info, found_declaration);
            }
            // Type switch statements
            "type_switch_statement" => {
                handle_type_switch(node, code, var_name, var_info, found_declaration);
            }
            // Regular identifiers (uses)
            "identifier" | "field_identifier" => {
                handle_identifier_use(node, code, var_name, var_info);
            }
            // Selector expressions (struct.field, interface.method)
            "selector_expression" => {
                handle_selector_expression(node, code, var_name, var_info);
            }
            _ => {}
        }

        // Recursively traverse children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                traverse_for_variable(child, code, var_name, var_info, found_declaration);
            }
        }
    }

    traverse_for_variable(
        search_root,
        code,
        var_name,
        &mut var_info,
        &mut found_declaration,
    );

    if found_declaration || !var_info.uses.is_empty() {
        Some(var_info)
    } else {
        None
    }
}

/// Handle variable declarations (var x = ..., x := ...)
fn handle_variable_declaration(
    node: tree_sitter::Node,
    code: &str,
    var_name: &str,
    var_info: &mut VariableInfo,
    found_declaration: &mut bool,
) {
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "identifier" {
                let byte_range = child.byte_range();
                if let Some(name) = code.get(byte_range.clone()) {
                    if name == var_name {
                        var_info.declaration = node_to_range(child);
                        var_info.var_id = VarId {
                            start_byte: byte_range.start,
                            end_byte: byte_range.end,
                        };
                        *found_declaration = true;

                        // Check if it's a pointer declaration
                        if let Some(parent) = node.parent() {
                            check_pointer_context(parent, code, var_info);
                        }
                    }
                }
            }
        }
    }
}

/// Handle function parameters
fn handle_parameter_declaration(
    node: tree_sitter::Node,
    code: &str,
    var_name: &str,
    var_info: &mut VariableInfo,
    found_declaration: &mut bool,
) {
    if let Some(name_node) = node.child_by_field_name("name") {
        let byte_range = name_node.byte_range();
        if let Some(name) = code.get(byte_range.clone()) {
            if name == var_name {
                var_info.declaration = node_to_range(name_node);
                var_info.var_id = VarId {
                    start_byte: byte_range.start,
                    end_byte: byte_range.end,
                };
                *found_declaration = true;

                // Check if parameter type is a pointer
                if let Some(type_node) = node.child_by_field_name("type") {
                    if type_node.kind() == "pointer_type" {
                        var_info.is_pointer = true;
                    }
                }
            }
        }
    }
}

/// Handle range clauses in for loops
fn handle_range_clause(
    node: tree_sitter::Node,
    code: &str,
    var_name: &str,
    var_info: &mut VariableInfo,
    found_declaration: &mut bool,
) {
    // Handle: for i, v := range slice
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "identifier" {
                let byte_range = child.byte_range();
                if let Some(name) = code.get(byte_range.clone()) {
                    if name == var_name {
                        var_info.declaration = node_to_range(child);
                        var_info.var_id = VarId {
                            start_byte: byte_range.start,
                            end_byte: byte_range.end,
                        };
                        *found_declaration = true;
                    }
                }
            }
        }
    }
}

/// Handle type switch statements
fn handle_type_switch(
    node: tree_sitter::Node,
    code: &str,
    var_name: &str,
    var_info: &mut VariableInfo,
    found_declaration: &mut bool,
) {
    // Handle: switch v := x.(type)
    if let Some(assign_node) = node.child_by_field_name("initializer") {
        handle_variable_declaration(assign_node, code, var_name, var_info, found_declaration);
    }
}

/// Handle identifier uses
fn handle_identifier_use(
    node: tree_sitter::Node,
    code: &str,
    var_name: &str,
    var_info: &mut VariableInfo,
) {
    let byte_range = node.byte_range();
    if let Some(name) = code.get(byte_range) {
        if name == var_name {
            let use_range = node_to_range(node);

            // Skip if this is the declaration itself
            if use_range == var_info.declaration {
                return;
            }

            // Skip if already recorded
            if var_info.uses.contains(&use_range) {
                return;
            }

            // Check context to determine if it's a pointer operation
            if let Some(parent) = node.parent() {
                check_pointer_context(parent, code, var_info);

                // Skip declarations in parent context
                if matches!(
                    parent.kind(),
                    "var_spec" | "short_var_declaration" | "parameter_declaration"
                ) {
                    return;
                }
            }

            var_info.uses.push(use_range);
        }
    }
}

/// Handle selector expressions (obj.field, interface.method)
fn handle_selector_expression(
    node: tree_sitter::Node,
    code: &str,
    var_name: &str,
    var_info: &mut VariableInfo,
) {
    // Check operand (left side of dot)
    if let Some(operand) = node.child_by_field_name("operand") {
        if operand.kind() == "identifier" {
            let byte_range = operand.byte_range();
            if let Some(name) = code.get(byte_range) {
                if name == var_name {
                    let use_range = node_to_range(operand);
                    if !var_info.uses.contains(&use_range) && use_range != var_info.declaration {
                        var_info.uses.push(use_range);
                    }
                }
            }
        }
    }

    // Check field (right side of dot) - for cases where we're looking for the field name
    if let Some(field) = node.child_by_field_name("field") {
        let byte_range = field.byte_range();
        if let Some(name) = code.get(byte_range) {
            if name == var_name {
                let use_range = node_to_range(field);
                if !var_info.uses.contains(&use_range) && use_range != var_info.declaration {
                    var_info.uses.push(use_range);
                }
            }
        }
    }
}

/// Check if the context indicates pointer operations
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
        _ => {
            // Check parent recursively
            if let Some(parent) = node.parent() {
                check_pointer_context(parent, code, var_info);
            }
        }
    }
}

/// Check if a variable usage is a reassignment (x = value or x := value after initial declaration)
pub fn is_variable_reassignment(tree: &Tree, var_name: &str, use_range: Range, code: &str) -> bool {
    let target_point = Point {
        row: use_range.start.line as usize,
        column: use_range.start.character as usize,
    };

    if let Some(node) = find_node_at_position(tree.root_node(), target_point) {
        if let Some(parent) = node.parent() {
            match parent.kind() {
                "assignment_statement" => {
                    // For assignment statements like: x = value
                    // Check if we can find the variable name in the left side
                    if let Some(left) = parent.child_by_field_name("left") {
                        // Check if the left side contains our variable
                        if contains_variable_name(left, var_name, code) {
                            return true;
                        }
                    }
                }
                "short_var_declaration" => {
                    // For := declarations, check if this is a redeclaration
                    // In Go, x := can be reassignment if x already exists in scope
                    if let Some(left) = parent.child_by_field_name("left") {
                        if contains_variable_name(left, var_name, code) {
                            // This is more complex - for now return false (conservative)
                            // In a complete implementation, we'd check if var already exists
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

/// Check if a node (like expression_list) contains a variable with the given name
fn contains_variable_name(node: tree_sitter::Node, var_name: &str, code: &str) -> bool {
    match node.kind() {
        "identifier" => {
            let node_text = tree_sitter_text(node, code);
            node_text == var_name
        }
        "expression_list" | "identifier_list" => {
            // Search through children for identifiers
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
            // For other node types, recursively search children
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

/// Helper function to extract text from a tree-sitter node
fn tree_sitter_text(node: tree_sitter::Node, code: &str) -> String {
    text(code, node).to_string()
}

/// Check if this is the initial declaration of the variable
#[allow(dead_code)]
fn is_initial_declaration(_tree: &Tree, _var_name: &str, _current_range: Range) -> bool {
    // This is a simplified implementation
    // In a complete implementation, we would analyze the AST structure to determine
    // if this is truly the initial declaration vs a reassignment
    true // Conservative default - assume it's initial declaration
}

/// Check if a variable is captured in a closure or goroutine
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

    // Find the usage node
    if let Some(use_node) = find_node_at_position(tree.root_node(), target_point) {
        // Find the declaration node
        if let Some(decl_node) = find_node_at_position(tree.root_node(), decl_point) {
            // Check if usage is inside a different scope than declaration
            return is_captured_in_different_scope(use_node, decl_node, var_name);
        }
    }
    false
}

/// Enhanced check for variable capture in different scopes
fn is_captured_in_different_scope(
    use_node: tree_sitter::Node,
    decl_node: tree_sitter::Node,
    _var_name: &str,
) -> bool {
    // Find the function/method that contains the declaration
    let decl_function = find_enclosing_function(decl_node);

    // Find any closure or goroutine that contains the usage
    let use_closure = find_enclosing_closure_or_goroutine(use_node);
    let use_function = find_enclosing_function(use_node);

    match (use_closure, decl_function, use_function) {
        (Some(_), Some(decl_func), Some(use_func)) => {
            // Variable is used in a closure/goroutine
            // Check if it's the same function scope
            if decl_func == use_func {
                // Same function, variable is captured from outer scope
                true
            } else {
                // Different functions - this would be parameter passing or global access
                false
            }
        }
        (Some(_), Some(_), None) => {
            // Usage in closure, declaration in function, but usage not in any function
            // This shouldn't happen in well-formed Go code
            false
        }
        (Some(_), None, _) => {
            // Usage in closure, declaration not in function (global?)
            // Consider this as capture
            true
        }
        (None, _, _) => {
            // Usage not in closure - not captured
            false
        }
    }
}

/// Find the enclosing function (function_declaration or method_declaration)
fn find_enclosing_function(node: tree_sitter::Node) -> Option<tree_sitter::Node> {
    let mut current = Some(node);

    while let Some(node) = current {
        match node.kind() {
            "function_declaration" | "method_declaration" => {
                return Some(node);
            }
            _ => {
                current = node.parent();
            }
        }
    }
    None
}

/// Check if two nodes are in different closure/goroutine scopes
#[allow(dead_code)]
fn is_in_different_closure_scope(
    use_node: tree_sitter::Node,
    decl_node: tree_sitter::Node,
) -> bool {
    let use_closure = find_enclosing_closure_or_goroutine(use_node);
    let decl_closure = find_enclosing_closure_or_goroutine(decl_node);

    match (use_closure, decl_closure) {
        (Some(use_closure_node), Some(decl_closure_node)) => {
            // Different closures
            use_closure_node != decl_closure_node
        }
        (Some(_), None) => {
            // Use is in closure, declaration is not
            true
        }
        (None, Some(_)) => {
            // Use is not in closure, declaration is - shouldn't happen normally
            false
        }
        (None, None) => {
            // Neither in closure
            false
        }
    }
}

/// Find the enclosing function literal or go statement
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
                // Don't go past function boundaries - this would be a different scope
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

/// Find if a position is within any goroutine context
fn find_goroutine_context(
    node: tree_sitter::Node,
    target_point: Point,
) -> Option<tree_sitter::Node> {
    // Check if target is within this node's range
    if node.start_position() > target_point || target_point > node.end_position() {
        return None;
    }

    match node.kind() {
        "go_statement" => {
            // Direct go statement: go func() {}
            if node.start_position() <= target_point && target_point <= node.end_position() {
                return Some(node);
            }
        }
        "function_literal" => {
            // Check if this function literal is part of a go statement
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
            // Check for go statement calling a function: go myFunc()
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

    // Recursively check children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if let Some(goroutine_node) = find_goroutine_context(child, target_point) {
                return Some(goroutine_node);
            }
        }
    }

    None
}

/// Enhanced function to detect different types of goroutine patterns
#[allow(dead_code)]
pub fn analyze_goroutine_usage(tree: &Tree, var_name: &str, code: &str) -> Vec<GoroutineUsage> {
    let mut usages = Vec::new();

    fn traverse_goroutines(
        node: tree_sitter::Node,
        var_name: &str,
        code: &str,
        usages: &mut Vec<GoroutineUsage>,
    ) {
        if node.kind() == "go_statement" {
            // Found a goroutine, check for variable usage within it
            let goroutine_usage = analyze_variable_in_goroutine(node, var_name, code);
            if let Some(usage) = goroutine_usage {
                usages.push(usage);
            }
        }

        // Recursively check children
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                traverse_goroutines(child, var_name, code, usages);
            }
        }
    }

    traverse_goroutines(tree.root_node(), var_name, code, &mut usages);
    usages
}

/// Analyze how a variable is used within a specific goroutine
#[allow(dead_code)]
fn analyze_variable_in_goroutine(
    goroutine_node: tree_sitter::Node,
    var_name: &str,
    code: &str,
) -> Option<GoroutineUsage> {
    let mut usage = GoroutineUsage {
        goroutine_range: node_to_range(goroutine_node),
        variable_accesses: Vec::new(),
        goroutine_type: classify_goroutine_type(goroutine_node, code),
        potential_race_level: RaceSeverity::Medium,
    };

    fn find_variable_accesses(
        node: tree_sitter::Node,
        var_name: &str,
        code: &str,
        accesses: &mut Vec<VariableAccess>,
    ) {
        if node.kind() == "identifier" {
            let byte_range = node.byte_range();
            if let Some(name) = code.get(byte_range) {
                if name == var_name {
                    let access_type = determine_access_type(node, code);
                    accesses.push(VariableAccess {
                        range: node_to_range(node),
                        access_type,
                        context: get_access_context(node, code),
                    });
                }
            }
        }

        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                find_variable_accesses(child, var_name, code, accesses);
            }
        }
    }

    find_variable_accesses(goroutine_node, var_name, code, &mut usage.variable_accesses);

    if !usage.variable_accesses.is_empty() {
        // Determine race level based on access patterns
        usage.potential_race_level = calculate_race_severity(&usage, code);
        Some(usage)
    } else {
        None
    }
}

/// Classify the type of goroutine (anonymous function, function call, etc.)
#[allow(dead_code)]
fn classify_goroutine_type(goroutine_node: tree_sitter::Node, _code: &str) -> GoroutineType {
    // Look for the expression being executed in the go statement
    for i in 0..goroutine_node.child_count() {
        if let Some(child) = goroutine_node.child(i) {
            match child.kind() {
                "function_literal" => return GoroutineType::AnonymousFunction,
                "call_expression" => {
                    // Check if it's a method call or regular function call
                    if let Some(func_node) = child.child_by_field_name("function") {
                        if func_node.kind() == "selector_expression" {
                            return GoroutineType::MethodCall;
                        } else {
                            return GoroutineType::FunctionCall;
                        }
                    }
                }
                "identifier" => return GoroutineType::FunctionCall,
                _ => {}
            }
        }
    }
    GoroutineType::Unknown
}

/// Determine the type of variable access (read, write, address-of, etc.)
#[allow(dead_code)]
fn determine_access_type(node: tree_sitter::Node, code: &str) -> VariableAccessType {
    if let Some(parent) = node.parent() {
        match parent.kind() {
            "assignment_statement" => {
                // Check if this identifier is on the left side (write) or right side (read)
                if let Some(left) = parent.child_by_field_name("left") {
                    if node_contains_position(left, node.start_position()) {
                        return VariableAccessType::Write;
                    }
                }
                VariableAccessType::Read
            }
            "unary_expression" => {
                // Check for address-of (&var) or dereference (*var)
                if let Some(operator) = parent.child_by_field_name("operator") {
                    let op_text = text(code, operator);
                    match op_text {
                        "&" => VariableAccessType::AddressOf,
                        "*" => VariableAccessType::Dereference,
                        _ => VariableAccessType::Read,
                    }
                } else {
                    VariableAccessType::Read
                }
            }
            "inc_statement" | "dec_statement" => VariableAccessType::Modify,
            "composite_literal" | "slice_expression" | "index_expression" => {
                VariableAccessType::Read
            }
            _ => VariableAccessType::Read,
        }
    } else {
        VariableAccessType::Read
    }
}

/// Get context information about the variable access
#[allow(dead_code)]
fn get_access_context(node: tree_sitter::Node, _code: &str) -> String {
    if let Some(parent) = node.parent() {
        match parent.kind() {
            "call_expression" => "function call".to_string(),
            "assignment_statement" => "assignment".to_string(),
            "if_statement" => "conditional".to_string(),
            "for_statement" => "loop".to_string(),
            "return_statement" => "return".to_string(),
            "send_statement" => "channel send".to_string(),
            _ => parent.kind().to_string(),
        }
    } else {
        "unknown".to_string()
    }
}

/// Calculate race severity based on access patterns
#[allow(dead_code)]
fn calculate_race_severity(usage: &GoroutineUsage, code: &str) -> RaceSeverity {
    let has_writes = usage.variable_accesses.iter().any(|access| {
        matches!(
            access.access_type,
            VariableAccessType::Write | VariableAccessType::Modify
        )
    });

    let has_address_taken = usage
        .variable_accesses
        .iter()
        .any(|access| matches!(access.access_type, VariableAccessType::AddressOf));

    // Check for synchronization in the goroutine
    let has_sync = has_synchronization_in_range(usage.goroutine_range, code);

    if has_writes || has_address_taken {
        if has_sync {
            RaceSeverity::Low
        } else {
            RaceSeverity::High
        }
    } else {
        // Only reads, lower severity
        if has_sync {
            RaceSeverity::Low
        } else {
            RaceSeverity::Medium
        }
    }
}

/// Helper function to check if synchronization exists in a range
#[allow(dead_code)]
fn has_synchronization_in_range(_range: Range, code: &str) -> bool {
    // This is a simplified version - in a full implementation,
    // you would parse the tree again and check for mutex/atomic operations
    code.contains("Lock") || code.contains("Unlock") || code.contains("atomic.")
}

/// Helper function to check if a node contains a position
#[allow(dead_code)]
fn node_contains_position(node: tree_sitter::Node, position: Point) -> bool {
    node.start_position() <= position && position <= node.end_position()
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
