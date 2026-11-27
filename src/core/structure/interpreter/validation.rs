use super::KaitaiInterpreter;
use super::scope::TypeScope;
use crate::core::structure::definition::*;
use crate::core::structure::expression::ExprEvaluator;
use crate::core::structure::stream::*;
use crate::core::structure::types::{FieldValue, ParseError};

impl KaitaiInterpreter {
    pub(super) fn resolve_enum_label(&self, attr: &KsyAttr, value: &FieldValue, scope: TypeScope) -> Option<String> {
        let enum_name = attr.enum_ref.as_ref()?;
        let key = value.to_i64().to_string();
        if let Some(enum_def) = scope.get_enum(enum_name).or_else(|| self.ksy.enums.get(enum_name))
            && let Some(val) = enum_def.get(&key)
        {
            return match val {
                serde_yaml::Value::String(s) => Some(s.clone()),
                serde_yaml::Value::Mapping(m) => m.get("id")?.as_str().map(|s| s.to_string()),
                _ => None,
            };
        }
        if let Some(norm_map) = self.all_enums.get(enum_name) {
            return norm_map.get(&key).cloned();
        }
        None
    }

    pub(super) fn validate_contents(&mut self, expected: &serde_yaml::Value, actual: &FieldValue, stream: &KaitaiStream) -> bool {
        let mut expected_bytes = Vec::new();
        let mut has_expected = false;
        if let Some(arr) = expected.as_sequence() {
            has_expected = true;
            for v in arr {
                if let Some(s) = v.as_str() {
                    expected_bytes.extend(s.as_bytes());
                } else if let Some(n) = v.as_i64() {
                    expected_bytes.push(n as u8);
                } else if let Some(u) = v.as_u64() {
                    expected_bytes.push(u as u8);
                }
            }
        } else if let Some(s) = expected.as_str() {
            has_expected = true;
            expected_bytes.extend(s.as_bytes());
        }
        if has_expected {
            if let FieldValue::Bytes(actual_bytes) = actual {
                if actual_bytes != &expected_bytes {
                    self.errors.borrow_mut().push(ParseError {
                        message: "contents mismatch".into(),
                        offset: stream.pos() as usize,
                    });
                    return false;
                }
            } else {
                self.errors.borrow_mut().push(ParseError {
                    message: "contents mismatch: actual value is not bytes".into(),
                    offset: stream.pos() as usize,
                });
                return false;
            }
        }
        true
    }

    pub(super) fn validate_value(&mut self, valid: &serde_yaml::Value, actual: &FieldValue, attr: &KsyAttr, scope: TypeScope, stream: &KaitaiStream) -> bool {
        let offset = stream.pos() as usize;
        let mut ok = true;
        match valid {
            serde_yaml::Value::Mapping(map) => {
                if let Some(eq_val) = map.get("eq")
                    && !self.check_val_eq(eq_val, actual, stream)
                {
                    self.errors.borrow_mut().push(ParseError {
                        message: format!("validation failed: value {:?} does not equal expected {:?}", actual, eq_val),
                        offset,
                    });
                    ok = false;
                }
                if let Some(min_val) = map.get("min")
                    && !self.check_val_min(min_val, actual, stream)
                {
                    self.errors.borrow_mut().push(ParseError {
                        message: format!("validation failed: value {:?} is less than min {:?}", actual, min_val),
                        offset,
                    });
                    ok = false;
                }
                if let Some(max_val) = map.get("max")
                    && !self.check_val_max(max_val, actual, stream)
                {
                    self.errors.borrow_mut().push(ParseError {
                        message: format!("validation failed: value {:?} is greater than max {:?}", actual, max_val),
                        offset,
                    });
                    ok = false;
                }
                if let Some(any_of) = map.get("any-of").and_then(|v| v.as_sequence()) {
                    let matched = any_of.iter().any(|expected| self.check_val_eq(expected, actual, stream));
                    if !matched {
                        self.errors.borrow_mut().push(ParseError {
                            message: format!("validation failed: value {:?} not in any-of set", actual),
                            offset,
                        });
                        ok = false;
                    }
                }
                if let Some(in_enum) = map.get("in-enum").and_then(|v| v.as_bool())
                    && in_enum
                    && self.resolve_enum_label(attr, actual, scope).is_none()
                {
                    self.errors.borrow_mut().push(ParseError {
                        message: format!("validation failed: value {:?} is not in enum {:?}", actual, attr.enum_ref),
                        offset,
                    });
                    ok = false;
                }
                if let Some(expr_val) = map.get("expr").and_then(|v| v.as_str()) {
                    let full_underscore_id = if self.id_stack.is_empty() {
                        "_".to_string()
                    } else {
                        format!("{}.{}", self.id_stack.join("."), "_")
                    };
                    self.record_field_value("_", actual);
                    self.record_field_value(&full_underscore_id, actual);
                    let ctx = self.make_eval_ctx(stream);
                    if !ExprEvaluator::eval_bool(expr_val, &ctx) {
                        self.errors.borrow_mut().push(ParseError {
                            message: format!("validation expression failed: {}", expr_val),
                            offset,
                        });
                        ok = false;
                    }
                }
            }
            serde_yaml::Value::Number(_) | serde_yaml::Value::String(_) | serde_yaml::Value::Bool(_) | serde_yaml::Value::Sequence(_)
                if !self.check_val_eq(valid, actual, stream) =>
            {
                self.errors.borrow_mut().push(ParseError {
                    message: format!("validation failed: value {:?} does not equal expected {:?}", actual, valid),
                    offset,
                });
                ok = false;
            }
            _ => {}
        }
        ok
    }

    fn check_val_eq(&self, expected: &serde_yaml::Value, actual: &FieldValue, stream: &KaitaiStream) -> bool {
        match expected {
            serde_yaml::Value::Number(n) => {
                if let FieldValue::F32(f) = actual
                    && let Some(ef) = n.as_f64()
                {
                    return (*f as f64 - ef).abs() < 1e-6;
                }
                if let FieldValue::F64(f) = actual
                    && let Some(ef) = n.as_f64()
                {
                    return (*f - ef).abs() < 1e-12;
                }
                if let Some(i) = n.as_i64() {
                    actual.to_i64() == i
                } else if let Some(u) = n.as_u64() {
                    actual.to_i64() as u64 == u
                } else {
                    false
                }
            }
            serde_yaml::Value::Bool(b) => (actual.to_i64() != 0) == *b,
            serde_yaml::Value::Sequence(seq) => {
                if let FieldValue::Bytes(actual_bytes) = actual {
                    let expected_bytes: Vec<u8> = seq
                        .iter()
                        .filter_map(|v| v.as_u64().map(|n| n as u8).or_else(|| v.as_i64().map(|n| n as u8)))
                        .collect();
                    actual_bytes == &expected_bytes
                } else {
                    false
                }
            }
            serde_yaml::Value::String(s) => {
                if (s.starts_with("0x") || s.starts_with("0X"))
                    && let Ok(u) = i64::from_str_radix(&s[2..], 16)
                {
                    return actual.to_i64() == u;
                }
                if let FieldValue::String(actual_str) = actual
                    && actual_str == s
                {
                    return true;
                }
                let ctx = self.make_eval_ctx_silent(stream);
                let eval_val = ExprEvaluator::evaluate_rich(s, &ctx);
                match (actual, &eval_val) {
                    (FieldValue::String(actual_s), crate::core::structure::expression::ExprValue::Str(exp_s)) => actual_s == exp_s,
                    (FieldValue::Bytes(actual_b), _) => eval_val.as_bytes().is_some_and(|exp_b| actual_b == &exp_b),
                    (FieldValue::F32(f), crate::core::structure::expression::ExprValue::Float(ef)) => (*f as f64 - *ef).abs() < 1e-6,
                    (FieldValue::F64(f), crate::core::structure::expression::ExprValue::Float(ef)) => (*f - *ef).abs() < 1e-12,
                    _ => actual.to_i64() == eval_val.to_i64(),
                }
            }
            _ => false,
        }
    }

    fn check_val_min(&self, min_val: &serde_yaml::Value, actual: &FieldValue, stream: &KaitaiStream) -> bool {
        if let FieldValue::F32(f) = actual
            && let Some(min_f) = min_val.as_f64()
        {
            return (*f as f64) >= min_f;
        }
        if let FieldValue::F64(f) = actual
            && let Some(min_f) = min_val.as_f64()
        {
            return *f >= min_f;
        }
        if let Some(min_n) = min_val.as_i64() {
            return actual.to_i64() >= min_n;
        }
        if let Some(s) = min_val.as_str() {
            let ctx = self.make_eval_ctx_silent(stream);
            let eval_val = ExprEvaluator::evaluate_rich(s, &ctx);
            if let (FieldValue::String(actual_s), crate::core::structure::expression::ExprValue::Str(min_s)) = (actual, &eval_val) {
                return actual_s >= min_s;
            }
            return actual.to_i64() >= eval_val.to_i64();
        }
        true
    }

    fn check_val_max(&self, max_val: &serde_yaml::Value, actual: &FieldValue, stream: &KaitaiStream) -> bool {
        if let FieldValue::F32(f) = actual
            && let Some(max_f) = max_val.as_f64()
        {
            return (*f as f64) <= max_f;
        }
        if let FieldValue::F64(f) = actual
            && let Some(max_f) = max_val.as_f64()
        {
            return *f <= max_f;
        }
        if let Some(max_n) = max_val.as_i64() {
            return actual.to_i64() <= max_n;
        }
        if let Some(s) = max_val.as_str() {
            let ctx = self.make_eval_ctx_silent(stream);
            let eval_val = ExprEvaluator::evaluate_rich(s, &ctx);
            if let (FieldValue::String(actual_s), crate::core::structure::expression::ExprValue::Str(max_s)) = (actual, &eval_val) {
                return actual_s <= max_s;
            }
            return actual.to_i64() <= eval_val.to_i64();
        }
        true
    }

    pub(super) fn resolve_size_attr(&self, attr: &KsyAttr, stream: &KaitaiStream) -> Option<usize> {
        if let Some(ref ast) = attr.compiled_size {
            let ctx = self.make_eval_ctx(stream);
            let val = ExprEvaluator::eval_ast_i64(ast, &ctx);
            Some(if val < 0 { 0 } else { val as usize })
        } else {
            self.resolve_size(&attr.size, stream)
        }
    }

    pub(super) fn resolve_pos(&self, attr: &KsyAttr, stream: &KaitaiStream) -> Option<usize> {
        if let Some(ref ast) = attr.compiled_pos {
            let ctx = self.make_eval_ctx(stream);
            let val = ExprEvaluator::eval_ast_i64(ast, &ctx);
            Some(if val < 0 { 0 } else { val as usize })
        } else {
            self.resolve_size(&attr.pos, stream)
        }
    }

    pub(super) fn resolve_size(&self, size_val: &Option<KsyValue>, stream: &KaitaiStream) -> Option<usize> {
        match size_val {
            Some(KsyValue::Int(n)) => Some(*n),
            Some(KsyValue::Expr(e)) => {
                let ctx = self.make_eval_ctx(stream);
                let val = ExprEvaluator::eval_i64(e, &ctx);
                Some(if val < 0 { 0 } else { val as usize })
            }
            None => None,
        }
    }

    pub(super) fn resolve_count(&self, expr: &str, stream: &KaitaiStream) -> usize {
        let ctx = self.make_eval_ctx(stream);
        let val = ExprEvaluator::eval_i64(expr, &ctx);
        if val < 0 { 0 } else { val as usize }
    }

    pub(super) fn resolve_count_ast(&self, ast: &crate::core::structure::expression::ExprAST, stream: &KaitaiStream) -> usize {
        let ctx = self.make_eval_ctx(stream);
        let val = ExprEvaluator::eval_ast_i64(ast, &ctx);
        if val < 0 { 0 } else { val as usize }
    }

    pub(super) fn remaining_bytes(&self, stream: &KaitaiStream) -> usize {
        (stream.size() as usize).saturating_sub(stream.pos() as usize)
    }

    pub(super) fn attr_terminator(attr: &KsyAttr) -> Option<u8> {
        match &attr.terminator {
            Some(serde_yaml::Value::Number(n)) => n.as_u64().map(|v| v as u8),
            Some(serde_yaml::Value::String(s)) => {
                if s.starts_with("0x") || s.starts_with("0X") {
                    u8::from_str_radix(&s[2..], 16).ok()
                } else if let Ok(n) = s.parse::<u8>() {
                    Some(n)
                } else {
                    s.as_bytes().first().copied()
                }
            }
            _ => None,
        }
    }

    pub(super) fn attr_pad_right(attr: &KsyAttr) -> Option<u8> {
        match &attr.pad_right {
            Some(serde_yaml::Value::Number(n)) => n.as_u64().map(|v| v as u8),
            Some(serde_yaml::Value::String(s)) => {
                if s.starts_with("0x") || s.starts_with("0X") {
                    u8::from_str_radix(&s[2..], 16).ok()
                } else if let Ok(n) = s.parse::<u8>() {
                    Some(n)
                } else {
                    s.as_bytes().first().copied()
                }
            }
            _ => None,
        }
    }
}
