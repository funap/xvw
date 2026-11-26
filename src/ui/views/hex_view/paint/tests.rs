use super::super::types::{ByteCategoryPalette, ColumnFadeRuns, FadeEdgeKind, HexGroupInfo, VerticalFadeRun};
use super::color::{ascii_item_text_color, contrast_ratio, hex_item_text_color, selection_background_color};
use super::description::{
    DescriptionPalette, build_continuation_field_runs, build_leaf_field_runs, build_structure_header_runs, calculate_description_indent_level,
    clean_single_line, span_x_from_glyph_positions,
};
use super::glyphs::{ColorRunTracker, HexInsertCursorParams, clip_column_mask, hex_insert_cursor_geometry};
use crate::core::radix::{ByteCategoryColors, ByteColoring, ByteGroupSize, CategoryColor, DisplayRadix, HexCase};
use crate::core::structure::IndexedField;
use gpui_kit::component::theme::Theme;
use gpui_kit::gpui::Font;
use gpui_kit::{hsla, px};

#[test]
fn insert_cursor_advances_through_four_byte_group() {
    let group = HexGroupInfo {
        chunk_start: 0,
        chunk_end: 4,
        start_slot: 0,
        text_start: 0,
        text_end: 8,
    };

    let positions: Vec<f32> = (0..4)
        .map(|cursor| {
            let (x, _) = hex_insert_cursor_geometry(
                group,
                cursor,
                HexInsertCursorParams {
                    radix: DisplayRadix::Hexadecimal,
                    group_size: ByteGroupSize::Four,
                    is_big_endian: false,
                    selection_active: true,
                    has_pending_nibble: false,
                    origin_x: px(0.0),
                    cell_width: px(1.0),
                },
            )
            .expect("cursor is inside the group");
            f32::from(x)
        })
        .collect();

    assert_eq!(positions, vec![0.0, 2.0, 4.0, 6.0]);

    // When has_pending_nibble is true, cursor advances by 1 character
    let (x_nibble1, _) = hex_insert_cursor_geometry(
        group,
        0,
        HexInsertCursorParams {
            radix: DisplayRadix::Hexadecimal,
            group_size: ByteGroupSize::Four,
            is_big_endian: false,
            selection_active: true,
            has_pending_nibble: true,
            origin_x: px(0.0),
            cell_width: px(1.0),
        },
    )
    .expect("cursor is inside the group");
    assert_eq!(f32::from(x_nibble1), 1.0);
}

#[test]
fn test_selection_background_color() {
    let base = hsla(0.6, 0.8, 0.5, 0.3);

    // Dark mode focused
    let dark_focused = selection_background_color(base, true, true);
    assert_eq!(dark_focused.h, base.h);
    assert_eq!(dark_focused.s, base.s);
    assert_eq!(dark_focused.l, base.l);
    assert_eq!(dark_focused.a, 0.55);

    // Dark mode unfocused
    let dark_unfocused = selection_background_color(base, false, true);
    assert_eq!(dark_unfocused.a, 0.28);

    // Light mode focused
    let light_focused = selection_background_color(base, true, false);
    assert_eq!(light_focused.a, 0.40);

    // Light mode unfocused
    let light_unfocused = selection_background_color(base, false, false);
    assert_eq!(light_unfocused.a, 0.20);
}

#[allow(clippy::field_reassign_with_default)]
fn test_theme() -> Theme {
    let mut theme = Theme::default();
    theme.background = hsla(0.0, 0.0, 0.0, 1.0);
    theme.foreground = hsla(0.0, 0.0, 1.0, 1.0);
    theme.muted_foreground = hsla(0.0, 0.0, 0.5, 1.0);
    theme.accent_foreground = hsla(0.0, 0.0, 0.9, 1.0);
    theme.red = hsla(0.0, 0.8, 0.5, 1.0);
    theme.green = hsla(120.0 / 360.0, 0.8, 0.5, 1.0);
    theme.blue = hsla(210.0 / 360.0, 0.8, 0.5, 1.0);
    theme.yellow = hsla(45.0 / 360.0, 0.8, 0.5, 1.0);
    theme.cyan = hsla(190.0 / 360.0, 0.8, 0.5, 1.0);
    theme.magenta = hsla(280.0 / 360.0, 0.8, 0.5, 1.0);
    theme.border = hsla(0.0, 0.0, 0.3, 1.0);
    theme
}

#[test]
fn test_hex_item_text_color() {
    let theme = test_theme();
    let fg = theme.foreground;
    let muted = theme.muted_foreground;

    let dim_zero_palette = ByteCategoryPalette::from_colors(ByteColoring::DimZero.colors().unwrap(), &theme);
    let uniform_palette = ByteCategoryPalette::from_colors(ByteColoring::Uniform.colors().unwrap(), &theme);
    let dim_padding_palette = ByteCategoryPalette::from_colors(ByteColoring::DimPadding.colors().unwrap(), &theme);
    let category_palette = ByteCategoryPalette::from_colors(ByteColoring::Category.colors().unwrap(), &theme);

    // When focused cursor is on item, use fg
    assert_eq!(hex_item_text_color(true, false, None, &[0x00], &dim_zero_palette, fg), fg);

    // When selected, use fg even if dimmed
    assert_eq!(hex_item_text_color(false, true, None, &[0x00], &dim_zero_palette, fg), fg);

    // When highlighted with dark highlight, ensure contrast keeps it readable
    let dark_hl = hsla(0.0, 0.0, 0.15, 1.0);
    let dark_hl_col = hex_item_text_color(false, false, Some(dark_hl), &[0x00], &dim_zero_palette, fg);
    assert!(contrast_ratio(dark_hl_col, dark_hl) >= 4.5);

    // When highlighted with bright yellow highlight, ensure contrast darkens text for legibility
    let yellow_hl = hsla(45.0 / 360.0, 0.8, 0.5, 1.0);
    let yellow_hl_col = hex_item_text_color(false, false, Some(yellow_hl), &[0x00], &dim_zero_palette, fg);
    assert!(contrast_ratio(yellow_hl_col, yellow_hl) >= 4.5);

    // Uniform mode: always fg
    assert_eq!(hex_item_text_color(false, false, None, &[0x00], &uniform_palette, fg), fg);

    // DimZero mode: 0x00 is dimmed
    assert_eq!(hex_item_text_color(false, false, None, &[0x00], &dim_zero_palette, fg), dim_zero_palette.null);
    assert_eq!(hex_item_text_color(false, false, None, &[0x42], &dim_zero_palette, fg), fg);

    // DimPadding mode: 0x00 and 0xFF are dimmed
    assert_eq!(hex_item_text_color(false, false, None, &[0xFF], &dim_padding_palette, fg), muted);

    // Category mode: distinct colors for different categories
    let null_col = hex_item_text_color(false, false, None, &[0x00], &category_palette, fg);
    let ascii_col = hex_item_text_color(false, false, None, b"A", &category_palette, fg);
    let ws_col = hex_item_text_color(false, false, None, b" ", &category_palette, fg);
    let ctrl_col = hex_item_text_color(false, false, None, &[0x01], &category_palette, fg);
    let non_ascii_col = hex_item_text_color(false, false, None, &[0x80], &category_palette, fg);
    let max_col = hex_item_text_color(false, false, None, &[0xFF], &category_palette, fg);

    assert_eq!(null_col, category_palette.null);
    assert_eq!(ascii_col, category_palette.ascii_printable);
    assert_eq!(ws_col, category_palette.ascii_whitespace);
    assert_eq!(ctrl_col, category_palette.ascii_control);
    assert_eq!(non_ascii_col, category_palette.non_ascii);
    assert_eq!(max_col, category_palette.max);

    assert_ne!(ascii_col, fg);
    assert_ne!(ascii_col, ws_col);
    assert_ne!(ws_col, ctrl_col);
    assert_ne!(ctrl_col, non_ascii_col);
    assert_ne!(non_ascii_col, max_col);

    // Mixed group defaults to fg
    let mixed_col = hex_item_text_color(false, false, None, &[b'A', 0x00], &category_palette, fg);
    assert_eq!(mixed_col, fg);
}

#[test]
fn test_ascii_item_text_color() {
    let theme = test_theme();
    let fg = theme.foreground;
    let muted = theme.muted_foreground;
    let dim_zero_palette = ByteCategoryPalette::from_colors(ByteColoring::DimZero.colors().unwrap(), &theme);
    let category_palette = ByteCategoryPalette::from_colors(ByteColoring::Category.colors().unwrap(), &theme);

    // Selection / highlight takes priority
    assert_eq!(ascii_item_text_color(true, None, b'A', true, &category_palette, fg, muted), fg);
    let hl_col = hsla(45.0 / 360.0, 0.8, 0.5, 1.0);
    let text_col = ascii_item_text_color(false, Some(hl_col), b'A', true, &category_palette, fg, muted);
    assert!(contrast_ratio(text_col, hl_col) >= 4.5);

    // Normal mode (DimZero)
    assert_eq!(ascii_item_text_color(false, None, b'A', true, &dim_zero_palette, fg, muted), fg);
    assert_eq!(ascii_item_text_color(false, None, 0x00, false, &dim_zero_palette, fg, muted), muted);
    assert_eq!(ascii_item_text_color(false, None, 0x01, false, &dim_zero_palette, fg, muted), muted);

    // Category mode
    let printable_col = ascii_item_text_color(false, None, b'A', true, &category_palette, fg, muted);
    let ws_col = ascii_item_text_color(false, None, b' ', true, &category_palette, fg, muted);
    assert_eq!(printable_col, category_palette.ascii_printable);
    assert_eq!(ws_col, category_palette.ascii_whitespace);

    let unprintable_col = ascii_item_text_color(false, None, 0x01, false, &category_palette, fg, muted);
    assert_eq!(unprintable_col, category_palette.ascii_control);
}

#[test]
fn test_alduin_theme_highlight_text_legibility() {
    // Alduin theme uses #9E9E9E as foreground and #facc15 (yellow-400) warning highlight
    let alduin_fg = hsla(0.0, 0.0, 0.62, 1.0); // #9E9E9E
    let warning_hl = hsla(48.0 / 360.0, 0.96, 0.53, 1.0); // #facc15 yellow-400
    let theme = test_theme();
    let category_palette = ByteCategoryPalette::from_colors(ByteColoring::Category.colors().unwrap(), &theme);

    let hex_text = hex_item_text_color(false, false, Some(warning_hl), &[0x41], &category_palette, alduin_fg);
    let hex_ratio = contrast_ratio(hex_text, warning_hl);
    assert!(hex_ratio >= 4.5, "Hex highlight text contrast ratio {hex_ratio} must be >= 4.5 on Alduin theme");

    let ascii_text = ascii_item_text_color(false, Some(warning_hl), b'A', true, &category_palette, alduin_fg, alduin_fg);
    let ascii_ratio = contrast_ratio(ascii_text, warning_hl);
    assert!(
        ascii_ratio >= 4.5,
        "ASCII highlight text contrast ratio {ascii_ratio} must be >= 4.5 on Alduin theme"
    );
}

#[test]
fn test_palette_from_colors() {
    let theme = test_theme();

    let custom_colors = ByteCategoryColors {
        null: CategoryColor::Muted,
        ascii_printable: CategoryColor::Default,
        ascii_whitespace: CategoryColor::Green,
        ascii_control: CategoryColor::Red,
        non_ascii: CategoryColor::Yellow,
        max: CategoryColor::Magenta,
    };

    let custom_palette = ByteCategoryPalette::from_colors(custom_colors, &theme);
    assert_eq!(custom_palette.null, theme.muted_foreground);
    assert_eq!(custom_palette.ascii_printable, theme.foreground);
    assert_eq!(custom_palette.ascii_whitespace, theme.green);
    assert_eq!(custom_palette.ascii_control, theme.red);
    assert_eq!(custom_palette.non_ascii, theme.yellow);
    assert_eq!(custom_palette.max, theme.magenta);
    assert_eq!(custom_palette.default, theme.foreground);
}

#[test]
fn test_build_structure_header_runs() {
    let theme = test_theme();
    let font = Font::default();
    let container = IndexedField {
        id: "sections[0]".into(),
        offset: 0x1E,
        size: 30,
        depth: 1,
        is_instance: false,
        type_name: "section".into(),
        collapse_key: "0x1E:1:sections[0]".into(),
        formatted_value: String::new(),
        enum_label: None,
        description: None,
        color: None,
        validation_failed: false,
        expression: String::new(),
    };

    let palette = DescriptionPalette::from_theme(&theme);

    // Expanded
    let (text_exp, runs_exp) = build_structure_header_runs(&container, false, font.clone(), &palette);
    assert_eq!(text_exp, "sections[0]: section (30B)");
    assert_eq!(runs_exp.iter().map(|r| r.len).sum::<usize>(), text_exp.len());

    // Collapsed
    let (text_col, runs_col) = build_structure_header_runs(&container, true, font, &palette);
    assert_eq!(text_col, "sections[0]: section (30B, collapsed)");
    assert_eq!(runs_col.iter().map(|r| r.len).sum::<usize>(), text_col.len());
}

#[test]
fn test_build_leaf_field_runs() {
    let theme = test_theme();
    let font = Font::default();
    let palette = DescriptionPalette::from_theme(&theme);
    let field = IndexedField {
        id: "magic".into(),
        offset: 0,
        size: 2,
        depth: 1,
        is_instance: false,
        type_name: "u2le".into(),
        collapse_key: "0:1:magic".into(),
        formatted_value: "0x3412 (13330)".into(),
        enum_label: Some("VALID".into()),
        description: Some("magic header".into()),
        color: None,
        validation_failed: false,
        expression: "magic = 3412h (13330)".into(),
    };

    let (text, runs) = build_leaf_field_runs(&field, font.clone(), &palette, true, false);

    assert_eq!(text, "▪ magic: u2le = 0x3412 (13330) (VALID) // magic header");
    assert_eq!(runs.iter().map(|r| r.len).sum::<usize>(), text.len());
    assert!(runs[0].background_color.is_none());

    // Test active highlight
    let (_act_text, act_runs) = build_leaf_field_runs(&field, font.clone(), &palette, true, true);
    assert!(act_runs[0].background_color.is_some());

    let (cont_text, cont_runs) = build_continuation_field_runs("body", 0x10, font.clone(), &palette, HexCase::Lower);
    assert_eq!(cont_text, "↳ body (+0x10)");
    assert_eq!(cont_runs.iter().map(|r| r.len).sum::<usize>(), cont_text.len());

    let (cont_text_upper, _) = build_continuation_field_runs("body", 0x1a, font.clone(), &palette, HexCase::Upper);
    assert_eq!(cont_text_upper, "↳ body (+0x1A)");

    // Test multi-line doc comment and values with newlines are sanitized and single-line
    let multiline_field = IndexedField {
        id: "data".into(),
        offset: 0,
        size: 4,
        depth: 0,
        is_instance: false,
        type_name: "str".into(),
        collapse_key: "0:0:data".into(),
        formatted_value: "\"line1\r\nline2\"".into(),
        enum_label: Some("LABEL\r\nCONTINUED".into()),
        description: Some("\n  Doc summary line\n  Second explanation line\n".into()),
        color: None,
        validation_failed: false,
        expression: "data = \"line1\r\nline2\"".into(),
    };

    let (ml_text, ml_runs) = build_leaf_field_runs(&multiline_field, font, &palette, true, false);

    assert!(!ml_text.contains('\n'));
    assert!(!ml_text.contains('\r'));
    assert_eq!(ml_runs.iter().map(|r| r.len).sum::<usize>(), ml_text.len());
    assert!(ml_text.contains("Doc summary line"));
    assert!(!ml_text.contains("Second explanation line"));
    assert_eq!(clean_single_line("hello\r\nworld\n"), "hello world ");
}

#[test]
fn test_description_palette_contrast_across_all_embedded_themes() {
    use crate::assets::Assets;
    use crate::theme::{load_themes_from_assets, theme_names};

    let registry = load_themes_from_assets(&Assets);
    let names = theme_names(registry.themes());
    assert!(!names.is_empty());

    for name in names {
        let config = registry.themes().get(&name).unwrap();
        let mut theme = Theme::default();
        theme.apply_config(config);

        let palette = DescriptionPalette::from_theme(&theme);
        let bg = theme.background;

        // Verify high contrast for all textual elements across every embedded theme
        assert!(
            contrast_ratio(palette.field_id, bg) >= 4.5,
            "Theme '{}' failed contrast for field_id: ratio was {:.2}:1",
            name,
            contrast_ratio(palette.field_id, bg)
        );
        assert!(
            contrast_ratio(palette.type_name, bg) >= 4.0,
            "Theme '{}' failed contrast for type_name: ratio was {:.2}:1",
            name,
            contrast_ratio(palette.type_name, bg)
        );
        assert!(
            contrast_ratio(palette.number_value, bg) >= 4.0,
            "Theme '{}' failed contrast for number_value: ratio was {:.2}:1",
            name,
            contrast_ratio(palette.number_value, bg)
        );
        assert!(
            contrast_ratio(palette.string_value, bg) >= 4.0,
            "Theme '{}' failed contrast for string_value: ratio was {:.2}:1",
            name,
            contrast_ratio(palette.string_value, bg)
        );
        assert!(
            contrast_ratio(palette.comment, bg) >= 3.5,
            "Theme '{}' failed contrast for comment: ratio was {:.2}:1",
            name,
            contrast_ratio(palette.comment, bg)
        );
    }
}

#[test]
fn test_calculate_description_indent_level_preserves_depth_for_duplicate_element_names() {
    // Scenario like zip.ksy:
    // pk_section.body is at depth 1 (container)
    // extra_field.body is at depth 5 (container)
    // local_file.body is at depth 2 (leaf)
    let outer_body_container = IndexedField {
        id: "body".into(),
        offset: 4,
        size: 100,
        depth: 1,
        is_instance: false,
        type_name: "local_file".into(),
        collapse_key: "4:1:body".into(),
        formatted_value: String::new(),
        enum_label: None,
        description: None,
        color: None,
        validation_failed: false,
        expression: String::new(),
    };

    let inner_body_container = IndexedField {
        id: "body".into(),
        offset: 40,
        size: 20,
        depth: 5,
        is_instance: false,
        type_name: "extended_timestamp".into(),
        collapse_key: "40:5:body".into(),
        formatted_value: String::new(),
        enum_label: None,
        description: None,
        color: None,
        validation_failed: false,
        expression: String::new(),
    };

    let leaf_body = IndexedField {
        id: "body".into(),
        offset: 60,
        size: 40,
        depth: 2,
        is_instance: false,
        type_name: "bytes".into(),
        collapse_key: "60:2:body".into(),
        formatted_value: "[40 bytes]".into(),
        enum_label: None,
        description: None,
        color: None,
        validation_failed: false,
        expression: "body = [40 bytes]".into(),
    };

    // 1. Header row for outer body: container.depth = 1
    let outer_indent = calculate_description_indent_level(None, Some(&outer_body_container), None, 1);
    assert_eq!(outer_indent, 1, "outer body must have indent level 1");

    // 2. Header row for inner body: container.depth = 5 (even when active_ranges has 6 items or ancestor has depth 1)
    let inner_indent = calculate_description_indent_level(None, Some(&inner_body_container), None, 6);
    assert_eq!(inner_indent, 5, "inner body must have indent level 5, not be overridden to ancestor depth");

    // 3. Leaf row for payload body: leaf.depth = 2 (even when active_ranges has 3 items)
    let leaf_indent = calculate_description_indent_level(None, None, Some(&leaf_body), 3);
    assert_eq!(leaf_indent, 2, "leaf body must have indent level 2");

    // 4. Continuation row for payload body: continuation_leaf.depth = 2
    let cont_indent = calculate_description_indent_level(Some(&leaf_body), None, None, 3);
    assert_eq!(cont_indent, 2, "continuation body must preserve indent level 2");
}

#[test]
fn test_span_x_from_glyph_positions() {
    // Two leaf fields separated by `"  "` (indices 10..12) on a 160px line.
    let glyphs = [(0, 0.0), (5, 35.0), (10, 70.0), (11, 77.0), (12, 84.0), (18, 126.0)];

    assert_eq!(span_x_from_glyph_positions(glyphs, 160.0, 0, 10), (0.0, 70.0));
    assert_eq!(span_x_from_glyph_positions(glyphs, 160.0, 12, 24), (84.0, 76.0));
    assert_eq!(span_x_from_glyph_positions(glyphs, 160.0, 10, 10), (0.0, 0.0));
    assert_eq!(span_x_from_glyph_positions(glyphs, 160.0, 30, 40), (0.0, 0.0));
}

#[test]
fn test_color_run_tracker_coalesces_and_clips_masks() {
    use gpui_kit::{Bounds, hsla, point, px, size};

    let c1 = hsla(0.1, 0.5, 0.5, 0.8);
    let c2 = hsla(0.6, 0.5, 0.5, 0.8);
    let transparent = hsla(0.0, 0.0, 0.0, 0.0);

    let inputs = [Some(c1), Some(c1), Some(c1), Some(c2), None, Some(transparent), Some(c2), Some(c2)];
    let mut runs = Vec::new();
    let mut tracker = ColorRunTracker::default();
    for (i, c) in inputs.into_iter().enumerate() {
        tracker.push(i, c, |s, e, color| runs.push((s, e, color)));
    }
    tracker.finish(|s, e, color| runs.push((s, e, color)));
    assert_eq!(runs, vec![(0, 2, c1), (3, 3, c2), (6, 7, c2)]);

    let scroll_bounds = Bounds::new(point(px(100.0), px(20.0)), size(px(300.0), px(20.0)));
    let partial_col = Bounds::new(point(px(50.0), px(20.0)), size(px(120.0), px(20.0)));
    let offscreen_col = Bounds::new(point(px(420.0), px(20.0)), size(px(80.0), px(20.0)));

    let clipped = clip_column_mask(scroll_bounds, partial_col);
    assert_eq!(clipped.origin.x, px(100.0));
    assert_eq!(clipped.size.width, px(70.0));
    assert_eq!(clip_column_mask(scroll_bounds, offscreen_col).size.width, px(0.0));
}

#[test]
fn test_column_fade_runs_coalesce_split_and_skip_empty() {
    use gpui_kit::{Bounds, point, px, size};

    let mut runs = ColumnFadeRuns::default();
    let mask = Bounds::new(point(px(100.0), px(0.0)), size(px(200.0), px(22.0)));
    let empty_mask = Bounds::new(point(px(100.0), px(0.0)), size(px(0.0), px(22.0)));

    // Zero-height and zero-width spans are ignored.
    runs.push(FadeEdgeKind::HexLeft, 0, px(100.0), mask, 40.0, 40.0);
    runs.push(FadeEdgeKind::HexLeft, 1, px(100.0), empty_mask, 40.0, 62.0);
    assert!(runs.runs_for(FadeEdgeKind::HexLeft).is_empty());

    // Contiguous rows coalesce.
    runs.push(FadeEdgeKind::HexLeft, 10, px(100.0), mask, 100.0, 122.0);
    runs.push(FadeEdgeKind::HexLeft, 11, px(100.0), mask, 122.0, 144.0);
    runs.push(FadeEdgeKind::HexLeft, 12, px(100.0), mask, 144.0, 166.0);

    // A missing/banner row at row 13 splits the run.
    runs.push(FadeEdgeKind::HexLeft, 14, px(100.0), mask, 188.0, 210.0);
    runs.push(FadeEdgeKind::HexLeft, 15, px(100.0), mask, 210.0, 232.0);

    assert_eq!(
        runs.runs_for(FadeEdgeKind::HexLeft),
        &[
            VerticalFadeRun {
                fade_start_x: 100.0,
                mask_left: 100.0,
                mask_width: 200.0,
                top_y: 100.0,
                bottom_y: 166.0,
                last_row_idx: 12,
            },
            VerticalFadeRun {
                fade_start_x: 100.0,
                mask_left: 100.0,
                mask_width: 200.0,
                top_y: 188.0,
                bottom_y: 232.0,
                last_row_idx: 15,
            },
        ]
    );

    runs.clear();
    assert!(runs.runs_for(FadeEdgeKind::HexLeft).is_empty());
}
