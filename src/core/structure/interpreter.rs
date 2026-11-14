use crate::core::structure::definition::*;
use crate::core::structure::expression::{EvalContext, ExprEvaluator};
use crate::core::structure::palette;
use crate::core::structure::stream::*;
use crate::core::structure::types::{FieldCollection, FieldValue, ParseError, ParseProgress, ParseResult, ParsedField, StructureIndexBuilder};
use std::collections::HashMap;
use std::sync::Arc;

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

pub struct KaitaiInterpreter {
    ksy: std::sync::Arc<KsyDefinition>,
    stream_size: usize,
    context: HashMap<String, i64>,
    string_context: HashMap<String, String>,
    byte_arrays: HashMap<String, Vec<u8>>,
    id_stack: Vec<String>,
    active_io_prefixes: Vec<String>,
    color_index: usize,
    errors: std::cell::RefCell<Vec<ParseError>>,
    global_endian: String,
    global_bit_endian: String,
    global_encoding: Option<String>,
    field_count: usize,
    recursion_depth: usize,
    all_enums: HashMap<String, HashMap<String, String>>,
}

const MAX_RECURSION: usize = 24;
const MAX_PROGRESS_FIELDS: usize = 512;

enum AttrOnceOutcome {
    Parsed(ParsedField),
    SkippedByCondition,
    Failed,
}

struct AttrRepeatedOutcome {
    fields: Vec<ParsedField>,
    skipped_by_condition: bool,
}

#[derive(Default)]
struct ContextUndoLog {
    ints: Vec<(String, Option<i64>)>,
    strings: Vec<(String, Option<String>)>,
    bytes: Vec<(String, Option<Vec<u8>>)>,
    enums: Vec<(String, Option<HashMap<String, String>>)>,
    seen_ints: std::collections::HashSet<String>,
    seen_strings: std::collections::HashSet<String>,
    seen_bytes: std::collections::HashSet<String>,
    seen_enums: std::collections::HashSet<String>,
}

struct SavedRootIoState {
    stream_size: usize,
    active_io_prefixes: Vec<String>,
    pos: Option<i64>,
    size: Option<i64>,
    eof: Option<i64>,
    eof_bool: Option<i64>,
}

fn is_cancelled(cancel_token: Option<&std::sync::atomic::AtomicBool>) -> bool {
    cancel_token.is_some_and(|token| token.load(std::sync::atomic::Ordering::Relaxed))
}

fn expr_uses_ident(expr: &str, target: &str) -> bool {
    let bytes = expr.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    let mut prev_escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if prev_escaped {
                prev_escaped = false;
            } else if b == b'\\' && q == b'"' {
                prev_escaped = true;
            } else if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'\'' || b == b'"' {
            quote = Some(b);
            i += 1;
            continue;
        }
        if b == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b.is_ascii_digit() {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            if &expr[start..i] == target {
                return true;
            }
            continue;
        }
        i += 1;
    }
    false
}

fn expr_references_any(expr: &str, targets: &std::collections::HashSet<String>, is_root: bool) -> bool {
    if targets.is_empty() {
        return false;
    }
    let bytes = expr.as_bytes();
    let mut i = 0;
    let mut quote: Option<u8> = None;
    let mut prev_escaped = false;
    while i < bytes.len() {
        let b = bytes[i];
        if let Some(q) = quote {
            if prev_escaped {
                prev_escaped = false;
            } else if b == b'\\' && q == b'"' {
                prev_escaped = true;
            } else if b == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        if b == b'\'' || b == b'"' {
            quote = Some(b);
            i += 1;
            continue;
        }
        if b == b'#' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b.is_ascii_digit() {
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let ident = &expr[start..i];
            let mut prev = start;
            while prev > 0 && bytes[prev - 1].is_ascii_whitespace() {
                prev -= 1;
            }
            let is_after_colon_colon = prev >= 2 && bytes[prev - 1] == b':' && bytes[prev - 2] == b':';
            let is_member_access = prev > 0 && bytes[prev - 1] == b'.';
            let is_root_member = if is_member_access && is_root {
                let mut dot_prev = prev - 1;
                while dot_prev > 0 && bytes[dot_prev - 1].is_ascii_whitespace() {
                    dot_prev -= 1;
                }
                let id_end = dot_prev;
                while dot_prev > 0 && (bytes[dot_prev - 1].is_ascii_alphanumeric() || bytes[dot_prev - 1] == b'_') {
                    dot_prev -= 1;
                }
                let prefix_id = &expr[dot_prev..id_end];
                let mut before_prefix = dot_prev;
                while before_prefix > 0 && bytes[before_prefix - 1].is_ascii_whitespace() {
                    before_prefix -= 1;
                }
                prefix_id == "_root" && (before_prefix == 0 || (bytes[before_prefix - 1] != b'.' && bytes[before_prefix - 1] != b':'))
            } else {
                false
            };
            if !is_after_colon_colon && (!is_member_access || is_root_member) && targets.contains(ident) {
                return true;
            }
            continue;
        }
        i += 1;
    }
    false
}

fn process_references_any(val: &serde_yaml::Value, targets: &std::collections::HashSet<String>, is_root: bool) -> bool {
    if let Some(s) = val.as_str() {
        let s = s.trim();
        for prefix in ["xor(", "rol(", "ror("] {
            if let Some(rest) = s.strip_prefix(prefix)
                && let Some(inner) = rest.strip_suffix(')')
            {
                return expr_references_any(inner, targets, is_root);
            }
        }
    } else if let Some(map) = val.as_mapping() {
        for key in ["key", "amount", "n"] {
            if let Some(v) = map.get(key)
                && let Some(s) = v.as_str()
                && expr_references_any(s, targets, is_root)
            {
                return true;
            }
        }
    }
    false
}

fn valid_references_any(val: &serde_yaml::Value, targets: &std::collections::HashSet<String>, is_root: bool) -> bool {
    if let Some(s) = val.as_str() {
        return expr_references_any(s, targets, is_root);
    } else if let Some(map) = val.as_mapping() {
        for key in ["eq", "min", "max", "expr"] {
            if let Some(v) = map.get(key)
                && let Some(s) = v.as_str()
                && expr_references_any(s, targets, is_root)
            {
                return true;
            }
        }
        if let Some(seq) = map.get("any-of").and_then(|v| v.as_sequence()) {
            for item in seq {
                if let Some(s) = item.as_str()
                    && expr_references_any(s, targets, is_root)
                {
                    return true;
                }
            }
        }
    }
    false
}

fn attr_has_blocked_dependency(attr: &KsyAttr, blocked: &std::collections::HashSet<String>, scope: TypeScope, is_root: bool) -> bool {
    if attr.condition.as_deref().is_some_and(|c| expr_references_any(c, blocked, is_root)) {
        return true;
    }
    if let Some(KsyValue::Expr(ref pos_expr)) = attr.pos
        && expr_references_any(pos_expr, blocked, is_root)
    {
        return true;
    }
    if let Some(KsyValue::Expr(ref size_expr)) = attr.size
        && expr_references_any(size_expr, blocked, is_root)
    {
        return true;
    }
    if attr.repeat_expr.as_deref().is_some_and(|e| expr_references_any(e, blocked, is_root)) {
        return true;
    }
    if attr.repeat_until.as_deref().is_some_and(|e| expr_references_any(e, blocked, is_root)) {
        return true;
    }
    if let Some(ref io_expr) = attr.io
        && io_expr != "_root._io"
        && (io_expr != "_io" || expr_references_any(io_expr, blocked, is_root))
    {
        return true;
    }
    if let Some(ref tval) = attr.attr_type {
        if let Some(tstr) = type_as_str(tval) {
            let (tname, args) = parse_type_with_args(&tstr);
            if scope.get_type(&tname).is_some() || args.iter().any(|arg| expr_references_any(arg, blocked, is_root)) {
                return true;
            }
        } else if let Some((switch_on, cases)) = attr.compiled_switch.clone().or_else(|| type_as_switch(tval)) {
            if expr_references_any(&switch_on, blocked, is_root) {
                return true;
            }
            for (case_key, case_type) in &cases {
                if case_key != "_"
                    && !(case_key.starts_with('"') && case_key.ends_with('"'))
                    && !(case_key.starts_with('\'') && case_key.ends_with('\''))
                    && expr_references_any(case_key, blocked, is_root)
                {
                    return true;
                }
                let (tname, args) = parse_type_with_args(case_type);
                if scope.get_type(&tname).is_some() || args.iter().any(|arg| expr_references_any(arg, blocked, is_root)) {
                    return true;
                }
            }
        }
    }
    if attr.process.as_ref().is_some_and(|p| process_references_any(p, blocked, is_root)) {
        return true;
    }
    if attr.valid.as_ref().is_some_and(|v| valid_references_any(v, blocked, is_root)) {
        return true;
    }
    false
}

fn compute_preeval_blocked_ids(seq: &[KsyAttr], instances: &HashMap<String, KsyAttr>, scope: TypeScope, is_root: bool) -> std::collections::HashSet<String> {
    let mut blocked: std::collections::HashSet<String> = seq.iter().filter_map(|a| a.id.clone()).collect();
    for _ in 0..instances.len() {
        let mut changed = false;
        for (id, inst_attr) in instances {
            if blocked.contains(id) {
                continue;
            }
            let value_depends_on_blocked = inst_attr.value.as_deref().is_some_and(|v| expr_references_any(v, &blocked, is_root));
            let is_non_preeval_pos = inst_attr.pos.is_some() && attr_has_blocked_dependency(inst_attr, &blocked, scope, is_root);
            let is_value_with_cond_dep = inst_attr.value.is_some() && attr_has_blocked_dependency(inst_attr, &blocked, scope, is_root);
            if value_depends_on_blocked || is_non_preeval_pos || is_value_with_cond_dep {
                blocked.insert(id.clone());
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    blocked
}

fn can_preeval_pos_instance_with_blocked(inst_attr: &KsyAttr, blocked: &std::collections::HashSet<String>, scope: TypeScope, is_root: bool) -> bool {
    if inst_attr.pos.is_none() || inst_attr.value.is_some() {
        return false;
    }
    !attr_has_blocked_dependency(inst_attr, blocked, scope, is_root)
}

#[cfg(test)]
fn can_preeval_pos_instance(inst_attr: &KsyAttr, seq: &[KsyAttr], instances: &HashMap<String, KsyAttr>, scope: TypeScope, is_root: bool) -> bool {
    let blocked = compute_preeval_blocked_ids(seq, instances, scope, is_root);
    can_preeval_pos_instance_with_blocked(inst_attr, &blocked, scope, is_root)
}

struct ProgressTracker<'a> {
    def_id: &'a str,
    total_bytes: usize,
    last_notify_time: std::time::Instant,
    last_notify_offset: usize,
    pending_fields: Vec<ParsedField>,
    index_builder: Option<StructureIndexBuilder>,
    on_progress: &'a mut dyn FnMut(&ParseProgress),
}

impl<'a> ProgressTracker<'a> {
    fn record_root_field(&mut self, field: &ParsedField) {
        if let Some(index_builder) = self.index_builder.as_mut() {
            index_builder.add_field(field);
        }
        if self.index_builder.is_some() {
            self.pending_fields.push(field.clone());
        }
    }

    fn maybe_notify(&mut self, current_offset: usize) {
        let now = std::time::Instant::now();
        let current_offset = current_offset.min(self.total_bytes);
        if now.duration_since(self.last_notify_time).as_millis() >= 50
            || current_offset.saturating_sub(self.last_notify_offset) >= 65536
            || self.pending_fields.len() >= MAX_PROGRESS_FIELDS
        {
            self.flush(current_offset, now);
        }
    }

    fn flush_if_pending(&mut self, current_offset: usize) {
        if !self.pending_fields.is_empty() {
            let current_offset = current_offset.min(self.total_bytes);
            self.flush(current_offset, std::time::Instant::now());
        }
    }

    fn flush(&mut self, current_offset: usize, now: std::time::Instant) {
        self.last_notify_time = now;
        self.last_notify_offset = current_offset;
        (self.on_progress)(&ParseProgress {
            definition_id: self.def_id.to_string(),
            fields: into_field_chunk(std::mem::take(&mut self.pending_fields)),
            parsed_offset: current_offset,
            total_bytes: self.total_bytes,
            is_done: false,
            is_finalizing: false,
            errors: Vec::new(),
            parse_result: None,
        });
    }
}

#[derive(Clone, Copy)]
struct ProgressOrigin {
    abs_base: usize,
    abs_limit: Option<usize>,
    emit_root_fields: bool,
}

impl ProgressOrigin {
    fn root(emit_root_fields: bool) -> Self {
        Self {
            abs_base: 0,
            abs_limit: None,
            emit_root_fields,
        }
    }

    fn resolve_offset(self, stream_pos: u64) -> usize {
        let raw = self.abs_base.saturating_add(stream_pos as usize);
        self.abs_limit.map_or(raw, |lim| raw.min(lim))
    }
}

enum ArrayRunningMinMax {
    Uninit,
    Strings { min: String, max: String },
    Ints { min: i64, max: i64 },
}

fn into_field_chunk(fields: Vec<ParsedField>) -> Arc<[ParsedField]> {
    Arc::from(fields.into_boxed_slice())
}

fn empty_field_chunk() -> Arc<[ParsedField]> {
    Arc::from(Vec::<ParsedField>::new().into_boxed_slice())
}

/// Helper: extract a simple type string from serde_yaml::Value
fn type_as_str(val: &serde_yaml::Value) -> Option<String> {
    match val {
        serde_yaml::Value::String(s) => Some(s.clone()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

/// Helper: parse parameterized type string, e.g. "utf8_codepoint(_io.pos)" -> ("utf8_codepoint", ["_io.pos"])
fn parse_type_with_args(type_str: &str) -> (String, Vec<String>) {
    let s = type_str.trim();
    if let Some(paren_open) = s.find('(')
        && s.ends_with(')')
    {
        let type_name = s[..paren_open].trim().to_string();
        let args_str = &s[paren_open + 1..s.len() - 1];
        let mut args = Vec::new();
        let mut current = String::new();
        let mut paren_depth = 0;
        for ch in args_str.chars() {
            match ch {
                '(' => {
                    paren_depth += 1;
                    current.push(ch);
                }
                ')' => {
                    if paren_depth > 0 {
                        paren_depth -= 1;
                    }
                    current.push(ch);
                }
                ',' if paren_depth == 0 => {
                    let trimmed = current.trim().to_string();
                    if !trimmed.is_empty() {
                        args.push(trimmed);
                    }
                    current.clear();
                }
                _ => {
                    current.push(ch);
                }
            }
        }
        let trimmed = current.trim().to_string();
        if !trimmed.is_empty() {
            args.push(trimmed);
        }
        return (type_name, args);
    }
    (s.to_string(), Vec::new())
}

/// Helper: check if type value is a switch-on/cases
fn type_as_switch(val: &serde_yaml::Value) -> Option<(String, HashMap<String, String>)> {
    parse_switch_map(val)
}

/// Normalize enum definitions: support both simple string values and {id: ...} map values,
/// as well as hex/negative integer keys.
fn normalize_enum(raw: &HashMap<String, serde_yaml::Value>) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for (key, val) in raw {
        let label_opt = match val {
            serde_yaml::Value::String(s) => Some(s.clone()),
            serde_yaml::Value::Mapping(m) => m.get("id").and_then(|v| v.as_str()).map(|s| s.to_string()),
            _ => None,
        };
        if let Some(label) = label_opt {
            result.insert(key.clone(), label.clone());
            let trimmed = key.trim();
            if let Some(hex_part) = trimmed.strip_prefix("0x").or_else(|| trimmed.strip_prefix("0X")) {
                if let Ok(int_val) = u64::from_str_radix(&hex_part.replace('_', ""), 16) {
                    result.insert((int_val as i64).to_string(), label);
                }
            } else if let Some(hex_part) = trimmed.strip_prefix("-0x").or_else(|| trimmed.strip_prefix("-0X")) {
                if let Ok(int_val) = u64::from_str_radix(&hex_part.replace('_', ""), 16) {
                    result.insert((int_val as i64).wrapping_neg().to_string(), label);
                }
            } else if let Ok(int_val) = trimmed.parse::<i64>() {
                result.insert(int_val.to_string(), label);
            }
        }
    }
    result
}

fn collect_enums_from_types(prefix: &str, types: &HashMap<String, KsyType>, all: &mut HashMap<String, HashMap<String, String>>) {
    for (tname, type_def) in types {
        let scoped_tname = if prefix.is_empty() { tname.clone() } else { format!("{prefix}::{tname}") };
        for (ename, raw) in &type_def.enums {
            let norm = normalize_enum(raw);
            all.insert(format!("{scoped_tname}::{ename}"), norm);
        }
        collect_enums_from_types(&scoped_tname, &type_def.types, all);
    }
}

fn collect_enums(ksy: &KsyDefinition) -> HashMap<String, HashMap<String, String>> {
    let mut all = HashMap::new();
    for (name, raw) in &ksy.enums {
        all.insert(name.clone(), normalize_enum(raw));
    }
    collect_enums_from_types("", &ksy.types, &mut all);
    all
}

impl KaitaiInterpreter {
    pub fn new(ksy: KsyDefinition) -> Self {
        let global_endian = ksy.meta.fixed_endian().unwrap_or("le").to_string();
        let global_bit_endian = ksy.meta.bit_endian.clone().unwrap_or_else(|| "be".to_string());
        let global_encoding = ksy.meta.encoding.clone();
        let all_enums = collect_enums(&ksy);
        let mut ksy = ksy;
        ksy.compile_expressions();
        let mut context = HashMap::new();
        let is_le_int = if global_endian == "le" { 1 } else { 0 };
        context.insert("_is_le".to_string(), is_le_int);
        context.insert("__bool__:_is_le".to_string(), is_le_int);
        Self {
            ksy: std::sync::Arc::new(ksy),
            stream_size: 0,
            context,
            string_context: HashMap::new(),
            byte_arrays: HashMap::new(),
            id_stack: Vec::new(),
            active_io_prefixes: vec!["_root".to_string()],
            color_index: 0,
            errors: std::cell::RefCell::new(Vec::new()),
            global_endian,
            global_bit_endian,
            global_encoding,
            field_count: 0,
            recursion_depth: 0,
            all_enums,
        }
    }

    fn set_endian(&mut self, endian: String) {
        let is_le_int = if endian == "le" { 1 } else { 0 };
        self.global_endian = endian;
        self.context.insert("_is_le".to_string(), is_le_int);
        self.context.insert("__bool__:_is_le".to_string(), is_le_int);
        if !self.id_stack.is_empty() {
            let scoped = format!("{}._is_le", self.id_stack.join("."));
            self.context.insert(scoped.clone(), is_le_int);
            self.context.insert(format!("__bool__:{scoped}"), is_le_int);
        }
    }

    fn update_endian_from_meta(&mut self, meta: &KsyMeta, stream: &KaitaiStream) {
        if let Some(fixed) = meta.fixed_endian() {
            if fixed == "le" || fixed == "be" {
                self.set_endian(fixed.to_string());
            }
        } else if let Some((switch_on, cases)) = meta.switch_endian() {
            let ctx = self.make_eval_ctx_silent(stream);
            let switch_val = ExprEvaluator::evaluate_rich(&switch_on, &ctx);
            if let Some(chosen) = self.match_switch_case(&switch_val, &cases, stream)
                && (chosen == "le" || chosen == "be")
            {
                self.set_endian(chosen);
            }
        }
    }

    pub fn parse(self, stream: &mut KaitaiStream) -> ParseResult {
        self.parse_with_progress_cancellable_impl(stream, None, |_| {}, false)
    }

    #[allow(dead_code)]
    pub fn parse_with_progress<F>(self, stream: &mut KaitaiStream, on_progress: F) -> ParseResult
    where
        F: FnMut(&ParseProgress),
    {
        self.parse_with_progress_cancellable_impl(stream, None, on_progress, true)
    }

    pub fn parse_with_progress_cancellable<F>(
        self,
        stream: &mut KaitaiStream,
        cancel_token: Option<&std::sync::atomic::AtomicBool>,
        on_progress: F,
    ) -> ParseResult
    where
        F: FnMut(&ParseProgress),
    {
        self.parse_with_progress_cancellable_impl(stream, cancel_token, on_progress, true)
    }

    fn parse_with_progress_cancellable_impl<F>(
        mut self,
        stream: &mut KaitaiStream,
        cancel_token: Option<&std::sync::atomic::AtomicBool>,
        mut on_progress: F,
        prepare_display_layout: bool,
    ) -> ParseResult
    where
        F: FnMut(&ParseProgress),
    {
        // Try to determine stream size
        self.stream_size = stream.size() as usize;
        self.record_io_state("_root", stream);
        let ksy_arc = self.ksy.clone();
        let def_id = ksy_arc.meta.id.clone();
        let total_bytes = self.stream_size;

        let mut fields = Vec::new();
        let root_scope = TypeScope::root(&ksy_arc.types, &ksy_arc.enums);

        // Initial progress notification
        on_progress(&ParseProgress {
            definition_id: def_id.clone(),
            fields: empty_field_chunk(),
            parsed_offset: stream.pos() as usize,
            total_bytes,
            is_done: false,
            is_finalizing: false,
            errors: Vec::new(),
            parse_result: None,
        });

        self.update_endian_from_meta(&ksy_arc.meta, stream);

        // Pre-evaluate pos-based instances before seq so seq attributes can reference them
        let errors_before_root = self.errors.borrow().len();
        let saved_root_pos = stream.pos();
        let root_blocked = compute_preeval_blocked_ids(&ksy_arc.seq, &ksy_arc.instances, root_scope, true);
        for (id, inst_attr) in &ksy_arc.instances {
            if is_cancelled(cancel_token) {
                break;
            }
            if can_preeval_pos_instance_with_blocked(inst_attr, &root_blocked, root_scope, true) {
                let mut inst_copy = inst_attr.clone();
                inst_copy.id = Some(id.clone());
                self.parse_attr_repeated_cancellable(&inst_copy, stream, root_scope, true, ProgressOrigin::root(false), cancel_token, None);
            }
        }
        stream.set_pos(saved_root_pos);
        self.errors.borrow_mut().truncate(errors_before_root);

        if !is_cancelled(cancel_token) {
            self.evaluate_value_instances_silent(&ksy_arc.instances, stream);
        }

        let mut tracker = prepare_display_layout.then(|| ProgressTracker {
            def_id: &def_id,
            total_bytes,
            last_notify_time: std::time::Instant::now(),
            last_notify_offset: 0,
            pending_fields: Vec::new(),
            index_builder: Some(StructureIndexBuilder::new()),
            on_progress: &mut on_progress,
        });

        for attr in &ksy_arc.seq {
            if is_cancelled(cancel_token) {
                break;
            }
            let parsed = self.parse_attr_repeated_cancellable(
                attr,
                stream,
                root_scope,
                false,
                ProgressOrigin::root(prepare_display_layout),
                cancel_token,
                tracker.as_mut(),
            );
            if !parsed.skipped_by_condition && parsed.fields.is_empty() && attr.repeat.is_none() {
                break;
            }
            fields.extend(parsed.fields);
            if let Some(t) = tracker.as_mut() {
                t.flush_if_pending(stream.pos() as usize);
            }
            if is_cancelled(cancel_token) {
                break;
            }
            self.update_endian_from_meta(&ksy_arc.meta, stream);
            self.evaluate_value_instances_silent(&ksy_arc.instances, stream);
        }

        // First evaluate pos-based instances, then refresh value instances, then emit value instances
        if !is_cancelled(cancel_token) {
            for is_pos_pass in [true, false] {
                if is_cancelled(cancel_token) {
                    break;
                }
                if !is_pos_pass {
                    self.evaluate_value_instances_silent(&ksy_arc.instances, stream);
                }
                for (id, attr) in &ksy_arc.instances {
                    if is_cancelled(cancel_token) {
                        break;
                    }
                    let is_pos_inst = attr.pos.is_some();
                    if is_pos_inst != is_pos_pass {
                        continue;
                    }
                    if attr.pos.is_some() || attr.value.is_some() {
                        let mut inst_attr = attr.clone();
                        inst_attr.id = Some(id.clone());
                        let parsed = self.parse_attr_repeated_cancellable(
                            &inst_attr,
                            stream,
                            root_scope,
                            true,
                            ProgressOrigin::root(prepare_display_layout),
                            cancel_token,
                            tracker.as_mut(),
                        );
                        fields.extend(parsed.fields);

                        if let Some(t) = tracker.as_mut() {
                            t.flush_if_pending(stream.pos() as usize);
                        }
                    }
                }
            }
        }

        let mut index_builder = tracker.take().and_then(|t| t.index_builder);

        let final_offset = stream.pos() as usize;
        let final_errors = self.errors.into_inner();
        if prepare_display_layout {
            // Let the UI show 100% while the final index and default line map
            // are prepared on the parser thread. The receiver treats this as
            // a separate phase and does not coalesce it with the final result.
            on_progress(&ParseProgress {
                definition_id: def_id.clone(),
                fields: empty_field_chunk(),
                parsed_offset: final_offset,
                total_bytes,
                is_done: false,
                is_finalizing: true,
                errors: final_errors.clone(),
                parse_result: None,
            });
        }

        let final_result = if prepare_display_layout {
            let fields = FieldCollection::from_vec(fields);
            let index = index_builder
                .take()
                .expect("display-layout parsing must initialize a structure index builder")
                .finish();
            ParseResult::new_with_index(def_id.clone(), fields, final_offset, final_errors.clone(), index).with_structure_line_map(total_bytes)
        } else {
            ParseResult::new(def_id.clone(), fields, final_offset, final_errors.clone())
        };
        let final_result_arc = std::sync::Arc::new(final_result.clone());

        on_progress(&ParseProgress {
            definition_id: def_id.clone(),
            fields: empty_field_chunk(),
            parsed_offset: final_offset,
            total_bytes,
            is_done: true,
            is_finalizing: false,
            errors: final_errors.clone(),
            parse_result: Some(final_result_arc),
        });

        final_result
    }

    fn make_eval_ctx<'b>(&'b self, stream: &KaitaiStream) -> EvalContext<'b> {
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

    fn make_eval_ctx_silent<'b>(&'b self, stream: &KaitaiStream) -> EvalContext<'b> {
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

    fn check_recursion_limit(&self, stream: &KaitaiStream) -> bool {
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

    fn record_io_state(&mut self, prefix: &str, stream: &KaitaiStream) {
        let pos = stream.pos() as i64;
        let size = stream.size() as i64;
        let eof = if stream.is_eof() { 1 } else { 0 };
        self.context.insert(format!("{prefix}._io.pos"), pos);
        self.context.insert(format!("{prefix}._io.size"), size);
        self.context.insert(format!("{prefix}._io.eof"), eof);
        self.context.insert(format!("__bool__:{prefix}._io.eof"), eof);
    }

    fn record_empty_io_state(&mut self, prefix: &str) {
        self.context.insert(format!("{prefix}._io.pos"), 0);
        self.context.insert(format!("{prefix}._io.size"), 0);
        self.context.insert(format!("{prefix}._io.eof"), 1);
        self.context.insert(format!("__bool__:{prefix}._io.eof"), 1);
    }

    fn sync_active_io_states(&mut self, stream: &KaitaiStream) {
        let prefixes = self.active_io_prefixes.clone();
        for prefix in &prefixes {
            self.record_io_state(prefix, stream);
        }
    }

    fn save_root_io_state(&self) -> SavedRootIoState {
        SavedRootIoState {
            stream_size: self.stream_size,
            active_io_prefixes: self.active_io_prefixes.clone(),
            pos: self.context.get("_root._io.pos").copied(),
            size: self.context.get("_root._io.size").copied(),
            eof: self.context.get("_root._io.eof").copied(),
            eof_bool: self.context.get("__bool__:_root._io.eof").copied(),
        }
    }

    fn restore_root_io_state(&mut self, saved: SavedRootIoState) {
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

    fn set_context_int_with_undo(&mut self, key: String, val: i64, undo: &mut ContextUndoLog) {
        if undo.seen_ints.insert(key.clone()) {
            let prev = self.context.insert(key.clone(), val);
            undo.ints.push((key, prev));
        } else {
            self.context.insert(key, val);
        }
    }

    fn set_context_string_with_undo(&mut self, key: String, val: String, undo: &mut ContextUndoLog) {
        if undo.seen_strings.insert(key.clone()) {
            let prev = self.string_context.insert(key.clone(), val);
            undo.strings.push((key, prev));
        } else {
            self.string_context.insert(key, val);
        }
    }

    fn set_context_bytes_with_undo(&mut self, key: String, val: Vec<u8>, undo: &mut ContextUndoLog) {
        if undo.seen_bytes.insert(key.clone()) {
            let prev = self.byte_arrays.insert(key.clone(), val);
            undo.bytes.push((key, prev));
        } else {
            self.byte_arrays.insert(key, val);
        }
    }

    fn set_enum_with_undo(&mut self, key: String, val: HashMap<String, String>, undo: &mut ContextUndoLog) {
        if undo.seen_enums.insert(key.clone()) {
            let prev = self.all_enums.insert(key.clone(), val);
            undo.enums.push((key, prev));
        } else {
            self.all_enums.insert(key, val);
        }
    }

    fn bind_scoped_enums_with_undo(&mut self, prefix: &str, type_def: &KsyType, undo: &mut ContextUndoLog) {
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

    fn record_expr_value_with_undo(&mut self, key: &str, val: &crate::core::structure::expression::ExprValue, undo: &mut ContextUndoLog) {
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

    fn restore_scope_bindings(&mut self, undo: ContextUndoLog) {
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

    fn record_expr_value(&mut self, key: &str, val: &crate::core::structure::expression::ExprValue) {
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

    fn record_field_value(&mut self, key: &str, val: &FieldValue) {
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

    fn record_children_values(&mut self, prefix: &str, children: &[ParsedField]) {
        for child in children {
            let child_key = format!("{prefix}.{}", child.id);
            self.record_field_value(&child_key, &child.value);
            self.context.insert(format!("{child_key}._sizeof"), child.size as i64);
            self.record_children_values(&child_key, &child.children);
        }
    }

    fn update_array_metadata(&mut self, attr: &KsyAttr, results: &[ParsedField], stats: &mut ArrayRunningMinMax) {
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

    fn evaluate_value_instances_silent(&mut self, instances: &HashMap<String, KsyAttr>, stream: &KaitaiStream) {
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

    #[allow(clippy::too_many_arguments)]
    fn parse_attr_repeated_cancellable(
        &mut self,
        attr: &KsyAttr,
        stream: &mut KaitaiStream,
        scope: TypeScope,
        is_instance: bool,
        origin: ProgressOrigin,
        cancel_token: Option<&std::sync::atomic::AtomicBool>,
        mut tracker: Option<&mut ProgressTracker<'_>>,
    ) -> AttrRepeatedOutcome {
        if is_cancelled(cancel_token) || self.check_recursion_limit(stream) {
            return AttrRepeatedOutcome {
                fields: Vec::new(),
                skipped_by_condition: false,
            };
        }

        // Redirect io: _root._io at attribute level so repeated reads advance on a single root stream.
        if attr.io.as_deref() == Some("_root._io") {
            let root_slice = stream.root_data();
            let mut root_stream = KaitaiStream::new(root_slice);
            let saved = self.save_root_io_state();
            self.sync_active_io_states(stream);
            self.stream_size = root_slice.len();
            self.active_io_prefixes = vec!["_root".to_string()];
            self.record_io_state("_root", &root_stream);
            let mut attr_no_io = attr.clone();
            attr_no_io.io = None;
            let res = self.parse_attr_repeated_cancellable(
                &attr_no_io,
                &mut root_stream,
                scope,
                is_instance,
                ProgressOrigin::root(origin.emit_root_fields),
                cancel_token,
                tracker,
            );
            self.restore_root_io_state(saved);
            return res;
        }

        let mut results = Vec::new();

        // Handle value instances (computed fields, no stream reading)
        if let Some(value_expr) = &attr.value {
            if let Some(cond) = &attr.condition {
                let ctx = self.make_eval_ctx(stream);
                let cond_holds = if let Some(ref ast) = attr.compiled_condition {
                    ExprEvaluator::eval_ast_bool(ast, &ctx)
                } else {
                    ExprEvaluator::eval_bool(cond, &ctx)
                };
                if !cond_holds {
                    return AttrRepeatedOutcome {
                        fields: results,
                        skipped_by_condition: true,
                    };
                }
            }
            let ctx = self.make_eval_ctx(stream);
            let rich_val = if let Some(ref ast) = attr.compiled_value {
                ExprEvaluator::eval_ast_rich(ast, &ctx)
            } else {
                ExprEvaluator::evaluate_rich(value_expr, &ctx)
            };
            let field_id = attr.id.clone().unwrap_or_else(|| format!("value_{}", self.field_count));
            let full_id = if self.id_stack.is_empty() {
                field_id.clone()
            } else {
                format!("{}.{}", self.id_stack.join("."), field_id)
            };
            self.record_expr_value(&full_id, &rich_val);
            if let Some(ref raw_id) = attr.id {
                self.record_expr_value(raw_id, &rich_val);
            }
            if is_instance {
                let f_val = match rich_val {
                    crate::core::structure::expression::ExprValue::Int(v) => {
                        if v >= 0 {
                            FieldValue::U64(v as u64)
                        } else {
                            FieldValue::I64(v)
                        }
                    }
                    crate::core::structure::expression::ExprValue::Float(v) => FieldValue::F64(v),
                    crate::core::structure::expression::ExprValue::Str(s) => FieldValue::String(s),
                    crate::core::structure::expression::ExprValue::Bool(b) => FieldValue::Bool(b),
                    crate::core::structure::expression::ExprValue::Bytes(b) => FieldValue::Bytes(b),
                    crate::core::structure::expression::ExprValue::Array(ref arr) => {
                        if let Some(b) = rich_val.as_bytes() {
                            FieldValue::Bytes(b)
                        } else {
                            FieldValue::U64(arr.len() as u64)
                        }
                    }
                };
                let enum_label = self.resolve_enum_label(attr, &f_val, scope);
                let color = palette::color(self.color_index);
                self.color_index += 1;
                self.field_count += 1;
                let field = ParsedField {
                    id: field_id,
                    field_type: "value".to_string(),
                    offset: stream.pos() as usize,
                    size: 0,
                    value: f_val,
                    color,
                    description: attr.doc.clone(),
                    children: Vec::new(),
                    enum_label,
                    is_instance: true,
                };
                results.push(field);
            }
            return AttrRepeatedOutcome {
                fields: results,
                skipped_by_condition: false,
            };
        }

        if let Some(repeat) = &attr.repeat {
            let cond_uses_index = attr.condition.as_deref().is_some_and(|c| expr_uses_ident(c, "_index"));
            // Check attribute condition once before starting repetition if it does not depend on _index
            if let Some(cond) = &attr.condition
                && !cond_uses_index
            {
                let ctx = self.make_eval_ctx(stream);
                let cond_holds = if let Some(ref ast) = attr.compiled_condition {
                    ExprEvaluator::eval_ast_bool(ast, &ctx)
                } else {
                    ExprEvaluator::eval_bool(cond, &ctx)
                };
                if !cond_holds {
                    return AttrRepeatedOutcome {
                        fields: results,
                        skipped_by_condition: true,
                    };
                }
            }

            // When `pos` does not depend on `_index` or `_`, seek once for the repetition block and
            // restore the original position afterwards so sequential reads advance and terminate.
            let pos_uses_index = matches!(
                &attr.pos,
                Some(KsyValue::Expr(expr)) if expr_uses_ident(expr, "_index") || expr_uses_ident(expr, "_")
            );
            let mut adjusted_attr = None;
            let mut saved_pos_before_repeat = None;
            if attr.pos.is_some()
                && !pos_uses_index
                && let Some(target_pos) = self.resolve_pos(attr, stream)
            {
                saved_pos_before_repeat = Some(stream.pos());
                stream.set_pos(u64::try_from(target_pos).unwrap_or(u64::MAX));
                let mut cloned = attr.clone();
                cloned.pos = None;
                cloned.compiled_pos = None;
                adjusted_attr = Some(cloned);
            }
            if attr.condition.is_some() && !cond_uses_index {
                let cloned = adjusted_attr.get_or_insert_with(|| attr.clone());
                cloned.condition = None;
                cloned.compiled_condition = None;
            }
            let loop_attr = adjusted_attr.as_ref().unwrap_or(attr);
            let stream_len = stream.size() as usize;
            let mut array_stats = ArrayRunningMinMax::Uninit;

            self.update_array_metadata(loop_attr, &results, &mut array_stats);

            match repeat.as_str() {
                "expr" => {
                    if let Some(expr) = &loop_attr.repeat_expr {
                        let count = if let Some(ref ast) = loop_attr.compiled_repeat_expr {
                            self.resolve_count_ast(ast, stream)
                        } else {
                            self.resolve_count(expr, stream)
                        };
                        let mut prev_zero_pos: Option<usize> = None;
                        for i in 0..count {
                            if is_cancelled(cancel_token) {
                                break;
                            }
                            self.set_repeat_index(i);
                            let pos_before = stream.pos();
                            match self.parse_attr_once(loop_attr, Some(i), stream, scope, is_instance, origin, cancel_token, tracker.as_deref_mut()) {
                                AttrOnceOutcome::Parsed(field) => {
                                    let field_offset = field.offset;
                                    let field_size = field.size;
                                    if let Some(t) = tracker.as_mut() {
                                        if origin.emit_root_fields {
                                            t.record_root_field(&field);
                                        }
                                        if !is_cancelled(cancel_token) {
                                            t.maybe_notify(origin.resolve_offset(stream.pos()));
                                        }
                                    }
                                    results.push(field);
                                    self.update_array_metadata(loop_attr, &results, &mut array_stats);
                                    if loop_attr.pos.is_none() {
                                        if stream.pos() <= pos_before {
                                            break;
                                        }
                                    } else if field_size == 0 && (field_offset >= stream_len || prev_zero_pos == Some(field_offset)) {
                                        break;
                                    } else if field_size == 0 {
                                        prev_zero_pos = Some(field_offset);
                                    } else {
                                        prev_zero_pos = None;
                                    }
                                }
                                AttrOnceOutcome::SkippedByCondition => {
                                    if cond_uses_index {
                                        continue;
                                    }
                                    break;
                                }
                                AttrOnceOutcome::Failed => break,
                            }
                        }
                    }
                }
                "eos" => {
                    let mut i = 0;
                    let mut max_end: Option<usize> = None;
                    while !stream.is_eof() {
                        if is_cancelled(cancel_token) {
                            break;
                        }
                        self.set_repeat_index(i);
                        let pos_before = stream.pos();
                        match self.parse_attr_once(loop_attr, Some(i), stream, scope, is_instance, origin, cancel_token, tracker.as_deref_mut()) {
                            AttrOnceOutcome::Parsed(field) => {
                                let span_end = field.offset.saturating_add(field.size);
                                if loop_attr.pos.is_some() && max_end.is_some_and(|me| span_end <= me) {
                                    break;
                                }
                                if let Some(t) = tracker.as_mut() {
                                    if origin.emit_root_fields {
                                        t.record_root_field(&field);
                                    }
                                    if !is_cancelled(cancel_token) {
                                        t.maybe_notify(origin.resolve_offset(stream.pos()));
                                    }
                                }
                                results.push(field);
                                self.update_array_metadata(loop_attr, &results, &mut array_stats);
                                if loop_attr.pos.is_none() {
                                    if stream.pos() <= pos_before {
                                        break;
                                    }
                                } else if span_end >= stream_len {
                                    break;
                                } else {
                                    max_end = Some(max_end.map_or(span_end, |me| me.max(span_end)));
                                }
                            }
                            AttrOnceOutcome::SkippedByCondition | AttrOnceOutcome::Failed => break,
                        }
                        i += 1;
                    }
                }
                "until" => {
                    if let Some(until) = &loop_attr.repeat_until {
                        let mut i = 0;
                        let mut max_end: Option<usize> = None;
                        let mut visited_spans: Option<std::collections::HashSet<(usize, usize)>> = None;
                        while !stream.is_eof() {
                            if is_cancelled(cancel_token) {
                                break;
                            }
                            self.set_repeat_index(i);
                            let pos_before = stream.pos();
                            match self.parse_attr_once(loop_attr, Some(i), stream, scope, is_instance, origin, cancel_token, tracker.as_deref_mut()) {
                                AttrOnceOutcome::Parsed(field) => {
                                    let span = (field.offset, field.size);
                                    let span_end = field.offset.saturating_add(field.size);
                                    if loop_attr.pos.is_some() {
                                        if max_end.is_some_and(|me| span_end <= me) {
                                            let seen = visited_spans.get_or_insert_with(|| results.iter().map(|f| (f.offset, f.size)).collect());
                                            if !seen.insert(span) {
                                                break;
                                            }
                                        } else if let Some(seen) = visited_spans.as_mut()
                                            && !seen.insert(span)
                                        {
                                            break;
                                        }
                                    }
                                    self.record_field_value("_", &field.value);
                                    self.record_children_values("_", &field.children);
                                    if !self.id_stack.is_empty() {
                                        let full_underscore = format!("{}._", self.id_stack.join("."));
                                        self.record_field_value(&full_underscore, &field.value);
                                        self.record_children_values(&full_underscore, &field.children);
                                    }
                                    if let Some(t) = tracker.as_mut() {
                                        if origin.emit_root_fields {
                                            t.record_root_field(&field);
                                        }
                                        if !is_cancelled(cancel_token) {
                                            t.maybe_notify(origin.resolve_offset(stream.pos()));
                                        }
                                    }
                                    results.push(field);
                                    self.update_array_metadata(loop_attr, &results, &mut array_stats);
                                    if is_cancelled(cancel_token) {
                                        break;
                                    }
                                    let ctx = self.make_eval_ctx(stream);
                                    let until_holds = if let Some(ref ast) = loop_attr.compiled_repeat_until {
                                        ExprEvaluator::eval_ast_bool(ast, &ctx)
                                    } else {
                                        ExprEvaluator::eval_bool(until, &ctx)
                                    };
                                    if until_holds {
                                        break;
                                    }
                                    if loop_attr.pos.is_none() {
                                        if stream.pos() <= pos_before {
                                            break;
                                        }
                                    } else {
                                        if span.1 == 0 && span.0 >= stream_len {
                                            break;
                                        }
                                        max_end = Some(max_end.map_or(span_end, |me| me.max(span_end)));
                                    }
                                }
                                AttrOnceOutcome::SkippedByCondition | AttrOnceOutcome::Failed => break,
                            }
                            i += 1;
                        }
                    }
                }
                _ => {}
            }

            if let Some(old_pos) = saved_pos_before_repeat {
                stream.set_pos(old_pos);
            }
            return AttrRepeatedOutcome {
                fields: results,
                skipped_by_condition: false,
            };
        }

        match self.parse_attr_once(attr, None, stream, scope, is_instance, origin, cancel_token, tracker.as_deref_mut()) {
            AttrOnceOutcome::Parsed(field) => {
                if let Some(t) = tracker.as_mut() {
                    if origin.emit_root_fields {
                        t.record_root_field(&field);
                    }
                    if !is_cancelled(cancel_token) {
                        t.maybe_notify(origin.resolve_offset(stream.pos()));
                    }
                }
                results.push(field);
                AttrRepeatedOutcome {
                    fields: results,
                    skipped_by_condition: false,
                }
            }
            AttrOnceOutcome::SkippedByCondition => AttrRepeatedOutcome {
                fields: results,
                skipped_by_condition: true,
            },
            AttrOnceOutcome::Failed => AttrRepeatedOutcome {
                fields: results,
                skipped_by_condition: false,
            },
        }
    }

    fn set_repeat_index(&mut self, index: usize) {
        self.context.insert("_index".to_string(), index as i64);
        if !self.id_stack.is_empty() {
            self.context.insert(format!("{}._index", self.id_stack.join(".")), index as i64);
        }
    }

    fn match_switch_case(
        &self,
        switch_val: &crate::core::structure::expression::ExprValue,
        cases: &HashMap<String, String>,
        stream: &KaitaiStream,
    ) -> Option<String> {
        let switch_str = switch_val.to_string_val();
        let switch_int = switch_val.to_i64();

        if let Some(chosen) = cases
            .get(&switch_str)
            .or_else(|| cases.get(&format!("\"{}\"", switch_str)))
            .or_else(|| cases.get(&format!("'{}'", switch_str)))
            .or_else(|| {
                if matches!(
                    switch_val,
                    crate::core::structure::expression::ExprValue::Int(_) | crate::core::structure::expression::ExprValue::Bool(_)
                ) {
                    cases.get(&switch_int.to_string())
                } else {
                    None
                }
            })
        {
            return Some(chosen.clone());
        }

        let silent_ctx = self.make_eval_ctx_silent(stream);
        for (k, v) in cases {
            if k == "_" {
                continue;
            }
            let case_val = ExprEvaluator::evaluate_rich(k, &silent_ctx);
            let matched = match (switch_val, &case_val) {
                (crate::core::structure::expression::ExprValue::Int(a), crate::core::structure::expression::ExprValue::Int(b)) => a == b,
                (crate::core::structure::expression::ExprValue::Bool(a), crate::core::structure::expression::ExprValue::Bool(b)) => a == b,
                (crate::core::structure::expression::ExprValue::Str(a), crate::core::structure::expression::ExprValue::Str(b)) => a == b,
                (crate::core::structure::expression::ExprValue::Bytes(a), _) => case_val.as_bytes().is_some_and(|b| a == &b),
                (_, crate::core::structure::expression::ExprValue::Bytes(b)) => switch_val.as_bytes().is_some_and(|a| &a == b),
                (crate::core::structure::expression::ExprValue::Array(a), crate::core::structure::expression::ExprValue::Array(b)) => a == b,
                _ => false,
            };
            if matched {
                return Some(v.clone());
            }
        }

        cases.get("_").cloned()
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_attr_once(
        &mut self,
        attr: &KsyAttr,
        index: Option<usize>,
        stream: &mut KaitaiStream,
        scope: TypeScope,
        is_instance: bool,
        progress_origin: ProgressOrigin,
        cancel_token: Option<&std::sync::atomic::AtomicBool>,
        tracker: Option<&mut ProgressTracker<'_>>,
    ) -> AttrOnceOutcome {
        if is_cancelled(cancel_token) || self.check_recursion_limit(stream) {
            return AttrOnceOutcome::Failed;
        }

        // Handle io: _root._io by redirecting stream reads to a stream over root_data
        if attr.io.as_deref() == Some("_root._io") {
            let root_slice = stream.root_data();
            let mut root_stream = KaitaiStream::new(root_slice);
            let saved = self.save_root_io_state();
            self.sync_active_io_states(stream);
            self.stream_size = root_slice.len();
            self.active_io_prefixes = vec!["_root".to_string()];
            self.record_io_state("_root", &root_stream);
            let mut attr_no_io = attr.clone();
            attr_no_io.io = None;
            let res = self.parse_attr_once(
                &attr_no_io,
                index,
                &mut root_stream,
                scope,
                is_instance,
                ProgressOrigin::root(progress_origin.emit_root_fields),
                cancel_token,
                tracker,
            );
            self.restore_root_io_state(saved);
            return res;
        }

        // Condition check
        if let Some(cond) = &attr.condition {
            let ctx = self.make_eval_ctx(stream);
            let cond_holds = if let Some(ref ast) = attr.compiled_condition {
                ExprEvaluator::eval_ast_bool(ast, &ctx)
            } else {
                ExprEvaluator::eval_bool(cond, &ctx)
            };
            if !cond_holds {
                return AttrOnceOutcome::SkippedByCondition;
            }
        }

        let old_pos = if attr.pos.is_some() {
            let Some(target_pos) = self.resolve_pos(attr, stream) else {
                return AttrOnceOutcome::Failed;
            };
            let prev = stream.pos();
            stream.set_pos(u64::try_from(target_pos).unwrap_or(u64::MAX));
            Some(prev)
        } else {
            None
        };
        let start_offset = stream.pos() as usize;

        let parsed = (|| -> Option<ParsedField> {
            let is_little = self.global_endian == "le";
            let mut size = self.resolve_size_attr(attr, stream);

            // size-eos: read remaining bytes
            if attr.size_eos {
                size = None;
            }

            // Contents-based size
            if !attr.size_eos
                && (size.is_none() || size == Some(0))
                && let Some(expected_contents) = &attr.contents
            {
                if let Some(arr) = expected_contents.as_sequence() {
                    let mut sum = 0;
                    for v in arr {
                        if let Some(s) = v.as_str() {
                            sum += s.len();
                        } else {
                            sum += 1;
                        }
                    }
                    size = Some(sum);
                } else if let Some(s) = expected_contents.as_str() {
                    size = Some(s.len());
                }
            }
            let computed_size = size.unwrap_or(0);

            // Resolve type and type arguments
            let (raw_type_str, type_args) = attr
                .attr_type
                .as_ref()
                .and_then(|v| {
                    if let Some(s) = type_as_str(v) {
                        let (tname, args) = parse_type_with_args(&s);
                        Some((tname, args))
                    } else if let Some((ref switch_on, ref cases)) = attr.compiled_switch {
                        let ctx = self.make_eval_ctx(stream);
                        let switch_val = ExprEvaluator::evaluate_rich(switch_on, &ctx);
                        let chosen = self.match_switch_case(&switch_val, cases, stream);
                        chosen.map(|s| parse_type_with_args(&s))
                    } else if let Some((switch_on, cases)) = type_as_switch(v) {
                        let ctx = self.make_eval_ctx(stream);
                        let switch_val = ExprEvaluator::evaluate_rich(&switch_on, &ctx);
                        let chosen = self.match_switch_case(&switch_val, &cases, stream);
                        chosen.map(|s| parse_type_with_args(&s))
                    } else {
                        None
                    }
                })
                .unzip();

            let resolved_type = raw_type_str;
            let type_args = type_args.unwrap_or_default();

            let typed_result = if let Some(type_str) = &resolved_type {
                self.parse_typed_value(
                    type_str,
                    &type_args,
                    index,
                    is_little,
                    computed_size,
                    attr,
                    start_offset,
                    old_pos,
                    stream,
                    scope,
                    progress_origin,
                    cancel_token,
                    tracker,
                )
            } else if attr.size.is_some() || attr.size_eos || attr.contents.is_some() {
                let read_size = if attr.size_eos { self.remaining_bytes(stream) } else { computed_size };
                stream.read_bytes(read_size).map(|buf| {
                    let processed = Self::process_fixed_bytes(&buf, Self::attr_terminator(attr), Self::attr_pad_right(attr), attr.include).to_vec();
                    (FieldValue::Bytes(processed), read_size, Vec::new())
                })
            } else if let Some(term) = Self::attr_terminator(attr) {
                let pos_before = stream.pos() as usize;
                stream.read_bytes_term(term, attr.include, attr.consume, attr.eos_error).map(|buf| {
                    let sz = (stream.pos() as usize).saturating_sub(pos_before).max(buf.len());
                    let processed = Self::process_fixed_bytes(&buf, None, Self::attr_pad_right(attr), attr.include).to_vec();
                    (FieldValue::Bytes(processed), sz, Vec::new())
                })
            } else {
                Some((FieldValue::Bytes(Vec::new()), 0, Vec::new()))
            };

            // Restore position if we jumped
            if let Some(p) = old_pos {
                stream.set_pos(p);
            }

            let (mut value, final_size, children) = typed_result?;

            // Apply process transformation if present (for raw bytes; custom types apply process before parsing their substream)
            if let Some(process_val) = &attr.process
                && let FieldValue::Bytes(ref bytes) = value
                && let Some(processed) = self.apply_process(process_val, bytes, stream)
            {
                value = FieldValue::Bytes(processed);
            }

            // Contents validation
            if let Some(expected) = &attr.contents
                && !self.validate_contents(expected, &value, stream)
            {
                return None;
            }

            // Value validation
            if let Some(valid_rule) = &attr.valid
                && !self.validate_value(valid_rule, &value, attr, scope, stream)
            {
                return None;
            }

            // Context update
            let field_id = if let Some(ref raw_id) = attr.id {
                let fid = if let Some(i) = index { format!("{}[{}]", raw_id, i) } else { raw_id.clone() };
                if self.id_stack.is_empty() {
                    self.record_field_value(&fid, &value);
                    self.context.insert(format!("{fid}._sizeof"), final_size as i64);
                    self.record_children_values(&fid, &children);
                    if index.is_some() {
                        self.record_field_value(raw_id, &value);
                        self.context.insert(format!("{raw_id}._sizeof"), final_size as i64);
                        self.record_children_values(raw_id, &children);
                    }
                } else {
                    let stack_prefix = self.id_stack.join(".");
                    let full_id = format!("{}.{}", stack_prefix, fid);
                    let unindexed_id = format!("{}.{}", stack_prefix, raw_id);

                    self.record_field_value(&full_id, &value);
                    self.record_field_value(&unindexed_id, &value);
                    self.record_field_value(&fid, &value);
                    self.record_field_value(raw_id, &value);

                    self.context.insert(format!("{full_id}._sizeof"), final_size as i64);
                    self.context.insert(format!("{unindexed_id}._sizeof"), final_size as i64);
                    self.context.insert(format!("{fid}._sizeof"), final_size as i64);
                    self.context.insert(format!("{raw_id}._sizeof"), final_size as i64);

                    self.record_children_values(&full_id, &children);
                    if index.is_some() {
                        self.record_children_values(&unindexed_id, &children);
                    }
                }
                fid
            } else if let Some(i) = index {
                format!("unnamed_{}[{}]", self.field_count, i)
            } else {
                format!("unnamed_{}", self.field_count)
            };

            // Enum label
            let enum_label = self.resolve_enum_label(attr, &value, scope);

            let color = palette::color(self.color_index);
            self.color_index += 1;
            self.field_count += 1;

            let type_name = resolved_type.unwrap_or_else(|| "bytes".to_string());
            Some(ParsedField {
                id: field_id,
                field_type: type_name,
                offset: start_offset,
                size: final_size,
                value,
                color,
                description: attr.doc.clone(),
                children,
                enum_label,
                is_instance,
            })
        })();

        match parsed {
            Some(field) => AttrOnceOutcome::Parsed(field),
            None => AttrOnceOutcome::Failed,
        }
    }

    fn process_fixed_bytes(buf: &[u8], term: Option<u8>, pad: Option<u8>, include: bool) -> &[u8] {
        let mut slice = buf;
        let mut found_term = false;
        if let Some(t) = term
            && let Some(pos) = slice.iter().position(|&b| b == t)
        {
            found_term = true;
            let end = if include { pos + 1 } else { pos };
            slice = &slice[..end];
        }
        if let Some(p) = pad {
            let skip_strip = found_term && include && term == Some(p);
            if !skip_strip {
                let mut end = slice.len();
                while end > 0 && slice[end - 1] == p {
                    end -= 1;
                }
                slice = &slice[..end];
            }
        }
        slice
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_typed_value(
        &mut self,
        type_str: &str,
        type_args: &[String],
        index: Option<usize>,
        is_little: bool,
        size: usize,
        attr: &KsyAttr,
        start_offset: usize,
        old_pos: Option<u64>,
        stream: &mut KaitaiStream,
        scope: TypeScope,
        progress_origin: ProgressOrigin,
        cancel_token: Option<&std::sync::atomic::AtomicBool>,
        tracker: Option<&mut ProgressTracker<'_>>,
    ) -> Option<(FieldValue, usize, Vec<ParsedField>)> {
        match type_str {
            "u1" => Some((FieldValue::U8(stream.read_u1()?), 1, Vec::new())),
            "u2" => {
                let v = if is_little { stream.read_u2le()? } else { stream.read_u2be()? };
                Some((FieldValue::U16(v), 2, Vec::new()))
            }
            "u4" => {
                let v = if is_little { stream.read_u4le()? } else { stream.read_u4be()? };
                Some((FieldValue::U32(v), 4, Vec::new()))
            }
            "u8" => {
                let v = if is_little { stream.read_u8le()? } else { stream.read_u8be()? };
                Some((FieldValue::U64(v), 8, Vec::new()))
            }
            "s1" => Some((FieldValue::I8(stream.read_s1()?), 1, Vec::new())),
            "s2" => {
                let v = if is_little { stream.read_s2le()? } else { stream.read_s2be()? };
                Some((FieldValue::I16(v), 2, Vec::new()))
            }
            "s4" => {
                let v = if is_little { stream.read_s4le()? } else { stream.read_s4be()? };
                Some((FieldValue::I32(v), 4, Vec::new()))
            }
            "s8" => {
                let v = if is_little { stream.read_s8le()? } else { stream.read_s8be()? };
                Some((FieldValue::I64(v), 8, Vec::new()))
            }
            // Explicit endian types
            "u2le" => Some((FieldValue::U16(stream.read_u2le()?), 2, Vec::new())),
            "u2be" => Some((FieldValue::U16(stream.read_u2be()?), 2, Vec::new())),
            "u4le" => Some((FieldValue::U32(stream.read_u4le()?), 4, Vec::new())),
            "u4be" => Some((FieldValue::U32(stream.read_u4be()?), 4, Vec::new())),
            "u8le" => Some((FieldValue::U64(stream.read_u8le()?), 8, Vec::new())),
            "u8be" => Some((FieldValue::U64(stream.read_u8be()?), 8, Vec::new())),
            "s2le" => Some((FieldValue::I16(stream.read_s2le()?), 2, Vec::new())),
            "s2be" => Some((FieldValue::I16(stream.read_s2be()?), 2, Vec::new())),
            "s4le" => Some((FieldValue::I32(stream.read_s4le()?), 4, Vec::new())),
            "s4be" => Some((FieldValue::I32(stream.read_s4be()?), 4, Vec::new())),
            "s8le" => Some((FieldValue::I64(stream.read_s8le()?), 8, Vec::new())),
            "s8be" => Some((FieldValue::I64(stream.read_s8be()?), 8, Vec::new())),
            // Float types
            "f4" => {
                let v = if is_little { stream.read_f4le()? } else { stream.read_f4be()? };
                Some((FieldValue::F32(v), 4, Vec::new()))
            }
            "f8" => {
                let v = if is_little { stream.read_f8le()? } else { stream.read_f8be()? };
                Some((FieldValue::F64(v), 8, Vec::new()))
            }
            "f4le" => Some((FieldValue::F32(stream.read_f4le()?), 4, Vec::new())),
            "f4be" => Some((FieldValue::F32(stream.read_f4be()?), 4, Vec::new())),
            "f8le" => Some((FieldValue::F64(stream.read_f8le()?), 8, Vec::new())),
            "f8be" => Some((FieldValue::F64(stream.read_f8be()?), 8, Vec::new())),
            // String types
            "str" => {
                if attr.size.is_some() || attr.size_eos {
                    let read_size = if attr.size_eos { self.remaining_bytes(stream) } else { size };
                    let buf = stream.read_bytes_slice(read_size)?;
                    let processed = Self::process_fixed_bytes(buf, Self::attr_terminator(attr), Self::attr_pad_right(attr), attr.include);
                    let s = self.decode_string(processed, attr);
                    Some((FieldValue::String(s), read_size, Vec::new()))
                } else if let Some(term) = Self::attr_terminator(attr) {
                    let pos_before = stream.pos() as usize;
                    let buf = stream.read_bytes_term(term, attr.include, attr.consume, attr.eos_error)?;
                    let read_sz = (stream.pos() as usize).saturating_sub(pos_before).max(buf.len());
                    let processed = Self::process_fixed_bytes(&buf, None, Self::attr_pad_right(attr), attr.include);
                    let s = self.decode_string(processed, attr);
                    Some((FieldValue::String(s), read_sz, Vec::new()))
                } else {
                    Some((FieldValue::String(String::new()), 0, Vec::new()))
                }
            }
            "strz" => {
                let term = Self::attr_terminator(attr).unwrap_or(0);
                if attr.size.is_some() || attr.size_eos {
                    let read_size = if attr.size_eos { self.remaining_bytes(stream) } else { size };
                    let buf = stream.read_bytes_slice(read_size)?;
                    let processed = Self::process_fixed_bytes(buf, Some(term), Self::attr_pad_right(attr), attr.include);
                    let s = self.decode_string(processed, attr);
                    Some((FieldValue::String(s), read_size, Vec::new()))
                } else {
                    let pos_before = stream.pos() as usize;
                    let buf = stream.read_bytes_term(term, attr.include, attr.consume, attr.eos_error)?;
                    let sz = (stream.pos() as usize).saturating_sub(pos_before).max(buf.len());
                    let processed = Self::process_fixed_bytes(&buf, None, Self::attr_pad_right(attr), attr.include);
                    let s = self.decode_string(processed, attr);
                    Some((FieldValue::String(s), sz, Vec::new()))
                }
            }
            // Bit fields (supporting bN, bNle, bNbe)
            t if t.starts_with('b')
                && !t[1..].trim_end_matches("le").trim_end_matches("be").is_empty()
                && t[1..].trim_end_matches("le").trim_end_matches("be").chars().all(|c| c.is_ascii_digit()) =>
            {
                let (bits_str, is_le) = if t.ends_with("le") {
                    (&t[1..t.len() - 2], true)
                } else if t.ends_with("be") {
                    (&t[1..t.len() - 2], false)
                } else {
                    (&t[1..], self.global_bit_endian == "le")
                };
                if let Ok(bits) = bits_str.parse::<usize>() {
                    let val = if is_le {
                        stream.read_bits_int_le(bits)?
                    } else {
                        stream.read_bits_int_be(bits)?
                    };
                    // size representation: we round up to byte representation for highlight purposes
                    let bytes_needed = bits.div_ceil(8);
                    Some((FieldValue::U64(val), bytes_needed, Vec::new()))
                } else {
                    None
                }
            }
            // Custom type
            custom => self.parse_custom_type(
                custom,
                type_args,
                index,
                attr,
                start_offset,
                old_pos,
                stream,
                scope,
                progress_origin,
                cancel_token,
                tracker,
            ),
        }
    }

    fn decode_string(&self, buf: &[u8], attr: &KsyAttr) -> String {
        if let Some(encoding_str) = attr.encoding.as_ref().or(self.global_encoding.as_ref()) {
            crate::core::structure::expression::decode_bytes(buf, encoding_str)
        } else if let Ok(s) = std::str::from_utf8(buf) {
            s.to_string()
        } else {
            String::from_utf8_lossy(buf).into_owned()
        }
    }

    fn rotate_bits(data: &[u8], amount: i64, left: bool) -> Option<Vec<u8>> {
        let shift = (amount.unsigned_abs() % 8) as u32;
        let effective_left = if amount >= 0 { left } else { !left };
        if effective_left {
            process::rotate_left(data, shift, 1).ok()
        } else {
            process::rotate_right(data, shift, 1).ok()
        }
    }

    fn apply_process(&self, process_val: &serde_yaml::Value, data: &[u8], stream: &KaitaiStream) -> Option<Vec<u8>> {
        if let Some(s) = process_val.as_str() {
            let s = s.trim();
            if s == "zlib" {
                return process::zlib_decompress(data);
            }

            if s.starts_with("xor(") && s.ends_with(')') {
                let expr = &s[4..s.len() - 1];
                let ctx = self.make_eval_ctx(stream);
                let val = ExprEvaluator::evaluate_rich(expr, &ctx);
                if let Some(key_bytes) = val.as_bytes() {
                    if key_bytes.is_empty() {
                        return Some(data.to_vec());
                    }
                    return Some(process::xor_many(data, &key_bytes));
                }
                return Some(process::xor_one(data, val.to_i64() as u8));
            }

            if s.starts_with("rol(") && s.ends_with(')') {
                let expr = &s[4..s.len() - 1];
                let ctx = self.make_eval_ctx(stream);
                let val = ExprEvaluator::eval_i64(expr, &ctx);
                return Self::rotate_bits(data, val, true);
            }

            if s.starts_with("ror(") && s.ends_with(')') {
                let expr = &s[4..s.len() - 1];
                let ctx = self.make_eval_ctx(stream);
                let val = ExprEvaluator::eval_i64(expr, &ctx);
                return Self::rotate_bits(data, val, false);
            }
        } else if let Some(map) = process_val.as_mapping() {
            let algo = map.get("algo")?.as_str()?;
            if algo == "xor" {
                let key_val = map.get("key")?;
                let ctx = self.make_eval_ctx(stream);
                if let Some(k_str) = key_val.as_str() {
                    let val = ExprEvaluator::evaluate_rich(k_str, &ctx);
                    if let Some(key_bytes) = val.as_bytes() {
                        return Some(process::xor_many(data, &key_bytes));
                    }
                    return Some(process::xor_one(data, val.to_i64() as u8));
                } else if let Some(k_int) = key_val.as_i64() {
                    return Some(process::xor_one(data, k_int as u8));
                } else if let Some(seq) = key_val.as_sequence() {
                    let key_bytes: Vec<u8> = seq.iter().filter_map(|v| v.as_u64().map(|n| n as u8)).collect();
                    return Some(process::xor_many(data, &key_bytes));
                }
            } else if algo == "rol" {
                let n = map.get("amount").or_else(|| map.get("n")).and_then(|v| {
                    v.as_i64()
                        .or_else(|| v.as_u64().map(|u| u as i64))
                        .or_else(|| v.as_str().map(|s| ExprEvaluator::eval_i64(s, &self.make_eval_ctx(stream))))
                })?;
                return Self::rotate_bits(data, n, true);
            } else if algo == "ror" {
                let n = map.get("amount").or_else(|| map.get("n")).and_then(|v| {
                    v.as_i64()
                        .or_else(|| v.as_u64().map(|u| u as i64))
                        .or_else(|| v.as_str().map(|s| ExprEvaluator::eval_i64(s, &self.make_eval_ctx(stream))))
                })?;
                return Self::rotate_bits(data, n, false);
            } else if algo == "zlib" {
                return process::zlib_decompress(data);
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_custom_type(
        &mut self,
        type_name: &str,
        type_args: &[String],
        index: Option<usize>,
        attr: &KsyAttr,
        start_offset: usize,
        _old_pos: Option<u64>,
        stream: &mut KaitaiStream,
        scope: TypeScope,
        progress_origin: ProgressOrigin,
        cancel_token: Option<&std::sync::atomic::AtomicBool>,
        mut tracker: Option<&mut ProgressTracker<'_>>,
    ) -> Option<(FieldValue, usize, Vec<ParsedField>)> {
        if is_cancelled(cancel_token) {
            return None;
        }
        if self.recursion_depth >= MAX_RECURSION {
            self.errors.borrow_mut().push(ParseError {
                message: format!("Maximum recursion depth ({MAX_RECURSION}) exceeded"),
                offset: stream.pos() as usize,
            });
            return None;
        }

        let type_def = match scope.get_type(type_name) {
            Some(t) => t,
            None => {
                // Type not found (e.g. imported type like dos_datetime).
                // Still consume the declared `size` bytes so the stream position stays correct.
                let size_val = self.resolve_size_attr(attr, stream);
                if let Some(sz) = size_val
                    && sz > 0
                {
                    let buf = stream.read_bytes(sz)?;
                    return Some((FieldValue::Bytes(buf), sz, Vec::new()));
                } else if attr.size_eos {
                    let buf = stream.read_bytes_remaining()?;
                    let sz = buf.len();
                    return Some((FieldValue::Bytes(buf), sz, Vec::new()));
                }
                return None;
            }
        };

        // Evaluate size and type_args in the caller's scope BEFORE pushing callee onto id_stack
        let size_val = self.resolve_size_attr(attr, stream);
        let mut evaluated_params = Vec::new();
        for (i, param) in type_def.params.iter().enumerate() {
            if let Some(arg_expr) = type_args.get(i) {
                let ctx = self.make_eval_ctx(stream);
                let rich_val = ExprEvaluator::evaluate_rich(arg_expr, &ctx);

                // Also collect any struct child entries if arg_expr references a struct in caller scope
                let arg_trimmed = arg_expr.trim();
                let mut struct_prefixes = Vec::new();
                if !self.id_stack.is_empty() {
                    struct_prefixes.push(format!("{}.{}.", self.id_stack.join("."), arg_trimmed));
                }
                struct_prefixes.push(format!("{arg_trimmed}."));

                let mut child_ints = Vec::new();
                let mut child_strs = Vec::new();
                let mut child_bytes = Vec::new();
                for sp in &struct_prefixes {
                    for (k, v) in &self.context {
                        if let Some(suffix) = k.strip_prefix(sp) {
                            child_ints.push((suffix.to_string(), *v));
                        } else if let Some(rest) = k.strip_prefix("__float__:")
                            && let Some(suffix) = rest.strip_prefix(sp)
                        {
                            child_ints.push((format!("__float__:{suffix}"), *v));
                        } else if let Some(rest) = k.strip_prefix("__bool__:")
                            && let Some(suffix) = rest.strip_prefix(sp)
                        {
                            child_ints.push((format!("__bool__:{suffix}"), *v));
                        }
                    }
                    for (k, v) in &self.string_context {
                        if let Some(suffix) = k.strip_prefix(sp) {
                            child_strs.push((suffix.to_string(), v.clone()));
                        }
                    }
                    for (k, v) in &self.byte_arrays {
                        if let Some(suffix) = k.strip_prefix(sp) {
                            child_bytes.push((suffix.to_string(), v.clone()));
                        }
                    }
                }
                evaluated_params.push((param.id.clone(), rich_val, child_ints, child_strs, child_bytes));
            }
        }

        let raw_id = attr.id.clone().unwrap_or_else(|| format!("field_{}", self.field_count));
        let stack_id = if let Some(i) = index { format!("{raw_id}[{i}]") } else { raw_id };
        self.id_stack.push(stack_id);
        self.recursion_depth += 1;

        // Bind evaluated parameters in callee's scope, tracking unqualified bindings for restoration
        let mut scope_undo = ContextUndoLog::default();
        let stack_prefix = self.id_stack.join(".");
        for (param_id, rich_val, child_ints, child_strs, child_bytes) in evaluated_params {
            let scoped_param = format!("{stack_prefix}.{param_id}");
            self.record_expr_value(&scoped_param, &rich_val);
            self.record_expr_value_with_undo(&param_id, &rich_val, &mut scope_undo);
            for (suffix, v) in child_ints {
                if let Some(f_suf) = suffix.strip_prefix("__float__:") {
                    self.context.insert(format!("__float__:{scoped_param}.{f_suf}"), v);
                    self.set_context_int_with_undo(format!("__float__:{param_id}.{f_suf}"), v, &mut scope_undo);
                } else if let Some(b_suf) = suffix.strip_prefix("__bool__:") {
                    self.context.insert(format!("__bool__:{scoped_param}.{b_suf}"), v);
                    self.set_context_int_with_undo(format!("__bool__:{param_id}.{b_suf}"), v, &mut scope_undo);
                } else {
                    self.context.insert(format!("{scoped_param}.{suffix}"), v);
                    self.set_context_int_with_undo(format!("{param_id}.{suffix}"), v, &mut scope_undo);
                }
            }
            for (suffix, s) in child_strs {
                self.string_context.insert(format!("{scoped_param}.{suffix}"), s.clone());
                self.set_context_string_with_undo(format!("{param_id}.{suffix}"), s, &mut scope_undo);
            }
            for (suffix, b) in child_bytes {
                self.byte_arrays.insert(format!("{scoped_param}.{suffix}"), b.clone());
                self.set_context_bytes_with_undo(format!("{param_id}.{suffix}"), b, &mut scope_undo);
            }
        }

        // Save and apply subtype-level meta (endian, bit-endian, encoding)
        let saved_endian = self.global_endian.clone();
        let saved_bit_endian = self.global_bit_endian.clone();
        let saved_encoding = self.global_encoding.clone();
        if let Some(ref sub_meta) = type_def.meta {
            if let Some(ref be) = sub_meta.bit_endian {
                self.global_bit_endian = be.clone();
            }
            if sub_meta.encoding.is_some() {
                self.global_encoding = sub_meta.encoding.clone();
            }
            self.update_endian_from_meta(sub_meta, stream);
        }

        let nested_scope = scope.child(&type_def.types, &type_def.enums);
        self.bind_scoped_enums_with_undo("", type_def, &mut scope_undo);

        // If size is explicitly 0, return an empty struct immediately (consume nothing).
        if size_val == Some(0) {
            self.record_empty_io_state(&stack_prefix);
            self.restore_scope_bindings(scope_undo);
            self.set_endian(saved_endian);
            self.global_bit_endian = saved_bit_endian;
            self.global_encoding = saved_encoding;
            self.recursion_depth -= 1;
            self.id_stack.pop();
            return Some((FieldValue::Struct, 0, Vec::new()));
        }

        // Align bit cursor when entering a new struct ONLY if neither fixed nor calculated bitfields are mid-byte?
        // Note: In Kaitai Struct, subtypes do not reset bit alignment unless stream changes (substream).
        let use_substream = (size_val.is_some() && size_val != Some(0)) || attr.size_eos;
        let saved_stream_size = self.stream_size;
        let saved_active_io = self.active_io_prefixes.clone();

        let res = (|| {
            if use_substream {
                let sub_size = if attr.size_eos { self.remaining_bytes(stream) } else { size_val.unwrap_or(0) };
                let processed_buf_opt = if let Some(ref process_val) = attr.process {
                    let raw_bytes = stream.read_bytes(sub_size)?;
                    Some(self.apply_process(process_val, &raw_bytes, stream)?)
                } else {
                    None
                };

                let mut sub_stream = if let Some(ref processed_buf) = processed_buf_opt {
                    KaitaiStream::new_with_root(processed_buf, stream.root_data())
                } else {
                    stream.take_substream(sub_size)?
                };

                let effective_sub_size = sub_stream.size() as usize;
                self.sync_active_io_states(stream);
                self.active_io_prefixes = vec![stack_prefix.clone()];
                self.stream_size = effective_sub_size;
                self.record_io_state(&stack_prefix, &sub_stream);

                let child_base = progress_origin.abs_base.saturating_add(start_offset);
                let child_limit = if processed_buf_opt.is_some() {
                    let proc_cap = child_base.saturating_add(sub_size);
                    Some(progress_origin.abs_limit.map_or(proc_cap, |lim| lim.min(proc_cap)))
                } else {
                    progress_origin.abs_limit
                };
                let child_origin = ProgressOrigin {
                    abs_base: child_base,
                    abs_limit: child_limit,
                    emit_root_fields: false,
                };

                let mut fields = Vec::new();

                // Pre-evaluate pos-based instances before seq (e.g. byte0 at pos: ofs)
                let errors_before_sub = self.errors.borrow().len();
                let saved_sub_pos = sub_stream.pos();
                let sub_blocked = compute_preeval_blocked_ids(&type_def.seq, &type_def.instances, nested_scope, false);
                for (id, inst_attr) in &type_def.instances {
                    if is_cancelled(cancel_token) {
                        break;
                    }
                    if can_preeval_pos_instance_with_blocked(inst_attr, &sub_blocked, nested_scope, false) {
                        let mut inst_copy = inst_attr.clone();
                        inst_copy.id = Some(id.clone());
                        self.parse_attr_repeated_cancellable(&inst_copy, &mut sub_stream, nested_scope, true, child_origin, cancel_token, None);
                    }
                }
                sub_stream.set_pos(saved_sub_pos);
                self.errors.borrow_mut().truncate(errors_before_sub);

                // Pre-evaluate value instances without recording errors (in case seq sizes depend on them)
                if !is_cancelled(cancel_token) {
                    self.evaluate_value_instances_silent(&type_def.instances, &sub_stream);
                }

                for nested_attr in &type_def.seq {
                    if is_cancelled(cancel_token) {
                        break;
                    }
                    let parsed = self.parse_attr_repeated_cancellable(
                        nested_attr,
                        &mut sub_stream,
                        nested_scope,
                        false,
                        child_origin,
                        cancel_token,
                        tracker.as_deref_mut(),
                    );
                    if parsed.fields.is_empty() && is_cancelled(cancel_token) {
                        break;
                    }
                    if !parsed.skipped_by_condition && parsed.fields.is_empty() && nested_attr.repeat.is_none() {
                        return None;
                    }
                    fields.extend(parsed.fields);
                    if is_cancelled(cancel_token) {
                        break;
                    }
                    if let Some(ref sub_meta) = type_def.meta {
                        self.update_endian_from_meta(sub_meta, &sub_stream);
                    }
                    self.evaluate_value_instances_silent(&type_def.instances, &sub_stream);
                }

                // Adjust seq offsets from substream-relative to absolute file offsets
                let mut pending = fields.iter_mut().map(|field| (field, start_offset)).collect::<Vec<_>>();
                while let Some((field, base)) = pending.pop() {
                    field.offset = field.offset.saturating_add(base);
                    for child in &mut field.children {
                        pending.push((child, base));
                    }
                }

                // Evaluate instances after seq: first pos instances, then refresh value instances, then emit value instances
                if !is_cancelled(cancel_token) {
                    for is_pos_pass in [true, false] {
                        if is_cancelled(cancel_token) {
                            break;
                        }
                        if !is_pos_pass {
                            self.evaluate_value_instances_silent(&type_def.instances, &sub_stream);
                        }
                        for (id, inst_attr) in &type_def.instances {
                            if is_cancelled(cancel_token) {
                                break;
                            }
                            let is_pos_inst = inst_attr.pos.is_some();
                            if is_pos_inst != is_pos_pass {
                                continue;
                            }
                            if inst_attr.pos.is_some() || inst_attr.value.is_some() {
                                let mut inst_copy = inst_attr.clone();
                                inst_copy.id = Some(id.clone());
                                let mut inst_fields = self
                                    .parse_attr_repeated_cancellable(
                                        &inst_copy,
                                        &mut sub_stream,
                                        nested_scope,
                                        true,
                                        child_origin,
                                        cancel_token,
                                        tracker.as_deref_mut(),
                                    )
                                    .fields;
                                if inst_attr.io.as_deref() != Some("_root._io") {
                                    let mut inst_pending = inst_fields.iter_mut().map(|field| (field, start_offset)).collect::<Vec<_>>();
                                    while let Some((field, base)) = inst_pending.pop() {
                                        field.offset = field.offset.saturating_add(base);
                                        for child in &mut field.children {
                                            inst_pending.push((child, base));
                                        }
                                    }
                                }
                                fields.extend(inst_fields);
                            }
                        }
                    }
                }

                self.record_io_state(&stack_prefix, &sub_stream);
                let new_pos = u64::try_from(start_offset.saturating_add(sub_size)).unwrap_or(u64::MAX);
                stream.set_pos(new_pos);
                Some(fields)
            } else {
                self.active_io_prefixes.push(stack_prefix.clone());
                let child_origin = ProgressOrigin {
                    emit_root_fields: false,
                    ..progress_origin
                };

                // Pre-evaluate pos-based instances before seq
                let errors_before = self.errors.borrow().len();
                let saved_pos = stream.pos();
                let sub_blocked = compute_preeval_blocked_ids(&type_def.seq, &type_def.instances, nested_scope, false);
                for (id, inst_attr) in &type_def.instances {
                    if is_cancelled(cancel_token) {
                        break;
                    }
                    if can_preeval_pos_instance_with_blocked(inst_attr, &sub_blocked, nested_scope, false) {
                        let mut inst_copy = inst_attr.clone();
                        inst_copy.id = Some(id.clone());
                        self.parse_attr_repeated_cancellable(&inst_copy, stream, nested_scope, true, child_origin, cancel_token, None);
                    }
                }
                stream.set_pos(saved_pos);
                self.errors.borrow_mut().truncate(errors_before);

                // Pre-evaluate value instances without recording errors (in case seq sizes depend on them)
                if !is_cancelled(cancel_token) {
                    self.evaluate_value_instances_silent(&type_def.instances, stream);
                }

                let mut fields = Vec::new();
                for nested_attr in &type_def.seq {
                    if is_cancelled(cancel_token) {
                        break;
                    }
                    let parsed =
                        self.parse_attr_repeated_cancellable(nested_attr, stream, nested_scope, false, child_origin, cancel_token, tracker.as_deref_mut());
                    if parsed.fields.is_empty() && is_cancelled(cancel_token) {
                        break;
                    }
                    if !parsed.skipped_by_condition && parsed.fields.is_empty() && nested_attr.repeat.is_none() {
                        return None;
                    }
                    fields.extend(parsed.fields);
                    if is_cancelled(cancel_token) {
                        break;
                    }
                    if let Some(ref sub_meta) = type_def.meta {
                        self.update_endian_from_meta(sub_meta, stream);
                    }
                    self.evaluate_value_instances_silent(&type_def.instances, stream);
                }

                // Evaluate instances after seq: first pos instances, then refresh value instances, then emit value instances
                if !is_cancelled(cancel_token) {
                    for is_pos_pass in [true, false] {
                        if is_cancelled(cancel_token) {
                            break;
                        }
                        if !is_pos_pass {
                            self.evaluate_value_instances_silent(&type_def.instances, stream);
                        }
                        for (id, inst_attr) in &type_def.instances {
                            if is_cancelled(cancel_token) {
                                break;
                            }
                            let is_pos_inst = inst_attr.pos.is_some();
                            if is_pos_inst != is_pos_pass {
                                continue;
                            }
                            if inst_attr.pos.is_some() || inst_attr.value.is_some() {
                                let mut inst_copy = inst_attr.clone();
                                inst_copy.id = Some(id.clone());
                                fields.extend(
                                    self.parse_attr_repeated_cancellable(
                                        &inst_copy,
                                        stream,
                                        nested_scope,
                                        true,
                                        child_origin,
                                        cancel_token,
                                        tracker.as_deref_mut(),
                                    )
                                    .fields,
                                );
                            }
                        }
                    }
                }
                self.record_io_state(&stack_prefix, stream);
                Some(fields)
            }
        })();

        self.stream_size = saved_stream_size;
        self.active_io_prefixes = saved_active_io;
        self.restore_scope_bindings(scope_undo);
        self.set_endian(saved_endian);
        self.global_bit_endian = saved_bit_endian;
        self.global_encoding = saved_encoding;
        self.recursion_depth -= 1;
        self.id_stack.pop();

        let nested_fields = res?;
        let current_pos = stream.pos() as usize;
        let total_size = current_pos.saturating_sub(start_offset);
        self.context.insert(format!("{stack_prefix}._sizeof"), total_size as i64);

        Some((FieldValue::Struct, total_size, nested_fields))
    }

    fn resolve_enum_label(&self, attr: &KsyAttr, value: &FieldValue, scope: TypeScope) -> Option<String> {
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

    fn validate_contents(&mut self, expected: &serde_yaml::Value, actual: &FieldValue, stream: &KaitaiStream) -> bool {
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

    fn validate_value(&mut self, valid: &serde_yaml::Value, actual: &FieldValue, attr: &KsyAttr, scope: TypeScope, stream: &KaitaiStream) -> bool {
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

    fn resolve_size_attr(&self, attr: &KsyAttr, stream: &KaitaiStream) -> Option<usize> {
        if let Some(ref ast) = attr.compiled_size {
            let ctx = self.make_eval_ctx(stream);
            let val = ExprEvaluator::eval_ast_i64(ast, &ctx);
            Some(if val < 0 { 0 } else { val as usize })
        } else {
            self.resolve_size(&attr.size, stream)
        }
    }

    fn resolve_pos(&self, attr: &KsyAttr, stream: &KaitaiStream) -> Option<usize> {
        if let Some(ref ast) = attr.compiled_pos {
            let ctx = self.make_eval_ctx(stream);
            let val = ExprEvaluator::eval_ast_i64(ast, &ctx);
            Some(if val < 0 { 0 } else { val as usize })
        } else {
            self.resolve_size(&attr.pos, stream)
        }
    }

    fn resolve_size(&self, size_val: &Option<KsyValue>, stream: &KaitaiStream) -> Option<usize> {
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

    fn resolve_count(&self, expr: &str, stream: &KaitaiStream) -> usize {
        let ctx = self.make_eval_ctx(stream);
        let val = ExprEvaluator::eval_i64(expr, &ctx);
        if val < 0 { 0 } else { val as usize }
    }

    fn resolve_count_ast(&self, ast: &crate::core::structure::expression::ExprAST, stream: &KaitaiStream) -> usize {
        let ctx = self.make_eval_ctx(stream);
        let val = ExprEvaluator::eval_ast_i64(ast, &ctx);
        if val < 0 { 0 } else { val as usize }
    }

    fn remaining_bytes(&self, stream: &KaitaiStream) -> usize {
        (stream.size() as usize).saturating_sub(stream.pos() as usize)
    }

    fn attr_terminator(attr: &KsyAttr) -> Option<u8> {
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

    fn attr_pad_right(attr: &KsyAttr) -> Option<u8> {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_def(yaml: &str) -> KsyDefinition {
        let mut def: KsyDefinition = serde_yaml::from_str(yaml).expect("valid test KSY YAML");
        def.compile_expressions();
        def
    }

    #[test]
    fn test_can_preeval_pos_instance_identifier_matching() {
        let def = parse_def(
            r#"
meta:
  id: ident_match_test
seq:
  - id: id
    type: u1
instances:
  hdr_val:
    value: 4
  safe_substr:
    pos: width_and_id + 1
    type: u1
  safe_literal_and_comment:
    pos: 0 # id should be ignored inside comment
    if: "'id' == 'id'"
    type: u1
  safe_member_id:
    pos: hdr_val.to_i
    type: u1
  unsafe_root_self:
    pos: _root.id
    type: u1
  unsafe_custom_io:
    pos: 0
    io: sub._io
    type: u1
"#,
        );
        let scope = TypeScope::root(&def.types, &def.enums);

        assert!(can_preeval_pos_instance(
            def.instances.get("safe_substr").unwrap(),
            &def.seq,
            &def.instances,
            scope,
            true
        ));
        assert!(can_preeval_pos_instance(
            def.instances.get("safe_literal_and_comment").unwrap(),
            &def.seq,
            &def.instances,
            scope,
            true
        ));
        assert!(can_preeval_pos_instance(
            def.instances.get("safe_member_id").unwrap(),
            &def.seq,
            &def.instances,
            scope,
            true
        ));
        assert!(!can_preeval_pos_instance(
            def.instances.get("unsafe_root_self").unwrap(),
            &def.seq,
            &def.instances,
            scope,
            true
        ));
        // Inside a subtype (`is_root == false`), `_root` is the already-available parent root.
        assert!(can_preeval_pos_instance(
            def.instances.get("unsafe_root_self").unwrap(),
            &def.seq,
            &def.instances,
            scope,
            false
        ));
        assert!(!can_preeval_pos_instance(
            def.instances.get("unsafe_custom_io").unwrap(),
            &def.seq,
            &def.instances,
            scope,
            true
        ));
    }

    #[test]
    fn test_can_preeval_pos_instance_dependencies_and_transitive_chains() {
        let def = parse_def(
            r#"
meta:
  id: dep_chain_test
types:
  sub_type:
    params:
      - id: p
    seq:
      - id: b
        type: u1
seq:
  - id: seq_field
    type: u1
instances:
  indep_pos:
    pos: 0
    type: u1
  val_leaf:
    value: seq_field + 1
  val_mid:
    value: val_leaf * 2
  val_cond_blocked:
    value: 5
    if: seq_field == 1
  via_chain:
    pos: val_mid
    type: u1
  via_cond_value:
    pos: val_cond_blocked
    type: u1
  via_size:
    pos: 0
    size: seq_field
  via_type_arg:
    pos: 0
    type: sub_type(seq_field)
  via_custom_type:
    pos: 0
    type: sub_type
  via_switch_on:
    pos: 0
    type:
      switch-on: seq_field
      cases:
        1: u1
        _: u2
  via_switch_case_expr:
    pos: 0
    type:
      switch-on: 1
      cases:
        seq_field: u1
        _: u2
  via_switch_case_custom_type:
    pos: 0
    type:
      switch-on: 1
      cases:
        1: sub_type
        _: u1
  safe_switch_quoted_case:
    pos: 0
    type:
      switch-on: "'a'"
      cases:
        "'seq_field'": u1
        _: u2
  via_repeat_expr:
    pos: 0
    type: u1
    repeat: expr
    repeat-expr: seq_field
  via_repeat_until:
    pos: 0
    type: u1
    repeat: until
    repeat-until: _ == seq_field
  via_condition:
    pos: 0
    type: u1
    if: seq_field > 0
  via_valid_min:
    pos: 0
    type: u1
    valid:
      min: seq_field
  via_valid_any_of:
    pos: 0
    type: u1
    valid:
      any-of:
        - seq_field
  via_process_str:
    pos: 0
    size: 2
    process: xor(seq_field)
  via_process_map:
    pos: 0
    size: 2
    process:
      algo: rol
      amount: seq_field
"#,
        );
        let scope = TypeScope::root(&def.types, &def.enums);

        assert!(can_preeval_pos_instance(
            def.instances.get("indep_pos").unwrap(),
            &def.seq,
            &def.instances,
            scope,
            true
        ));
        assert!(can_preeval_pos_instance(
            def.instances.get("safe_switch_quoted_case").unwrap(),
            &def.seq,
            &def.instances,
            scope,
            true
        ));
        assert!(!can_preeval_pos_instance(
            def.instances.get("val_leaf").unwrap(),
            &def.seq,
            &def.instances,
            scope,
            true
        ));

        for blocked_name in [
            "via_chain",
            "via_cond_value",
            "via_size",
            "via_type_arg",
            "via_custom_type",
            "via_switch_on",
            "via_switch_case_expr",
            "via_switch_case_custom_type",
            "via_repeat_expr",
            "via_repeat_until",
            "via_condition",
            "via_valid_min",
            "via_valid_any_of",
            "via_process_str",
            "via_process_map",
        ] {
            assert!(
                !can_preeval_pos_instance(def.instances.get(blocked_name).unwrap(), &def.seq, &def.instances, scope, true),
                "expected {blocked_name} to be blocked from pre-evaluation"
            );
        }
    }
}
