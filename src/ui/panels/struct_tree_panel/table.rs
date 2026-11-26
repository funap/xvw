use super::model::{
    AUTOFIT_HORIZONTAL_PADDING, AUTOFIT_MAX_WIDTH, AUTOFIT_MIN_WIDTH, COLUMN_GAP, FlattenedField, StructColumn, StructValueStyle, TREE_INDENT_WIDTH,
    TREE_INDICATOR_WIDTH, categorize_field_value, estimated_text_width, resolve_field_at_path, value_text,
};
use super::{CopyFieldName, CopyFieldOffset, CopyFieldValue, StructTreeView};
use crate::core::appearance::Appearance;
use crate::core::format::format_with_commas;
use crate::core::radix::{DisplayRadix, HexCase};
use crate::core::structure::{ParseResult, ParsedField};
use crate::ui::icon::IconName;
use crate::ui::views::hex_view::paint::DescriptionPalette;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu};
use gpui_kit::component::table::{Column, TableDelegate, TableState};
use gpui_kit::component::{StyledExt as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::collections::HashMap;

pub struct StructTreeTableDelegate {
    pub(super) panel: WeakEntity<StructTreeView>,
    pub(super) flattened_fields: Vec<FlattenedField>,
    pub(super) parse_result: Option<std::sync::Arc<ParseResult>>,
    pub(super) value_radix: DisplayRadix,
    pub(super) selected_index: Option<usize>,
    pub(super) visible_columns: Vec<StructColumn>,
    pub(super) column_widths: HashMap<StructColumn, Pixels>,
    pub(super) focus_handle: FocusHandle,
}

impl StructTreeTableDelegate {
    fn field_at_path<'a>(&'a self, path: &[usize]) -> Option<&'a ParsedField> {
        resolve_field_at_path(&self.parse_result, path)
    }
}

impl TableDelegate for StructTreeTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        self.visible_columns.len()
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.flattened_fields.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        let Some(col) = self.visible_columns.get(col_ix) else {
            return Column::default();
        };

        let width = self.column_widths.get(col).copied().unwrap_or_else(|| col.default_width());
        Column::new(col.key(), col.name()).width(width).min_width(col.min_width()).resizable(true)
    }

    fn fit_column(&mut self, col_ix: usize, _window: &mut Window, _cx: &mut Context<TableState<Self>>) -> Option<Pixels> {
        let col = self.visible_columns.get(col_ix)?;
        let mut max_width = estimated_text_width(col.name());
        if *col == StructColumn::Field {
            max_width += TREE_INDICATOR_WIDTH + COLUMN_GAP;
        }

        let hex_case = _cx.try_global::<HexCase>().copied().unwrap_or_default();
        for field in self.flattened_fields.iter().take(128) {
            let Some(parsed_field) = self.field_at_path(&field.path) else {
                continue;
            };

            let text = match col {
                StructColumn::Field => parsed_field.id.clone(),
                StructColumn::Address => hex_case.format_prefixed(parsed_field.offset),
                StructColumn::Type => {
                    if parsed_field.field_type.is_empty() {
                        if field.has_children { "struct".to_string() } else { "value".to_string() }
                    } else {
                        parsed_field.field_type.clone()
                    }
                }
                StructColumn::Size => format!("{} B", format_with_commas(field.size)),
                StructColumn::Value => value_text(parsed_field, self.value_radix, hex_case),
            };
            let mut text_width = estimated_text_width(&text);
            if *col == StructColumn::Field {
                text_width += field.depth as f32 * TREE_INDENT_WIDTH + TREE_INDICATOR_WIDTH + COLUMN_GAP;
            }
            max_width = max_width.max(text_width);
        }

        let width = (max_width + AUTOFIT_HORIZONTAL_PADDING).clamp(AUTOFIT_MIN_WIDTH, AUTOFIT_MAX_WIDTH);
        self.column_widths.insert(*col, px(width));
        Some(px(width))
    }

    fn render_th(&mut self, col_ix: usize, _window: &mut Window, _cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let col = self.visible_columns.get(col_ix);
        let name = col.map(|c| c.name()).unwrap_or_default();
        let is_address_visible = self.visible_columns.contains(&StructColumn::Address);
        let is_type_visible = self.visible_columns.contains(&StructColumn::Type);
        let is_size_visible = self.visible_columns.contains(&StructColumn::Size);
        let is_value_visible = self.visible_columns.contains(&StructColumn::Value);
        let value_radix = self.value_radix;
        let context_focus = self.focus_handle.clone();

        let header_content = if col == Some(&StructColumn::Field) {
            h_flex()
                .size_full()
                .items_center()
                .gap_1()
                .child(div().flex_shrink_0().w(px(TREE_INDICATOR_WIDTH)).h(px(TREE_INDICATOR_WIDTH)))
                .child(div().flex_1().min_w_0().truncate().whitespace_nowrap().child(name))
                .into_any_element()
        } else {
            div().size_full().child(name).into_any_element()
        };

        div().size_full().child(header_content).context_menu(move |menu, window, cx| {
            menu.action_context(context_focus.clone())
                .label("Visible columns")
                .separator()
                .menu_with_check("Address", is_address_visible, Box::new(crate::actions::ToggleStructureAddressColumn))
                .menu_with_check("Type", is_type_visible, Box::new(crate::actions::ToggleStructureTypeColumn))
                .menu_with_check("Size", is_size_visible, Box::new(crate::actions::ToggleStructureSizeColumn))
                .menu_with_check("Value", is_value_visible, Box::new(crate::actions::ToggleStructureValueColumn))
                .separator()
                .submenu("Value format", window, cx, move |menu, _window, _cx| {
                    menu.menu_with_check(
                        "Hexadecimal (16)",
                        value_radix == DisplayRadix::Hexadecimal,
                        Box::new(crate::actions::SetRadixHex),
                    )
                    .menu_with_check("Decimal (10)", value_radix == DisplayRadix::Decimal, Box::new(crate::actions::SetRadixDec))
                    .menu_with_check("Octal (8)", value_radix == DisplayRadix::Octal, Box::new(crate::actions::SetRadixOct))
                    .menu_with_check("Binary (2)", value_radix == DisplayRadix::Binary, Box::new(crate::actions::SetRadixBin))
                })
        })
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, window: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(field) = self.flattened_fields.get(row_ix) else {
            return menu;
        };
        let Some(parsed_field) = self.field_at_path(&field.path) else {
            return menu;
        };
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let field_id = parsed_field.id.clone();
        let field_offset = parsed_field.offset;
        let field_value = value_text(parsed_field, self.value_radix, hex_case);
        let value_radix = self.value_radix;
        let offset_hex = hex_case.format_prefixed_pad(field_offset, 8);
        let context_focus = self.focus_handle.clone();

        let value_format_submenu = PopupMenu::build(window, cx, move |menu, _window, _cx| {
            menu.menu_with_check(
                "Hexadecimal (16)",
                value_radix == DisplayRadix::Hexadecimal,
                Box::new(crate::actions::SetRadixHex),
            )
            .menu_with_check("Decimal (10)", value_radix == DisplayRadix::Decimal, Box::new(crate::actions::SetRadixDec))
            .menu_with_check("Octal (8)", value_radix == DisplayRadix::Octal, Box::new(crate::actions::SetRadixOct))
            .menu_with_check("Binary (2)", value_radix == DisplayRadix::Binary, Box::new(crate::actions::SetRadixBin))
        });

        menu.action_context(context_focus)
            .menu_with_icon(
                format!("Go to Offset ({})", offset_hex),
                IconName::ChevronRight,
                Box::new(crate::actions::NavigateToOffset {
                    target: offset_hex.clone(),
                    target_file: None,
                }),
            )
            .separator()
            .menu_with_icon(
                format!("Copy Value ({})", field_value),
                IconName::Copy,
                Box::new(CopyFieldValue { value: field_value }),
            )
            .menu_with_icon(
                format!("Copy Field Name ({})", field_id),
                IconName::Copy,
                Box::new(CopyFieldName { name: field_id }),
            )
            .menu_with_icon(
                format!("Copy Offset ({})", offset_hex),
                IconName::Hash,
                Box::new(CopyFieldOffset { offset: offset_hex }),
            )
            .separator()
            .item(gpui_kit::component::menu::PopupMenuItem::submenu("Value format", value_format_submenu))
            .separator()
            .menu_with_icon("Copy Structure Analysis", IconName::Copy, Box::new(crate::actions::CopyStructureResult))
            .menu_with_icon(
                "Export Structure as YAML",
                IconName::HardDriveDownload,
                Box::new(crate::actions::ExportStructureYaml),
            )
            .separator()
            .menu_with_icon("Expand All", IconName::ChevronDown, Box::new(crate::actions::ExpandAllStructure))
            .menu_with_icon("Collapse All", IconName::ChevronUp, Box::new(crate::actions::CollapseAllStructure))
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let Some(field) = self.flattened_fields.get(row_ix) else {
            return div().into_any_element();
        };
        let Some(parsed_field) = self.field_at_path(&field.path) else {
            return div().into_any_element();
        };
        let Some(col) = self.visible_columns.get(col_ix) else {
            return div().into_any_element();
        };

        let theme = cx.theme();
        let palette = DescriptionPalette::from_theme(theme);
        let font_family = cx.global::<Appearance>().font_family.clone();
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let is_selected = self.selected_index == Some(row_ix);

        match col {
            StructColumn::Field => {
                let id = SharedString::from(parsed_field.id.clone());
                let chevron_symbol = if field.has_children {
                    if field.is_collapsed { "▶" } else { "▼" }
                } else {
                    " "
                };
                let has_children = field.has_children;
                let depth = field.depth;
                let panel = self.panel.clone();

                h_flex()
                    .items_center()
                    .gap_1()
                    .child(
                        h_flex()
                            .flex_shrink_0()
                            .w(px(TREE_INDENT_WIDTH * depth as f32 + TREE_INDICATOR_WIDTH))
                            .h(px(TREE_INDICATOR_WIDTH))
                            .items_center()
                            .justify_end()
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .w(px(TREE_INDICATOR_WIDTH))
                                    .h(px(TREE_INDICATOR_WIDTH))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .when(has_children, |this| {
                                        this.cursor_pointer().on_mouse_down(MouseButton::Left, move |_event, _window, cx| {
                                            cx.stop_propagation();
                                            if let Some(panel) = panel.upgrade() {
                                                cx.defer(move |cx| {
                                                    panel.update(cx, |this, cx| {
                                                        this.toggle_collapse_at(row_ix, cx);
                                                    });
                                                });
                                            }
                                        })
                                    })
                                    .child(chevron_symbol),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .whitespace_nowrap()
                            .text_sm()
                            .font_medium()
                            .text_color(palette.field_id)
                            .child(id),
                    )
                    .into_any_element()
            }
            StructColumn::Address => {
                let field_offset = parsed_field.offset;
                div()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(if is_selected { theme.accent_foreground } else { theme.muted_foreground })
                    .truncate()
                    .whitespace_nowrap()
                    .child(hex_case.format_prefixed(field_offset))
                    .into_any_element()
            }
            StructColumn::Type => {
                let type_name = if parsed_field.field_type.is_empty() {
                    if field.has_children { "struct" } else { "value" }
                } else {
                    parsed_field.field_type.as_str()
                };
                let type_label = SharedString::from(type_name.to_string());
                div()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(palette.type_name)
                    .truncate()
                    .whitespace_nowrap()
                    .child(type_label)
                    .into_any_element()
            }
            StructColumn::Size => div()
                .text_xs()
                .font_family(font_family)
                .text_color(palette.number_value)
                .truncate()
                .whitespace_nowrap()
                .child(format!("{} B", format_with_commas(field.size)))
                .into_any_element(),
            StructColumn::Value => match categorize_field_value(parsed_field, self.value_radix, hex_case) {
                StructValueStyle::StructSummary(count) => {
                    let text = format!("{} child{}", count, if count == 1 { "" } else { "ren" });
                    div()
                        .text_xs()
                        .font_family(font_family)
                        .text_color(theme.muted_foreground)
                        .truncate()
                        .whitespace_nowrap()
                        .child(text)
                        .into_any_element()
                }
                StructValueStyle::Enum { value, label } => h_flex()
                    .items_center()
                    .text_xs()
                    .font_family(font_family)
                    .truncate()
                    .whitespace_nowrap()
                    .min_w_0()
                    .child(div().text_color(palette.number_value).child(value))
                    .child(div().text_color(palette.punctuation).child(" ("))
                    .child(div().text_color(palette.enum_label).child(label))
                    .child(div().text_color(palette.punctuation).child(")"))
                    .into_any_element(),
                StructValueStyle::Number(val_str) => div()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(palette.number_value)
                    .truncate()
                    .whitespace_nowrap()
                    .child(val_str)
                    .into_any_element(),
                StructValueStyle::String(val_str) => div()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(palette.string_value)
                    .truncate()
                    .whitespace_nowrap()
                    .child(val_str)
                    .into_any_element(),
                StructValueStyle::Bytes(val_str) => div()
                    .text_xs()
                    .font_family(font_family)
                    .text_color(palette.bytes_value)
                    .truncate()
                    .whitespace_nowrap()
                    .child(val_str)
                    .into_any_element(),
            },
        }
    }
}
