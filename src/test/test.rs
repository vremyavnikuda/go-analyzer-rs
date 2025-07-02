// tests.rs
// Unit tests for synchronization and AST analysis utilities
use super::*;
use tower_lsp::lsp_types::Position;
use tree_sitter::Parser;

fn parse_go(code: &str) -> tree_sitter::Tree {
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
        assert_eq!(info.declaration.start.line, 0);
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
}
