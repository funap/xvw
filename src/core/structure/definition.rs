use crate::core::structure::expression::ExprAST;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KsyDefinition {
    pub meta: KsyMeta,
    #[serde(default)]
    pub params: Vec<KsyParam>,
    #[serde(default)]
    pub seq: Vec<KsyAttr>,
    #[serde(default)]
    pub types: HashMap<String, KsyType>,
    #[serde(default)]
    pub enums: HashMap<String, HashMap<String, serde_yaml::Value>>,
    #[serde(default)]
    pub instances: HashMap<String, KsyAttr>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KsyParam {
    pub id: String,
    #[serde(rename = "type")]
    pub param_type: Option<String>,
    pub doc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct KsyMeta {
    #[serde(default)]
    pub id: String,
    pub endian: Option<serde_yaml::Value>,
    #[serde(rename = "bit-endian")]
    pub bit_endian: Option<String>,
    pub encoding: Option<String>,
}

impl KsyMeta {
    /// Returns the static endianness string (`"le"` or `"be"`) if `endian` is a plain string.
    pub fn fixed_endian(&self) -> Option<&str> {
        self.endian.as_ref().and_then(|v| v.as_str())
    }

    /// Returns `(switch_on_expr, cases_map)` if `endian` is a calculated `switch-on` mapping.
    pub fn switch_endian(&self) -> Option<(String, HashMap<String, String>)> {
        self.endian.as_ref().and_then(parse_switch_map)
    }
}

/// Extracts `(switch_on, cases)` from a YAML mapping with `switch-on` and `cases` keys.
pub fn parse_switch_map(val: &serde_yaml::Value) -> Option<(String, HashMap<String, String>)> {
    let map = val.as_mapping()?;
    let switch_on = map.get("switch-on")?.as_str()?.to_string();
    let cases_map = map.get("cases")?.as_mapping()?;
    let mut cases = HashMap::new();
    for (k, v) in cases_map {
        let key = match k {
            serde_yaml::Value::String(s) => s.clone(),
            serde_yaml::Value::Number(n) => n.to_string(),
            serde_yaml::Value::Bool(b) => b.to_string(),
            serde_yaml::Value::Sequence(seq) => {
                let parts: Vec<String> = seq
                    .iter()
                    .map(|item| match item {
                        serde_yaml::Value::Number(n) => n.to_string(),
                        serde_yaml::Value::String(s) => s.clone(),
                        _ => String::new(),
                    })
                    .collect();
                format!("[{}]", parts.join(", "))
            }
            _ => continue,
        };
        if let Some(s) = v.as_str() {
            cases.insert(key.clone(), s.to_string());
            let trimmed = key.trim();
            if (trimmed.starts_with("0x") || trimmed.starts_with("0X"))
                && let Ok(int_val) = i64::from_str_radix(&trimmed[2..].replace('_', ""), 16)
            {
                cases.insert(int_val.to_string(), s.to_string());
                cases.insert(format!("0x{:x}", int_val), s.to_string());
                cases.insert(format!("0x{:X}", int_val), s.to_string());
                cases.insert(format!("0x{:04x}", int_val), s.to_string());
                cases.insert(format!("0x{:04X}", int_val), s.to_string());
            } else if let Ok(int_val) = trimmed.parse::<i64>() {
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

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KsyAttr {
    pub id: Option<String>,
    #[serde(rename = "type")]
    pub attr_type: Option<serde_yaml::Value>,
    pub size: Option<KsyValue>,
    #[serde(rename = "size-eos", default)]
    pub size_eos: bool,
    #[serde(rename = "if")]
    pub condition: Option<String>,
    pub repeat: Option<String>,
    #[serde(rename = "repeat-expr")]
    pub repeat_expr: Option<String>,
    #[serde(rename = "repeat-until")]
    pub repeat_until: Option<String>,
    #[serde(rename = "pos")]
    pub pos: Option<KsyValue>,
    #[serde(rename = "enum")]
    pub enum_ref: Option<String>,
    pub io: Option<String>,
    pub contents: Option<serde_yaml::Value>,
    pub encoding: Option<String>,
    pub doc: Option<String>,
    #[serde(rename = "doc-ref")]
    pub doc_ref: Option<serde_yaml::Value>,
    pub terminator: Option<serde_yaml::Value>,
    #[serde(default = "default_true")]
    pub consume: bool,
    #[serde(default)]
    pub include: bool,
    #[serde(rename = "eos-error", default = "default_true")]
    pub eos_error: bool,
    #[serde(rename = "pad-right")]
    pub pad_right: Option<serde_yaml::Value>,
    pub value: Option<String>,
    pub valid: Option<serde_yaml::Value>,
    pub process: Option<serde_yaml::Value>,

    #[serde(skip)]
    pub compiled_condition: Option<ExprAST>,
    #[serde(skip)]
    pub compiled_repeat_expr: Option<ExprAST>,
    #[serde(skip)]
    pub compiled_repeat_until: Option<ExprAST>,
    #[serde(skip)]
    pub compiled_value: Option<ExprAST>,
    #[serde(skip)]
    pub compiled_size: Option<ExprAST>,
    #[serde(skip)]
    pub compiled_pos: Option<ExprAST>,
    #[serde(skip)]
    pub compiled_switch: Option<(String, HashMap<String, String>)>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum KsyValue {
    Int(usize),
    Expr(String),
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KsyType {
    #[serde(default)]
    pub meta: Option<KsyMeta>,
    #[serde(default)]
    pub params: Vec<KsyParam>,
    #[serde(default)]
    pub seq: Vec<KsyAttr>,
    #[serde(default)]
    pub types: HashMap<String, KsyType>,
    #[serde(default)]
    pub instances: HashMap<String, KsyAttr>,
    #[serde(default)]
    pub enums: HashMap<String, HashMap<String, serde_yaml::Value>>,
}

impl KsyAttr {
    pub fn compile_expressions(&mut self) {
        if let Some(ref cond) = self.condition {
            self.compiled_condition = ExprAST::compile(cond);
        }
        if let Some(ref expr) = self.repeat_expr {
            self.compiled_repeat_expr = ExprAST::compile(expr);
        }
        if let Some(ref until) = self.repeat_until {
            self.compiled_repeat_until = ExprAST::compile(until);
        }
        if let Some(ref val) = self.value {
            self.compiled_value = ExprAST::compile(val);
        }
        if let Some(KsyValue::Expr(ref expr)) = self.size {
            self.compiled_size = ExprAST::compile(expr);
        }
        if let Some(KsyValue::Expr(ref expr)) = self.pos {
            self.compiled_pos = ExprAST::compile(expr);
        }
        if let Some(ref val) = self.attr_type
            && let Some(parsed) = parse_switch_map(val)
        {
            self.compiled_switch = Some(parsed);
        }
    }
}

impl KsyDefinition {
    pub fn compile_expressions(&mut self) {
        for attr in &mut self.seq {
            attr.compile_expressions();
        }
        for attr in self.instances.values_mut() {
            attr.compile_expressions();
        }
        compile_type_expressions(&mut self.types);
    }
}

fn compile_type_expressions(types: &mut HashMap<String, KsyType>) {
    let mut pending: Vec<&mut KsyType> = types.values_mut().collect();

    while let Some(type_def) = pending.pop() {
        for attr in &mut type_def.seq {
            attr.compile_expressions();
        }
        for attr in type_def.instances.values_mut() {
            attr.compile_expressions();
        }
        pending.extend(type_def.types.values_mut());
    }
}
