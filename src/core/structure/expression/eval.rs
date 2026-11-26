use super::ast::{BinaryOp, ExprAST, UnaryOp};
use super::encoding::decode_bytes;
use std::collections::HashMap;

pub struct ExprEvaluator;

pub type InstanceResolver<'a> = dyn Fn(&str) -> Option<i64> + 'a;

/// Context for expression evaluation, providing access to parsed field values,
/// stream state, and enum definitions.
pub struct EvalContext<'a> {
    pub values: &'a HashMap<String, i64>,
    pub string_values: &'a HashMap<String, String>,
    pub byte_arrays: &'a HashMap<String, Vec<u8>>,
    pub base_path: &'a [String],
    pub active_io_prefixes: &'a [String],
    pub stream_eof: bool,
    pub stream_size: usize,
    pub stream_pos: usize,
    pub enums: &'a HashMap<String, HashMap<String, String>>,
    pub errors: Option<&'a std::cell::RefCell<Vec<crate::core::structure::types::ParseError>>>,
    pub instance_resolver: Option<&'a InstanceResolver<'a>>,
}

impl<'a> EvalContext<'a> {
    pub fn simple(values: &'a HashMap<String, i64>, base_path: &'a [String]) -> Self {
        let empty_strings = &G_EMPTY_STRING_MAP;
        let empty_enums = &G_EMPTY_ENUM_MAP;
        let empty_bytes = &G_EMPTY_BYTE_MAP;
        Self {
            values,
            string_values: empty_strings,
            byte_arrays: empty_bytes,
            base_path,
            active_io_prefixes: &[],
            stream_eof: false,
            stream_size: 0,
            stream_pos: 0,
            enums: empty_enums,
            errors: None,
            instance_resolver: None,
        }
    }
}

pub static G_EMPTY_STRING_MAP: std::sync::LazyLock<HashMap<String, String>> = std::sync::LazyLock::new(HashMap::new);
pub static G_EMPTY_ENUM_MAP: std::sync::LazyLock<HashMap<String, HashMap<String, String>>> = std::sync::LazyLock::new(HashMap::new);
pub static G_EMPTY_BYTE_MAP: std::sync::LazyLock<HashMap<String, Vec<u8>>> = std::sync::LazyLock::new(HashMap::new);

/// Expression value - can be integer, float, string, bool, byte array, or general array
#[derive(Debug, Clone, PartialEq)]
pub enum ExprValue {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Bytes(Vec<u8>),
    Array(Vec<ExprValue>),
}

impl ExprValue {
    pub fn to_i64(&self) -> i64 {
        match self {
            ExprValue::Int(v) => *v,
            ExprValue::Float(v) => *v as i64,
            ExprValue::Str(_) => 0,
            ExprValue::Bool(v) => {
                if *v {
                    1
                } else {
                    0
                }
            }
            ExprValue::Bytes(_) | ExprValue::Array(_) => 0,
        }
    }
    pub fn to_bool(&self) -> bool {
        match self {
            ExprValue::Int(v) => *v != 0,
            ExprValue::Float(v) => *v != 0.0,
            ExprValue::Str(s) => !s.is_empty(),
            ExprValue::Bool(v) => *v,
            ExprValue::Bytes(b) => !b.is_empty(),
            ExprValue::Array(a) => !a.is_empty(),
        }
    }
    pub fn to_string_val(&self) -> String {
        match self {
            ExprValue::Int(v) => v.to_string(),
            ExprValue::Float(v) => v.to_string(),
            ExprValue::Str(s) => s.clone(),
            ExprValue::Bool(v) => v.to_string(),
            ExprValue::Bytes(b) => String::from_utf8_lossy(b).to_string(),
            ExprValue::Array(a) => {
                let parts: Vec<String> = a.iter().map(|v| v.to_string_val()).collect();
                format!("[{}]", parts.join(", "))
            }
        }
    }
    pub fn is_float(&self) -> bool {
        matches!(self, ExprValue::Float(_))
    }
    pub fn to_f64(&self) -> f64 {
        match self {
            ExprValue::Int(v) => *v as f64,
            ExprValue::Float(v) => *v,
            ExprValue::Str(_) => 0.0,
            ExprValue::Bool(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            ExprValue::Bytes(_) | ExprValue::Array(_) => 0.0,
        }
    }
    /// Coerces a byte array or an array of byte integers `[0x41, 0x42]` to a `Vec<u8>`.
    pub fn as_bytes(&self) -> Option<Vec<u8>> {
        match self {
            ExprValue::Bytes(b) => Some(b.clone()),
            ExprValue::Array(items) => {
                let mut bytes = Vec::with_capacity(items.len());
                for item in items {
                    match item {
                        ExprValue::Int(n) => bytes.push(*n as u8),
                        _ => return None,
                    }
                }
                Some(bytes)
            }
            _ => None,
        }
    }
}

impl ExprEvaluator {
    /// Legacy evaluate - returns i64
    pub fn evaluate(expr: &str, context: &HashMap<String, i64>, base_path: &[String]) -> i64 {
        let ctx = EvalContext::simple(context, base_path);
        Self::evaluate_rich(expr, &ctx).to_i64()
    }

    /// Legacy evaluate_bool
    pub fn evaluate_bool(expr: &str, context: &HashMap<String, i64>, base_path: &[String]) -> bool {
        let ctx = EvalContext::simple(context, base_path);
        Self::evaluate_rich(expr, &ctx).to_bool()
    }

    /// Rich evaluation returning ExprValue
    pub fn evaluate_rich(expr: &str, ctx: &EvalContext) -> ExprValue {
        if let Some(ast) = ExprAST::compile(expr) {
            ast.eval(ctx)
        } else {
            if let Some(err_cell) = ctx.errors {
                err_cell.borrow_mut().push(crate::core::structure::types::ParseError {
                    message: "Empty expression".to_string(),
                    offset: ctx.stream_pos,
                });
            }
            ExprValue::Int(0)
        }
    }

    /// Evaluate with full context, return i64
    pub fn eval_i64(expr: &str, ctx: &EvalContext) -> i64 {
        Self::evaluate_rich(expr, ctx).to_i64()
    }

    /// Evaluate with full context, return bool
    pub fn eval_bool(expr: &str, ctx: &EvalContext) -> bool {
        Self::evaluate_rich(expr, ctx).to_bool()
    }

    /// Evaluate with full context, return string
    pub fn eval_string(expr: &str, ctx: &EvalContext) -> String {
        Self::evaluate_rich(expr, ctx).to_string_val()
    }

    /// Evaluate pre-compiled AST returning ExprValue
    pub fn eval_ast_rich(ast: &ExprAST, ctx: &EvalContext) -> ExprValue {
        ast.eval(ctx)
    }

    /// Evaluate pre-compiled AST returning i64
    pub fn eval_ast_i64(ast: &ExprAST, ctx: &EvalContext) -> i64 {
        ast.eval(ctx).to_i64()
    }

    /// Evaluate pre-compiled AST returning bool
    pub fn eval_ast_bool(ast: &ExprAST, ctx: &EvalContext) -> bool {
        ast.eval(ctx).to_bool()
    }

    /// Evaluate pre-compiled AST returning string
    pub fn eval_ast_string(ast: &ExprAST, ctx: &EvalContext) -> String {
        ast.eval(ctx).to_string_val()
    }
}

#[derive(Debug, Clone)]
enum PathSegment {
    Ident(String),
    Index(i64),
}

pub(super) fn parse_str_to_int(s: &str, radix: u32) -> i64 {
    let cleaned: String = s.trim().chars().filter(|&c| c != '_').collect();
    if cleaned.is_empty() {
        return 0;
    }
    let (is_neg, rest) = if let Some(r) = cleaned.strip_prefix('-') {
        (true, r)
    } else if let Some(r) = cleaned.strip_prefix('+') {
        (false, r)
    } else {
        (false, cleaned.as_str())
    };
    let parsed = if (rest.starts_with("0x") || rest.starts_with("0X")) && (radix == 16 || radix == 10) {
        u64::from_str_radix(&rest[2..], 16)
    } else if (rest.starts_with("0b") || rest.starts_with("0B")) && (radix == 2 || radix == 10) {
        u64::from_str_radix(&rest[2..], 2)
    } else if (rest.starts_with("0o") || rest.starts_with("0O")) && (radix == 8 || radix == 10) {
        u64::from_str_radix(&rest[2..], 8)
    } else {
        u64::from_str_radix(rest, radix.clamp(2, 36))
    };
    match parsed {
        Ok(v) => {
            let v = v as i64;
            if is_neg { v.wrapping_neg() } else { v }
        }
        Err(_) => 0,
    }
}

fn eval_array_min(items: &[ExprValue]) -> Option<ExprValue> {
    let first = items.first()?;
    match first {
        ExprValue::Str(_) => items.iter().map(|v| v.to_string_val()).min().map(ExprValue::Str),
        ExprValue::Float(_) => items.iter().map(|v| v.to_f64()).reduce(f64::min).map(ExprValue::Float),
        _ => items.iter().map(|v| v.to_i64()).min().map(ExprValue::Int),
    }
}

fn eval_array_max(items: &[ExprValue]) -> Option<ExprValue> {
    let first = items.first()?;
    match first {
        ExprValue::Str(_) => items.iter().map(|v| v.to_string_val()).max().map(ExprValue::Str),
        ExprValue::Float(_) => items.iter().map(|v| v.to_f64()).reduce(f64::max).map(ExprValue::Float),
        _ => items.iter().map(|v| v.to_i64()).max().map(ExprValue::Int),
    }
}

impl ExprAST {
    fn lookup_ctx_path(path: &str, ctx: &EvalContext) -> Option<ExprValue> {
        if path.is_empty() {
            return None;
        }
        if let Some(&bits) = ctx.values.get(&format!("__float__:{path}")) {
            return Some(ExprValue::Float(f64::from_bits(bits as u64)));
        }
        if let Some(&b) = ctx.values.get(&format!("__bool__:{path}")) {
            return Some(ExprValue::Bool(b != 0));
        }
        if let Some(val) = ctx.string_values.get(path) {
            return Some(ExprValue::Str(val.clone()));
        }
        if let Some(val) = ctx.byte_arrays.get(path) {
            return Some(ExprValue::Bytes(val.clone()));
        }
        if let Some(&val) = ctx.values.get(path) {
            return Some(ExprValue::Int(val));
        }
        None
    }

    fn eval_builtin_property(base_val: ExprValue, member: &str) -> Option<ExprValue> {
        match member {
            "to_s" => match base_val {
                ExprValue::Array(_) => {
                    if let Some(bytes) = base_val.as_bytes() {
                        Some(ExprValue::Str(String::from_utf8_lossy(&bytes).into_owned()))
                    } else {
                        Some(ExprValue::Str(base_val.to_string_val()))
                    }
                }
                other => Some(ExprValue::Str(other.to_string_val())),
            },
            "to_i" => match base_val {
                ExprValue::Str(ref s) => Some(ExprValue::Int(parse_str_to_int(s, 10))),
                other => Some(ExprValue::Int(other.to_i64())),
            },
            "length" | "size" => match base_val {
                ExprValue::Bytes(ref b) => Some(ExprValue::Int(b.len() as i64)),
                ExprValue::Str(ref s) => Some(ExprValue::Int(s.chars().count() as i64)),
                ExprValue::Array(ref a) => Some(ExprValue::Int(a.len() as i64)),
                _ => None,
            },
            "reverse" => match base_val {
                ExprValue::Str(ref s) => Some(ExprValue::Str(s.chars().rev().collect())),
                ExprValue::Bytes(ref b) => Some(ExprValue::Bytes(b.iter().copied().rev().collect())),
                ExprValue::Array(ref a) => Some(ExprValue::Array(a.iter().cloned().rev().collect())),
                _ => None,
            },
            "first" => match base_val {
                ExprValue::Bytes(ref b) => Some(ExprValue::Int(b.first().copied().unwrap_or(0) as i64)),
                ExprValue::Array(ref a) => Some(a.first().cloned().unwrap_or(ExprValue::Int(0))),
                _ => None,
            },
            "last" => match base_val {
                ExprValue::Bytes(ref b) => Some(ExprValue::Int(b.last().copied().unwrap_or(0) as i64)),
                ExprValue::Array(ref a) => Some(a.last().cloned().unwrap_or(ExprValue::Int(0))),
                _ => None,
            },
            "min" => match base_val {
                ExprValue::Bytes(ref b) => Some(ExprValue::Int(b.iter().copied().min().unwrap_or(0) as i64)),
                ExprValue::Array(ref a) => eval_array_min(a),
                _ => None,
            },
            "max" => match base_val {
                ExprValue::Bytes(ref b) => Some(ExprValue::Int(b.iter().copied().max().unwrap_or(0) as i64)),
                ExprValue::Array(ref a) => eval_array_max(a),
                _ => None,
            },
            _ => None,
        }
    }

    fn extract_path_segments(&self, ctx: &EvalContext) -> Option<Vec<PathSegment>> {
        let mut segments = Vec::new();
        let mut curr = self;
        loop {
            match curr {
                ExprAST::Identifier(id) => {
                    segments.push(PathSegment::Ident(id.clone()));
                    break;
                }
                ExprAST::MemberAccess { base, member, is_enum } => {
                    if *is_enum {
                        return None;
                    }
                    segments.push(PathSegment::Ident(member.clone()));
                    curr = base;
                }
                ExprAST::IndexAccess { base, index } => {
                    let idx = index.eval(ctx).to_i64();
                    segments.push(PathSegment::Index(idx));
                    curr = base;
                }
                _ => return None,
            }
        }
        segments.reverse();
        Some(segments)
    }

    fn apply_segments_inner(
        mut stack: Vec<String>,
        segments: &[PathSegment],
        resolve_first_last: bool,
        resolve_terminal_first_last: bool,
        reject_parent_underflow: bool,
        ctx: &EvalContext,
    ) -> Option<String> {
        for (idx, seg) in segments.iter().enumerate() {
            match seg {
                PathSegment::Ident(name) => {
                    if name == "_root" {
                        stack.clear();
                    } else if name == "_parent" {
                        if stack.pop().is_none() && reject_parent_underflow {
                            return None;
                        }
                    } else if name == "_" && idx == 0 {
                        stack.push("_".to_string());
                    } else if resolve_first_last
                        && (name == "first" || name == "last")
                        && !stack.is_empty()
                        && (resolve_terminal_first_last || idx + 1 < segments.len())
                    {
                        let current_prefix = stack.join(".");
                        let last_ident = stack.last().cloned().unwrap_or_default();
                        if name == "first" {
                            if let Some(last) = stack.last_mut() {
                                *last = format!("{last}[0]");
                            }
                        } else if let Some(&len) = ctx
                            .values
                            .get(&format!("__array_len__:{current_prefix}"))
                            .or_else(|| ctx.values.get(&format!("__array_len__:{last_ident}")))
                            && len > 0
                        {
                            if let Some(last) = stack.last_mut() {
                                *last = format!("{last}[{}]", len - 1);
                            }
                        } else {
                            stack.push(name.clone());
                        }
                    } else {
                        stack.push(name.clone());
                    }
                }
                PathSegment::Index(i) => {
                    if let Some(last) = stack.last_mut() {
                        *last = format!("{last}[{i}]");
                    }
                }
            }
        }
        Some(stack.join("."))
    }

    fn apply_segments(stack: Vec<String>, segments: &[PathSegment], resolve_first_last: bool, ctx: &EvalContext) -> String {
        Self::apply_segments_inner(stack, segments, resolve_first_last, false, false, ctx).unwrap_or_default()
    }

    fn collect_candidate_paths(&self, ctx: &EvalContext) -> Option<Vec<String>> {
        let segments = self.extract_path_segments(ctx)?;
        let root_id = match segments.first()? {
            PathSegment::Ident(s) => s.as_str(),
            _ => return None,
        };

        let mut paths = Vec::new();
        let mut push_unique = |s: String| {
            if !s.is_empty() && !paths.contains(&s) {
                paths.push(s);
            }
        };

        // 1. Walk from full base_path down to empty root
        for i in (0..=ctx.base_path.len()).rev() {
            let base = ctx.base_path[..i].to_vec();
            push_unique(Self::apply_segments(base.clone(), &segments, false, ctx));
            push_unique(Self::apply_segments(base, &segments, true, ctx));
        }

        // 2. If root_id matches an element in base_path (e.g. "header" or "sections[0]" when root_id is "sections")
        if let Some(idx) = ctx.base_path.iter().rposition(|x| {
            let base_name = x.split('[').next().unwrap_or(x);
            base_name == root_id
        }) {
            let mut replaced = segments.clone();
            replaced[0] = PathSegment::Ident(ctx.base_path[idx].clone());
            let base = ctx.base_path[..idx].to_vec();
            push_unique(Self::apply_segments(base.clone(), &replaced, false, ctx));
            push_unique(Self::apply_segments(base, &replaced, true, ctx));
        }

        Some(paths)
    }

    fn resolve_io_owner(&self, ctx: &EvalContext) -> Option<(bool, Vec<String>)> {
        let segments = self.extract_path_segments(ctx)?;
        let root_id = match segments.first()? {
            PathSegment::Ident(s) => s.as_str(),
            _ => return None,
        };

        if root_id == "_root" || root_id == "_parent" {
            let base = ctx.base_path.to_vec();
            let mut paths = Vec::new();
            let mut reaches_root = false;
            if let Some(resolved) = Self::apply_segments_inner(base.clone(), &segments, true, true, true, ctx) {
                if resolved.is_empty() {
                    reaches_root = true;
                } else {
                    paths.push(resolved);
                }
            }
            if let Some(resolved_raw) = Self::apply_segments_inner(base, &segments, false, false, true, ctx) {
                if resolved_raw.is_empty() {
                    reaches_root = true;
                } else if !paths.contains(&resolved_raw) {
                    paths.push(resolved_raw);
                }
            }
            return Some((reaches_root, paths));
        }

        let mut paths = Vec::new();
        let mut push_unique = |s: Option<String>| {
            if let Some(s) = s
                && !s.is_empty()
                && !paths.contains(&s)
            {
                paths.push(s);
            }
        };

        for i in (0..=ctx.base_path.len()).rev() {
            let base = ctx.base_path[..i].to_vec();
            push_unique(Self::apply_segments_inner(base.clone(), &segments, true, true, false, ctx));
            push_unique(Self::apply_segments_inner(base, &segments, false, false, false, ctx));
        }

        if let Some(idx) = ctx.base_path.iter().rposition(|x| {
            let base_name = x.split('[').next().unwrap_or(x);
            base_name == root_id
        }) {
            let mut replaced = segments.clone();
            replaced[0] = PathSegment::Ident(ctx.base_path[idx].clone());
            let base = ctx.base_path[..idx].to_vec();
            push_unique(Self::apply_segments_inner(base.clone(), &replaced, true, true, false, ctx));
            push_unique(Self::apply_segments_inner(base, &replaced, false, false, false, ctx));
        }

        Some((false, paths))
    }

    fn extract_enum_path(base: &ExprAST) -> Option<String> {
        let mut parts = Vec::new();
        let mut curr = base;
        loop {
            match curr {
                ExprAST::Identifier(name) => {
                    parts.push(name.as_str());
                    break;
                }
                ExprAST::MemberAccess {
                    base: inner,
                    member,
                    is_enum: true,
                } => {
                    parts.push(member.as_str());
                    curr = inner;
                }
                _ => return None,
            }
        }
        parts.reverse();
        Some(parts.join("::"))
    }

    fn format_expr_path(&self) -> Option<String> {
        match self {
            ExprAST::Identifier(id) => Some(id.clone()),
            ExprAST::MemberAccess { base, member, is_enum: false } => Some(format!("{}.{}", base.format_expr_path()?, member)),
            ExprAST::IndexAccess { base, index } => {
                if let ExprAST::Number(idx) = &**index {
                    Some(format!("{}[{}]", base.format_expr_path()?, idx))
                } else {
                    Some(format!("{}[?]", base.format_expr_path()?))
                }
            }
            _ => None,
        }
    }

    pub fn eval(&self, ctx: &EvalContext) -> ExprValue {
        match self {
            ExprAST::Number(n) => ExprValue::Int(*n),
            ExprAST::Float(f) => ExprValue::Float(*f),
            ExprAST::Str(s) => ExprValue::Str(s.clone()),
            ExprAST::Bool(b) => ExprValue::Bool(*b),
            ExprAST::Array(items) => ExprValue::Array(items.iter().map(|item| item.eval(ctx)).collect()),
            ExprAST::Identifier(id) => self.eval_identifier(id, ctx),
            ExprAST::MemberAccess { base, member, is_enum } => self.eval_member_access(base, member, *is_enum, ctx),
            ExprAST::IndexAccess { base, index } => {
                let idx_i64 = index.eval(ctx).to_i64();
                if idx_i64 < 0 {
                    return ExprValue::Int(0);
                }
                let idx_val = idx_i64 as usize;

                // First check if `base[idx]` is a repeated field element in context
                if let Some(candidates) = self.collect_candidate_paths(ctx) {
                    for path in &candidates {
                        if let Some(val) = Self::lookup_ctx_path(path, ctx) {
                            return val;
                        }
                    }
                }

                let base_val = base.eval(ctx);
                match base_val {
                    ExprValue::Array(ref arr) => arr.get(idx_val).cloned().unwrap_or(ExprValue::Int(0)),
                    ExprValue::Bytes(ref b) => ExprValue::Int(b.get(idx_val).copied().unwrap_or(0) as i64),
                    ExprValue::Str(ref s) => ExprValue::Int(s.as_bytes().get(idx_val).copied().unwrap_or(0) as i64),
                    _ => ExprValue::Int(0),
                }
            }
            ExprAST::MethodCall { base, method, args } => {
                let base_val = base.eval(ctx);
                match method.as_str() {
                    "to_s" => {
                        let encoding = args.first().map(|a| a.eval(ctx).to_string_val()).unwrap_or_else(|| "UTF-8".to_string());
                        match base_val {
                            ExprValue::Bytes(ref b) => ExprValue::Str(decode_bytes(b, &encoding)),
                            ExprValue::Array(_) => {
                                if let Some(b) = base_val.as_bytes() {
                                    ExprValue::Str(decode_bytes(&b, &encoding))
                                } else {
                                    ExprValue::Str(base_val.to_string_val())
                                }
                            }
                            ExprValue::Int(v) => ExprValue::Str(v.to_string()),
                            ExprValue::Float(v) => ExprValue::Str(v.to_string()),
                            ExprValue::Bool(v) => ExprValue::Str(v.to_string()),
                            ExprValue::Str(s) => ExprValue::Str(s),
                        }
                    }
                    "to_i" => {
                        let radix = args.first().map(|a| a.eval(ctx).to_i64() as u32).unwrap_or(10);
                        match base_val {
                            ExprValue::Str(ref s) => ExprValue::Int(parse_str_to_int(s, radix)),
                            _ => ExprValue::Int(base_val.to_i64()),
                        }
                    }
                    "substring" => {
                        let s = base_val.to_string_val();
                        let from = args.first().map(|a| a.eval(ctx).to_i64().max(0) as usize).unwrap_or(0);
                        let to = args.get(1).map(|a| a.eval(ctx).to_i64().max(0) as usize).unwrap_or_else(|| s.chars().count());
                        let chars: Vec<char> = s.chars().collect();
                        let start = from.min(chars.len());
                        let end = to.min(chars.len()).max(start);
                        let sub: String = chars[start..end].iter().collect();
                        ExprValue::Str(sub)
                    }
                    other => Self::eval_builtin_property(base_val, other).unwrap_or(ExprValue::Int(0)),
                }
            }
            ExprAST::Unary { op, operand } => {
                let val = operand.eval(ctx);
                match op {
                    UnaryOp::Not => ExprValue::Bool(!val.to_bool()),
                    UnaryOp::Neg => {
                        if val.is_float() {
                            ExprValue::Float(-val.to_f64())
                        } else {
                            ExprValue::Int(val.to_i64().wrapping_neg())
                        }
                    }
                    UnaryOp::BitNot => ExprValue::Int(!val.to_i64()),
                }
            }
            ExprAST::Ternary {
                cond,
                then_branch,
                else_branch,
            } => {
                if cond.eval(ctx).to_bool() {
                    then_branch.eval(ctx)
                } else {
                    else_branch.eval(ctx)
                }
            }
            ExprAST::Binary { op, left, right } => match op {
                BinaryOp::LogicalAnd => {
                    let l = left.eval(ctx);
                    if !l.to_bool() {
                        return ExprValue::Bool(false);
                    }
                    let r = right.eval(ctx);
                    ExprValue::Bool(r.to_bool())
                }
                BinaryOp::LogicalOr => {
                    let l = left.eval(ctx);
                    if l.to_bool() {
                        return ExprValue::Bool(true);
                    }
                    let r = right.eval(ctx);
                    ExprValue::Bool(r.to_bool())
                }
                _ => {
                    let l = left.eval(ctx);
                    let r = right.eval(ctx);
                    match op {
                        BinaryOp::Add => {
                            if matches!(l, ExprValue::Str(_)) || matches!(r, ExprValue::Str(_)) {
                                ExprValue::Str(format!("{}{}", l.to_string_val(), r.to_string_val()))
                            } else if l.is_float() || r.is_float() {
                                ExprValue::Float(l.to_f64() + r.to_f64())
                            } else {
                                ExprValue::Int(l.to_i64().wrapping_add(r.to_i64()))
                            }
                        }
                        BinaryOp::Sub => {
                            if l.is_float() || r.is_float() {
                                ExprValue::Float(l.to_f64() - r.to_f64())
                            } else {
                                ExprValue::Int(l.to_i64().wrapping_sub(r.to_i64()))
                            }
                        }
                        BinaryOp::Mul => {
                            if l.is_float() || r.is_float() {
                                ExprValue::Float(l.to_f64() * r.to_f64())
                            } else {
                                ExprValue::Int(l.to_i64().wrapping_mul(r.to_i64()))
                            }
                        }
                        BinaryOp::Div => {
                            if l.is_float() || r.is_float() {
                                let r_val = r.to_f64();
                                if r_val == 0.0 {
                                    if let Some(err_cell) = ctx.errors {
                                        err_cell.borrow_mut().push(crate::core::structure::types::ParseError {
                                            message: "Division by zero in float expression".to_string(),
                                            offset: ctx.stream_pos,
                                        });
                                    }
                                    ExprValue::Float(0.0)
                                } else {
                                    ExprValue::Float(l.to_f64() / r_val)
                                }
                            } else {
                                let r_val = r.to_i64();
                                if let Some(res) = l.to_i64().checked_div(r_val) {
                                    ExprValue::Int(res)
                                } else {
                                    if let Some(err_cell) = ctx.errors {
                                        err_cell.borrow_mut().push(crate::core::structure::types::ParseError {
                                            message: if r_val == 0 {
                                                "Division by zero in integer expression".to_string()
                                            } else {
                                                "Integer division overflow in expression".to_string()
                                            },
                                            offset: ctx.stream_pos,
                                        });
                                    }
                                    ExprValue::Int(0)
                                }
                            }
                        }
                        BinaryOp::Mod => {
                            let l_val = l.to_i64();
                            let r_val = r.to_i64();
                            if let Some(rem) = l_val.checked_rem(r_val) {
                                let res = if r_val > 0 && rem < 0 { rem + r_val } else { rem };
                                ExprValue::Int(res)
                            } else {
                                if let Some(err_cell) = ctx.errors {
                                    err_cell.borrow_mut().push(crate::core::structure::types::ParseError {
                                        message: if r_val == 0 {
                                            "Modulo by zero in expression".to_string()
                                        } else {
                                            "Integer modulo overflow in expression".to_string()
                                        },
                                        offset: ctx.stream_pos,
                                    });
                                }
                                ExprValue::Int(0)
                            }
                        }
                        BinaryOp::Shl => ExprValue::Int(l.to_i64().wrapping_shl(r.to_i64() as u32)),
                        BinaryOp::Shr => ExprValue::Int(l.to_i64().wrapping_shr(r.to_i64() as u32)),
                        BinaryOp::BitAnd => ExprValue::Int(l.to_i64() & r.to_i64()),
                        BinaryOp::BitOr => ExprValue::Int(l.to_i64() | r.to_i64()),
                        BinaryOp::BitXor => ExprValue::Int(l.to_i64() ^ r.to_i64()),
                        BinaryOp::Equal => {
                            if (matches!(l, ExprValue::Bytes(_)) || matches!(r, ExprValue::Bytes(_)))
                                && let (Some(b1), Some(b2)) = (l.as_bytes(), r.as_bytes())
                            {
                                ExprValue::Bool(b1 == b2)
                            } else if let (ExprValue::Array(a1), ExprValue::Array(a2)) = (&l, &r) {
                                ExprValue::Bool(a1 == a2)
                            } else if matches!(l, ExprValue::Str(_)) || matches!(r, ExprValue::Str(_)) {
                                ExprValue::Bool(l.to_string_val() == r.to_string_val())
                            } else if matches!(l, ExprValue::Bool(_)) || matches!(r, ExprValue::Bool(_)) {
                                ExprValue::Bool(l.to_bool() == r.to_bool())
                            } else if l.is_float() || r.is_float() {
                                ExprValue::Bool(l.to_f64() == r.to_f64())
                            } else {
                                ExprValue::Bool(l.to_i64() == r.to_i64())
                            }
                        }
                        BinaryOp::NotEqual => {
                            if (matches!(l, ExprValue::Bytes(_)) || matches!(r, ExprValue::Bytes(_)))
                                && let (Some(b1), Some(b2)) = (l.as_bytes(), r.as_bytes())
                            {
                                ExprValue::Bool(b1 != b2)
                            } else if let (ExprValue::Array(a1), ExprValue::Array(a2)) = (&l, &r) {
                                ExprValue::Bool(a1 != a2)
                            } else if matches!(l, ExprValue::Str(_)) || matches!(r, ExprValue::Str(_)) {
                                ExprValue::Bool(l.to_string_val() != r.to_string_val())
                            } else if matches!(l, ExprValue::Bool(_)) || matches!(r, ExprValue::Bool(_)) {
                                ExprValue::Bool(l.to_bool() != r.to_bool())
                            } else if l.is_float() || r.is_float() {
                                ExprValue::Bool(l.to_f64() != r.to_f64())
                            } else {
                                ExprValue::Bool(l.to_i64() != r.to_i64())
                            }
                        }
                        BinaryOp::Greater => {
                            if (matches!(l, ExprValue::Bytes(_)) || matches!(r, ExprValue::Bytes(_)))
                                && let (Some(b1), Some(b2)) = (l.as_bytes(), r.as_bytes())
                            {
                                ExprValue::Bool(b1 > b2)
                            } else if matches!(l, ExprValue::Str(_)) || matches!(r, ExprValue::Str(_)) {
                                ExprValue::Bool(l.to_string_val() > r.to_string_val())
                            } else if l.is_float() || r.is_float() {
                                ExprValue::Bool(l.to_f64() > r.to_f64())
                            } else {
                                ExprValue::Bool(l.to_i64() > r.to_i64())
                            }
                        }
                        BinaryOp::GreaterEqual => {
                            if (matches!(l, ExprValue::Bytes(_)) || matches!(r, ExprValue::Bytes(_)))
                                && let (Some(b1), Some(b2)) = (l.as_bytes(), r.as_bytes())
                            {
                                ExprValue::Bool(b1 >= b2)
                            } else if matches!(l, ExprValue::Str(_)) || matches!(r, ExprValue::Str(_)) {
                                ExprValue::Bool(l.to_string_val() >= r.to_string_val())
                            } else if l.is_float() || r.is_float() {
                                ExprValue::Bool(l.to_f64() >= r.to_f64())
                            } else {
                                ExprValue::Bool(l.to_i64() >= r.to_i64())
                            }
                        }
                        BinaryOp::Less => {
                            if (matches!(l, ExprValue::Bytes(_)) || matches!(r, ExprValue::Bytes(_)))
                                && let (Some(b1), Some(b2)) = (l.as_bytes(), r.as_bytes())
                            {
                                ExprValue::Bool(b1 < b2)
                            } else if matches!(l, ExprValue::Str(_)) || matches!(r, ExprValue::Str(_)) {
                                ExprValue::Bool(l.to_string_val() < r.to_string_val())
                            } else if l.is_float() || r.is_float() {
                                ExprValue::Bool(l.to_f64() < r.to_f64())
                            } else {
                                ExprValue::Bool(l.to_i64() < r.to_i64())
                            }
                        }
                        BinaryOp::LessEqual => {
                            if (matches!(l, ExprValue::Bytes(_)) || matches!(r, ExprValue::Bytes(_)))
                                && let (Some(b1), Some(b2)) = (l.as_bytes(), r.as_bytes())
                            {
                                ExprValue::Bool(b1 <= b2)
                            } else if matches!(l, ExprValue::Str(_)) || matches!(r, ExprValue::Str(_)) {
                                ExprValue::Bool(l.to_string_val() <= r.to_string_val())
                            } else if l.is_float() || r.is_float() {
                                ExprValue::Bool(l.to_f64() <= r.to_f64())
                            } else {
                                ExprValue::Bool(l.to_i64() <= r.to_i64())
                            }
                        }
                        BinaryOp::LogicalAnd | BinaryOp::LogicalOr => unreachable!(),
                    }
                }
            },
        }
    }

    fn eval_identifier(&self, id: &str, ctx: &EvalContext) -> ExprValue {
        match id {
            "true" => return ExprValue::Bool(true),
            "false" => return ExprValue::Bool(false),
            "_io" => return ExprValue::Int(0),
            _ => {}
        }

        // Fast path: if base_path is empty, direct lookup
        if ctx.base_path.is_empty() {
            if let Some(val) = Self::lookup_ctx_path(id, ctx) {
                return val;
            }
            if let Some(resolver) = ctx.instance_resolver
                && let Some(val) = resolver(id)
            {
                return ExprValue::Int(val);
            }
        }

        let full_id = if id == "_root" {
            String::new()
        } else if id == "_parent" {
            if ctx.base_path.len() > 1 {
                ctx.base_path[..ctx.base_path.len() - 1].join(".")
            } else {
                String::new()
            }
        } else if id == "_" {
            if ctx.base_path.is_empty() {
                "_".to_string()
            } else {
                format!("{}._", ctx.base_path.join("."))
            }
        } else if ctx.base_path.is_empty() {
            id.to_string()
        } else {
            format!("{}.{}", ctx.base_path.join("."), id)
        };

        if let Some(val) = Self::lookup_ctx_path(&full_id, ctx) {
            return val;
        }

        // Walk up scope hierarchy from base_path
        if !ctx.base_path.is_empty() {
            for i in (0..ctx.base_path.len()).rev() {
                let scope_id = if i == 0 {
                    id.to_string()
                } else {
                    format!("{}.{}", ctx.base_path[..i].join("."), id)
                };
                if let Some(val) = Self::lookup_ctx_path(&scope_id, ctx) {
                    return val;
                }
            }
        }

        // Fallback for direct bare identifier if present at top-level
        if let Some(val) = Self::lookup_ctx_path(id, ctx) {
            return val;
        }

        // Try instance resolver if provided
        if let Some(resolver) = ctx.instance_resolver
            && let Some(val) = resolver(&full_id).or_else(|| resolver(id))
        {
            return ExprValue::Int(val);
        }

        if !id.starts_with('_')
            && id != "true"
            && id != "false"
            && let Some(err_cell) = ctx.errors
        {
            err_cell.borrow_mut().push(crate::core::structure::types::ParseError {
                message: format!("Unresolved identifier: {}", full_id),
                offset: ctx.stream_pos,
            });
        }

        ExprValue::Int(0)
    }

    fn eval_member_access(&self, base: &ExprAST, member: &str, is_enum: bool, ctx: &EvalContext) -> ExprValue {
        if is_enum && let Some(enum_path) = Self::extract_enum_path(base) {
            if let Some(enum_def) = ctx.enums.get(&enum_path) {
                for (key, label) in enum_def {
                    if label == member {
                        let v = parse_str_to_int(key, 10);
                        return ExprValue::Int(v);
                    }
                }
                return ExprValue::Int(0);
            }
            if let Some(err_cell) = ctx.errors {
                err_cell.borrow_mut().push(crate::core::structure::types::ParseError {
                    message: format!("Unresolved identifier: {}::{}", enum_path, member),
                    offset: ctx.stream_pos,
                });
            }
            return ExprValue::Int(0);
        }

        if let ExprAST::Identifier(id) = base
            && id == "_io"
        {
            return match member {
                "eof" => ExprValue::Bool(ctx.stream_eof),
                "size" => ExprValue::Int(ctx.stream_size as i64),
                "pos" => ExprValue::Int(ctx.stream_pos as i64),
                _ => ExprValue::Int(0),
            };
        }

        if let ExprAST::MemberAccess {
            base: io_owner,
            member: io_member,
            is_enum: false,
        } = base
            && io_member == "_io"
        {
            if !matches!(member, "eof" | "size" | "pos") {
                return ExprValue::Int(0);
            }
            let live_io_val = || match member {
                "eof" => ExprValue::Bool(ctx.stream_eof),
                "size" => ExprValue::Int(ctx.stream_size as i64),
                "pos" => ExprValue::Int(ctx.stream_pos as i64),
                _ => ExprValue::Int(0),
            };
            if let Some((reaches_root, owner_candidates)) = io_owner.resolve_io_owner(ctx) {
                if reaches_root {
                    if ctx.active_io_prefixes.iter().any(|p| p == "_root") {
                        return live_io_val();
                    }
                    if let Some(val) = Self::lookup_ctx_path(&format!("_root._io.{member}"), ctx) {
                        return val;
                    }
                    if ctx.active_io_prefixes.is_empty() {
                        return live_io_val();
                    }
                }
                for owner_path in &owner_candidates {
                    if ctx.active_io_prefixes.iter().any(|p| p == owner_path) {
                        return live_io_val();
                    }
                    if let Some(val) = Self::lookup_ctx_path(&format!("{owner_path}._io.{member}"), ctx) {
                        return val;
                    }
                }
            }
            let unresolved_path = io_owner
                .collect_candidate_paths(ctx)
                .and_then(|p| p.into_iter().next())
                .or_else(|| io_owner.format_expr_path())
                .unwrap_or_else(|| io_member.clone());
            if let Some(err_cell) = ctx.errors {
                err_cell.borrow_mut().push(crate::core::structure::types::ParseError {
                    message: format!("Unresolved identifier: {unresolved_path}._io.{member}"),
                    offset: ctx.stream_pos,
                });
            }
            return ExprValue::Int(0);
        }

        let full_node = ExprAST::MemberAccess {
            base: Box::new(base.clone()),
            member: member.to_string(),
            is_enum: false,
        };
        let candidate_paths = full_node.collect_candidate_paths(ctx);

        if let Some(ref paths) = candidate_paths {
            for path in paths {
                if let Some(val) = Self::lookup_ctx_path(path, ctx) {
                    return val;
                }
            }
        }

        // Try built-in property evaluation on `base` (using silent context if `base` is an identifier/path)
        let silent_ctx = EvalContext { errors: None, ..*ctx };
        let base_val = base.eval(if candidate_paths.is_some() { &silent_ctx } else { ctx });
        if let Some(prop_val) = Self::eval_builtin_property(base_val, member) {
            return prop_val;
        }

        let first_path = candidate_paths.and_then(|p| p.into_iter().next()).unwrap_or_else(|| member.to_string());
        if !first_path.starts_with('_')
            && let Some(err_cell) = ctx.errors
        {
            err_cell.borrow_mut().push(crate::core::structure::types::ParseError {
                message: format!("Unresolved identifier: {}", first_path),
                offset: ctx.stream_pos,
            });
        }

        ExprValue::Int(0)
    }
}
