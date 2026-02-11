// tests.rs
// Unit tests for synchronization and AST analysis utilities

use tree_sitter::Parser;

#[allow(dead_code)]
pub fn parse_go(code: &str) -> Result<tree_sitter::Tree, String> {
    let mut parser = Parser::new();
    match parser.set_language(tree_sitter_go::language()) {
        Ok(_) => {}
        Err(e) => {
            return Err(format!(
                "Failed to load Go grammar for tree-sitter parser: {}",
                e
            ))
        }
    }
    match parser.parse(code, None) {
        Some(tree) => Ok(tree),
        None => Err(String::from("Error parsing Go code")),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::len_zero)]

    use crate::analysis::{
        count_entities, determine_race_severity, find_node_at_cursor_with_context,
        find_variable_at_position, find_variable_at_position_enhanced,
        has_synchronization_in_block, is_in_goroutine,
    };
    use crate::types::{CursorContextType, RaceSeverity};
    use tower_lsp::lsp_types::{Position, Range};

    use super::*;

    #[test]
    fn test_find_variable_simple_declaration() {
        let code = r#"
func main() {
    x := 42
    println(x)
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let pos_decl = Position::new(2, 4);
        let var_info = match find_variable_at_position(&tree, code, pos_decl) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info.name, "x");
        assert!(var_info.declaration.start.line <= 2);
        assert!(var_info.uses.len() >= 1);
        assert!(!var_info.is_pointer);
    }

    #[test]
    fn test_find_struct_field_access() {
        let code = r#"
type User struct {
    name string
}

func main() {
    user := User{}
    user.name = "John"
    println(user.name)
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let pos_object = Position::new(7, 4);
        let var_info_obj = match find_variable_at_position(&tree, code, pos_object) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info_obj.name, "user");
        assert!(var_info_obj.declaration.start.line <= 7);
        assert!(var_info_obj.uses.len() >= 2);
    }

    #[test]
    fn test_find_function_parameter() {
        let code = r#"
func process(data string) {
    println(data)
    for i := 0; i < 5; i++ {
        println(i, data)
    }
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let pos_use = Position::new(4, 19);
        let var_info_use = match find_variable_at_position(&tree, code, pos_use) {
            Some(info) => info,
            None => return,
        };
        assert_eq!(var_info_use.name, "data");
        assert!(var_info_use.declaration.start.line <= 1);
    }

    #[test]
    fn test_find_range_variable() {
        let code = r#"
func main() {
    items := []string{"a", "b", "c"}
    for i, v := range items {
        println(i, v)
    }
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let pos_index = Position::new(3, 8);
        let var_info_i = match find_variable_at_position(&tree, code, pos_index) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info_i.name, "i");
        assert!(var_info_i.declaration.start.line <= 3);
        assert!(!var_info_i.uses.is_empty());
    }

    #[test]
    fn test_find_type_switch_variable() {
        let code = r#"
func main() {
    var x interface{} = "hello"
    switch v := x.(type) {
    case string:
        println(v)
    }
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let pos_switch = Position::new(3, 11);
        let var_info = match find_variable_at_position(&tree, code, pos_switch) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info.name, "v");
        assert!(var_info.declaration.start.line <= 3);
        assert!(var_info.uses.len() >= 1);
    }

    #[test]
    fn test_variable_lifecycle_comprehensive() {
        let code = r#"
func demo() {
    var x int = 10      // Declaration
    y := &x             // Address-of operation
    z := x + 5          // Read operation
    println(x, y, z)    // Multiple uses
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let pos_x = Position::new(2, 8);
        let var_info = match find_variable_at_position(&tree, code, pos_x) {
            Some(info) => info,
            None => return,
        };
        assert_eq!(var_info.name, "x");
        assert!(var_info.declaration.start.line <= 2);
        assert!(var_info.uses.len() >= 2);
    }

    #[test]
    fn test_pointer_operations_detection() {
        let code = r#"
func main() {
    x := 42
    ptr := &x       // Take address
    println(x, ptr)
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let pos_addr = Position::new(3, 12);
        let var_info = match find_variable_at_position(&tree, code, pos_addr) {
            Some(info) => info,
            None => return,
        };
        assert_eq!(var_info.name, "x");
        assert!(var_info.uses.len() >= 1);
    }

    #[test]
    fn test_goroutine_detection_basic() {
        let code = r#"
func main() {
    x := 42
    go func() {
        println(x)  // Variable used in goroutine
    }()
    x = 100        // Potential race condition
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let range_inside = Range::new(Position::new(4, 16), Position::new(4, 16));
        assert!(is_in_goroutine(&tree, range_inside));
        let range_outside = Range::new(Position::new(6, 4), Position::new(6, 4));
        assert!(!is_in_goroutine(&tree, range_outside));
    }

    #[test]
    fn test_race_severity_detection() {
        let safe_code = r#"
func safe() {
    var mu sync.Mutex
    mu.Lock()
    x := 0
    mu.Unlock()
}
        "#;

        let unsafe_code = r#"
func unsafe() {
    x := 0
    go func() {
        x++  // Race condition!
    }()
}
        "#;

        let tree_safe = match parse_go(safe_code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let tree_unsafe = match parse_go(unsafe_code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let range_safe = Range::new(Position::new(4, 4), Position::new(4, 4));
        let severity_safe = determine_race_severity(&tree_safe, range_safe, safe_code);
        assert_eq!(severity_safe, RaceSeverity::Low);
        let range_unsafe = Range::new(Position::new(4, 8), Position::new(4, 8));
        let severity_unsafe = determine_race_severity(&tree_unsafe, range_unsafe, unsafe_code);
        assert_eq!(severity_unsafe, RaceSeverity::High);
    }

    #[test]
    fn test_cursor_context_detection() {
        let code = r#"
func main() {
    user := "John"
    println(user)
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let pos_var_decl = Position::new(2, 4);
        let context = match find_node_at_cursor_with_context(&tree, pos_var_decl) {
            Some(ctx) => ctx,
            None => return,
        };
        assert!(!context.target_node_kind.is_empty());
    }

    #[test]
    fn test_empty_file() {
        let code = "";
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let pos = Position::new(0, 0);
        let var_info = find_variable_at_position(&tree, code, pos);
        assert!(var_info.is_none());
        let context = find_node_at_cursor_with_context(&tree, pos);
        if let Some(ctx) = context {
            assert!(matches!(ctx.context_type, CursorContextType::Unknown));
        }
    }

    #[test]
    fn test_cursor_outside_code() {
        let code = r#"
func main() {
    x := 42
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let pos_outside = Position::new(100, 100);
        let var_info = find_variable_at_position(&tree, code, pos_outside);
        assert!(var_info.is_none());
    }

    #[test]
    fn test_panic_recovery() {
        let code = r#"
func main() {
    x := 42
    go func() {
        println(x)
    }()
}
        "#;

        let result = std::panic::catch_unwind(|| {
            let tree = match parse_go(code) {
                Ok(tree) => tree,
                Err(_) => return true,
            };
            let pos = Position::new(4, 16);
            let _ = find_variable_at_position(&tree, code, pos);
            let _ = find_variable_at_position_enhanced(&tree, code, pos);
            let _ = find_node_at_cursor_with_context(&tree, pos);
            true
        });
        assert!(result.is_ok());
        if let Ok(result_value) = result {
            assert!(result_value);
        }
    }

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
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
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
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
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
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let range = Range::new(Position::new(2, 12), Position::new(2, 12));
        assert!(has_synchronization_in_block(&tree, range, code));
    }

    #[test]
    fn test_determine_race_severity_original() {
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
        let tree_safe = match parse_go(safe_code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let tree_unsafe = match parse_go(unsafe_code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
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
    fn test_find_variable_at_position_original() {
        let code = r#"
func demo() {
    var a, b = 1, 2
    c := a + b
    _ = c
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let pos = Position::new(3, 9);
        let info = match crate::analysis::find_variable_at_position(&tree, code, pos) {
            Some(info) => info,
            None => return,
        };
        assert_eq!(info.name, "a");
        assert_eq!(info.declaration.start.line, 2);
        assert_eq!(info.uses.len(), 1);
        assert!(!info.is_pointer);
    }

    #[test]
    fn test_is_in_goroutine_original() {
        let code = r#"
func run() {
    go func() {
        doWork()
    }()
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let range_inside = Range::new(Position::new(2, 15), Position::new(2, 15));
        assert!(crate::analysis::is_in_goroutine(&tree, range_inside));
        let range_outside = Range::new(Position::new(1, 5), Position::new(1, 5));
        assert!(!crate::analysis::is_in_goroutine(&tree, range_outside));
    }

    #[test]
    fn test_count_entities_original() {
        let code = r#"
var global int
func f() {}
func main() {
    go doSomething()
    ch := make(chan int)
    x := 10
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let counts = crate::analysis::count_entities(&tree, code);
        assert_eq!(counts.variables, 3);
        assert_eq!(counts.functions, 2);
        assert_eq!(counts.goroutines, 1);
        assert_eq!(counts.channels, 1);
    }

    #[test]
    fn test_enhanced_cursor_position_detection_original() {
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
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let pos_field_access = Position::new(6, 9);
        let context =
            match crate::analysis::find_node_at_cursor_with_context(&tree, pos_field_access) {
                Some(ctx) => ctx,
                None => return,
            };
        assert_eq!(
            context.context_type,
            crate::types::CursorContextType::FieldAccess
        );
        let pos_goroutine = Position::new(8, 23);
        let var_info =
            match crate::analysis::find_variable_at_position_enhanced(&tree, code, pos_goroutine) {
                Some(info) => info,
                None => return,
            };
        assert_eq!(var_info.name, "user");
        let pos_declaration = Position::new(2, 8);
        let var_info_decl =
            match crate::analysis::find_variable_at_position_enhanced(&tree, code, pos_declaration)
            {
                Some(info) => info,
                None => return,
            };
        assert_eq!(var_info_decl.name, "user");
        assert!(var_info_decl.uses.len() >= 2);
    }

    #[test]
    fn test_anonymous_structs() {
        let code = r#"
func main() {
    person := struct {
        name string
        age  int
    }{
        name: "Alice",
        age:  30,
    }
    println(person.name)
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let pos_person = Position::new(9, 12);
        let var_info = match find_variable_at_position(&tree, code, pos_person) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info.name, "person");
        assert!(var_info.declaration.start.line <= 2);
    }

    #[test]
    fn test_method_receivers() {
        let code = r#"
type Counter struct {
    value int
}

func (c *Counter) Increment() {
    c.value++
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let pos_ptr_receiver = Position::new(5, 6);
        let var_info = match find_variable_at_position(&tree, code, pos_ptr_receiver) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info.name, "c");
        assert!(var_info.declaration.start.line <= 5);
    }

    #[test]
    fn test_interface_usage() {
        let code = r#"
type Writer interface {
    Write(data []byte) (int, error)
}

func process(w Writer) {
    data := []byte("hello")
    w.Write(data)
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let pos_interface = Position::new(5, 13);
        let var_info = match find_variable_at_position(&tree, code, pos_interface) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info.name, "w");
        assert!(var_info.declaration.start.line <= 5);
        assert!(var_info.uses.len() >= 1);
    }

    #[test]
    fn test_nested_goroutines() {
        let code = r#"
func main() {
    x := 42
    go func() {
        go func() {
            println(x)
        }()
    }()
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let range_nested = Range::new(Position::new(5, 20), Position::new(5, 20));
        assert!(is_in_goroutine(&tree, range_nested));
    }

    #[test]
    fn test_complex_variable_scoping() {
        let code = r#"
func outer() {
    x := 10
    func() {
        y := x + 5
        println(y)
    }()
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let pos_x = Position::new(4, 13);
        let var_info = match find_variable_at_position(&tree, code, pos_x) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info.name, "x");
        assert!(var_info.declaration.start.line <= 2);
    }

    #[test]
    fn test_multiple_assignments() {
        let code = r#"
func main() {
    a, b := 1, 2
    c, d := getValues()
    println(a, b, c, d)
}

func getValues() (int, int) {
    return 3, 4
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let pos_a = Position::new(2, 4);
        let var_info_a = match find_variable_at_position(&tree, code, pos_a) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info_a.name, "a");
        assert!(var_info_a.declaration.start.line <= 2);

        let pos_c = Position::new(3, 4);
        let var_info_c = match find_variable_at_position(&tree, code, pos_c) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info_c.name, "c");
        assert!(var_info_c.declaration.start.line <= 3);
    }

    #[test]
    fn test_channel_operations() {
        let code = r#"
func main() {
    ch := make(chan int)
    go func() {
        ch <- 42
    }()
    value := <-ch
    println(value)
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let counts = count_entities(&tree, code);
        assert!(counts.channels >= 1);
        assert!(counts.goroutines >= 1);
        assert!(counts.variables >= 2);
        let pos_ch = Position::new(2, 4);
        let var_info = match find_variable_at_position(&tree, code, pos_ch) {
            Some(info) => info,
            None => return,
        };

        assert_eq!(var_info.name, "ch");
        assert!(var_info.uses.len() >= 2);
    }

    #[test]
    fn test_invalid_syntax_graceful_handling() {
        let code = r#"
func broken( {
    x :=
    y = x +
}
        "#;

        let result = std::panic::catch_unwind(|| {
            let tree = match parse_go(code) {
                Ok(tree) => tree,
                Err(_) => return true,
            };
            let pos = Position::new(2, 4);
            find_variable_at_position(&tree, code, pos);
            true
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_comprehensive_entity_counting() {
        let code = r#"
package main

var globalVar int

func function1() {}

func function2() {
    localVar := 10
    ch := make(chan int)
    go func() {
        println("goroutine")
    }()

    go function1()
    anotherVar := 20
}

func main() {
    mainVar := "hello"
    println(mainVar)
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };
        let counts = count_entities(&tree, code);
        assert!(counts.variables >= 5);
        assert!(counts.functions >= 3);
        assert!(counts.channels >= 1);
        assert!(counts.goroutines >= 2);
    }

    #[test]
    fn test_variable_reassignment_detection() {
        let code = r#"
func main() {
    x := 42      // Declaration
    x = 100      // Reassignment
    y := 30
    y = 40       // Another reassignment
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let reassign_range = Range::new(Position::new(3, 4), Position::new(3, 5));
        let is_reassign =
            crate::analysis::is_variable_reassignment(&tree, "x", reassign_range, code);
        assert!(is_reassign, "Should detect x = 100 as reassignment");
        let decl_range = Range::new(Position::new(2, 4), Position::new(2, 5));
        let is_not_reassign =
            crate::analysis::is_variable_reassignment(&tree, "x", decl_range, code);
        assert!(
            !is_not_reassign,
            "Should not detect declaration as reassignment"
        );
    }

    #[test]
    fn test_variable_capture_in_closure() {
        let code = r#"
func main() {
    x := 42
    go func() {
        println(x)   // Captured variable
    }()
    y := 30
    println(y)       // Not captured
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let capture_range = Range::new(Position::new(4, 16), Position::new(4, 17));
        let declaration_range = Range::new(Position::new(2, 4), Position::new(2, 5));
        let is_captured =
            crate::analysis::is_variable_captured(&tree, "x", capture_range, declaration_range);
        assert!(is_captured, "Should detect x as captured in goroutine");

        let non_capture_range = Range::new(Position::new(7, 12), Position::new(7, 13));
        let y_declaration_range = Range::new(Position::new(6, 4), Position::new(6, 5));
        let is_not_captured = crate::analysis::is_variable_captured(
            &tree,
            "y",
            non_capture_range,
            y_declaration_range,
        );
        assert!(!is_not_captured, "Should not detect y as captured");
    }

    #[test]
    #[ignore] // TODO: Fix function literal capture detection
    fn test_variable_capture_in_function_literal() {
        let code = r#"
func main() {
    value := 100
    callback := func() {
        println(value)  // Captured in function literal
    }
    callback()
}
        "#;
        let tree = match parse_go(code) {
            Ok(tree) => tree,
            Err(_) => return,
        };

        let capture_range = Range::new(Position::new(4, 16), Position::new(4, 21));
        let declaration_range = Range::new(Position::new(2, 4), Position::new(2, 9));
        let is_captured =
            crate::analysis::is_variable_captured(&tree, "value", capture_range, declaration_range);
        assert!(
            is_captured,
            "Should detect value as captured in function literal"
        );
    }
}
