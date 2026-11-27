use super::dependency::can_preeval_pos_instance;
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
