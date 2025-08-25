// tests.rs
// Unit tests for synchronization and AST analysis utilities
use tree_sitter::Parser;

pub fn parse_go(code: &str) -> tree_sitter::Tree {
    let mut parser = Parser::new();
    parser
        .set_language(tree_sitter_go::language())
        .expect("Error loading Go grammar");
    parser.parse(code, None).expect("Failed to parse code")
}

#[cfg(test)]
mod tests {
    use crate::analysis::has_synchronization_in_block;

    use super::*;
    use tower_lsp::lsp_types::{Position, Range};

    #[test]
    fn test_has_synchronization_in_block_mutex() {
        let code = r#"
func example() {
    var x int
    {
        mutex.Lock()
        x = 1
        mutex.Unlock()
    }
}
        "#;
        let tree = parse_go(code);
        // Position inside the inner block
        let range = Range::new(Position::new(2, 12), Position::new(2, 12));
        assert!(has_synchronization_in_block(&tree, range, code));
    }

    #[test]
    fn test_has_synchronization_in_block_none() {
        let code = r#"
func example() {
    {
        x = 2
    }
}
        "#;
        let tree = parse_go(code);
        let range = Range::new(Position::new(2, 16), Position::new(2, 16));
        assert!(!has_synchronization_in_block(&tree, range, code));
    }

    #[test]
    fn test_has_synchronization_in_block_atomic() {
        let code = r#"
func inc() {
    atomic.AddInt32(&counter, 1)
}
        "#;
        let tree = parse_go(code);
        let range = Range::new(Position::new(2, 12), Position::new(2, 12));
        assert!(has_synchronization_in_block(&tree, range, code));
    }

    #[test]
    fn test_determine_race_severity() {
        let safe_code = r#"
func safe() {
    m.Lock();
    x++;
    m.Unlock();
}
        "#;
        let unsafe_code = r#"
func unsafe() {
    x = x + 1
}
        "#;
        let tree_safe = parse_go(safe_code);
        let tree_unsafe = parse_go(unsafe_code);
        let range_safe = Range::new(Position::new(2, 10), Position::new(2, 10));
        let range_unsafe = Range::new(Position::new(2, 5), Position::new(2, 5));

        assert_eq!(
            crate::analysis::determine_race_severity(&tree_safe, range_safe, safe_code),
            crate::types::RaceSeverity::Low
        );
        assert_eq!(
            crate::analysis::determine_race_severity(&tree_unsafe, range_unsafe, unsafe_code),
            crate::types::RaceSeverity::High
        );
    }

    #[test]
    fn test_find_variable_at_position() {
        let code = r#"
func demo() {
    var a, b = 1, 2
    c := a + b
    _ = c
}
        "#;
        let tree = parse_go(code);
        use crate::util::node_to_range;
        let root = tree.root_node();
        let mut _cursor = root.walk();
        fn print_identifiers(node: tree_sitter::Node, code: &str) {
            if node.kind() == "identifier" {
                eprintln!(
                    "IDENT: '{}' at {:?}",
                    &code[node.byte_range()],
                    node_to_range(node)
                );
            }
            let mut cursor = node.walk();
            if cursor.goto_first_child() {
                loop {
                    print_identifiers(cursor.node(), code);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
            }
        }
        print_identifiers(root, code);
        // Position at 'a' in expression c := a + b
        let pos = Position::new(3, 9);
        let info = crate::analysis::find_variable_at_position(&tree, code, pos)
            .expect("Variable should be found");
        eprintln!("USES: {:?}", info.uses);
        assert_eq!(info.name, "a");
        assert_eq!(info.declaration.start.line, 2); // Variable 'a' is declared on line 2 (var a, b = 1, 2)
        assert_eq!(info.uses.len(), 1);
        assert!(!info.is_pointer);
    }

    #[test]
    fn test_is_in_goroutine() {
        let code = r#"
func run() {
    go func() {
        doWork()
    }()
}
        "#;
        let tree = parse_go(code);
        // Position inside the goroutine literal
        let range_inside = Range::new(Position::new(2, 15), Position::new(2, 15));
        assert!(crate::analysis::is_in_goroutine(&tree, range_inside));
        // Position outside
        let range_outside = Range::new(Position::new(1, 5), Position::new(1, 5));
        assert!(!crate::analysis::is_in_goroutine(&tree, range_outside));
    }

    #[test]
    fn test_count_entities() {
        let code = r#"
var global int
func f() {}
func main() {
    go doSomething()
    ch := make(chan int)
    x := 10
}
        "#;
        let tree = parse_go(code);
        let counts = crate::analysis::count_entities(&tree, code);
        assert_eq!(counts.variables, 3);
        assert_eq!(counts.functions, 2);
        assert_eq!(counts.goroutines, 1);
        assert_eq!(counts.channels, 1);
    }

    #[test]
    fn test_enhanced_cursor_position_detection() {
        let code = r#"
func example() {
    var user struct {
        name string
        age  int
    }
    user.name = "John"
    go func() {
        fmt.Println(user.age)
    }()
}
        "#;
        let tree = parse_go(code);

        // Test cursor on struct field access (user.name)
        let pos_field_access = Position::new(6, 9); // Position on "name" in "user.name"
        let context = crate::analysis::find_node_at_cursor_with_context(&tree, pos_field_access);
        assert!(context.is_some());
        let context = context.unwrap();
        assert_eq!(
            context.context_type,
            crate::types::CursorContextType::FieldAccess
        );

        // Test cursor on variable in goroutine
        let pos_goroutine = Position::new(8, 23); // Position on "user" in goroutine
        let var_info =
            crate::analysis::find_variable_at_position_enhanced(&tree, code, pos_goroutine);
        assert!(var_info.is_some());
        let var_info = var_info.unwrap();
        assert_eq!(var_info.name, "user");

        // Test enhanced detection on struct declaration
        let pos_declaration = Position::new(2, 8); // Position on "user" in declaration
        let var_info_decl =
            crate::analysis::find_variable_at_position_enhanced(&tree, code, pos_declaration);
        assert!(var_info_decl.is_some());
        let var_info_decl = var_info_decl.unwrap();
        assert_eq!(var_info_decl.name, "user");
        assert!(var_info_decl.uses.len() >= 2); // Should find multiple uses
    }
}
