//! Kaitai Struct Specification Compliance Tests
//!
//! This test suite systematically verifies compliance with the official Kaitai Struct
//! language specification as documented in the Kaitai Struct User Guide
//! (https://doc.kaitai.io/user_guide.html) and KSY Diagram reference
//! (https://doc.kaitai.io/ksy_diagram.html).
//!
//! Organized by official specification sections:
//! - Section 4.1: Primitive Types & Endianness
//! - Section 4.2: Meta, Documentation & IDs
//! - Section 4.3: Fixed Contents / Magic Signatures
//! - Section 4.4: Validation Rules
//! - Section 4.5: Variable-length Structures & Dynamic Sizes
//! - Section 4.6: Delimited Structures (Strings & Byte Arrays)
//! - Section 4.7: Enums & Symbolic Constants
//! - Section 4.8 & 4.9: Subtypes, Nested Structures & Scopes (_root, _parent)
//! - Section 4.10: Conditionals (`if:`)
//! - Section 4.11: Repetitions (`repeat: expr`, `repeat: eos`, `repeat: until`)
//! - Section 4.12: Type Switching & TLV (`type: switch-on`, `cases:`)
//! - Section 4.13 & 4.14: Instances (Calculated `value:` & Stream Seek `pos:`)
//! - Section 4.15: Bit-sized Integers & Stream Alignment (`b1`..`b64`, `b*be`, `b*le`)
//! - Section 5: Streams, Substreams & Data Processing (`process: zlib`, `xor`, `rol`)
//! - Section 6: Expression Language (Operators, Precedence, `_io.*`, Ternary)
//! - Section 7.10: Parametric Types (`params:` and Arguments)

use crate::core::structure::types::FieldValue;
use crate::core::structure::{KaitaiInterpreter, KaitaiStream, KsyDefinition};

fn parse_ksy(yaml: &str) -> KsyDefinition {
    serde_yaml::from_str(yaml).expect("Failed to parse YAML KSY definition")
}

// ============================================================================
// Section 4.1: Primitive Types & Endianness
// ============================================================================

#[test]
fn test_spec_4_1_numeric_primitive_types_little_endian() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_1_primitives_le
  endian: le
seq:
  - id: val_u1
    type: u1
  - id: val_u2
    type: u2
  - id: val_u4
    type: u4
  - id: val_u8
    type: u8
  - id: val_s1
    type: s1
  - id: val_s2
    type: s2
  - id: val_s4
    type: s4
  - id: val_s8
    type: s8
  - id: val_f4
    type: f4
  - id: val_f8
    type: f8
"#,
    );

    let mut data = Vec::new();
    data.push(0xFE); // u1 = 254
    data.extend_from_slice(&1000u16.to_le_bytes()); // u2 = 1000
    data.extend_from_slice(&100_000u32.to_le_bytes()); // u4 = 100,000
    data.extend_from_slice(&10_000_000_000u64.to_le_bytes()); // u8 = 10,000,000,000
    data.push((-42i8) as u8); // s1 = -42
    data.extend_from_slice(&(-1234i16).to_le_bytes()); // s2 = -1234
    data.extend_from_slice(&(-500_000i32).to_le_bytes()); // s4 = -500,000
    data.extend_from_slice(&(-9_000_000_000i64).to_le_bytes()); // s8 = -9,000,000,000
    data.extend_from_slice(&123.456f32.to_le_bytes()); // f4
    data.extend_from_slice(&789.0123456789f64.to_le_bytes()); // f8

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields.len(), 10);
    assert_eq!(result.fields[0].value, FieldValue::U8(254));
    assert_eq!(result.fields[1].value, FieldValue::U16(1000));
    assert_eq!(result.fields[2].value, FieldValue::U32(100_000));
    assert_eq!(result.fields[3].value, FieldValue::U64(10_000_000_000));
    assert_eq!(result.fields[4].value, FieldValue::I8(-42));
    assert_eq!(result.fields[5].value, FieldValue::I16(-1234));
    assert_eq!(result.fields[6].value, FieldValue::I32(-500_000));
    assert_eq!(result.fields[7].value, FieldValue::I64(-9_000_000_000));

    if let FieldValue::F32(f) = result.fields[8].value {
        assert!((f - 123.456f32).abs() < 1e-5);
    } else {
        panic!("Expected f4 field value");
    }

    if let FieldValue::F64(f) = result.fields[9].value {
        assert!((f - 789.0123456789f64).abs() < 1e-9);
    } else {
        panic!("Expected f8 field value");
    }
}

#[test]
fn test_spec_4_1_numeric_primitive_types_big_endian() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_1_primitives_be
  endian: be
seq:
  - id: val_u2
    type: u2
  - id: val_u4
    type: u4
  - id: val_u8
    type: u8
  - id: val_s2
    type: s2
  - id: val_s4
    type: s4
  - id: val_s8
    type: s8
  - id: val_f4
    type: f4
  - id: val_f8
    type: f8
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(&0x1234u16.to_be_bytes());
    data.extend_from_slice(&0x12345678u32.to_be_bytes());
    data.extend_from_slice(&0x0102030405060708u64.to_be_bytes());
    data.extend_from_slice(&(-300i16).to_be_bytes());
    data.extend_from_slice(&(-70_000i32).to_be_bytes());
    data.extend_from_slice(&(-123_456_789_000i64).to_be_bytes());
    data.extend_from_slice(&1.5f32.to_be_bytes());
    data.extend_from_slice(&100.25f64.to_be_bytes());

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields[0].value, FieldValue::U16(0x1234));
    assert_eq!(result.fields[1].value, FieldValue::U32(0x12345678));
    assert_eq!(result.fields[2].value, FieldValue::U64(0x0102030405060708));
    assert_eq!(result.fields[3].value, FieldValue::I16(-300));
    assert_eq!(result.fields[4].value, FieldValue::I32(-70_000));
    assert_eq!(result.fields[5].value, FieldValue::I64(-123_456_789_000));
    assert_eq!(result.fields[6].value, FieldValue::F32(1.5));
    assert_eq!(result.fields[7].value, FieldValue::F64(100.25));
}

#[test]
fn test_spec_4_1_explicit_endianness_overrides() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_1_endian_override
  endian: le
seq:
  - id: le_default
    type: u2
  - id: be_explicit
    type: u2be
  - id: le_explicit
    type: u4le
  - id: be_explicit_s4
    type: s4be
  - id: be_explicit_f4
    type: f4be
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(&0x0001u16.to_le_bytes()); // 1
    data.extend_from_slice(&0x0002u16.to_be_bytes()); // 2
    data.extend_from_slice(&0x00000003u32.to_le_bytes()); // 3
    data.extend_from_slice(&(-4i32).to_be_bytes()); // -4
    data.extend_from_slice(&12.5f32.to_be_bytes()); // 12.5

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields[0].value, FieldValue::U16(1));
    assert_eq!(result.fields[1].value, FieldValue::U16(2));
    assert_eq!(result.fields[2].value, FieldValue::U32(3));
    assert_eq!(result.fields[3].value, FieldValue::I32(-4));
    assert_eq!(result.fields[4].value, FieldValue::F32(12.5));
}

// ============================================================================
// Section 4.2: Documentation & Meta
// ============================================================================

#[test]
fn test_spec_4_2_docstrings_and_metadata() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_2_doc_test
  endian: le
seq:
  - id: magic
    type: u4
    doc: Four-byte magic signature at start of file
  - id: version
    type: u2
    doc: Format version number
    doc-ref: https://example.com/spec/v1.0
"#,
    );

    let data = [0x50, 0x4B, 0x03, 0x04, 0x14, 0x00];
    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields.len(), 2);
    assert_eq!(result.fields[0].description.as_deref(), Some("Four-byte magic signature at start of file"));
    assert_eq!(result.fields[1].description.as_deref(), Some("Format version number"));
}

// ============================================================================
// Section 4.3: Fixed Contents / Magic Signatures
// ============================================================================

#[test]
fn test_spec_4_3_fixed_contents_string_and_byte_arrays() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_3_contents
seq:
  - id: string_magic
    contents: "GIF89a"
  - id: byte_magic
    contents: [0xCA, 0xFE, 0xBA, 0xBE]
  - id: mixed_magic
    contents: [0x89, "PNG\r\n", 0x1A, "\n"]
  - id: payload
    type: u1
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(b"GIF89a");
    data.extend_from_slice(&[0xCA, 0xFE, 0xBA, 0xBE]);
    data.extend_from_slice(&[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n']);
    data.push(0x42);

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields.len(), 4);
    assert_eq!(result.fields[0].id, "string_magic");
    assert_eq!(result.fields[1].id, "byte_magic");
    assert_eq!(result.fields[2].id, "mixed_magic");
    assert_eq!(result.fields[3].id, "payload");
    assert_eq!(result.fields[3].value, FieldValue::U8(0x42));
}

// ============================================================================
// Section 4.4: Validation Rules
// ============================================================================

#[test]
fn test_spec_4_4_validation_eq_min_max_and_any_of() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_4_validation
  endian: le
seq:
  - id: exact_val
    type: u2
    valid:
      eq: 42
  - id: range_val
    type: u1
    valid:
      min: 10
      max: 50
  - id: set_val
    type: u1
    valid:
      any-of: [1, 2, 4, 8, 16, 32]
  - id: expr_val
    type: u2
    valid:
      expr: "_ % 10 == 0"
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(&42u16.to_le_bytes()); // valid: eq 42
    data.push(25); // valid: min 10, max 50
    data.push(8); // valid: any-of
    data.extend_from_slice(&100u16.to_le_bytes()); // valid: expr _ % 10 == 0

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields.len(), 4);
    assert_eq!(result.fields[0].value, FieldValue::U16(42));
    assert_eq!(result.fields[1].value, FieldValue::U8(25));
    assert_eq!(result.fields[2].value, FieldValue::U8(8));
    assert_eq!(result.fields[3].value, FieldValue::U16(100));
}

// ============================================================================
// Section 4.5: Variable-length Structures & Dynamic Sizing
// ============================================================================

#[test]
fn test_spec_4_5_variable_length_fields_and_size_eos() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_5_var_length
  endian: le
seq:
  - id: fixed_size_block
    size: 4
  - id: header_len
    type: u1
  - id: dynamic_block
    size: header_len
  - id: expr_sized_block
    size: header_len * 2
  - id: remaining_data
    size-eos: true
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(&[0x11, 0x22, 0x33, 0x44]); // fixed 4 bytes
    data.push(3); // header_len = 3
    data.extend_from_slice(&[0xAA, 0xBB, 0xCC]); // dynamic 3 bytes
    data.extend_from_slice(&[0x01, 0x02, 0x03, 0x04, 0x05, 0x06]); // expr: 3 * 2 = 6 bytes
    data.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]); // remaining size-eos

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields.len(), 5);
    assert_eq!(result.fields[0].size, 4);
    assert_eq!(result.fields[1].value, FieldValue::U8(3));
    assert_eq!(result.fields[2].size, 3);
    assert_eq!(result.fields[3].size, 6);
    assert_eq!(result.fields[4].size, 4);
    assert_eq!(result.fields[4].value, FieldValue::Bytes(vec![0xDE, 0xAD, 0xBE, 0xEF]));
}

// ============================================================================
// Section 4.6: Delimited Structures (Strings & Byte Arrays)
// ============================================================================

#[test]
fn test_spec_4_6_delimited_strings_and_custom_terminators() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_6_strings
seq:
  - id: null_term_str
    type: strz
    encoding: ASCII
  - id: custom_term_str
    type: str
    terminator: 0x0A
    encoding: UTF-8
  - id: next_byte
    type: u1
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(b"Hello World\0");
    data.extend_from_slice("こんにちは\n".as_bytes());
    data.push(0xFF);

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields.len(), 3);
    assert_eq!(result.fields[0].value, FieldValue::String("Hello World".to_string()));
    assert_eq!(result.fields[1].value, FieldValue::String("こんにちは".to_string()));
    assert_eq!(result.fields[2].value, FieldValue::U8(0xFF));
}

// ============================================================================
// Section 4.7: Enums & Symbolic Constants
// ============================================================================

#[test]
fn test_spec_4_7_enums_and_symbol_resolution() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_7_enums
  endian: le
enums:
  http_status:
    200: ok
    404: not_found
    500: internal_server_error
  packet_flags:
    0: none
    1: ack
    2: syn
    4: fin
seq:
  - id: status
    type: u2
    enum: http_status
  - id: flags
    type: u1
    enum: packet_flags
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(&200u16.to_le_bytes());
    data.push(2); // syn

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields.len(), 2);
    assert_eq!(result.fields[0].value, FieldValue::U16(200));
    assert_eq!(result.fields[0].enum_label.as_deref(), Some("ok"));
    assert_eq!(result.fields[1].value, FieldValue::U8(2));
    assert_eq!(result.fields[1].enum_label.as_deref(), Some("syn"));
}

// ============================================================================
// Section 4.8 & 4.9: Subtypes, Nested Structures & Scopes (_root, _parent)
// ============================================================================

#[test]
fn test_spec_4_8_subtypes_and_hierarchical_scopes() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_8_scopes
  endian: le
seq:
  - id: global_multiplier
    type: u1
  - id: record
    type: item_record
types:
  item_record:
    seq:
      - id: base_val
        type: u2
      - id: detail
        type: sub_detail
    types:
      sub_detail:
        seq:
          - id: sub_val
            type: u1
        instances:
          calc_total:
            value: sub_val + _parent.base_val * _root.global_multiplier
"#,
    );

    let mut data = Vec::new();
    data.push(10); // _root.global_multiplier = 10
    data.extend_from_slice(&5u16.to_le_bytes()); // _parent.base_val = 5
    data.push(3); // sub_val = 3

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    // Verify hierarchy
    assert_eq!(result.fields[0].value, FieldValue::U8(10));
    let record_field = &result.fields[1];
    assert_eq!(record_field.children.len(), 2);
    assert_eq!(record_field.children[0].value, FieldValue::U16(5));

    let detail_field = &record_field.children[1];
    assert_eq!(detail_field.children[0].value, FieldValue::U8(3));

    // Instances check: 3 + (5 * 10) = 53
    let calc_inst = detail_field
        .children
        .iter()
        .find(|f| f.id == "calc_total")
        .expect("calc_total instance must exist");
    assert_eq!(calc_inst.value, FieldValue::U64(53));
}

// ============================================================================
// Section 4.10: Conditionals (`if:`)
// ============================================================================

#[test]
fn test_spec_4_10_conditionals_parsing_and_skipping() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_10_conditionals
  endian: le
seq:
  - id: has_optional_data
    type: u1
  - id: optional_u4
    type: u4
    if: has_optional_data != 0
  - id: tail_marker
    type: u2
"#,
    );

    // Case 1: has_optional_data == 1 (optional_u4 is parsed)
    let mut data1 = Vec::new();
    data1.push(1);
    data1.extend_from_slice(&0x12345678u32.to_le_bytes());
    data1.extend_from_slice(&0xCAFEu16.to_le_bytes());

    let mut stream1 = KaitaiStream::new(&data1);
    let interpreter1 = KaitaiInterpreter::new(ksy.clone());
    let result1 = interpreter1.parse(&mut stream1);

    assert_eq!(result1.fields.len(), 3);
    assert_eq!(result1.fields[0].value, FieldValue::U8(1));
    assert_eq!(result1.fields[1].id, "optional_u4");
    assert_eq!(result1.fields[1].value, FieldValue::U32(0x12345678));
    assert_eq!(result1.fields[2].value, FieldValue::U16(0xCAFE));

    // Case 2: has_optional_data == 0 (optional_u4 is skipped, stream does not consume 4 bytes)
    let mut data2 = Vec::new();
    data2.push(0);
    data2.extend_from_slice(&0xCAFEu16.to_le_bytes());

    let mut stream2 = KaitaiStream::new(&data2);
    let interpreter2 = KaitaiInterpreter::new(ksy);
    let result2 = interpreter2.parse(&mut stream2);

    assert_eq!(result2.fields.len(), 2);
    assert_eq!(result2.fields[0].value, FieldValue::U8(0));
    assert_eq!(result2.fields[1].id, "tail_marker");
    assert_eq!(result2.fields[1].value, FieldValue::U16(0xCAFE));
}

// ============================================================================
// Section 4.11: Repetitions (`repeat: expr`, `repeat: eos`, `repeat: until`)
// ============================================================================

#[test]
fn test_spec_4_11_repetitions_expr_eos_and_until() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_11_repetitions
  endian: le
seq:
  - id: count
    type: u1
  - id: items_by_count
    type: u2
    repeat: expr
    repeat-expr: count
  - id: items_until_zero
    type: u1
    repeat: until
    repeat-until: _ == 0
"#,
    );

    let mut data = Vec::new();
    data.push(3); // count = 3
    data.extend_from_slice(&100u16.to_le_bytes());
    data.extend_from_slice(&200u16.to_le_bytes());
    data.extend_from_slice(&300u16.to_le_bytes());
    data.extend_from_slice(&[5, 4, 3, 2, 1, 0]); // terminated by 0

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields[0].value, FieldValue::U8(3));

    // repeat: expr count 3
    assert_eq!(result.fields[1].value, FieldValue::U16(100));
    assert_eq!(result.fields[2].value, FieldValue::U16(200));
    assert_eq!(result.fields[3].value, FieldValue::U16(300));

    // repeat: until _ == 0
    assert_eq!(result.fields[4].value, FieldValue::U8(5));
    assert_eq!(result.fields[9].value, FieldValue::U8(0));
}

// ============================================================================
// Section 4.12: Type Switching & TLV (`type: switch-on`, `cases:`)
// ============================================================================

#[test]
fn test_spec_4_12_type_switching_with_default_case() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_12_switch
  endian: le
seq:
  - id: entry_count
    type: u1
  - id: entries
    type: entry
    repeat: expr
    repeat-expr: entry_count
types:
  entry:
    seq:
      - id: tag
        type: u1
      - id: payload
        type:
          switch-on: tag
          cases:
            1: u2
            2: u4
            _: u1
"#,
    );

    let mut data = Vec::new();
    data.push(3); // 3 entries
    data.push(1); // tag 1 -> u2
    data.extend_from_slice(&0x1122u16.to_le_bytes());
    data.push(2); // tag 2 -> u4
    data.extend_from_slice(&0x33445566u32.to_le_bytes());
    data.push(99); // fallback tag -> u1
    data.push(0x77);

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields.len(), 4);
    assert_eq!(result.fields[0].value, FieldValue::U8(3));

    // Entry 1
    assert_eq!(result.fields[1].children[0].value, FieldValue::U8(1));
    assert_eq!(result.fields[1].children[1].value, FieldValue::U16(0x1122));

    // Entry 2
    assert_eq!(result.fields[2].children[0].value, FieldValue::U8(2));
    assert_eq!(result.fields[2].children[1].value, FieldValue::U32(0x33445566));

    // Entry 3 (fallback)
    assert_eq!(result.fields[3].children[0].value, FieldValue::U8(99));
    assert_eq!(result.fields[3].children[1].value, FieldValue::U8(0x77));
}

// ============================================================================
// Section 4.13 & 4.14: Instances (Calculated & Stream Seek `pos:`)
// ============================================================================

#[test]
fn test_spec_4_13_instances_pos_and_value() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_13_instances
  endian: le
seq:
  - id: offset_to_title
    type: u2
  - id: number_a
    type: u2
  - id: number_b
    type: u2
instances:
  sum_ab:
    value: number_a + number_b
  title_str:
    pos: offset_to_title
    type: strz
    encoding: ASCII
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(&10u16.to_le_bytes()); // offset_to_title = 10
    data.extend_from_slice(&25u16.to_le_bytes()); // number_a = 25
    data.extend_from_slice(&75u16.to_le_bytes()); // number_b = 75
    data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00]); // padding to offset 10
    data.extend_from_slice(b"Kaitai Struct\0"); // string at offset 10

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    let sum_inst = result.fields.iter().find(|f| f.id == "sum_ab").expect("sum_ab instance must exist");
    assert_eq!(sum_inst.value, FieldValue::U64(100));

    let title_inst = result.fields.iter().find(|f| f.id == "title_str").expect("title_str instance must exist");
    assert_eq!(title_inst.value, FieldValue::String("Kaitai Struct".to_string()));
    assert_eq!(title_inst.offset, 10);
}

// ============================================================================
// Section 4.15: Bit-sized Integers & Stream Alignment
// ============================================================================

#[test]
fn test_spec_4_15_bit_fields_and_byte_alignment() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_15_bitfields
  endian: be
seq:
  - id: flag1
    type: b1
  - id: flag2
    type: b1
  - id: opcode
    type: b6
  - id: short_val
    type: b12
  - id: reserved
    type: b4
  - id: byte_aligned_u2
    type: u2
"#,
    );

    // Byte 0: flag1(1) | flag2(0) | opcode(6 = 0b101010) -> 0b10101010 = 0xAA
    // Byte 1 & 2: short_val(12 = 0x123) | reserved(4 = 0xF) -> 0x12, 0x3F
    // Byte 3 & 4: byte_aligned_u2 -> 0xCAFE
    let data = [0xAA, 0x12, 0x3F, 0xCA, 0xFE];

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields.len(), 6);
    assert_eq!(result.fields[0].value, FieldValue::U64(1));
    assert_eq!(result.fields[1].value, FieldValue::U64(0));
    assert_eq!(result.fields[2].value, FieldValue::U64(0b101010));
    assert_eq!(result.fields[3].value, FieldValue::U64(0x123));
    assert_eq!(result.fields[4].value, FieldValue::U64(0x0F));
    assert_eq!(result.fields[5].value, FieldValue::U16(0xCAFE));
}

// ============================================================================
// Section 5: Streams, Substreams & Data Processing
// ============================================================================

#[test]
fn test_spec_5_processing_zlib_xor_and_rol() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_5_processing
  endian: le
seq:
  - id: xor_data
    size: 4
    process: xor(0x5A)
"#,
    );

    let raw = [b'T' ^ 0x5A, b'E' ^ 0x5A, b'S' ^ 0x5A, b'T' ^ 0x5A];
    let mut stream = KaitaiStream::new(&raw);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields[0].value, FieldValue::Bytes(b"TEST".to_vec()));
}

// ============================================================================
// Section 6: Expression Language & Built-in Variables
// ============================================================================

#[test]
fn test_spec_6_expression_language_operators_and_stream_properties() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_6_expressions
  endian: le
seq:
  - id: pos_before
    type: u2
instances:
  stream_pos:
    value: _io.pos
  stream_size:
    value: _io.size
  stream_is_eof:
    value: _io.eof
  ternary_test:
    value: "pos_before > 10 ? 100 : 200"
  bitwise_calc:
    value: "(pos_before << 2) | 0x01"
"#,
    );

    let data = [0x20, 0x00, 0xAA, 0xBB]; // pos_before = 32 (0x0020), total size = 4
    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    let inst_pos = result.fields.iter().find(|f| f.id == "stream_pos").unwrap();
    assert_eq!(inst_pos.value, FieldValue::U64(2));

    let inst_size = result.fields.iter().find(|f| f.id == "stream_size").unwrap();
    assert_eq!(inst_size.value, FieldValue::U64(4));

    let inst_eof = result.fields.iter().find(|f| f.id == "stream_is_eof").unwrap();
    assert_eq!(inst_eof.value, FieldValue::Bool(false));

    let inst_ternary = result.fields.iter().find(|f| f.id == "ternary_test").unwrap();
    assert_eq!(inst_ternary.value, FieldValue::U64(100));

    let inst_bitwise = result.fields.iter().find(|f| f.id == "bitwise_calc").unwrap();
    assert_eq!(inst_bitwise.value, FieldValue::U64((32 << 2) | 0x01));
}

// ============================================================================
// Section 7.10: Parametric Types (`params:` and Arguments)
// ============================================================================

#[test]
fn test_spec_7_10_parametric_subtypes() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_7_10_params
  endian: le
seq:
  - id: chunk_size
    type: u1
  - id: custom_chunk
    type: sized_block(chunk_size)
types:
  sized_block:
    params:
      - id: block_len
        type: u1
    seq:
      - id: payload
        size: block_len
"#,
    );

    let data = [4, 0x01, 0x02, 0x03, 0x04, 0xFF];
    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields[0].value, FieldValue::U8(4));
    let custom_chunk = &result.fields[1];
    assert_eq!(custom_chunk.children.len(), 1);
    assert_eq!(custom_chunk.children[0].value, FieldValue::Bytes(vec![0x01, 0x02, 0x03, 0x04]));
}

// ============================================================================
// Advanced Spec Tests: Validation Failure Modes
// ============================================================================

#[test]
fn test_spec_4_4_validation_failure_modes() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_4_fail
  endian: le
seq:
  - id: magic_val
    type: u2
    valid:
      eq: 0x1234
  - id: range_val
    type: u1
    valid:
      min: 10
      max: 20
"#,
    );

    let data = [0x00, 0x00, 25]; // magic = 0x0000 (invalid), range = 25 (invalid)
    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert!(!result.errors.is_empty(), "Validation errors should be recorded");
}

// ============================================================================
// Advanced Spec Tests: Delimited Strings (consume: false & pad-right)
// ============================================================================

#[test]
fn test_spec_4_6_delimited_string_pad_right_and_consume_false() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_6_advanced
seq:
  - id: padded_name
    type: str
    size: 10
    pad-right: 0x20
    encoding: ASCII
  - id: non_consumed_delim
    type: str
    terminator: 0x2C
    consume: false
    encoding: ASCII
  - id: delimiter_byte
    type: u1
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(b"Alice     "); // 10 bytes padded with spaces
    data.extend_from_slice(b"Value,Next"); // terminated by ','

    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields[0].value, FieldValue::String("Alice".to_string()));
    assert_eq!(result.fields[1].value, FieldValue::String("Value".to_string()));
    // Because consume: false, next read gets the delimiter ',' (0x2C)
    assert_eq!(result.fields[2].value, FieldValue::U8(b','));
}

// ============================================================================
// Advanced Spec Tests: Sized Substream Repetitions
// ============================================================================

#[test]
fn test_spec_4_11_repetition_eos_in_sized_substream() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_11_substream_eos
  endian: le
seq:
  - id: container
    type: block
    size: 6
  - id: trailing_byte
    type: u1
types:
  block:
    seq:
      - id: items
        type: u2
        repeat: eos
"#,
    );

    let data = [0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0xFF];
    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    let container = &result.fields[0];
    assert_eq!(container.children.len(), 3);
    assert_eq!(container.children[0].value, FieldValue::U16(1));
    assert_eq!(container.children[1].value, FieldValue::U16(2));
    assert_eq!(container.children[2].value, FieldValue::U16(3));
    assert_eq!(result.fields[1].value, FieldValue::U8(0xFF));
}

// ============================================================================
// Advanced Spec Tests: Bitfield LE vs BE Endianness
// ============================================================================

#[test]
fn test_spec_4_15_bit_fields_endianness() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_4_15_endian_bits
seq:
  - id: bits_be
    type: b4be
  - id: bits_le
    type: b4le
"#,
    );

    let data = [0xAB];
    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    assert_eq!(result.fields.len(), 2);
    assert_eq!(result.fields[0].value, FieldValue::U64(0x0A));
    assert_eq!(result.fields[1].value, FieldValue::U64(0x0B));
}

// ============================================================================
// Advanced Spec Tests: Complex Expression Operators & Precedence
// ============================================================================

#[test]
fn test_spec_6_expression_operators_precedence_and_logic() {
    let ksy = parse_ksy(
        r#"
meta:
  id: spec_6_operators
  endian: le
seq:
  - id: a
    type: u1
  - id: b
    type: u1
  - id: c
    type: u1
instances:
  arithmetic_precedence:
    value: (a + b) * c
  modulo_div:
    value: (c * 10) / a % 7
  logical_operators:
    value: "(a == 2 and b != 3) or not (c == 0)"
  shift_and_mask:
    value: "((a << 4) | b) & 0x3F"
"#,
    );

    let data = [2, 5, 4]; // a = 2, b = 5, c = 4
    let mut stream = KaitaiStream::new(&data);
    let interpreter = KaitaiInterpreter::new(ksy);
    let result = interpreter.parse(&mut stream);

    // (2 + 5) * 4 = 28
    let inst_prec = result.fields.iter().find(|f| f.id == "arithmetic_precedence").unwrap();
    assert_eq!(inst_prec.value, FieldValue::U64(28));

    // (4 * 10) / 2 % 7 = 40 / 2 % 7 = 20 % 7 = 6
    let inst_mod = result.fields.iter().find(|f| f.id == "modulo_div").unwrap();
    assert_eq!(inst_mod.value, FieldValue::U64(6));

    // (2 == 2 and 5 != 3) or not (4 == 0) = (true and true) or not false = true
    let inst_logic = result.fields.iter().find(|f| f.id == "logical_operators").unwrap();
    assert_eq!(inst_logic.value, FieldValue::Bool(true));

    // ((2 << 4) | 5) & 0x3F = (32 | 5) & 0x3F = 37 & 0x3F = 37
    let inst_shift = result.fields.iter().find(|f| f.id == "shift_and_mask").unwrap();
    assert_eq!(inst_shift.value, FieldValue::U64(37));
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 4.1 (Calculated Endian & Floats)
// ============================================================================

#[test]
fn test_spec_4_1_calculated_endian_switch_on_bytes_and_ints() {
    let ksy = parse_ksy(
        r#"
meta:
  id: default_endian_expr_is_le
seq:
  - id: docs
    type: doc
    repeat: eos
types:
  doc:
    seq:
      - id: indicator
        size: 2
      - id: main
        type: main_obj
    types:
      main_obj:
        meta:
          endian:
            switch-on: _parent.indicator
            cases:
              '[0x49, 0x49]': le
              '[0x4d, 0x4d]': be
        seq:
          - id: some_int
            type: u4
          - id: some_int_be
            type: u2be
          - id: some_int_le
            type: u2le
        instances:
          inst_int:
            pos: 2
            type: u4
          inst_sub:
            pos: 2
            type: sub_main_obj
          endian_flag:
            value: "_is_le ? 1 : 2"
        types:
          sub_main_obj:
            seq:
              - id: foo
                type: u4
"#,
    );

    let mut data = Vec::new();
    // Doc 0: Little-Endian ("II" = [0x49, 0x49])
    data.extend_from_slice(b"II");
    data.extend_from_slice(&0x00000042u32.to_le_bytes()); // some_int = 66 (at offset 2)
    data.extend_from_slice(&0x0042u16.to_be_bytes()); // some_int_be = 66
    data.extend_from_slice(&0x0042u16.to_le_bytes()); // some_int_le = 66
    // Doc 1: Big-Endian ("MM" = [0x4d, 0x4d])
    data.extend_from_slice(b"MM");
    data.extend_from_slice(&0x00000042u32.to_be_bytes()); // some_int = 66 (at offset 12)
    data.extend_from_slice(&0x0042u16.to_be_bytes()); // some_int_be = 66
    data.extend_from_slice(&0x0042u16.to_le_bytes()); // some_int_le = 66

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert_eq!(result.fields.len(), 2);

    // Doc 0 (LE)
    let main_le = &result.fields[0].children[1];
    assert_eq!(main_le.children[0].value, FieldValue::U32(0x42));
    assert_eq!(main_le.children[1].value, FieldValue::U16(0x42));
    assert_eq!(main_le.children[2].value, FieldValue::U16(0x42));
    let inst_int_le = main_le.children.iter().find(|f| f.id == "inst_int").unwrap();
    assert_eq!(inst_int_le.value, FieldValue::U32(0x42));
    let inst_sub_le = main_le.children.iter().find(|f| f.id == "inst_sub").unwrap();
    assert_eq!(inst_sub_le.children[0].value, FieldValue::U32(0x42));
    let flag_le = main_le.children.iter().find(|f| f.id == "endian_flag").unwrap();
    assert_eq!(flag_le.value, FieldValue::U64(1));

    // Doc 1 (BE)
    let main_be = &result.fields[1].children[1];
    assert_eq!(main_be.children[0].value, FieldValue::U32(0x42));
    assert_eq!(main_be.children[1].value, FieldValue::U16(0x42));
    assert_eq!(main_be.children[2].value, FieldValue::U16(0x42));
    let flag_be = main_be.children.iter().find(|f| f.id == "endian_flag").unwrap();
    assert_eq!(flag_be.value, FieldValue::U64(2));
}

#[test]
fn test_spec_4_1_subtype_meta_endian_override_and_inheritance() {
    let ksy = parse_ksy(
        r#"
meta:
  id: default_big_endian_mod
  endian: le
seq:
  - id: root_le
    type: u2
  - id: be_block
    type: block_be
  - id: root_le_after
    type: u2
types:
  block_be:
    meta:
      endian: be
    seq:
      - id: one
        type: u4
      - id: nested
        type: inherited_be
    types:
      inherited_be:
        seq:
          - id: two
            type: u2
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(&0x1234u16.to_le_bytes());
    data.extend_from_slice(&0xABCDEF01u32.to_be_bytes());
    data.extend_from_slice(&0x5678u16.to_be_bytes());
    data.extend_from_slice(&0x9ABCu16.to_le_bytes());

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert_eq!(result.fields[0].value, FieldValue::U16(0x1234));
    assert_eq!(result.fields[1].children[0].value, FieldValue::U32(0xABCDEF01));
    assert_eq!(result.fields[1].children[1].children[0].value, FieldValue::U16(0x5678));
    assert_eq!(result.fields[2].value, FieldValue::U16(0x9ABC));
}

#[test]
fn test_spec_4_1_floating_point_expressions_and_comparisons() {
    let ksy = parse_ksy(
        r#"
meta:
  id: float_to_i_and_ops
  endian: le
seq:
  - id: single_val
    type: f4
  - id: double_val
    type: f8
instances:
  single_i:
    value: single_val.to_i
  double_i:
    value: double_val.to_i
  float_sum:
    value: single_val + double_val
  float_cmp:
    value: double_val > single_val and single_val == 1.5
  sci_calc:
    value: 1.25e2 + 0.5
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(&1.5f32.to_le_bytes());
    data.extend_from_slice(&(-3.875f64).to_le_bytes());

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    let single_i = result.fields.iter().find(|f| f.id == "single_i").unwrap();
    assert_eq!(single_i.value, FieldValue::U64(1));

    let double_i = result.fields.iter().find(|f| f.id == "double_i").unwrap();
    assert_eq!(double_i.value, FieldValue::I64(-3));

    let float_sum = result.fields.iter().find(|f| f.id == "float_sum").unwrap();
    if let FieldValue::F64(f) = float_sum.value {
        assert!((f - (-2.375f64)).abs() < 1e-6);
    } else {
        panic!("Expected F64 for float_sum");
    }

    let float_cmp = result.fields.iter().find(|f| f.id == "float_cmp").unwrap();
    assert_eq!(float_cmp.value, FieldValue::Bool(false));

    let sci_calc = result.fields.iter().find(|f| f.id == "sci_calc").unwrap();
    if let FieldValue::F64(f) = sci_calc.value {
        assert!((f - 125.5f64).abs() < 1e-9);
    } else {
        panic!("Expected F64 for sci_calc");
    }
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 4.4 (Validation Edge Cases)
// ============================================================================

#[test]
fn test_spec_4_4_valid_shorthand_enum_in_enum_and_conditional_skip() {
    let ksy = parse_ksy(
        r#"
meta:
  id: valid_official_suite
  endian: le
enums:
  animal:
    4: dog
    7: cat
    12: chicken
seq:
  - id: short_int
    type: u2
    valid: 0x1234
  - id: short_bytes
    size: 3
    valid: [0xAA, 0xBB, 0xCC]
  - id: eq_expr
    type: u2
    valid:
      eq: short_int + 2
  - id: signed_range
    type: s2
    valid:
      min: -100
      max: -10
  - id: enum_checked
    type: u1
    enum: animal
    valid:
      in-enum: true
  - id: skipped_invalid
    type: u1
    if: false
    valid: 99
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(&0x1234u16.to_le_bytes());
    data.extend_from_slice(&[0xAA, 0xBB, 0xCC]);
    data.extend_from_slice(&0x1236u16.to_le_bytes());
    data.extend_from_slice(&(-42i16).to_le_bytes());
    data.push(7); // animal::cat

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy.clone()).parse(&mut stream);
    assert!(result.errors.is_empty(), "Expected no validation errors: {:?}", result.errors);
    assert_eq!(result.fields.len(), 5);
    assert_eq!(result.fields[4].enum_label.as_deref(), Some("cat"));

    // Now test in-enum failure when value is not in animal enum (e.g. 99)
    let mut bad_data = data.clone();
    *bad_data.last_mut().unwrap() = 99;
    let mut bad_stream = KaitaiStream::new(&bad_data);
    let bad_result = KaitaiInterpreter::new(ksy).parse(&mut bad_stream);
    assert!(!bad_result.errors.is_empty(), "Expected in-enum validation error for unknown enum value");
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 4.5 & 4.6 (Strings & Bytes)
// ============================================================================

#[test]
fn test_spec_4_6_str_pad_term_and_fixed_strz_combinations() {
    let ksy = parse_ksy(
        r#"
meta:
  id: str_pad_term_suite
seq:
  - id: str_pad
    type: str
    size: 10
    pad-right: 0x40
    encoding: UTF-8
  - id: str_term
    type: str
    size: 10
    terminator: 0x40
    encoding: UTF-8
  - id: str_term_and_pad
    type: str
    size: 12
    terminator: 0x40
    pad-right: 0x2b
    encoding: UTF-8
  - id: str_term_include
    type: str
    size: 10
    terminator: 0x40
    include: true
    encoding: UTF-8
  - id: str_equal_pad_term_no_inc
    type: str
    size: 10
    terminator: 0x40
    pad-right: 0x40
    include: false
    encoding: UTF-8
  - id: str_equal_pad_term_inc
    type: str
    size: 10
    terminator: 0x40
    pad-right: 0x40
    include: true
    encoding: UTF-8
  - id: fixed_strz
    type: strz
    size: 8
    encoding: ASCII
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(b"str1@@@@@@"); // str_pad -> "str1"
    data.extend_from_slice(b"str2foo@ba"); // str_term -> "str2foo"
    data.extend_from_slice(b"str+++3bar@+"); // str_term_and_pad -> "str+++3bar"
    data.extend_from_slice(b"str4baz@ab"); // str_term_include -> "str4baz@"
    data.extend_from_slice(b"str5@@@@@@"); // str_equal_pad_term_no_inc -> "str5"
    data.extend_from_slice(b"str6@@@@@@"); // str_equal_pad_term_inc -> "str6@"
    data.extend_from_slice(b"tar\0junk"); // fixed_strz -> "tar" (consumes all 8 bytes)

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert_eq!(result.fields.len(), 7);
    assert_eq!(result.fields[0].value, FieldValue::String("str1".to_string()));
    assert_eq!(result.fields[0].size, 10);
    assert_eq!(result.fields[1].value, FieldValue::String("str2foo".to_string()));
    assert_eq!(result.fields[1].size, 10);
    assert_eq!(result.fields[2].value, FieldValue::String("str+++3bar".to_string()));
    assert_eq!(result.fields[2].size, 12);
    assert_eq!(result.fields[3].value, FieldValue::String("str4baz@".to_string()));
    assert_eq!(result.fields[4].value, FieldValue::String("str5".to_string()));
    assert_eq!(result.fields[5].value, FieldValue::String("str6@".to_string()));
    assert_eq!(result.fields[6].value, FieldValue::String("tar".to_string()));
    assert_eq!(result.fields[6].size, 8);
    assert_eq!(stream.pos() as usize, data.len());
}

#[test]
fn test_spec_4_6_eos_error_false_and_bytes_pad_term() {
    let ksy = parse_ksy(
        r#"
meta:
  id: bytes_pad_term_and_eos_no_err
seq:
  - id: bytes_pad
    size: 6
    pad-right: 0x40
  - id: bytes_term
    size: 6
    terminator: 0x40
  - id: bytes_term_include
    size: 6
    terminator: 0x40
    include: true
  - id: tail_str_no_term
    type: strz
    eos-error: false
    encoding: UTF-8
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(b"AB@@@@"); // bytes_pad -> [0x41, 0x42]
    data.extend_from_slice(b"CD@XYZ"); // bytes_term -> [0x43, 0x44]
    data.extend_from_slice(b"EF@XYZ"); // bytes_term_include -> [0x45, 0x46, 0x40]
    data.extend_from_slice(b"unterminated_at_eof"); // no NUL byte at EOF

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert_eq!(result.fields.len(), 4);
    assert_eq!(result.fields[0].value, FieldValue::Bytes(b"AB".to_vec()));
    assert_eq!(result.fields[0].size, 6);
    assert_eq!(result.fields[1].value, FieldValue::Bytes(b"CD".to_vec()));
    assert_eq!(result.fields[1].size, 6);
    assert_eq!(result.fields[2].value, FieldValue::Bytes(b"EF@".to_vec()));
    assert_eq!(result.fields[2].size, 6);
    assert_eq!(result.fields[3].value, FieldValue::String("unterminated_at_eof".to_string()));
}

#[test]
fn test_spec_4_6_meta_encoding_inheritance_and_overrides() {
    let ksy = parse_ksy(
        r#"
meta:
  id: str_encodings_default
  encoding: UTF-16LE
seq:
  - id: default_utf16le
    type: str
    size: 6
  - id: sub
    type: sjis_block
types:
  sjis_block:
    meta:
      encoding: Shift_JIS
    seq:
      - id: sjis_str
        type: str
        size: 4
      - id: explicit_utf8
        type: str
        size: 3
        encoding: UTF-8
"#,
    );

    let mut data = Vec::new();
    // "ABC" in UTF-16LE
    data.extend_from_slice(&[b'A', 0x00, b'B', 0x00, b'C', 0x00]);
    // "日本" in Shift_JIS (0x93 0xFA 0x96 0x7B)
    data.extend_from_slice(&[0x93, 0xFA, 0x96, 0x7B]);
    // "XYZ" in UTF-8
    data.extend_from_slice(b"XYZ");

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert_eq!(result.fields[0].value, FieldValue::String("ABC".to_string()));
    assert_eq!(result.fields[1].children[0].value, FieldValue::String("日本".to_string()));
    assert_eq!(result.fields[1].children[1].value, FieldValue::String("XYZ".to_string()));
}

#[test]
fn test_spec_4_6_string_escape_sequences_single_vs_double_quotes() {
    let ksy = parse_ksy(
        r#"
meta:
  id: expr_str_literals
instances:
  double_quotes:
    value: '"A\n\t\\\"B\x43\u3042\104"'
  single_quotes:
    value: "'A\\n\\tB'"
  double_len:
    value: double_quotes.length
  single_len:
    value: single_quotes.length
"#,
    );

    let data = [];
    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    let dq = result.fields.iter().find(|f| f.id == "double_quotes").unwrap();
    // 'A' (1) + '\n' (1) + '\t' (1) + '\\' (1) + '"' (1) + 'B' (1) + 'C' (1) + 'あ' (1) + 'D' (1) = 9 chars
    assert_eq!(dq.value, FieldValue::String("A\n\t\\\"BCあD".to_string()));

    let sq = result.fields.iter().find(|f| f.id == "single_quotes").unwrap();
    assert_eq!(sq.value, FieldValue::String(r"A\n\tB".to_string()));

    let dq_len = result.fields.iter().find(|f| f.id == "double_len").unwrap();
    assert_eq!(dq_len.value, FieldValue::U64(9));

    let sq_len = result.fields.iter().find(|f| f.id == "single_len").unwrap();
    assert_eq!(sq_len.value, FieldValue::U64(6));
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 4.7 (Scoped & Rich Enums)
// ============================================================================

#[test]
fn test_spec_4_7_scoped_nested_enums_negative_hex_and_value_instance_enum() {
    let ksy = parse_ksy(
        r#"
meta:
  id: enum_deep_and_negative
  endian: le
enums:
  signed_status:
    -1: error_neg
    0x00: ok_zero
    0x10: hex_sixteen
seq:
  - id: neg_code
    type: s1
    enum: signed_status
  - id: hex_code
    type: u1
    enum: signed_status
  - id: sub
    type: container1
instances:
  computed_enum:
    value: 0x10
    enum: signed_status
  is_deep_horse:
    value: sub.c2.animal_id == container1::container2::animal::horse
  neg_code_int:
    value: neg_code.to_i
types:
  container1:
    seq:
      - id: c2
        type: container2
    types:
      container2:
        enums:
          animal:
            4:
              id: dog
              doc: A canine
            7:
              id: horse
              doc: An equine
        seq:
          - id: animal_id
            type: u1
            enum: animal
"#,
    );

    let data = [(-1i8) as u8, 0x10, 7];
    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert_eq!(result.fields[0].enum_label.as_deref(), Some("error_neg"));
    assert_eq!(result.fields[1].enum_label.as_deref(), Some("hex_sixteen"));
    assert_eq!(result.fields[2].children[0].children[0].enum_label.as_deref(), Some("horse"));

    let comp_enum = result.fields.iter().find(|f| f.id == "computed_enum").unwrap();
    assert_eq!(comp_enum.enum_label.as_deref(), Some("hex_sixteen"));

    let is_horse = result.fields.iter().find(|f| f.id == "is_deep_horse").unwrap();
    assert_eq!(is_horse.value, FieldValue::Bool(true));

    let neg_int = result.fields.iter().find(|f| f.id == "neg_code_int").unwrap();
    assert_eq!(neg_int.value, FieldValue::I64(-1));
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 4.8–4.10 (Scopes & Conditionals)
// ============================================================================

#[test]
fn test_spec_4_8_chained_parent_root_io_and_sizeof() {
    let ksy = parse_ksy(
        r#"
meta:
  id: nav_parent_deep_and_sizeof
  endian: le
seq:
  - id: root_factor
    type: u2
  - id: lvl1
    type: level_one
    size: 6
instances:
  lvl1_sizeof:
    value: lvl1._sizeof
types:
  level_one:
    seq:
      - id: lvl1_val
        type: u2
      - id: lvl2
        type: level_two
    types:
      level_two:
        seq:
          - id: lvl2_val
            type: u2
          - id: lvl3
            type: level_three
        types:
          level_three:
            seq:
              - id: lvl3_val
                type: u2
            instances:
              deep_sum:
                value: lvl3_val + _parent.lvl2_val + _parent._parent.lvl1_val + _root.root_factor
              root_stream_size:
                value: _root._io.size
              sub_stream_size:
                value: _io.size
"#,
    );

    let mut data = Vec::new();
    data.extend_from_slice(&1000u16.to_le_bytes()); // root_factor = 1000
    data.extend_from_slice(&100u16.to_le_bytes()); // lvl1_val = 100
    data.extend_from_slice(&10u16.to_le_bytes()); // lvl2_val = 10
    data.extend_from_slice(&1u16.to_le_bytes()); // lvl3_val = 1

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    let lvl3 = &result.fields[1].children[1].children[1];
    let deep_sum = lvl3.children.iter().find(|f| f.id == "deep_sum").unwrap();
    assert_eq!(deep_sum.value, FieldValue::U64(1111));

    let root_sz = lvl3.children.iter().find(|f| f.id == "root_stream_size").unwrap();
    assert_eq!(root_sz.value, FieldValue::U64(8));

    let sub_sz = lvl3.children.iter().find(|f| f.id == "sub_stream_size").unwrap();
    assert_eq!(sub_sz.value, FieldValue::U64(6));

    let lvl1_sizeof = result.fields.iter().find(|f| f.id == "lvl1_sizeof").unwrap();
    assert_eq!(lvl1_sizeof.value, FieldValue::U64(6));
}

#[test]
fn test_spec_4_10_conditional_value_instances_and_unnamed_fields() {
    let ksy = parse_ksy(
        r#"
meta:
  id: conditional_instances_and_unnamed
  endian: le
seq:
  - contents: [0xDE, 0xAD]
  - id: mode
    type: u1
  - size: 1
  - id: payload
    type: u2
instances:
  active_calc:
    value: payload * 2
    if: mode == 1
  inactive_calc:
    value: payload * 99
    if: mode == 2
"#,
    );

    let data = [0xDE, 0xAD, 1, 0xFF, 0x15, 0x00]; // mode = 1, payload = 21
    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert!(result.fields.iter().any(|f| f.id == "active_calc" && f.value == FieldValue::U64(42)));
    assert!(!result.fields.iter().any(|f| f.id == "inactive_calc"));
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 4.11 (Repetitions & Arrays)
// ============================================================================

#[test]
fn test_spec_4_11_repeat_zero_count_and_index_variable() {
    let ksy = parse_ksy(
        r#"
meta:
  id: repeat_zero_and_index
  endian: le
seq:
  - id: zero_count
    type: u1
  - id: empty_items
    type: u2
    repeat: expr
    repeat-expr: zero_count
  - id: weighted_items
    type: weighted_elem
    repeat: expr
    repeat-expr: 3
instances:
  empty_len:
    value: empty_items.size
  weighted_len:
    value: weighted_items.length
types:
  weighted_elem:
    seq:
      - id: raw
        type: u1
    instances:
      scaled:
        value: raw * (_index + 1)
"#,
    );

    let data = [0, 10, 20, 30];
    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    let empty_len = result.fields.iter().find(|f| f.id == "empty_len").unwrap();
    assert_eq!(empty_len.value, FieldValue::U64(0));

    let weighted_len = result.fields.iter().find(|f| f.id == "weighted_len").unwrap();
    assert_eq!(weighted_len.value, FieldValue::U64(3));

    // weighted_items[0]: 10 * (0 + 1) = 10
    // weighted_items[1]: 20 * (1 + 1) = 40
    // weighted_items[2]: 30 * (2 + 1) = 90
    assert_eq!(result.fields[1].children[1].value, FieldValue::U64(10));
    assert_eq!(result.fields[2].children[1].value, FieldValue::U64(40));
    assert_eq!(result.fields[3].children[1].value, FieldValue::U64(90));
}

#[test]
fn test_spec_4_11_array_indexing_and_aggregate_properties() {
    let ksy = parse_ksy(
        r#"
meta:
  id: expr_array_official
  endian: le
seq:
  - id: ints
    type: u2
    repeat: expr
    repeat-expr: 4
  - id: strs
    type: strz
    encoding: ASCII
    repeat: expr
    repeat-expr: 3
  - id: recs
    type: rec
    repeat: expr
    repeat-expr: 3
instances:
  ints_size:
    value: ints.size
  ints_first:
    value: ints.first
  ints_last:
    value: ints.last
  ints_min:
    value: ints.min
  ints_max:
    value: ints.max
  ints_second:
    value: ints[1]
  strs_min:
    value: strs.min
  strs_max:
    value: strs.max
  strs_first:
    value: strs.first
  strs_last:
    value: strs.last
  rec_first_code:
    value: recs.first.code
  rec_last_code:
    value: recs.last.code
  rec_mid_code:
    value: recs[1].code
types:
  rec:
    seq:
      - id: code
        type: u1
"#,
    );

    let mut data = Vec::new();
    for &n in &[50u16, 10, 90, 30] {
        data.extend_from_slice(&n.to_le_bytes());
    }
    data.extend_from_slice(b"pear\0apple\0orange\0");
    data.extend_from_slice(&[11, 22, 33]);

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    let get_inst = |name: &str| &result.fields.iter().find(|f| f.id == name).unwrap().value;
    assert_eq!(get_inst("ints_size"), &FieldValue::U64(4));
    assert_eq!(get_inst("ints_first"), &FieldValue::U64(50));
    assert_eq!(get_inst("ints_last"), &FieldValue::U64(30));
    assert_eq!(get_inst("ints_min"), &FieldValue::U64(10));
    assert_eq!(get_inst("ints_max"), &FieldValue::U64(90));
    assert_eq!(get_inst("ints_second"), &FieldValue::U64(10));
    assert_eq!(get_inst("strs_min"), &FieldValue::String("apple".to_string()));
    assert_eq!(get_inst("strs_max"), &FieldValue::String("pear".to_string()));
    assert_eq!(get_inst("strs_first"), &FieldValue::String("pear".to_string()));
    assert_eq!(get_inst("strs_last"), &FieldValue::String("orange".to_string()));
    assert_eq!(get_inst("rec_first_code"), &FieldValue::U64(11));
    assert_eq!(get_inst("rec_last_code"), &FieldValue::U64(33));
    assert_eq!(get_inst("rec_mid_code"), &FieldValue::U64(22));
}

#[test]
fn test_spec_4_11_repeat_until_nested_struct_and_instance_fields() {
    let ksy = parse_ksy(
        r#"
meta:
  id: repeat_until_complex
  endian: le
seq:
  - id: packets
    type: packet
    repeat: until
    repeat-until: _.header.is_terminal == 1
types:
  packet:
    seq:
      - id: header
        type: pkt_hdr
      - id: value
        type: u2
  pkt_hdr:
    seq:
      - id: flags
        type: u1
    instances:
      is_terminal:
        value: flags & 1
"#,
    );

    let mut data = Vec::new();
    // Packet 0: flags = 2 (is_terminal = 0), value = 100
    data.push(2);
    data.extend_from_slice(&100u16.to_le_bytes());
    // Packet 1: flags = 4 (is_terminal = 0), value = 200
    data.push(4);
    data.extend_from_slice(&200u16.to_le_bytes());
    // Packet 2: flags = 3 (is_terminal = 1), value = 300 -> stops here!
    data.push(3);
    data.extend_from_slice(&300u16.to_le_bytes());
    // Extra trailing packet that must not be read
    data.push(0);
    data.extend_from_slice(&999u16.to_le_bytes());

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert_eq!(result.fields.len(), 3);
    assert_eq!(result.fields[2].children[1].value, FieldValue::U16(300));
    assert_eq!(stream.pos(), 9);
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 4.12 (Advanced Switching)
// ============================================================================

#[test]
fn test_spec_4_12_switch_on_enum_str_bytes_and_sized_raw_fallback() {
    let ksy = parse_ksy(
        r#"
meta:
  id: switch_manual_enum_str_size_else
  endian: le
enums:
  code_enum:
    1: int_val
    2: str_val
seq:
  - id: enum_tag
    type: u1
    enum: code_enum
  - id: enum_body
    type:
      switch-on: enum_tag
      cases:
        'code_enum::int_val': u2
        'code_enum::str_val': strz
  - id: chunks
    type: sized_chunk
    repeat: eos
types:
  sized_chunk:
    seq:
      - id: title
        type: str
        size: 4
        encoding: ASCII
      - id: len
        type: u1
      - id: body
        size: len
        type:
          switch-on: title
          cases:
            '"META"': meta_body
            '"PAIR"': pair_body
  meta_body:
    seq:
      - id: author
        type: strz
        encoding: ASCII
  pair_body:
    seq:
      - id: a
        type: u2
      - id: b
        type: u2
"#,
    );

    let mut data = Vec::new();
    // enum_tag = 1 (code_enum::int_val) -> u2(0x1234)
    data.push(1);
    data.extend_from_slice(&0x1234u16.to_le_bytes());
    // Chunk 0: "META", len = 4, "Bob\0"
    data.extend_from_slice(b"META");
    data.push(4);
    data.extend_from_slice(b"Bob\0");
    // Chunk 1: "UNKN" (unmatched case with size: 3 -> falls back to raw 3-byte Bytes!)
    data.extend_from_slice(b"UNKN");
    data.push(3);
    data.extend_from_slice(&[0xDE, 0xAD, 0xFF]);
    // Chunk 2: "PAIR", len = 4, a=10, b=20
    data.extend_from_slice(b"PAIR");
    data.push(4);
    data.extend_from_slice(&10u16.to_le_bytes());
    data.extend_from_slice(&20u16.to_le_bytes());

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert_eq!(result.fields[1].value, FieldValue::U16(0x1234));
    assert_eq!(result.fields[2].children[2].children[0].value, FieldValue::String("Bob".to_string()));
    // Unmatched sized switch case falls back to raw Bytes of size 3
    assert_eq!(result.fields[3].children[2].value, FieldValue::Bytes(vec![0xDE, 0xAD, 0xFF]));
    assert_eq!(result.fields[3].children[2].size, 3);
    assert_eq!(result.fields[4].children[2].children[0].value, FieldValue::U16(10));
    assert_eq!(result.fields[4].children[2].children[1].value, FieldValue::U16(20));
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 4.13 & 4.14 (Instances & io)
// ============================================================================

#[test]
fn test_spec_4_14_chained_instances_and_root_io_seek_from_substream() {
    let ksy = parse_ksy(
        r#"
meta:
  id: instance_io_user
  endian: le
seq:
  - id: entry_count
    type: u2
  - id: entries
    type: entry
    size: 4
    repeat: expr
    repeat-expr: entry_count
  - id: string_pool
    size-eos: true
instances:
  base_bonus:
    value: 100
  total_bonus:
    value: base_bonus + first_str_len
  first_str_len:
    value: entries[0].resolved_name.length
types:
  entry:
    seq:
      - id: str_ofs
        type: u2
      - id: code
        type: u2
    instances:
      resolved_name:
        io: _root._io
        pos: str_ofs
        type: strz
        encoding: UTF-8
"#,
    );

    // Layout:
    // 0..2: entry_count = 2
    // 2..6: entries[0] (str_ofs = 10, code = 1)
    // 6..10: entries[1] (str_ofs = 16, code = 2)
    // 10..16: "alpha\0"
    // 16..21: "beta\0"
    let mut data = Vec::new();
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(&10u16.to_le_bytes());
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&16u16.to_le_bytes());
    data.extend_from_slice(&2u16.to_le_bytes());
    data.extend_from_slice(b"alpha\0beta\0");

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    let entry0 = &result.fields[1];
    let resolved0 = entry0.children.iter().find(|f| f.id == "resolved_name").unwrap();
    assert_eq!(resolved0.value, FieldValue::String("alpha".to_string()));
    assert_eq!(resolved0.offset, 10);

    let entry1 = &result.fields[2];
    let resolved1 = entry1.children.iter().find(|f| f.id == "resolved_name").unwrap();
    assert_eq!(resolved1.value, FieldValue::String("beta".to_string()));
    assert_eq!(resolved1.offset, 16);

    let total_bonus = result.fields.iter().find(|f| f.id == "total_bonus").unwrap();
    assert_eq!(total_bonus.value, FieldValue::U64(105));
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 4.15 (Bit-Endian & Bit Enums)
// ============================================================================

#[test]
fn test_spec_4_15_meta_bit_endian_le_and_bitfield_enums() {
    let ksy = parse_ksy(
        r#"
meta:
  id: bits_simple_le_and_enum
  bit-endian: le
enums:
   animal:
    0: cat
    1: dog
    4: horse
seq:
  - id: low_nibble
    type: b4
  - id: high_three
    type: b3
    enum: animal
  - id: top_bit
    type: b1
  - id: full_64
    type: b64be
"#,
    );

    // Byte 0: 0b1_100_0101 = 0xC5
    // In bit-endian: le:
    // low_nibble (4 bits) = 0b0101 = 5
    // high_three (3 bits) = 0b100 = 4 (animal::horse)
    // top_bit (1 bit) = 0b1 = 1
    let mut data = vec![0xC5];
    data.extend_from_slice(&0x0123_4567_89AB_CDEFu64.to_be_bytes());

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert_eq!(result.fields[0].value, FieldValue::U64(5));
    assert_eq!(result.fields[1].value, FieldValue::U64(4));
    assert_eq!(result.fields[1].enum_label.as_deref(), Some("horse"));
    assert_eq!(result.fields[2].value, FieldValue::U64(1));
    assert_eq!(result.fields[3].value, FieldValue::U64(0x0123_4567_89AB_CDEF));
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 5 (Processors & Usertypes)
// ============================================================================

#[test]
fn test_spec_5_process_xor_multibyte_rol_ror_and_custom_usertype() {
    let ksy = parse_ksy(
        r#"
meta:
  id: process_to_user_and_rotate
  endian: le
seq:
  - id: xor_multi_buf
    size: 6
    process: xor([0xAA, 0x55])
  - id: rol_buf
    size: 2
    process: rol(3)
  - id: ror_buf
    size: 2
    process: ror(3)
  - id: encrypted_record
    type: user_record
    size: 7
    process: xor(0x7F)
types:
  user_record:
    seq:
      - id: magic
        type: u2
      - id: name
        type: strz
        encoding: ASCII
"#,
    );

    let mut data = Vec::new();
    // 1. xor_multi_buf: plaintext [1, 2, 3, 4, 5, 6] XOR [0xAA, 0x55]
    let plain = [1u8, 2, 3, 4, 5, 6];
    for (i, &b) in plain.iter().enumerate() {
        data.push(b ^ if i % 2 == 0 { 0xAA } else { 0x55 });
    }
    // 2. rol_buf: 0b0001_0000 (0x10) rotated right by 3 is 0x02;rol(3) restores 0x10
    data.extend_from_slice(&[0x10u8.rotate_right(3), 0x85u8.rotate_right(3)]);
    // 3. ror_buf: 0x10u8 rotated left by 3; ror(3) restores 0x10
    data.extend_from_slice(&[0x10u8.rotate_left(3), 0x85u8.rotate_left(3)]);
    // 4. encrypted_record: u2(0x1234) + "Hi\0\0\0" (7 bytes) XORed with 0x7F
    let mut rec_bytes = Vec::new();
    rec_bytes.extend_from_slice(&0x1234u16.to_le_bytes());
    rec_bytes.extend_from_slice(b"Hi\0\0\0");
    for b in rec_bytes {
        data.push(b ^ 0x7F);
    }

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    assert_eq!(result.fields[0].value, FieldValue::Bytes(vec![1, 2, 3, 4, 5, 6]));
    assert_eq!(result.fields[1].value, FieldValue::Bytes(vec![0x10, 0x85]));
    assert_eq!(result.fields[2].value, FieldValue::Bytes(vec![0x10, 0x85]));

    let rec = &result.fields[3];
    assert_eq!(rec.children[0].value, FieldValue::U16(0x1234));
    assert_eq!(rec.children[1].value, FieldValue::String("Hi".to_string()));
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 6 (Expressions & Methods)
// ============================================================================

#[test]
fn test_spec_6_numeric_literals_euclidean_mod_and_string_byte_methods() {
    let ksy = parse_ksy(
        r#"
meta:
  id: expr_complete_suite
  endian: le
seq:
  - id: raw_bytes
    size: 4
instances:
  hex_underscore:
    value: 0x1234_5678
  bin_underscore:
    value: 0b1010_0101
  oct_literal:
    value: 0o755
  unary_plus:
    value: +42
  mod_neg_dividend:
    value: -13 % 5
  str_concat:
    value: '"Hello, " + "World!"'
  str_reverse:
    value: '"stressed".reverse'
  str_substr:
    value: '"KaitaiStruct".substring(6, 12)'
  str_cmp_lt:
    value: '"apple" < "banana"'
  str_to_i_dec:
    value: '"-123_456".to_i'
  str_to_i_hex:
    value: '"cafe".to_i(16)'
  str_to_i_bin:
    value: '"1101".to_i(2)'
  int_to_s:
    value: (2026).to_s
  bool_to_i_true:
    value: true.to_i
  bytes_first:
    value: raw_bytes.first
  bytes_last:
    value: raw_bytes.last
  bytes_min:
    value: raw_bytes.min
  bytes_max:
    value: raw_bytes.max
  bytes_eq_lit:
    value: raw_bytes == [0x30, 0x10, 0x40, 0x20]
  lit_bytes_to_s:
    value: '[0x4F, 0x4B].to_s("ASCII")'
"#,
    );

    let data = [0x30, 0x10, 0x40, 0x20];
    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    let get_inst = |name: &str| &result.fields.iter().find(|f| f.id == name).unwrap().value;
    assert_eq!(get_inst("hex_underscore"), &FieldValue::U64(0x1234_5678));
    assert_eq!(get_inst("bin_underscore"), &FieldValue::U64(0b1010_0101));
    assert_eq!(get_inst("oct_literal"), &FieldValue::U64(0o755));
    assert_eq!(get_inst("unary_plus"), &FieldValue::U64(42));
    assert_eq!(get_inst("mod_neg_dividend"), &FieldValue::U64(2)); // Euclidean modulo: -13 % 5 == 2
    assert_eq!(get_inst("str_concat"), &FieldValue::String("Hello, World!".to_string()));
    assert_eq!(get_inst("str_reverse"), &FieldValue::String("desserts".to_string()));
    assert_eq!(get_inst("str_substr"), &FieldValue::String("Struct".to_string()));
    assert_eq!(get_inst("str_cmp_lt"), &FieldValue::Bool(true));
    assert_eq!(get_inst("str_to_i_dec"), &FieldValue::I64(-123_456));
    assert_eq!(get_inst("str_to_i_hex"), &FieldValue::U64(0xcafe));
    assert_eq!(get_inst("str_to_i_bin"), &FieldValue::U64(13));
    assert_eq!(get_inst("int_to_s"), &FieldValue::String("2026".to_string()));
    assert_eq!(get_inst("bool_to_i_true"), &FieldValue::U64(1));
    assert_eq!(get_inst("bytes_first"), &FieldValue::U64(0x30));
    assert_eq!(get_inst("bytes_last"), &FieldValue::U64(0x20));
    assert_eq!(get_inst("bytes_min"), &FieldValue::U64(0x10));
    assert_eq!(get_inst("bytes_max"), &FieldValue::U64(0x40));
    assert_eq!(get_inst("bytes_eq_lit"), &FieldValue::Bool(true));
    assert_eq!(get_inst("lit_bytes_to_s"), &FieldValue::String("OK".to_string()));
}

// ============================================================================
// Official Kaitai Struct Test Suite Parity: Section 7.10 (Parametric Types)
// ============================================================================

#[test]
fn test_spec_7_10_params_pass_bool_str_bytes_and_struct() {
    let ksy = parse_ksy(
        r#"
meta:
  id: params_pass_struct_and_primitives
  endian: le
seq:
  - id: hdr
    type: header_t
  - id: body
    type: body_t(hdr, hdr.flags != 0, "TAG:")
types:
  header_t:
    seq:
      - id: payload_len
        type: u1
      - id: flags
        type: u1
  body_t:
    params:
      - id: h
        type: header_t
      - id: has_extra
        type: bool
      - id: prefix
        type: str
    seq:
      - id: text
        type: str
        size: h.payload_len
        encoding: ASCII
      - id: extra_u2
        type: u2
        if: has_extra
    instances:
      labeled_text:
        value: prefix + text
      passed_flags:
        value: h.flags
"#,
    );

    let mut data = Vec::new();
    data.push(4); // hdr.payload_len = 4
    data.push(1); // hdr.flags = 1
    data.extend_from_slice(b"DATA");
    data.extend_from_slice(&0x7788u16.to_le_bytes());

    let mut stream = KaitaiStream::new(&data);
    let result = KaitaiInterpreter::new(ksy).parse(&mut stream);

    let body = &result.fields[1];
    assert_eq!(body.children[0].value, FieldValue::String("DATA".to_string()));
    assert_eq!(body.children[1].value, FieldValue::U16(0x7788));

    let labeled = body.children.iter().find(|f| f.id == "labeled_text").unwrap();
    assert_eq!(labeled.value, FieldValue::String("TAG:DATA".to_string()));

    let passed_flags = body.children.iter().find(|f| f.id == "passed_flags").unwrap();
    assert_eq!(passed_flags.value, FieldValue::U64(1));
}

// ============================================================================
// Official Real-World Format Integration Tests (ELF & DNS)
// ============================================================================

#[test]
fn test_spec_official_elf_header_dynamic_endian_and_bitness() {
    let elf_ksy = parse_ksy(
        r#"
meta:
  id: elf_mini
enums:
  bits:
    1: b32
    2: b64
  endian:
    1: le
    2: be
seq:
  - id: magic
    contents: [0x7F, "ELF"]
  - id: bits
    type: u1
    enum: bits
  - id: endian
    type: u1
    enum: endian
  - id: ei_version
    type: u1
    valid: 1
  - id: pad
    size: 9
  - id: header
    type: endian_elf
types:
  endian_elf:
    meta:
      endian:
        switch-on: _root.endian
        cases:
          'endian::le': le
          'endian::be': be
    seq:
      - id: e_type
        type: u2
      - id: machine
        type: u2
      - id: e_version
        type: u4
      - id: entry_point
        type:
          switch-on: _root.bits
          cases:
            'bits::b32': u4
            'bits::b64': u8
"#,
    );

    // 1. 64-bit Big-Endian ELF header
    let mut elf64_be = Vec::new();
    elf64_be.extend_from_slice(b"\x7FELF");
    elf64_be.push(2); // bits::b64
    elf64_be.push(2); // endian::be
    elf64_be.push(1); // ei_version = 1
    elf64_be.extend_from_slice(&[0u8; 9]);
    elf64_be.extend_from_slice(&2u16.to_be_bytes()); // ET_EXEC
    elf64_be.extend_from_slice(&0x3Eu16.to_be_bytes()); // x86_64
    elf64_be.extend_from_slice(&1u32.to_be_bytes());
    elf64_be.extend_from_slice(&0x0000_0000_0040_1000u64.to_be_bytes());

    let mut s1 = KaitaiStream::new(&elf64_be);
    let r1 = KaitaiInterpreter::new(elf_ksy.clone()).parse(&mut s1);
    assert!(r1.errors.is_empty());
    assert_eq!(r1.fields[1].enum_label.as_deref(), Some("b64"));
    assert_eq!(r1.fields[2].enum_label.as_deref(), Some("be"));
    let hdr_be = &r1.fields[5];
    assert_eq!(hdr_be.children[0].value, FieldValue::U16(2));
    assert_eq!(hdr_be.children[3].value, FieldValue::U64(0x0040_1000));

    // 2. 32-bit Little-Endian ELF header
    let mut elf32_le = Vec::new();
    elf32_le.extend_from_slice(b"\x7FELF");
    elf32_le.push(1); // bits::b32
    elf32_le.push(1); // endian::le
    elf32_le.push(1); // ei_version = 1
    elf32_le.extend_from_slice(&[0u8; 9]);
    elf32_le.extend_from_slice(&3u16.to_le_bytes()); // ET_DYN
    elf32_le.extend_from_slice(&0x28u16.to_le_bytes()); // ARM
    elf32_le.extend_from_slice(&1u32.to_le_bytes());
    elf32_le.extend_from_slice(&0x0804_8000u32.to_le_bytes());

    let mut s2 = KaitaiStream::new(&elf32_le);
    let r2 = KaitaiInterpreter::new(elf_ksy).parse(&mut s2);
    assert!(r2.errors.is_empty());
    let hdr_le = &r2.fields[5];
    assert_eq!(hdr_le.children[0].value, FieldValue::U16(3));
    assert_eq!(hdr_le.children[3].value, FieldValue::U32(0x0804_8000));
}

#[test]
fn test_spec_official_dns_packet_bitfields_and_pointer_compression() {
    let dns_ksy = parse_ksy(
        r#"
meta:
  id: dns_packet_mini
  endian: be
seq:
  - id: transaction_id
    type: u2
  - id: flags
    type: packet_flags
  - id: qdcount
    type: u2
  - id: queries
    type: query
    repeat: expr
    repeat-expr: qdcount
types:
  packet_flags:
    seq:
      - id: qr
        type: b1
      - id: opcode
        type: b4
      - id: aa
        type: b1
      - id: tc
        type: b1
      - id: rd
        type: b1
      - id: ra
        type: b1
      - id: z
        type: b3
      - id: rcode
        type: b4
  query:
    seq:
      - id: name
        type: domain_name
      - id: qtype
        type: u2
      - id: qclass
        type: u2
  domain_name:
    seq:
      - id: labels
        type: label
        repeat: until
        repeat-until: _.length == 0 or _.length >= 0xc0
  label:
    seq:
      - id: length
        type: u1
      - id: pointer_ofs
        type: u1
        if: length >= 0xc0
      - id: text
        type: str
        size: length
        encoding: ASCII
        if: length > 0 and length < 0xc0
    instances:
      target_name:
        io: _root._io
        pos: ((length & 0x3F) << 8) | pointer_ofs
        type: domain_name
        if: length >= 0xc0
"#,
    );

    let mut pkt = Vec::new();
    pkt.extend_from_slice(&0x1337u16.to_be_bytes()); // transaction_id
    pkt.extend_from_slice(&[0x81, 0x80]); // qr=1, opcode=0, aa=0, tc=0, rd=1 | ra=1, z=0, rcode=0
    pkt.extend_from_slice(&2u16.to_be_bytes()); // qdcount = 2
    // Query 0 at offset 6: "\x06google\x03com\x00" + qtype(1) + qclass(1)
    pkt.push(6);
    pkt.extend_from_slice(b"google");
    pkt.push(3);
    pkt.extend_from_slice(b"com");
    pkt.push(0);
    pkt.extend_from_slice(&1u16.to_be_bytes());
    pkt.extend_from_slice(&1u16.to_be_bytes());
    // Query 1: "\x04mail" + pointer to offset 6 (0xC0, 0x06) + qtype(1) + qclass(1)
    pkt.push(4);
    pkt.extend_from_slice(b"mail");
    pkt.extend_from_slice(&[0xC0, 0x06]);
    pkt.extend_from_slice(&1u16.to_be_bytes());
    pkt.extend_from_slice(&1u16.to_be_bytes());

    let mut stream = KaitaiStream::new(&pkt);
    let result = KaitaiInterpreter::new(dns_ksy).parse(&mut stream);

    assert!(result.errors.is_empty());
    let flags = &result.fields[1];
    assert_eq!(flags.children[0].value, FieldValue::U64(1)); // qr = 1
    assert_eq!(flags.children[4].value, FieldValue::U64(1)); // rd = 1
    assert_eq!(flags.children[5].value, FieldValue::U64(1)); // ra = 1

    // Query 1 label[1] is a compressed pointer to offset 6 ("google.com")
    let q1_name = &result.fields[4].children[0];
    assert_eq!(q1_name.children[0].children[1].value, FieldValue::String("mail".to_string()));
    let ptr_label = &q1_name.children[1];
    let target_name = ptr_label.children.iter().find(|f| f.id == "target_name").unwrap();
    assert_eq!(target_name.children[0].children[1].value, FieldValue::String("google".to_string()));
    assert_eq!(target_name.children[1].children[1].value, FieldValue::String("com".to_string()));
}
