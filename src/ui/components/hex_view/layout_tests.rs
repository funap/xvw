use super::layout::{build_ascii_char_map, calculate_scroll_top_for_range, centered_glyph_offset};
use super::paint::highlight_color_for_range;
use super::types::{AUTO_FIT_SCAN_BYTES, AsciiChar, HexViewLayout, HexViewLayoutState, HorizontalScrollTarget, LayoutInput, ScrollColumn};
use super::{
    HexView, ascii_byte_index_from_world_x, bounded_auto_fit_range, build_hex_text_source, can_chain_to_outer, hex_grid_width, hex_grid_x,
    make_hex_view_layout, weighted_text_width,
};
use crate::core::buffer::Buffer;
use crate::core::document::Document;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, DisplayRadix};
use crate::core::structure::types::{FieldValue, ParseResult, ParsedField};
use gpui_kit::{hsla, px};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

fn layout(width: f32, is_struct_mode: bool, show_offset: bool, show_ascii: bool) -> HexViewLayout {
    make_hex_view_layout(LayoutInput {
        bounds_width: width,
        is_struct_mode,
        show_ascii,
        ascii_col_width: 160.0,
        ascii_inner_max: 0.0,
        fixed_column_width: if is_struct_mode || show_offset { 80.0 } else { 0.0 },
        hex_col_width: 200.0,
        desc_col_width: 240.0,
        comment_col_width: 300.0,
        hex_inner_max: 120.0,
        desc_inner_max: 80.0,
        comment_inner_max: 60.0,
        section_gap: 16.0,
        content_padding: 8.0,
        scrollbar_width: 12.0,
    })
}

fn check_layout_and_scrolling() {
    let l = layout(600.0, false, true, true);
    assert_eq!(l.fixed_width, 96.0);
    assert_eq!(l.hex.start, l.fixed_width);
    assert_eq!(l.hex.inner_max, 120.0);
    assert!(l.outer_max > 0.0);
    assert!(l.outer_max < l.content_width);

    let l_no_offset = layout(800.0, false, false, true);
    assert_eq!(l_no_offset.fixed_width, 16.0);
    assert_eq!(l_no_offset.hex.start, 16.0);

    let l_hit = layout(600.0, false, true, true);
    let outer_scroll_x = 40.0;
    let hex_x = l_hit.fixed_width + 10.0;
    assert_eq!(l_hit.column_at(hex_x, outer_scroll_x), Some(ScrollColumn::Hex));
    let ascii_x = l_hit.ascii.expect("ASCII column").start - outer_scroll_x + 10.0;
    assert_eq!(l_hit.column_at(ascii_x, outer_scroll_x), Some(ScrollColumn::Ascii));
    let fixed_x = l_hit.fixed_width - 2.0;
    assert_eq!(l_hit.column_at(fixed_x, outer_scroll_x), None);

    let l_struct = layout(600.0, true, true, false);
    let description = l_struct.description.expect("description column");
    assert!(l_struct.ascii.is_none());
    assert_eq!(l_struct.max_offset(HorizontalScrollTarget::Column(ScrollColumn::Description)), 80.0);
    assert_eq!(l_struct.progress(HorizontalScrollTarget::Column(ScrollColumn::Description), 40.0), 0.5);
    assert_eq!(l_struct.column_at(description.start + 10.0, 0.0), Some(ScrollColumn::Description));

    let l_zero = make_hex_view_layout(LayoutInput {
        bounds_width: 1200.0,
        is_struct_mode: false,
        show_ascii: false,
        ascii_col_width: 160.0,
        ascii_inner_max: 0.0,
        fixed_column_width: 80.0,
        hex_col_width: 200.0,
        desc_col_width: 240.0,
        comment_col_width: 300.0,
        hex_inner_max: 0.0,
        desc_inner_max: 0.0,
        comment_inner_max: 0.0,
        section_gap: 16.0,
        content_padding: 8.0,
        scrollbar_width: 12.0,
    });
    assert_eq!(l_zero.progress(HorizontalScrollTarget::Column(ScrollColumn::Hex), 10.0), 0.0);

    let l_ascii = make_hex_view_layout(LayoutInput {
        bounds_width: 600.0,
        is_struct_mode: false,
        show_ascii: true,
        ascii_col_width: 320.0,
        ascii_inner_max: 0.0,
        fixed_column_width: 80.0,
        hex_col_width: 200.0,
        desc_col_width: 240.0,
        comment_col_width: 300.0,
        hex_inner_max: 0.0,
        desc_inner_max: 0.0,
        comment_inner_max: 0.0,
        section_gap: 16.0,
        content_padding: 8.0,
        scrollbar_width: 12.0,
    });
    assert_eq!(l_ascii.ascii.expect("ASCII column").width, 320.0);

    let l_ascii_scroll = make_hex_view_layout(LayoutInput {
        bounds_width: 600.0,
        is_struct_mode: false,
        show_ascii: true,
        ascii_col_width: 80.0,
        ascii_inner_max: 80.0,
        fixed_column_width: 80.0,
        hex_col_width: 200.0,
        desc_col_width: 240.0,
        comment_col_width: 300.0,
        hex_inner_max: 0.0,
        desc_inner_max: 0.0,
        comment_inner_max: 0.0,
        section_gap: 16.0,
        content_padding: 8.0,
        scrollbar_width: 12.0,
    });
    assert_eq!(l_ascii_scroll.max_offset(HorizontalScrollTarget::Column(ScrollColumn::Ascii)), 80.0);
    assert_eq!(l_ascii_scroll.progress(HorizontalScrollTarget::Column(ScrollColumn::Ascii), 40.0), 0.5);
    let ascii_col = l_ascii_scroll.ascii.expect("ASCII column");
    assert_eq!(ascii_byte_index_from_world_x(ascii_col.start + 5.0, ascii_col, 0.0), 0);
    assert_eq!(ascii_byte_index_from_world_x(ascii_col.start + 5.0, ascii_col, 20.0), 2);

    for column in [ScrollColumn::Hex, ScrollColumn::Ascii, ScrollColumn::Description, ScrollColumn::Comment] {
        assert!(can_chain_to_outer(HorizontalScrollTarget::Column(column), 1.0));
    }
    assert!(!can_chain_to_outer(HorizontalScrollTarget::View, 1.0));
    assert!(!can_chain_to_outer(HorizontalScrollTarget::Column(ScrollColumn::Hex), 0.0));
}

fn check_character_mapping_and_auto_fit() {
    let buffer = "€".as_bytes();
    let map = build_ascii_char_map(Encoding::Utf8, buffer, 0, 2);
    assert_eq!(map, vec![None, Some(AsciiChar::Printable('€', 3))]);

    let map2 = build_ascii_char_map(Encoding::Utf8, buffer, 1, 2);
    assert_eq!(map2, vec![None, None]);

    // Shift-JIS spanning row boundaries: "あい" is [0x82, 0xA0, 0x82, 0xA2]
    let sjis_bytes = [0x82, 0xA0, 0x82, 0xA2];
    let map_sjis_row1 = build_ascii_char_map(Encoding::ShiftJis, &sjis_bytes, 0, 1);
    assert_eq!(map_sjis_row1, vec![Some(AsciiChar::Printable('あ', 2))]);

    let map_sjis_row2 = build_ascii_char_map(Encoding::ShiftJis, &sjis_bytes, 1, 3);
    assert_eq!(map_sjis_row2, vec![None, Some(AsciiChar::Printable('い', 2)), None]);

    // Shift-JIS multi-byte character crossing 16-byte row boundary with following characters
    let mut sjis_doc = vec![b'X'];
    sjis_doc.extend_from_slice(&[0x82, 0xA0, 0x82, 0xA2, 0x82, 0xA4, 0x82, 0xA6, 0x82, 0xA8, 0x82, 0xAA, 0x82, 0xAC]);
    sjis_doc.extend_from_slice(&[0x82, 0xAE]); // 'く' (offsets 15, 16)
    sjis_doc.extend_from_slice(&[0x82, 0xB0]); // 'け' (offsets 17, 18)
    sjis_doc.extend_from_slice(&[0x82, 0xB2]); // 'こ' (offsets 19, 20)

    let row1 = build_ascii_char_map(Encoding::ShiftJis, &sjis_doc, 0, 16);
    assert_eq!(row1[0], Some(AsciiChar::Printable('X', 1)));
    assert_eq!(row1[1], Some(AsciiChar::Printable('あ', 2)));
    assert_eq!(row1[15], Some(AsciiChar::Printable('ぐ', 2)));

    let row2 = build_ascii_char_map(Encoding::ShiftJis, &sjis_doc, 16, 16);
    assert_eq!(row2[0], None); // Continuation byte of 'ぐ' skipped without corruption
    assert_eq!(row2[1], Some(AsciiChar::Printable('げ', 2))); // 'げ' decoded cleanly
    assert_eq!(row2[3], Some(AsciiChar::Printable('ご', 2))); // 'ご' decoded cleanly

    // User report regression test:
    // 00000380: 81 41 93 c7 82 dd 8d 9e 82 de 83 41 83 76 83 8a (|、読み込むアプリ|)
    // 00000390: 82 cc 95 b6 8e 9a 83 52 81 5b 83 68 82 aa 83 59 (|の文字コードがズ|)
    let mut sample_bytes = vec![0u8; 0x380];
    sample_bytes.extend_from_slice(&[0x81, 0x41, 0x93, 0xc7, 0x82, 0xdd, 0x8d, 0x9e, 0x82, 0xde, 0x83, 0x41, 0x83, 0x76, 0x83, 0x8a]);
    sample_bytes.extend_from_slice(&[0x82, 0xcc, 0x95, 0xb6, 0x8e, 0x9a, 0x83, 0x52, 0x81, 0x5b, 0x83, 0x68, 0x82, 0xaa, 0x83, 0x59]);

    let row_380 = build_ascii_char_map(Encoding::ShiftJis, &sample_bytes, 0x380, 16);
    assert_eq!(row_380[0], Some(AsciiChar::Printable('、', 2)));
    assert_eq!(row_380[14], Some(AsciiChar::Printable('リ', 2)));

    let row_390 = build_ascii_char_map(Encoding::ShiftJis, &sample_bytes, 0x390, 16);
    assert_eq!(row_390[0], Some(AsciiChar::Printable('の', 2))); // 82 CC correctly decoded as 'の', not blank + 'フ'
    assert_eq!(row_390[2], Some(AsciiChar::Printable('文', 2)));
    assert_eq!(row_390[4], Some(AsciiChar::Printable('字', 2)));
    assert_eq!(row_390[6], Some(AsciiChar::Printable('コ', 2)));
    assert_eq!(row_390[8], Some(AsciiChar::Printable('ー', 2)));
    assert_eq!(row_390[10], Some(AsciiChar::Printable('ド', 2)));
    assert_eq!(row_390[12], Some(AsciiChar::Printable('が', 2)));
    assert_eq!(row_390[14], Some(AsciiChar::Printable('ズ', 2)));

    // Character range selection check in multi-byte encodings
    assert_eq!(Encoding::ShiftJis.char_range_at(&sample_bytes, 0x390), 0x390..0x392);
    assert_eq!(Encoding::ShiftJis.char_range_at(&sample_bytes, 0x391), 0x390..0x392);
    assert_eq!(Encoding::ShiftJis.char_range_at(&sample_bytes, 0x392), 0x392..0x394);

    let range = bounded_auto_fit_range(1024 * 1024, 500_000, 500_512);
    assert_eq!(range.len(), AUTO_FIT_SCAN_BYTES);
    assert!(range.start <= 500_000);
    assert!(range.end >= 500_512);

    assert_eq!(bounded_auto_fit_range(1024, 100, 200), 0..1024);
    let range_eof = bounded_auto_fit_range(1024 * 1024, 1_020_000, 1_020_512);
    assert_eq!(range_eof.end, 1024 * 1024);
    assert!(range_eof.start <= 1_020_000);
    assert_eq!(range_eof.len(), AUTO_FIT_SCAN_BYTES);
}

fn check_structure_and_highlights() {
    let doc = Arc::new(RwLock::new(Document::new(PathBuf::from("test.bin"), Buffer::new(vec![0; 32]))));
    let editor = Editor::new(doc);

    let field1 = ParsedField {
        id: "magic".into(),
        field_type: "u4".into(),
        offset: 0,
        size: 4,
        value: FieldValue::U32(0x12345678),
        color: crate::core::color::RgbaColor::default(),
        description: None,
        children: vec![],
        enum_label: None,
        is_instance: false,
    };
    let field2 = ParsedField {
        id: "flags".into(),
        field_type: "u4".into(),
        offset: 4,
        size: 4,
        value: FieldValue::U32(0x00000001),
        color: crate::core::color::RgbaColor::default(),
        description: None,
        children: vec![],
        enum_label: None,
        is_instance: false,
    };
    let field3 = ParsedField {
        id: "version".into(),
        field_type: "u4".into(),
        offset: 8,
        size: 4,
        value: FieldValue::U32(2),
        color: crate::core::color::RgbaColor::default(),
        description: None,
        children: vec![],
        enum_label: None,
        is_instance: false,
    };

    let container = ParsedField {
        id: "header".into(),
        field_type: "Header".into(),
        offset: 0,
        size: 12,
        value: FieldValue::Struct,
        color: crate::core::color::RgbaColor::default(),
        description: None,
        children: vec![field1.clone(), field2.clone(), field3.clone()],
        enum_label: None,
        is_instance: false,
    };

    let parse_result = ParseResult::new("test_struct".into(), vec![container], 12, vec![]);
    let char_w = 8.0;

    let width_expanded = HexView::description_content_width_in_range(&editor, &parse_result, &(0..16), char_w);
    let single_field_width = weighted_text_width(&field1.format_expression(), char_w);
    assert!(
        width_expanded > single_field_width * 2.5,
        "Expanded row width ({width_expanded}) must aggregate all fields, strictly greater than a single field ({single_field_width})"
    );

    let mut editor_collapsed = Editor::new(Arc::new(RwLock::new(Document::new(PathBuf::from("test.bin"), Buffer::new(vec![0; 32])))));
    editor_collapsed.structure.collapsed_struct_ids.insert("header".into());
    let width_collapsed = HexView::description_content_width_in_range(&editor_collapsed, &parse_result, &(0..16), char_w);
    assert!(
        width_collapsed < width_expanded,
        "Collapsed width ({width_collapsed}) should be smaller than expanded width ({width_expanded})"
    );

    let source = build_hex_text_source(&[0x12, 0x34, 0x56], 0, DisplayRadix::Hexadecimal, ByteGroupSize::One, false);
    let cell_width = px(8.0);
    assert_eq!(source.text.as_ref(), "12 34 56");
    assert_eq!(f32::from(hex_grid_x(source.groups[0].text_start, cell_width)), 0.0);
    assert_eq!(f32::from(hex_grid_x(source.groups[1].text_start, cell_width)), 24.0);
    assert_eq!(f32::from(hex_grid_x(source.groups[2].text_start, cell_width)), 48.0);
    assert_eq!(f32::from(hex_grid_width(source.text.len(), cell_width)), 64.0);

    let source2 = build_hex_text_source(&[0x12], 1, DisplayRadix::Hexadecimal, ByteGroupSize::Four, false);
    assert_eq!(source2.text.as_ref(), "..12....");
    assert_eq!(f32::from(hex_grid_x(source2.groups[0].text_start, cell_width)), 0.0);
    assert_eq!(f32::from(hex_grid_x(source2.groups[0].text_end, cell_width)), 64.0);
    assert_eq!(f32::from(hex_grid_width(source2.text.len(), cell_width)), 64.0);

    assert_eq!(centered_glyph_offset(10.0, 6.0), 2.0);
    assert_eq!(centered_glyph_offset(10.0, 12.0), 0.0);

    let state = HexViewLayoutState {
        address_col_width: 120.0,
        hex_col_width: 350.0,
        desc_col_width: 280.0,
        comment_col_width: 400.0,
        ascii_col_width: 180.0,
        show_offset: false,
        show_ascii: true,
        show_header: false,
        scroll_offset: 42,
        outer_scroll_x: 15.0,
        hex_scroll_x: 25.0,
        ascii_scroll_x: 35.0,
        desc_scroll_x: 45.0,
        comment_scroll_x: 55.0,
    };
    let cloned = state.clone();
    assert_eq!(cloned.address_col_width, 120.0);
    assert_eq!(cloned.scroll_offset, 42);

    let highlight_color = hsla(0.1, 0.8, 0.5, 0.35);
    let highlights = [(0..16, highlight_color)];
    assert_eq!(highlight_color_for_range(4, 8, &highlights), Some(highlight_color));
    assert_eq!(highlight_color_for_range(20, 24, &highlights), None);
}

fn check_ascii_non_printable_mapping() {
    let ascii_bytes = [b'H', b'e', 0x00, 0x1F, b'!', b'.', 0x7F, 0xFF];
    let map = build_ascii_char_map(Encoding::Ascii, &ascii_bytes, 0, 8);
    assert_eq!(
        map,
        vec![
            Some(AsciiChar::Printable('H', 1)),
            Some(AsciiChar::Printable('e', 1)),
            Some(AsciiChar::NonPrintable),
            Some(AsciiChar::NonPrintable),
            Some(AsciiChar::Printable('!', 1)),
            Some(AsciiChar::Printable('.', 1)),
            Some(AsciiChar::NonPrintable),
            Some(AsciiChar::NonPrintable),
        ]
    );

    // Verify helper methods on AsciiChar
    assert_eq!(AsciiChar::Printable('A', 1).character(), 'A');
    assert_eq!(AsciiChar::Printable('A', 1).byte_len(), 1);
    assert!(AsciiChar::Printable('A', 1).is_printable());

    assert_eq!(AsciiChar::NonPrintable.character(), '.');
    assert_eq!(AsciiChar::NonPrintable.byte_len(), 1);
    assert!(!AsciiChar::NonPrintable.is_printable());

    // Verify EOF boundary handling: past-EOF cells remain None
    let short_buffer = [0x00, b'A'];
    let map_short = build_ascii_char_map(Encoding::Ascii, &short_buffer, 0, 4);
    assert_eq!(map_short, vec![Some(AsciiChar::NonPrintable), Some(AsciiChar::Printable('A', 1)), None, None,]);
}

fn check_calculate_scroll_top_for_range() {
    // 1. Target is already inside visible rows (10..=19 for top=10, visible=10, total=100)
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 10, 10), None);
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 15, 15), None);
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 19, 19), None);
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 12, 17), None);
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 10, 19), None);

    // 2. Target is above visible rows (4-row top margin above start_row)
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 5, 5), Some(1));
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 0, 0), Some(0));
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 4, 8), Some(0));
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 8, 15), Some(4));

    // 3. Target is below visible rows (4-row top margin above start_row)
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 20, 20), Some(16));
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 25, 25), Some(21));
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 20, 24), Some(16));
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 15, 22), Some(11));

    // 4. Target range larger than viewport (span >= visible_rows)
    // Aligns 4-row margin above start_row
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 30, 50), Some(26));
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 10, 30), Some(6));

    // 5. Total rows / document boundary clamping
    assert_eq!(calculate_scroll_top_for_range(0, 10, 50, 48, 48), Some(40));
    assert_eq!(calculate_scroll_top_for_range(0, 10, 50, 49, 49), Some(40));
    assert_eq!(calculate_scroll_top_for_range(0, 10, 50, 60, 60), Some(40));

    // 6. Degenerate and edge cases
    assert_eq!(calculate_scroll_top_for_range(0, 0, 50, 5, 5), Some(1)); // visible_rows = 0 treated as 1
    assert_eq!(calculate_scroll_top_for_range(10, 10, 100, 8, 4), Some(0)); // inverted range normalized
    assert_eq!(calculate_scroll_top_for_range(0, 10, 1, 0, 0), None); // single-row document
}

fn check_hex_editing_state() {
    use super::input_controller::{HexCommit, HexInputResult, PendingHexInput};
    use crate::ui::components::hex_view::types::EditColumn;

    let mut input = crate::ui::components::hex_view::InputController::new();
    assert!(!input.has_pending());
    assert_eq!(input.pending_hex_digit(), None);
    assert!(input.is_hex());
    assert!(!input.is_ascii());

    // Switch column
    input.set_active_column(EditColumn::Ascii);
    assert!(input.is_ascii());
    assert!(!input.is_hex());
    input.set_active_column(EditColumn::Hex);

    // 1. Two-digit typing sequence (0x0A then 0x0B -> 0xAB)
    let res1 = input.handle_hex_digit(0x0a, 0, None, false, DisplayRadix::Hexadecimal);
    assert_eq!(res1, HexInputResult::Pending(PendingHexInput { offset: 0, digit: 0x0a }));
    assert!(input.has_pending());
    assert_eq!(input.pending_hex_digit(), Some((0, 0x0a)));

    let res2 = input.handle_hex_digit(0x0b, 0, None, false, DisplayRadix::Hexadecimal);
    assert_eq!(
        res2,
        HexInputResult::Commit(HexCommit {
            offset: 0,
            value: 0xab,
            replacement_range: None,
        })
    );
    assert!(!input.has_pending());
    assert_eq!(input.pending_hex_digit(), None);

    // 2. Navigation commit with zero padding (typing 0x01 then moving away -> 0x01)
    let res3 = input.handle_hex_digit(0x01, 2, None, false, DisplayRadix::Hexadecimal);
    assert_eq!(res3, HexInputResult::Pending(PendingHexInput { offset: 2, digit: 0x01 }));
    let commit = input.commit_pending_with_zero(DisplayRadix::Hexadecimal, false);
    assert_eq!(
        commit,
        Some(HexCommit {
            offset: 2,
            value: 0x01,
            replacement_range: None,
        })
    );
    assert!(!input.has_pending());

    // 3. Selection replacement
    let res4 = input.handle_hex_digit(0x0f, 0, Some(3..7), false, DisplayRadix::Hexadecimal);
    assert_eq!(res4, HexInputResult::Pending(PendingHexInput { offset: 3, digit: 0x0f }));
    let res5 = input.handle_hex_digit(0x0e, 3, None, false, DisplayRadix::Hexadecimal);
    assert_eq!(
        res5,
        HexInputResult::Commit(HexCommit {
            offset: 3,
            value: 0xfe,
            replacement_range: Some(3..7),
        })
    );

    // 4. Cancel pending (e.g. Backspace / Escape)
    input.handle_hex_digit(0x09, 10, None, false, DisplayRadix::Hexadecimal);
    assert!(input.has_pending());
    assert!(input.cancel_pending());
    assert!(!input.has_pending());
    assert!(!input.cancel_pending());

    // 5. Read-only and non-hex radix rejection
    assert_eq!(input.handle_hex_digit(0x01, 0, None, true, DisplayRadix::Hexadecimal), HexInputResult::Ignored);
    assert_eq!(input.handle_hex_digit(0x01, 0, None, false, DisplayRadix::Binary), HexInputResult::Ignored);

    // 6. ASCII encoding
    assert_eq!(input.encode_ascii_char('A', Encoding::Utf8, false), Some(vec![0x41]));
    assert_eq!(input.encode_ascii_char('\n', Encoding::Utf8, false), None);
    assert_eq!(input.encode_ascii_char('A', Encoding::Utf8, true), None);

    // 7. 16-bit LE navigation
    assert_eq!(crate::core::radix::next_visual_byte(1, 4, ByteGroupSize::Two, false), 0);
    assert_eq!(crate::core::radix::next_visual_byte(0, 4, ByteGroupSize::Two, false), 3);
    assert_eq!(crate::core::radix::prev_visual_byte(0, 4, ByteGroupSize::Two, false), 1);
    assert_eq!(crate::core::radix::prev_visual_byte(3, 4, ByteGroupSize::Two, false), 0);
}

#[test]
fn test_hex_view_layout_suite() {
    check_layout_and_scrolling();
    check_character_mapping_and_auto_fit();
    check_structure_and_highlights();
    check_ascii_non_printable_mapping();
    check_calculate_scroll_top_for_range();
    check_hex_editing_state();
}
