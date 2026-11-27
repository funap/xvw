use super::progress::ArrayRunningMinMax;
use super::{KaitaiInterpreter, MAX_RECURSION, normalize_enum};
use crate::core::structure::definition::*;
use crate::core::structure::expression::{EvalContext, ExprEvaluator};
use crate::core::structure::stream::*;
use crate::core::structure::types::{FieldValue, ParseError, ParsedField};
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub struct TypeScope<'a> {
    parent: Option<&'a TypeScope<'a>>,
    types: &'a HashMap<String, KsyType>,
    enums: &'a HashMap<String, HashMap<String, serde_yaml::Value>>,
}

impl<'a> TypeScope<'a> {
    pub fn root(types: &'a HashMap<String, KsyType>, enums: &'a HashMap<String, HashMap<String, serde_yaml::Value>>) -> Self {
        Self { parent: None, types, enums }
    }

    pub fn child(&'a self, types: &'a HashMap<String, KsyType>, enums: &'a HashMap<String, HashMap<String, serde_yaml::Value>>) -> Self {
        Self {
            parent: Some(self),
            types,
            enums,
        }
    }

    pub fn get_type(&self, name: &str) -> Option<&'a KsyType> {
        let mut scope = Some(self);
        while let Some(current) = scope {
            if let Some(type_def) = current.types.get(name) {
                return Some(type_def);
            }
            scope = current.parent;
        }
        None
    }

    pub fn get_enum(&self, name: &str) -> Option<&'a HashMap<String, serde_yaml::Value>> {
        let mut scope = Some(self);
        while let Some(current) = scope {
            if let Some(enum_def) = current.enums.get(name) {
                return Some(enum_def);
            }
            scope = current.parent;
        }
        None
    }
}

#[derive(Default)]
pub(super) struct ContextUndoLog {
    ints: Vec<(String, Option<i64>)>,
    strings: Vec<(String, Option<String>)>,
    bytes: Vec<(String, Option<Vec<u8>>)>,
    enums: Vec<(String, Option<HashMap<String, String>>)>,
    seen_ints: std::collections::HashSet<String>,
    seen_strings: std::collections::HashSet<String>,
    seen_bytes: std::collections::HashSet<String>,
    seen_enums: std::collections::HashSet<String>,
}

pub(super) struct SavedRootIoState {
    stream_size: usize,
    active_io_prefixes: Vec<String>,
    pos: Option<i64>,
    size: Option<i64>,
    eof: Option<i64>,
    eof_bool: Option<i64>,
}

impl KaitaiInterpreter {
    pub(super) fn make_eval_ctx<'b>(&'b self, stream: &KaitaiStream) -> EvalContext<'b> {
        EvalContext {
            values: &self.context,
            string_values: &self.string_context,
            byte_arrays: &self.byte_arrays,
            base_path: &self.id_stack,
            active_io_prefixes: &self.active_io_prefixes,
            stream_eof: stream.is_eof(),
            stream_size: self.stream_size,
            stream_pos: stream.pos() as usize,
            enums: &self.all_enums,
            errors: Some(&self.errors),
            instance_resolver: None,
        }
    }

    pub(super) fn make_eval_ctx_silent<'b>(&'b self, stream: &KaitaiStream) -> EvalContext<'b> {
        EvalContext {
            values: &self.context,
            string_values: &self.string_context,
            byte_arrays: &self.byte_arrays,
            base_path: &self.id_stack,
            active_io_prefixes: &self.active_io_prefixes,
            stream_eof: stream.is_eof(),
            stream_size: self.stream_size,
            stream_pos: stream.pos() as usize,
            enums: &self.all_enums,
            errors: None,
            instance_resolver: None,
        }
    }

    pub(super) fn check_recursion_limit(&self, stream: &KaitaiStream) -> bool {
        if self.recursion_depth > MAX_RECURSION {
            self.errors.borrow_mut().push(ParseError {
                message: format!("Maximum recursion depth ({MAX_RECURSION}) exceeded"),
                offset: stream.pos() as usize,
            });
            true
        } else {
            false
        }
    }

    pub(super) fn record_io_state(&mut self, prefix: &str, stream: &KaitaiStream) {
        let pos = stream.pos() as i64;
        let size = stream.size() as i64;
        let eof = if stream.is_eof() { 1 } else { 0 };
        self.context.insert(format!("{prefix}._io.pos"), pos);
        self.context.insert(format!("{prefix}._io.size"), size);
        self.context.insert(format!("{prefix}._io.eof"), eof);
        self.context.insert(format!("__bool__:{prefix}._io.eof"), eof);
    }

    pub(super) fn record_empty_io_state(&mut self, prefix: &str) {
        self.context.insert(format!("{prefix}._io.pos"), 0);
        self.context.insert(format!("{prefix}._io.size"), 0);
        self.context.insert(format!("{prefix}._io.eof"), 1);
        self.context.insert(format!("__bool__:{prefix}._io.eof"), 1);
    }

    pub(super) fn sync_active_io_states(&mut self, stream: &KaitaiStream) {
        let prefixes = self.active_io_prefixes.clone();
        for prefix in &prefixes {
            self.record_io_state(prefix, stream);
        }
    }

    pub(super) fn save_root_io_state(&self) -> SavedRootIoState {
        SavedRootIoState {
            stream_size: self.stream_size,
            active_io_prefixes: self.active_io_prefixes.clone(),
            pos: self.context.get("_root._io.pos").copied(),
            size: self.context.get("_root._io.size").copied(),
            eof: self.context.get("_root._io.eof").copied(),
            eof_bool: self.context.get("__bool__:_root._io.eof").copied(),
        }
    }

    pub(super) fn restore_root_io_state(&mut self, saved: SavedRootIoState) {
        self.stream_size = saved.stream_size;
        self.active_io_prefixes = saved.active_io_prefixes;
        let restore_key = |ctx: &mut HashMap<String, i64>, key: &str, val: Option<i64>| match val {
            Some(v) => {
                ctx.insert(key.to_string(), v);
            }
            None => {
                ctx.remove(key);
            }
        };
        restore_key(&mut self.context, "_root._io.pos", saved.pos);
        restore_key(&mut self.context, "_root._io.size", saved.size);
        restore_key(&mut self.context, "_root._io.eof", saved.eof);
        restore_key(&mut self.context, "__bool__:_root._io.eof", saved.eof_bool);
    }

    pub(super) fn set_context_int_with_undo(&mut self, key: String, val: i64, undo: &mut ContextUndoLog) {
        if undo.seen_ints.insert(key.clone()) {
            let prev = self.context.insert(key.clone(), val);
            undo.ints.push((key, prev));
        } else {
            self.context.insert(key, val);
        }
    }

    pub(super) fn set_context_string_with_undo(&mut self, key: String, val: String, undo: &mut ContextUndoLog) {
        if undo.seen_strings.insert(key.clone()) {
            let prev = self.string_context.insert(key.clone(), val);
            undo.strings.push((key, prev));
        } else {
            self.string_context.insert(key, val);
        }
    }

    pub(super) fn set_context_bytes_with_undo(&mut self, key: String, val: Vec<u8>, undo: &mut ContextUndoLog) {
        if undo.seen_bytes.insert(key.clone()) {
            let prev = self.byte_arrays.insert(key.clone(), val);
            undo.bytes.push((key, prev));
        } else {
            self.byte_arrays.insert(key, val);
        }
    }

    pub(super) fn set_enum_with_undo(&mut self, key: String, val: HashMap<String, String>, undo: &mut ContextUndoLog) {
        if undo.seen_enums.insert(key.clone()) {
            let prev = self.all_enums.insert(key.clone(), val);
            undo.enums.push((key, prev));
        } else {
            self.all_enums.insert(key, val);
        }
    }

    pub(super) fn bind_scoped_enums_with_undo(&mut self, prefix: &str, type_def: &KsyType, undo: &mut ContextUndoLog) {
        for (ename, raw) in &type_def.enums {
            let key = if prefix.is_empty() { ename.clone() } else { format!("{prefix}::{ename}") };
            self.set_enum_with_undo(key, normalize_enum(raw), undo);
        }
        for (child_name, child_def) in &type_def.types {
            let child_prefix = if prefix.is_empty() {
                child_name.clone()
            } else {
                format!("{prefix}::{child_name}")
            };
            self.bind_scoped_enums_with_undo(&child_prefix, child_def, undo);
        }
    }

    pub(super) fn record_expr_value_with_undo(&mut self, key: &str, val: &crate::core::structure::expression::ExprValue, undo: &mut ContextUndoLog) {
        self.set_context_int_with_undo(key.to_string(), val.to_i64(), undo);
        match val {
            crate::core::structure::expression::ExprValue::Float(f) => {
                self.set_context_int_with_undo(format!("__float__:{key}"), f.to_bits() as i64, undo);
            }
            crate::core::structure::expression::ExprValue::Bool(b) => {
                self.set_context_int_with_undo(format!("__bool__:{key}"), if *b { 1 } else { 0 }, undo);
            }
            crate::core::structure::expression::ExprValue::Str(s) => {
                self.set_context_string_with_undo(key.to_string(), s.clone(), undo);
            }
            crate::core::structure::expression::ExprValue::Bytes(b) => {
                self.set_context_bytes_with_undo(key.to_string(), b.clone(), undo);
            }
            crate::core::structure::expression::ExprValue::Array(arr) => {
                if let Some(b) = val.as_bytes() {
                    self.set_context_bytes_with_undo(key.to_string(), b, undo);
                }
                let len = arr.len() as i64;
                self.set_context_int_with_undo(format!("__array_len__:{key}"), len, undo);
                self.set_context_int_with_undo(format!("{key}.size"), len, undo);
                self.set_context_int_with_undo(format!("{key}.length"), len, undo);
                for (idx, item) in arr.iter().enumerate() {
                    self.record_expr_value_with_undo(&format!("{key}[{idx}]"), item, undo);
                }
                if let Some(first) = arr.first() {
                    self.record_expr_value_with_undo(&format!("{key}.first"), first, undo);
                }
                if let Some(last) = arr.last() {
                    self.record_expr_value_with_undo(&format!("{key}.last"), last, undo);
                }
            }
            crate::core::structure::expression::ExprValue::Int(_) => {}
        }
    }

    pub(super) fn restore_scope_bindings(&mut self, undo: ContextUndoLog) {
        for (k, prev) in undo.ints {
            match prev {
                Some(v) => {
                    self.context.insert(k, v);
                }
                None => {
                    self.context.remove(&k);
                }
            }
        }
        for (k, prev) in undo.strings {
            match prev {
                Some(v) => {
                    self.string_context.insert(k, v);
                }
                None => {
                    self.string_context.remove(&k);
                }
            }
        }
        for (k, prev) in undo.bytes {
            match prev {
                Some(v) => {
                    self.byte_arrays.insert(k, v);
                }
                None => {
                    self.byte_arrays.remove(&k);
                }
            }
        }
        for (k, prev) in undo.enums {
            match prev {
                Some(v) => {
                    self.all_enums.insert(k, v);
                }
                None => {
                    self.all_enums.remove(&k);
                }
            }
        }
    }

    pub(super) fn record_expr_value(&mut self, key: &str, val: &crate::core::structure::expression::ExprValue) {
        self.context.insert(key.to_string(), val.to_i64());
        match val {
            crate::core::structure::expression::ExprValue::Float(f) => {
                self.context.insert(format!("__float__:{key}"), f.to_bits() as i64);
            }
            crate::core::structure::expression::ExprValue::Bool(b) => {
                self.context.insert(format!("__bool__:{key}"), if *b { 1 } else { 0 });
            }
            crate::core::structure::expression::ExprValue::Str(s) => {
                self.string_context.insert(key.to_string(), s.clone());
            }
            crate::core::structure::expression::ExprValue::Bytes(b) => {
                self.byte_arrays.insert(key.to_string(), b.clone());
            }
            crate::core::structure::expression::ExprValue::Array(arr) => {
                if let Some(b) = val.as_bytes() {
                    self.byte_arrays.insert(key.to_string(), b);
                }
                let len = arr.len() as i64;
                self.context.insert(format!("__array_len__:{key}"), len);
                self.context.insert(format!("{key}.size"), len);
                self.context.insert(format!("{key}.length"), len);
                for (idx, item) in arr.iter().enumerate() {
                    self.record_expr_value(&format!("{key}[{idx}]"), item);
                }
                if let Some(first) = arr.first() {
                    self.record_expr_value(&format!("{key}.first"), first);
                }
                if let Some(last) = arr.last() {
                    self.record_expr_value(&format!("{key}.last"), last);
                }
            }
            crate::core::structure::expression::ExprValue::Int(_) => {}
        }
    }

    pub(super) fn record_field_value(&mut self, key: &str, val: &FieldValue) {
        self.context.insert(key.to_string(), val.to_i64());
        match val {
            FieldValue::F32(f) => {
                self.context.insert(format!("__float__:{key}"), (*f as f64).to_bits() as i64);
            }
            FieldValue::F64(f) => {
                self.context.insert(format!("__float__:{key}"), f.to_bits() as i64);
            }
            FieldValue::Bool(b) => {
                self.context.insert(format!("__bool__:{key}"), if *b { 1 } else { 0 });
            }
            FieldValue::String(s) => {
                self.string_context.insert(key.to_string(), s.clone());
            }
            FieldValue::Bytes(b) => {
                self.byte_arrays.insert(key.to_string(), b.clone());
            }
            _ => {}
        }
    }

    pub(super) fn record_children_values(&mut self, prefix: &str, children: &[ParsedField]) {
        for child in children {
            let child_key = format!("{prefix}.{}", child.id);
            self.record_field_value(&child_key, &child.value);
            self.context.insert(format!("{child_key}._sizeof"), child.size as i64);
            self.record_children_values(&child_key, &child.children);
        }
    }

    pub(super) fn update_array_metadata(&mut self, attr: &KsyAttr, results: &[ParsedField], stats: &mut ArrayRunningMinMax) {
        let Some(ref raw_id) = attr.id else {
            return;
        };
        let mut prefixes = vec![raw_id.clone()];
        if !self.id_stack.is_empty() {
            prefixes.push(format!("{}.{}", self.id_stack.join("."), raw_id));
        }

        let len = results.len() as i64;
        if let Some(last) = results.last() {
            match (&mut *stats, &last.value) {
                (s @ ArrayRunningMinMax::Uninit, FieldValue::String(str_val)) => {
                    *s = ArrayRunningMinMax::Strings {
                        min: str_val.clone(),
                        max: str_val.clone(),
                    };
                }
                (s @ ArrayRunningMinMax::Uninit, val) => {
                    let n = val.to_i64();
                    *s = ArrayRunningMinMax::Ints { min: n, max: n };
                }
                (ArrayRunningMinMax::Strings { min, max }, FieldValue::String(str_val)) => {
                    if str_val < min {
                        *min = str_val.clone();
                    }
                    if str_val > max {
                        *max = str_val.clone();
                    }
                }
                (ArrayRunningMinMax::Strings { .. }, _) => {}
                (ArrayRunningMinMax::Ints { min, max }, val) => {
                    let n = val.to_i64();
                    if n < *min {
                        *min = n;
                    }
                    if n > *max {
                        *max = n;
                    }
                }
            }
        }

        for prefix in &prefixes {
            self.context.insert(format!("__array_len__:{prefix}"), len);
            self.context.insert(format!("{prefix}.size"), len);
            self.context.insert(format!("{prefix}.length"), len);

            if results.len() == 1
                && let Some(first) = results.first()
            {
                self.record_field_value(&format!("{prefix}.first"), &first.value);
                self.record_children_values(&format!("{prefix}.first"), &first.children);
            }
            if let Some(last) = results.last() {
                self.record_field_value(&format!("{prefix}.last"), &last.value);
                self.record_children_values(&format!("{prefix}.last"), &last.children);
            }
            match &*stats {
                ArrayRunningMinMax::Uninit => {}
                ArrayRunningMinMax::Strings { min, max } => {
                    self.string_context.insert(format!("{prefix}.min"), min.clone());
                    self.string_context.insert(format!("{prefix}.max"), max.clone());
                }
                ArrayRunningMinMax::Ints { min, max } => {
                    self.context.insert(format!("{prefix}.min"), *min);
                    self.context.insert(format!("{prefix}.max"), *max);
                }
            }
        }
    }

    pub(super) fn evaluate_value_instances_silent(&mut self, instances: &HashMap<String, KsyAttr>, stream: &KaitaiStream) {
        let passes = instances.len().clamp(1, 6);
        for _ in 0..passes {
            for (id, inst_attr) in instances {
                if inst_attr.value.is_some() && inst_attr.pos.is_none() {
                    if let Some(ref cond) = inst_attr.condition {
                        let ctx = self.make_eval_ctx_silent(stream);
                        let cond_holds = if let Some(ref ast) = inst_attr.compiled_condition {
                            ExprEvaluator::eval_ast_bool(ast, &ctx)
                        } else {
                            ExprEvaluator::eval_bool(cond, &ctx)
                        };
                        if !cond_holds {
                            continue;
                        }
                    }
                    let ctx = self.make_eval_ctx_silent(stream);
                    let rich_val = if let Some(ref ast) = inst_attr.compiled_value {
                        ExprEvaluator::eval_ast_rich(ast, &ctx)
                    } else if let Some(ref val_expr) = inst_attr.value {
                        ExprEvaluator::evaluate_rich(val_expr, &ctx)
                    } else {
                        continue;
                    };
                    let full_id = if self.id_stack.is_empty() {
                        id.clone()
                    } else {
                        format!("{}.{}", self.id_stack.join("."), id)
                    };
                    self.record_expr_value(&full_id, &rich_val);
                    self.record_expr_value(id, &rich_val);
                }
            }
        }
    }
}
