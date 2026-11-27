use super::eval::parse_str_to_int;
use super::lexer::{Lexer, Token};
use super::*;
use std::collections::HashMap;

#[test]
fn test_lexer_non_ascii_symbols_stay_on_char_boundary() {
    for input in ["→", "1 + →", "1 → 2", "★ == 👍", "「1 + 2」", "±"] {
        let mut lexer = Lexer::new(input);
        for _ in 0..16 {
            let _ = lexer.next_token();
            assert!(lexer.input.is_char_boundary(lexer.pos));
        }
        assert_eq!(lexer.next_token(), Token::EOF);
        assert!(lexer.input.is_char_boundary(lexer.pos));
    }
}

#[test]
fn test_expr_compile_and_eval_non_ascii_does_not_panic() {
    let values = HashMap::new();
    let errors = std::cell::RefCell::new(Vec::new());
    let ctx = EvalContext {
        errors: Some(&errors),
        ..EvalContext::simple(&values, &[])
    };

    for input in ["→", "1 + →", "→ + 1", "(1 → 2)", "[1, ★]", "👍 == 0", "# 日本語コメント"] {
        let ast = ExprAST::compile(input).expect("non-empty expr compiles");
        let _ = ExprEvaluator::eval_ast_rich(&ast, &ctx);
        let _ = ExprEvaluator::evaluate_rich(input, &ctx);
    }

    assert_eq!(ExprEvaluator::eval_i64("1 + 2 * 3", &ctx), 7);
    assert_eq!(ExprEvaluator::eval_i64("1 + 2 # 日本語コメント\n+ 3", &ctx), 6);
    assert_eq!(ExprEvaluator::eval_i64("42 # 末尾コメント😀", &ctx), 42);
    assert_eq!(ExprEvaluator::eval_i64("'あいう'.length", &ctx), 3);
    assert_eq!(ExprEvaluator::eval_string("'あいう'.reverse", &ctx), "ういあ");
    assert_eq!(ExprEvaluator::eval_string("'👍a😀b'.substring(1, 3)", &ctx), "a😀");
    assert_eq!(ExprEvaluator::eval_string("\"あ\" + \"いう\"", &ctx), "あいう");
    assert!(errors.borrow().is_empty());
}

#[test]
fn test_empty_expression_reports_error_and_evaluates_to_zero() {
    let values = HashMap::new();
    let errors = std::cell::RefCell::new(Vec::new());
    let ctx = EvalContext {
        errors: Some(&errors),
        ..EvalContext::simple(&values, &[])
    };

    assert_eq!(ExprEvaluator::eval_i64("", &ctx), 0);
    assert_eq!(ExprEvaluator::eval_i64("   \t\n ", &ctx), 0);
    assert_eq!(errors.borrow().len(), 2);
    assert!(errors.borrow().iter().all(|e| e.message == "Empty expression"));
}

#[test]
fn test_i64_min_negation_and_wrapping_arithmetic() {
    let mut values = HashMap::new();
    values.insert("min_v".to_string(), i64::MIN);
    values.insert("max_v".to_string(), i64::MAX);
    let ctx = EvalContext::simple(&values, &[]);

    // Unary negation of i64::MIN wraps instead of panicking
    assert_eq!(ExprEvaluator::eval_i64("-min_v", &ctx), i64::MIN);

    // parse_str_to_int handles signed i64::MIN bounds without panicking
    assert_eq!(parse_str_to_int("-9223372036854775808", 10), i64::MIN);
    assert_eq!(parse_str_to_int("-0x8000000000000000", 16), i64::MIN);
    assert_eq!(ExprEvaluator::eval_string("'-9223372036854775808'.to_i.to_s", &ctx), i64::MIN.to_string());

    // +/-/* wrap consistently on i64 boundary values
    assert_eq!(ExprEvaluator::eval_i64("max_v + 1", &ctx), i64::MIN);
    assert_eq!(ExprEvaluator::eval_i64("min_v - 1", &ctx), i64::MAX);
    assert_eq!(ExprEvaluator::eval_i64("max_v * 2", &ctx), i64::MAX.wrapping_mul(2));
}

#[test]
fn test_unresolved_extra_and_io_identifiers_report_errors() {
    let values = HashMap::new();
    let string_values = HashMap::new();
    let byte_arrays = HashMap::new();
    let enums = HashMap::new();
    let base_path = vec!["hdr".to_string()];
    let active_io = vec!["hdr".to_string()];
    let errors = std::cell::RefCell::new(Vec::new());

    let ctx = EvalContext {
        values: &values,
        string_values: &string_values,
        byte_arrays: &byte_arrays,
        base_path: &base_path,
        active_io_prefixes: &active_io,
        stream_eof: true,
        stream_size: 99,
        stream_pos: 12,
        enums: &enums,
        errors: Some(&errors),
        instance_resolver: None,
    };

    assert_eq!(ExprEvaluator::eval_i64("extra", &ctx), 0);
    assert_eq!(ExprEvaluator::eval_i64("has_extra", &ctx), 0);
    assert_eq!(ExprEvaluator::eval_i64("sub.extra_field", &ctx), 0);
    assert_eq!(ExprEvaluator::eval_i64("missing._io.pos", &ctx), 0);
    assert_eq!(ExprEvaluator::eval_i64("_parent._parent._io.eof", &ctx), 0);

    let msgs: Vec<String> = errors.borrow().iter().map(|e| e.message.clone()).collect();
    assert_eq!(msgs.len(), 5, "Expected 5 errors, got: {msgs:?}");
    assert!(msgs.iter().any(|m| m == "Unresolved identifier: hdr.extra"));
    assert!(msgs.iter().any(|m| m == "Unresolved identifier: hdr.has_extra"));
    assert!(msgs.iter().any(|m| m == "Unresolved identifier: hdr.sub.extra_field"));
    assert!(msgs.iter().any(|m| m == "Unresolved identifier: hdr.missing._io.pos"));
    assert!(msgs.iter().any(|m| m == "Unresolved identifier: _parent._parent._io.eof"));
}

#[test]
fn test_scoped_enum_path_lookup_and_error_reporting() {
    let values = HashMap::new();
    let string_values = HashMap::new();
    let byte_arrays = HashMap::new();
    let mut enums = HashMap::new();
    let mut inner_animal = HashMap::new();
    inner_animal.insert("7".to_string(), "horse".to_string());
    enums.insert("pkg::inner::animal".to_string(), inner_animal);
    let errors = std::cell::RefCell::new(Vec::new());

    let ctx = EvalContext {
        values: &values,
        string_values: &string_values,
        byte_arrays: &byte_arrays,
        base_path: &[],
        active_io_prefixes: &[],
        stream_eof: false,
        stream_size: 0,
        stream_pos: 0,
        enums: &enums,
        errors: Some(&errors),
        instance_resolver: None,
    };

    assert_eq!(ExprEvaluator::eval_i64("pkg::inner::animal::horse", &ctx), 7);
    assert!(errors.borrow().is_empty());

    // Must NOT match bare `animal::horse` or wrong scope `other::inner::animal::horse`
    assert_eq!(ExprEvaluator::eval_i64("animal::horse", &ctx), 0);
    assert_eq!(ExprEvaluator::eval_i64("other::inner::animal::horse", &ctx), 0);
    let msgs: Vec<String> = errors.borrow().iter().map(|e| e.message.clone()).collect();
    assert_eq!(msgs.len(), 2, "Expected 2 unresolved enum errors, got: {msgs:?}");
}
