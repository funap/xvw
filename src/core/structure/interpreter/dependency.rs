use super::scope::TypeScope;
use super::{parse_type_with_args, type_as_str, type_as_switch};
use crate::core::structure::definition::{KsyAttr, KsyValue};
use std::collections::HashMap;

pub(super) fn is_cancelled(cancel_token: Option<&std::sync::atomic::AtomicBool>) -> bool {
    cancel_token.is_some_and(|token| token.load(std::sync::atomic::Ordering::Relaxed))
}

pub(super) fn expr_uses_ident(expr: &str, target: &str) -> bool {
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

pub(super) fn compute_preeval_blocked_ids(
    seq: &[KsyAttr],
    instances: &HashMap<String, KsyAttr>,
    scope: TypeScope,
    is_root: bool,
) -> std::collections::HashSet<String> {
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

pub(super) fn can_preeval_pos_instance_with_blocked(inst_attr: &KsyAttr, blocked: &std::collections::HashSet<String>, scope: TypeScope, is_root: bool) -> bool {
    if inst_attr.pos.is_none() || inst_attr.value.is_some() {
        return false;
    }
    !attr_has_blocked_dependency(inst_attr, blocked, scope, is_root)
}

#[cfg(test)]
pub(super) fn can_preeval_pos_instance(inst_attr: &KsyAttr, seq: &[KsyAttr], instances: &HashMap<String, KsyAttr>, scope: TypeScope, is_root: bool) -> bool {
    let blocked = compute_preeval_blocked_ids(seq, instances, scope, is_root);
    can_preeval_pos_instance_with_blocked(inst_attr, &blocked, scope, is_root)
}
