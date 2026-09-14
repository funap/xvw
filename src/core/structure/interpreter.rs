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
    color_index: usize,
    errors: std::cell::RefCell<Vec<ParseError>>,
    global_endian: String,
    field_count: usize,
    recursion_depth: usize,
    all_enums: HashMap<String, HashMap<String, String>>,
}

const MAX_RECURSION: usize = 64;
const MAX_PROGRESS_FIELDS: usize = 512;

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
    let map = val.as_mapping()?;
    let switch_on = map.get("switch-on")?.as_str()?.to_string();
    let cases_val = map.get("cases")?.as_mapping()?;
    let mut cases = HashMap::new();
    for (k, v) in cases_val {
        let key = match k {
            serde_yaml::Value::String(s) => s.clone(),
            serde_yaml::Value::Number(n) => n.to_string(),
            serde_yaml::Value::Bool(b) => b.to_string(),
            _ => continue,
        };
        if let Some(s) = v.as_str() {
            cases.insert(key.clone(), s.to_string());
            let trimmed = key.trim();
            if let Ok(int_val) = i64::from_str_radix(trimmed.trim_start_matches("0x").trim_start_matches("0X"), 16) {
                cases.insert(int_val.to_string(), s.to_string());
                cases.insert(format!("0x{:x}", int_val), s.to_string());
                cases.insert(format!("0x{:X}", int_val), s.to_string());
                cases.insert(format!("0x{:04x}", int_val), s.to_string());
                cases.insert(format!("0x{:04X}", int_val), s.to_string());
            }
            if let Ok(int_val) = trimmed.parse::<i64>() {
                cases.insert(int_val.to_string(), s.to_string());
                cases.insert(format!("0x{:x}", int_val), s.to_string());
                cases.insert(format!("0x{:X}", int_val), s.to_string());
                cases.insert(format!("0x{:04x}", int_val), s.to_string());
                cases.insert(format!("0x{:04X}", int_val), s.to_string());
            }
        }
    }
    Some((switch_on, cases))
}

/// Normalize enum definitions: support both simple string values and {id: ...} map values
fn normalize_enum(raw: &HashMap<String, serde_yaml::Value>) -> HashMap<String, String> {
    let mut result = HashMap::new();
    for (key, val) in raw {
        match val {
            serde_yaml::Value::String(s) => {
                result.insert(key.clone(), s.clone());
            }
            serde_yaml::Value::Mapping(m) => {
                if let Some(id_val) = m.get("id")
                    && let Some(s) = id_val.as_str()
                {
                    result.insert(key.clone(), s.to_string());
                }
            }
            _ => {}
        }
    }
    result
}

fn collect_enums(ksy: &KsyDefinition) -> HashMap<String, HashMap<String, String>> {
    let mut all = HashMap::new();
    for (name, raw) in &ksy.enums {
        all.insert(name.clone(), normalize_enum(raw));
    }

    let mut pending = vec![&ksy.types];
    while let Some(types) = pending.pop() {
        for type_def in types.values() {
            for (ename, raw) in &type_def.enums {
                all.insert(ename.clone(), normalize_enum(raw));
            }
            pending.push(&type_def.types);
        }
    }

    all
}

impl KaitaiInterpreter {
    pub fn new(ksy: KsyDefinition) -> Self {
        let global_endian = ksy.meta.endian.clone().unwrap_or_else(|| "le".to_string());
        let all_enums = collect_enums(&ksy);
        let mut ksy = ksy;
        ksy.compile_expressions();
        Self {
            ksy: std::sync::Arc::new(ksy),
            stream_size: 0,
            context: HashMap::new(),
            string_context: HashMap::new(),
            byte_arrays: HashMap::new(),
            id_stack: Vec::new(),
            color_index: 0,
            errors: std::cell::RefCell::new(Vec::new()),
            global_endian,
            field_count: 0,
            recursion_depth: 0,
            all_enums,
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
        let ksy_arc = self.ksy.clone();
        let def_id = ksy_arc.meta.id.clone();
        let total_bytes = self.stream_size;

        let mut fields = Vec::new();
        let root_scope = TypeScope::root(&ksy_arc.types, &ksy_arc.enums);

        let mut last_notify_time = std::time::Instant::now();
        let mut last_notify_offset = 0usize;
        let mut pending_fields = Vec::new();
        let mut index_builder = prepare_display_layout.then(StructureIndexBuilder::new);

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

        self.evaluate_value_instances_silent(&ksy_arc.instances, stream);

        for attr in &ksy_arc.seq {
            if let Some(token) = cancel_token
                && token.load(std::sync::atomic::Ordering::Relaxed)
            {
                break;
            }
            let parsed = self.parse_attr_repeated_cb_cancellable(attr, stream, root_scope, false, cancel_token, &mut |items, current_offset| {
                if let Some(field) = items.last() {
                    if let Some(index_builder) = index_builder.as_mut() {
                        index_builder.add_field(field);
                    }
                    if prepare_display_layout {
                        pending_fields.push(field.clone());
                    }
                }
                let now = std::time::Instant::now();
                if now.duration_since(last_notify_time).as_millis() >= 50
                    || current_offset.saturating_sub(last_notify_offset) >= 65536
                    || pending_fields.len() >= MAX_PROGRESS_FIELDS
                {
                    last_notify_time = now;
                    last_notify_offset = current_offset;

                    on_progress(&ParseProgress {
                        definition_id: def_id.clone(),
                        fields: into_field_chunk(std::mem::take(&mut pending_fields)),
                        parsed_offset: current_offset,
                        total_bytes,
                        is_done: false,
                        is_finalizing: false,
                        errors: Vec::new(),
                        parse_result: None,
                    });
                }
            });
            let should_parse = if let Some(cond) = &attr.condition {
                let ctx = self.make_eval_ctx(stream);
                if let Some(ref ast) = attr.compiled_condition {
                    ExprEvaluator::eval_ast_bool(ast, &ctx)
                } else {
                    ExprEvaluator::eval_bool(cond, &ctx)
                }
            } else {
                true
            };
            if should_parse && parsed.is_empty() && attr.repeat.is_none() {
                break;
            }
            fields.extend(parsed);
            self.evaluate_value_instances_silent(&ksy_arc.instances, stream);

            let cur_pos = stream.pos() as usize;
            if !pending_fields.is_empty() {
                last_notify_time = std::time::Instant::now();
                last_notify_offset = cur_pos;

                on_progress(&ParseProgress {
                    definition_id: def_id.clone(),
                    fields: into_field_chunk(std::mem::take(&mut pending_fields)),
                    parsed_offset: cur_pos,
                    total_bytes,
                    is_done: false,
                    is_finalizing: false,
                    errors: Vec::new(),
                    parse_result: None,
                });
            }
        }

        for (id, attr) in &ksy_arc.instances {
            if let Some(token) = cancel_token
                && token.load(std::sync::atomic::Ordering::Relaxed)
            {
                break;
            }
            if attr.pos.is_some() || attr.value.is_some() {
                let mut inst_attr = attr.clone();
                inst_attr.id = Some(id.clone());
                let parsed = self.parse_attr_repeated_cb_cancellable(&inst_attr, stream, root_scope, true, cancel_token, &mut |items, current_offset| {
                    if let Some(field) = items.last() {
                        if let Some(index_builder) = index_builder.as_mut() {
                            index_builder.add_field(field);
                        }
                        if prepare_display_layout {
                            pending_fields.push(field.clone());
                        }
                    }
                    let now = std::time::Instant::now();
                    if now.duration_since(last_notify_time).as_millis() >= 50
                        || current_offset.saturating_sub(last_notify_offset) >= 65536
                        || pending_fields.len() >= MAX_PROGRESS_FIELDS
                    {
                        last_notify_time = now;
                        last_notify_offset = current_offset;

                        on_progress(&ParseProgress {
                            definition_id: def_id.clone(),
                            fields: into_field_chunk(std::mem::take(&mut pending_fields)),
                            parsed_offset: current_offset,
                            total_bytes,
                            is_done: false,
                            is_finalizing: false,
                            errors: Vec::new(),
                            parse_result: None,
                        });
                    }
                });
                fields.extend(parsed);

                if !pending_fields.is_empty() {
                    last_notify_time = std::time::Instant::now();
                    last_notify_offset = stream.pos() as usize;
                    on_progress(&ParseProgress {
                        definition_id: def_id.clone(),
                        fields: into_field_chunk(std::mem::take(&mut pending_fields)),
                        parsed_offset: last_notify_offset,
                        total_bytes,
                        is_done: false,
                        is_finalizing: false,
                        errors: Vec::new(),
                        parse_result: None,
                    });
                }
            }
        }

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
            stream_eof: stream.is_eof(),
            stream_size: self.stream_size,
            stream_pos: stream.pos() as usize,
            enums: &self.all_enums,
            errors: None,
            instance_resolver: None,
        }
    }

    fn evaluate_value_instances_silent(&mut self, instances: &HashMap<String, KsyAttr>, stream: &KaitaiStream) {
        for (id, inst_attr) in instances {
            if inst_attr.value.is_some() && inst_attr.pos.is_none() {
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
                let val = rich_val.to_i64();
                self.context.insert(full_id.clone(), val);
                self.context.insert(id.clone(), val);
                match rich_val {
                    crate::core::structure::expression::ExprValue::Str(s) => {
                        self.string_context.insert(full_id.clone(), s.clone());
                        self.string_context.insert(id.clone(), s);
                    }
                    crate::core::structure::expression::ExprValue::Bytes(b) => {
                        self.byte_arrays.insert(full_id.clone(), b.clone());
                        self.byte_arrays.insert(id.clone(), b);
                    }
                    _ => {}
                }
            }
        }
    }

    fn parse_attr_repeated(&mut self, attr: &KsyAttr, stream: &mut KaitaiStream, scope: TypeScope, is_instance: bool) -> Vec<ParsedField> {
        self.parse_attr_repeated_cb_cancellable(attr, stream, scope, is_instance, None, &mut |_, _| {})
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_attr_repeated_cb_cancellable<F>(
        &mut self,
        attr: &KsyAttr,
        stream: &mut KaitaiStream,
        scope: TypeScope,
        is_instance: bool,
        cancel_token: Option<&std::sync::atomic::AtomicBool>,
        on_item: &mut F,
    ) -> Vec<ParsedField>
    where
        F: FnMut(&[ParsedField], usize),
    {
        if self.recursion_depth > MAX_RECURSION {
            return Vec::new();
        }
        let mut results = Vec::new();

        // Handle value instances (computed fields, no stream reading)
        if let Some(value_expr) = &attr.value {
            let ctx = self.make_eval_ctx(stream);
            let rich_val = if let Some(ref ast) = attr.compiled_value {
                ExprEvaluator::eval_ast_rich(ast, &ctx)
            } else {
                ExprEvaluator::evaluate_rich(value_expr, &ctx)
            };
            let val = rich_val.to_i64();
            let field_id = attr.id.clone().unwrap_or_else(|| format!("value_{}", self.field_count));
            let full_id = if self.id_stack.is_empty() {
                field_id.clone()
            } else {
                format!("{}.{}", self.id_stack.join("."), field_id)
            };
            self.context.insert(full_id.clone(), val);
            if let Some(ref raw_id) = attr.id {
                self.context.insert(raw_id.clone(), val);
            }
            match &rich_val {
                crate::core::structure::expression::ExprValue::Str(s) => {
                    self.string_context.insert(full_id.clone(), s.clone());
                    if let Some(ref raw_id) = attr.id {
                        self.string_context.insert(raw_id.clone(), s.clone());
                    }
                }
                crate::core::structure::expression::ExprValue::Bytes(b) => {
                    self.byte_arrays.insert(full_id.clone(), b.clone());
                    if let Some(ref raw_id) = attr.id {
                        self.byte_arrays.insert(raw_id.clone(), b.clone());
                    }
                }
                _ => {}
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
                };
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
                    enum_label: None,
                    is_instance: true,
                };
                results.push(field);
            }
            return results;
        }

        if let Some(repeat) = &attr.repeat {
            match repeat.as_str() {
                "expr" => {
                    if let Some(expr) = &attr.repeat_expr {
                        let count = if let Some(ref ast) = attr.compiled_repeat_expr {
                            self.resolve_count_ast(ast, stream)
                        } else {
                            self.resolve_count(expr, stream)
                        };
                        for i in 0..count {
                            if let Some(token) = cancel_token
                                && token.load(std::sync::atomic::Ordering::Relaxed)
                            {
                                break;
                            }
                            let pos_before = stream.pos();
                            if let Some(field) = self.parse_attr_once(attr, Some(i), stream, scope, is_instance) {
                                results.push(field);
                                on_item(&results, stream.pos() as usize);
                                if stream.pos() <= pos_before && attr.pos.is_none() {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
                "eos" => {
                    let mut i = 0;
                    while !stream.is_eof() {
                        if let Some(token) = cancel_token
                            && token.load(std::sync::atomic::Ordering::Relaxed)
                        {
                            break;
                        }
                        let pos_before = stream.pos();
                        if let Some(field) = self.parse_attr_once(attr, Some(i), stream, scope, is_instance) {
                            results.push(field);
                            on_item(&results, stream.pos() as usize);
                            if stream.pos() <= pos_before && attr.pos.is_none() {
                                break;
                            }
                        } else {
                            break;
                        }
                        i += 1;
                    }
                }
                "until" => {
                    if let Some(until) = &attr.repeat_until {
                        let mut i = 0;
                        while !stream.is_eof() {
                            if let Some(token) = cancel_token
                                && token.load(std::sync::atomic::Ordering::Relaxed)
                            {
                                break;
                            }
                            let pos_before = stream.pos();
                            if let Some(field) = self.parse_attr_once(attr, Some(i), stream, scope, is_instance) {
                                self.context.insert("_".to_string(), field.value.to_i64());
                                let full_underscore = if self.id_stack.is_empty() {
                                    "_".to_string()
                                } else {
                                    format!("{}._", self.id_stack.join("."))
                                };
                                self.context.insert(full_underscore, field.value.to_i64());
                                for child in &field.children {
                                    self.context.insert(format!("_.{}", child.id), child.value.to_i64());
                                    if let FieldValue::String(s) = &child.value {
                                        self.string_context.insert(format!("_.{}", child.id), s.clone());
                                    }
                                    if let FieldValue::Bytes(b) = &child.value {
                                        self.byte_arrays.insert(format!("_.{}", child.id), b.clone());
                                    }
                                }
                                results.push(field);
                                on_item(&results, stream.pos() as usize);
                                let ctx = self.make_eval_ctx(stream);
                                let until_holds = if let Some(ref ast) = attr.compiled_repeat_until {
                                    ExprEvaluator::eval_ast_bool(ast, &ctx)
                                } else {
                                    ExprEvaluator::eval_bool(until, &ctx)
                                };
                                if until_holds {
                                    break;
                                }
                                if stream.pos() <= pos_before && attr.pos.is_none() {
                                    break;
                                }
                            } else {
                                break;
                            }
                            i += 1;
                        }
                    }
                }
                _ => {}
            }
        } else if let Some(field) = self.parse_attr_once(attr, None, stream, scope, is_instance) {
            results.push(field);
            on_item(&results, stream.pos() as usize);
        }
        results
    }

    fn parse_attr_once(&mut self, attr: &KsyAttr, index: Option<usize>, stream: &mut KaitaiStream, scope: TypeScope, is_instance: bool) -> Option<ParsedField> {
        if self.recursion_depth > MAX_RECURSION {
            return None;
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
                return None;
            }
        }

        let start_offset = if attr.pos.is_some() {
            self.resolve_pos(attr, stream)?
        } else {
            stream.pos() as usize
        };

        let old_pos = if attr.pos.is_some() { Some(stream.pos()) } else { None };
        if attr.pos.is_some() {
            stream.set_pos(start_offset as u64);
        }

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
                    let switch_str = switch_val.to_string_val();
                    let switch_int = switch_val.to_i64();
                    let chosen = cases
                        .get(&switch_str)
                        .or_else(|| cases.get(&format!("\"{}\"", switch_str)))
                        .or_else(|| cases.get(&switch_int.to_string()))
                        .or_else(|| cases.get("_"));
                    chosen.map(|s| parse_type_with_args(s))
                } else if let Some((switch_on, cases)) = type_as_switch(v) {
                    let ctx = self.make_eval_ctx(stream);
                    let switch_val = ExprEvaluator::evaluate_rich(&switch_on, &ctx);
                    let switch_str = switch_val.to_string_val();
                    let switch_int = switch_val.to_i64();
                    let chosen = cases
                        .get(&switch_str)
                        .or_else(|| cases.get(&format!("\"{}\"", switch_str)))
                        .or_else(|| cases.get(&switch_int.to_string()))
                        .or_else(|| cases.get("_"));
                    chosen.map(|s| parse_type_with_args(s))
                } else {
                    None
                }
            })
            .unzip();

        let resolved_type = raw_type_str;
        let type_args = type_args.unwrap_or_default();

        let typed_result = if let Some(type_str) = &resolved_type {
            self.parse_typed_value(type_str, &type_args, is_little, computed_size, attr, start_offset, old_pos, stream, scope)
        } else if let Some(term) = Self::attr_terminator(attr) {
            let buf = stream.read_bytes_term(term, attr.include, attr.consume, attr.eos_error)?;
            let sz = if attr.consume { buf.len() + 1 } else { buf.len() };
            Some((FieldValue::Bytes(buf), sz, Vec::new()))
        } else if attr.size_eos {
            let buf = self.read_remaining(stream);
            let s = buf.len();
            Some((FieldValue::Bytes(buf), s, Vec::new()))
        } else if computed_size > 0 {
            stream.read_bytes(computed_size).map(|buf| (FieldValue::Bytes(buf), computed_size, Vec::new()))
        } else {
            Some((FieldValue::Bytes(Vec::new()), 0, Vec::new()))
        };

        // Restore position if we jumped
        if let Some(p) = old_pos {
            stream.set_pos(p);
        }

        let (mut value, final_size, children) = typed_result?;

        // Apply process transformation if present
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
            && !self.validate_value(valid_rule, &value, stream)
        {
            return None;
        }

        // Context update
        let field_id = if let Some(ref raw_id) = attr.id {
            let fid = if let Some(i) = index { format!("{}[{}]", raw_id, i) } else { raw_id.clone() };
            if self.id_stack.is_empty() {
                self.context.insert(fid.clone(), value.to_i64());
                if index.is_some() {
                    self.context.insert(raw_id.clone(), value.to_i64());
                }
                if let FieldValue::String(ref s) = value {
                    self.string_context.insert(fid.clone(), s.clone());
                    if index.is_some() {
                        self.string_context.insert(raw_id.clone(), s.clone());
                    }
                }
                if let FieldValue::Bytes(ref b) = value {
                    self.byte_arrays.insert(fid.clone(), b.clone());
                    if index.is_some() {
                        self.byte_arrays.insert(raw_id.clone(), b.clone());
                    }
                }
            } else {
                let stack_prefix = self.id_stack.join(".");
                let full_id = format!("{}.{}", stack_prefix, fid);
                let unindexed_id = format!("{}.{}", stack_prefix, raw_id);

                self.context.insert(full_id.clone(), value.to_i64());
                self.context.insert(unindexed_id.clone(), value.to_i64());
                self.context.insert(raw_id.clone(), value.to_i64());

                if let FieldValue::String(ref s) = value {
                    self.string_context.insert(full_id.clone(), s.clone());
                    self.string_context.insert(unindexed_id.clone(), s.clone());
                    self.string_context.insert(raw_id.clone(), s.clone());
                }
                if let FieldValue::Bytes(ref b) = value {
                    self.byte_arrays.insert(full_id, b.clone());
                    self.byte_arrays.insert(unindexed_id, b.clone());
                    self.byte_arrays.insert(raw_id.clone(), b.clone());
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
    }

    #[allow(clippy::too_many_arguments)]
    fn parse_typed_value(
        &mut self,
        type_str: &str,
        type_args: &[String],
        is_little: bool,
        size: usize,
        attr: &KsyAttr,
        start_offset: usize,
        old_pos: Option<u64>,
        stream: &mut KaitaiStream,
        scope: TypeScope,
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
                if let Some(term) = Self::attr_terminator(attr) {
                    let buf = stream.read_bytes_term(term, attr.include, attr.consume, attr.eos_error)?;
                    let read_sz = if attr.consume { buf.len() + 1 } else { buf.len() };
                    let s = self.decode_string(&buf, attr);
                    Some((FieldValue::String(s), read_sz, Vec::new()))
                } else {
                    let read_size = if attr.size_eos { self.remaining_bytes(stream) } else { size };
                    let buf = stream.read_bytes_slice(read_size)?;
                    let buf_trimmed = if let Some(pad) = Self::attr_pad_right(attr) {
                        let mut end = buf.len();
                        while end > 0 && buf[end - 1] == pad {
                            end -= 1;
                        }
                        &buf[..end]
                    } else {
                        buf
                    };
                    let s = self.decode_string(buf_trimmed, attr);
                    Some((FieldValue::String(s), read_size, Vec::new()))
                }
            }
            "strz" => {
                let term = Self::attr_terminator(attr).unwrap_or(0);
                let buf = stream.read_bytes_term(term, attr.include, attr.consume, attr.eos_error)?;
                let sz = if attr.consume { buf.len() + 1 } else { buf.len() };
                let s = self.decode_string(&buf, attr);
                Some((FieldValue::String(s), sz, Vec::new()))
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
                    (&t[1..], false) // default is BE
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
            custom => self.parse_custom_type(custom, type_args, attr, start_offset, old_pos, stream, scope),
        }
    }

    fn decode_string(&self, buf: &[u8], attr: &KsyAttr) -> String {
        if let Some(encoding_str) = &attr.encoding {
            crate::core::structure::expression::decode_bytes(buf, encoding_str)
        } else if let Ok(s) = std::str::from_utf8(buf) {
            s.to_string()
        } else {
            String::from_utf8_lossy(buf).into_owned()
        }
    }

    fn apply_process(&self, process_val: &serde_yaml::Value, data: &[u8], stream: &KaitaiStream) -> Option<Vec<u8>> {
        if let Some(s) = process_val.as_str() {
            if s == "zlib" {
                return process::zlib_decompress(data);
            }

            if s.starts_with("xor(") && s.ends_with(')') {
                let expr = &s[4..s.len() - 1];
                let ctx = self.make_eval_ctx(stream);
                let val = ExprEvaluator::eval_i64(expr, &ctx);
                return Some(process::xor_one(data, val as u8));
            }

            if s.starts_with("rol(") && s.ends_with(')') {
                let expr = &s[4..s.len() - 1];
                let ctx = self.make_eval_ctx(stream);
                let val = ExprEvaluator::eval_i64(expr, &ctx);
                return process::rotate_left(data, val as u32, 1).ok();
            }
        } else if let Some(map) = process_val.as_mapping() {
            let algo = map.get("algo")?.as_str()?;
            if algo == "xor" {
                let key_val = map.get("key")?;
                let ctx = self.make_eval_ctx(stream);
                if let Some(k_str) = key_val.as_str() {
                    let key = ExprEvaluator::eval_i64(k_str, &ctx);
                    return Some(process::xor_one(data, key as u8));
                } else if let Some(k_int) = key_val.as_i64() {
                    return Some(process::xor_one(data, k_int as u8));
                }
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
        attr: &KsyAttr,
        start_offset: usize,
        _old_pos: Option<u64>,
        stream: &mut KaitaiStream,
        scope: TypeScope,
    ) -> Option<(FieldValue, usize, Vec<ParsedField>)> {
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
        let field_id = attr.id.clone().unwrap_or_else(|| format!("field_{}", self.field_count));
        self.id_stack.push(field_id);
        self.recursion_depth += 1;

        // Bind parameters from type_def.params using type_args evaluated in caller's context
        for (i, param) in type_def.params.iter().enumerate() {
            if let Some(arg_expr) = type_args.get(i) {
                let ctx = self.make_eval_ctx(stream);
                let val = ExprEvaluator::eval_i64(arg_expr, &ctx);
                let scoped_param = if self.id_stack.is_empty() {
                    param.id.clone()
                } else {
                    format!("{}.{}", self.id_stack.join("."), param.id)
                };
                self.context.insert(scoped_param, val);
                self.context.insert(param.id.clone(), val);
            }
        }

        let nested_scope = scope.child(&type_def.types, &type_def.enums);
        for (k, v) in &type_def.enums {
            self.all_enums.insert(k.clone(), normalize_enum(v));
        }

        let size_val = self.resolve_size_attr(attr, stream);

        // If size is explicitly 0, return an empty struct immediately (consume nothing).
        if size_val == Some(0) {
            self.recursion_depth -= 1;
            self.id_stack.pop();
            return Some((FieldValue::Struct, 0, Vec::new()));
        }

        let use_substream = (size_val.is_some() && size_val != Some(0)) || attr.size_eos;

        let res = (|| {
            if use_substream {
                let sub_size = if attr.size_eos { self.remaining_bytes(stream) } else { size_val.unwrap_or(0) };
                // Construct a bounded nested stream over the original bytes.
                // The parent cursor is advanced by `take_substream`, so this
                // avoids copying every size-bounded custom type.
                let mut sub_stream = stream.take_substream(sub_size)?;

                let old_stream_size = self.stream_size;
                self.stream_size = sub_size;
                let mut fields = Vec::new();

                // Pre-evaluate pos-based instances before seq (e.g. byte0 at pos: ofs)
                let errors_before_sub = self.errors.borrow().len();
                for (id, inst_attr) in &type_def.instances {
                    if inst_attr.pos.is_some() && inst_attr.value.is_none() {
                        let mut inst_copy = inst_attr.clone();
                        inst_copy.id = Some(id.clone());
                        self.parse_attr_repeated(&inst_copy, &mut sub_stream, nested_scope, true);
                    }
                }
                self.errors.borrow_mut().truncate(errors_before_sub);

                // Pre-evaluate value instances without recording errors (in case seq sizes depend on them)
                self.evaluate_value_instances_silent(&type_def.instances, &sub_stream);

                for nested_attr in &type_def.seq {
                    let parsed = self.parse_attr_repeated(nested_attr, &mut sub_stream, nested_scope, false);
                    let should_parse = if let Some(cond) = &nested_attr.condition {
                        let ctx = self.make_eval_ctx(&sub_stream);
                        if let Some(ref ast) = nested_attr.compiled_condition {
                            ExprEvaluator::eval_ast_bool(ast, &ctx)
                        } else {
                            ExprEvaluator::eval_bool(cond, &ctx)
                        }
                    } else {
                        true
                    };
                    if should_parse && parsed.is_empty() && nested_attr.repeat.is_none() {
                        return None;
                    }
                    fields.extend(parsed);
                    self.evaluate_value_instances_silent(&type_def.instances, &sub_stream);
                }

                // Evaluate instances after seq so that instances can refer to parsed seq attributes
                for (id, inst_attr) in &type_def.instances {
                    if inst_attr.pos.is_some() || inst_attr.value.is_some() {
                        let mut inst_copy = inst_attr.clone();
                        inst_copy.id = Some(id.clone());
                        fields.extend(self.parse_attr_repeated(&inst_copy, &mut sub_stream, nested_scope, true));
                    }
                }

                let new_pos = (start_offset + sub_size) as u64;
                stream.set_pos(new_pos);
                self.stream_size = old_stream_size;
                // Adjust offsets from substream-relative to absolute file offsets
                let mut fields_adj = fields;
                let mut pending = fields_adj.iter_mut().map(|field| (field, start_offset)).collect::<Vec<_>>();
                while let Some((field, base)) = pending.pop() {
                    field.offset += base;
                    for child in &mut field.children {
                        pending.push((child, base));
                    }
                }
                Some(fields_adj)
            } else {
                // Pre-evaluate pos-based instances before seq
                let errors_before = self.errors.borrow().len();
                for (id, inst_attr) in &type_def.instances {
                    if inst_attr.pos.is_some() && inst_attr.value.is_none() {
                        let mut inst_copy = inst_attr.clone();
                        inst_copy.id = Some(id.clone());
                        self.parse_attr_repeated(&inst_copy, stream, nested_scope, true);
                    }
                }
                self.errors.borrow_mut().truncate(errors_before);

                // Pre-evaluate value instances without recording errors (in case seq sizes depend on them)
                self.evaluate_value_instances_silent(&type_def.instances, stream);

                let mut fields = Vec::new();
                for nested_attr in &type_def.seq {
                    let parsed = self.parse_attr_repeated(nested_attr, stream, nested_scope, false);
                    let should_parse = if let Some(cond) = &nested_attr.condition {
                        let ctx = self.make_eval_ctx(stream);
                        if let Some(ref ast) = nested_attr.compiled_condition {
                            ExprEvaluator::eval_ast_bool(ast, &ctx)
                        } else {
                            ExprEvaluator::eval_bool(cond, &ctx)
                        }
                    } else {
                        true
                    };
                    if should_parse && parsed.is_empty() && nested_attr.repeat.is_none() {
                        return None;
                    }
                    fields.extend(parsed);
                    self.evaluate_value_instances_silent(&type_def.instances, stream);
                }

                // Evaluate instances after seq so that instances can refer to parsed seq attributes
                for (id, inst_attr) in &type_def.instances {
                    if inst_attr.pos.is_some() || inst_attr.value.is_some() {
                        let mut inst_copy = inst_attr.clone();
                        inst_copy.id = Some(id.clone());
                        fields.extend(self.parse_attr_repeated(&inst_copy, stream, nested_scope, true));
                    }
                }
                Some(fields)
            }
        })();

        self.recursion_depth -= 1;
        self.id_stack.pop();

        let nested_fields = res?;
        let current_pos = stream.pos() as usize;
        let total_size = current_pos.saturating_sub(start_offset);

        Some((FieldValue::Struct, total_size, nested_fields))
    }

    fn resolve_enum_label(&self, attr: &KsyAttr, value: &FieldValue, scope: TypeScope) -> Option<String> {
        let enum_name = attr.enum_ref.as_ref()?;
        let enum_def = scope.get_enum(enum_name).or_else(|| self.ksy.enums.get(enum_name))?;
        let key = value.to_i64().to_string();
        let val = enum_def.get(&key)?;
        match val {
            serde_yaml::Value::String(s) => Some(s.clone()),
            serde_yaml::Value::Mapping(m) => m.get("id")?.as_str().map(|s| s.to_string()),
            _ => None,
        }
    }

    fn validate_contents(&mut self, expected: &serde_yaml::Value, actual: &FieldValue, stream: &KaitaiStream) -> bool {
        let mut expected_bytes = Vec::new();
        if let Some(arr) = expected.as_sequence() {
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
            expected_bytes.extend(s.as_bytes());
        }
        if !expected_bytes.is_empty() {
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

    fn validate_value(&mut self, valid: &serde_yaml::Value, actual: &FieldValue, stream: &KaitaiStream) -> bool {
        let offset = stream.pos() as usize;
        let mut ok = true;
        match valid {
            serde_yaml::Value::Mapping(map) => {
                if let Some(eq_val) = map.get("eq")
                    && !Self::check_val_eq(eq_val, actual)
                {
                    self.errors.borrow_mut().push(ParseError {
                        message: format!("validation failed: value {:?} does not equal expected {:?}", actual, eq_val),
                        offset,
                    });
                    ok = false;
                }
                if let Some(min_val) = map.get("min")
                    && let Some(min_n) = min_val.as_i64()
                    && actual.to_i64() < min_n
                {
                    self.errors.borrow_mut().push(ParseError {
                        message: format!("validation failed: value {} is less than min {}", actual.to_i64(), min_n),
                        offset,
                    });
                    ok = false;
                }
                if let Some(max_val) = map.get("max")
                    && let Some(max_n) = max_val.as_i64()
                    && actual.to_i64() > max_n
                {
                    self.errors.borrow_mut().push(ParseError {
                        message: format!("validation failed: value {} is greater than max {}", actual.to_i64(), max_n),
                        offset,
                    });
                    ok = false;
                }
                if let Some(any_of) = map.get("any-of").and_then(|v| v.as_sequence()) {
                    let matched = any_of.iter().any(|expected| Self::check_val_eq(expected, actual));
                    if !matched {
                        self.errors.borrow_mut().push(ParseError {
                            message: format!("validation failed: value {:?} not in any-of set", actual),
                            offset,
                        });
                        ok = false;
                    }
                }
                if let Some(expr_val) = map.get("expr").and_then(|v| v.as_str()) {
                    let full_underscore_id = if self.id_stack.is_empty() {
                        "_".to_string()
                    } else {
                        format!("{}.{}", self.id_stack.join("."), "_")
                    };
                    self.context.insert("_".to_string(), actual.to_i64());
                    self.context.insert(full_underscore_id.clone(), actual.to_i64());
                    if let FieldValue::Bytes(b) = actual {
                        self.byte_arrays.insert("_".to_string(), b.clone());
                        self.byte_arrays.insert(full_underscore_id.clone(), b.clone());
                    }
                    if let FieldValue::String(s) = actual {
                        self.string_context.insert("_".to_string(), s.clone());
                        self.string_context.insert(full_underscore_id.clone(), s.clone());
                    }
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
            serde_yaml::Value::Number(_) | serde_yaml::Value::String(_) if !Self::check_val_eq(valid, actual) => {
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

    fn check_val_eq(expected: &serde_yaml::Value, actual: &FieldValue) -> bool {
        match expected {
            serde_yaml::Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    actual.to_i64() == i
                } else if let Some(u) = n.as_u64() {
                    actual.to_i64() as u64 == u
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
                actual.to_string_value() == *s
            }
            _ => false,
        }
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

    fn read_remaining(&self, stream: &mut KaitaiStream) -> Vec<u8> {
        stream.read_bytes_remaining().unwrap_or_default()
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
