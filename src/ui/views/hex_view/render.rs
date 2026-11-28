use super::HexView;
use super::actions::*;
use super::layout::*;
use super::paint::*;
use super::row_cache::*;
use super::types::*;

use crate::actions::{
    AddCustomBreak, BookmarkBlue, BookmarkCyan, BookmarkGreen, BookmarkOrange, BookmarkPink, BookmarkPurple, BookmarkRed, BookmarkYellow, ClearAllBookmarks,
    ClearAllCustomBreaks, ClearBookmark, ClearStructureDefinition, Copy, CopyAsBase64, CopyAsBinary, CopyAsCppArray, CopyAsEscapedString, CopyAsHexDump,
    CopyAsHexSpaces, CopyAsHexStream, CopyAsJsonArray, CopyAsPrintableText, CopyAsRustArray, Cut, ExportBookmarks, FillSelection, HideAllBookmarks,
    ImportBookmarks, JoinLine, LoadStructureDefinition, Paste, Redo, RemoveCustomBreaksInRange, SelectAll as AppSelectAll, ShowAllBookmarks, ShowBookmarksTab,
    ShowOnlyBookmarkBlue, ShowOnlyBookmarkCyan, ShowOnlyBookmarkGreen, ShowOnlyBookmarkOrange, ShowOnlyBookmarkPink, ShowOnlyBookmarkPurple,
    ShowOnlyBookmarkRed, ShowOnlyBookmarkYellow, ShowStructureTab, ToggleBookmarkBlue, ToggleBookmarkCyan, ToggleBookmarkGreen, ToggleBookmarkOrange,
    ToggleBookmarkPink, ToggleBookmarkPurple, ToggleBookmarkRed, ToggleBookmarkYellow, ToggleHideUnbookmarked, ToggleInlineStructureView, ToggleSearch, Undo,
    UnfoldBookmarkAtCursor,
};
use crate::app_state::InsertModeState;
use crate::core::editor::Editor;
use crate::core::radix::{ByteCategoryColors, HexCase};
use crate::ui::icon::IconName;
use gpui_kit::component::menu::ContextMenuExt;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::sync::Arc;

fn header_clip_indicator(is_left: bool, theme: &gpui_kit::component::Theme) -> Div {
    let base = div()
        .absolute()
        .top_0()
        .bottom_0()
        .w(px(16.0))
        .flex()
        .items_center()
        .bg(theme.sidebar)
        .text_xs()
        .font_semibold()
        .text_color(theme.muted_foreground)
        .child("…");
    if is_left {
        base.left_0().justify_start().pl(px(2.0))
    } else {
        base.right_0().justify_end().pr(px(2.0))
    }
}

fn header_focus_flash_overlay(id_prefix: &'static str, epoch: usize, primary: Hsla) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .with_animation((id_prefix, epoch), Animation::new(super::COLUMN_FOCUS_FLASH_DURATION), move |el, progress| {
            let alpha = column_focus_flash_alpha(progress);
            el.bg(primary.opacity(alpha * 0.10))
                .child(div().absolute().left_0().right_0().bottom_0().h(px(2.0)).bg(primary.opacity(alpha)))
        })
}

impl Render for HexView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();
        let is_focused = self.focus_handle.is_focused(window);
        let insert_mode = InsertModeState::is_enabled(cx);
        let cursor_visible = insert_mode && is_focused && window.is_window_active() && self.cursor_blink.read(cx).visible();
        let theme = cx.theme();
        let font_family = self.font_family_prop.clone();
        let font_size = self.font_size_prop;
        let row_height = self.row_height();
        let header_height = self.header_height();
        let ascii_cell_width = self.ascii_cell_width();

        let (total_rows, bytes_per_row, max_bytes_per_row, is_struct_mode, total_size, line_starts, reveal_cursor_offset) = {
            let editor = self.editor.read(cx);
            let line_starts = editor.line_starts();
            let total_size = editor.total_size();
            let reveal_cursor_offset = if insert_mode { editor.insert_cursor_offset() } else { editor.cursor.offset };
            let total_rows = Self::effective_total_rows_for(&line_starts, total_size, reveal_cursor_offset);
            (
                total_rows,
                line_starts.bytes_per_row(),
                line_starts.max_bytes_per_row(),
                editor.structure.show_inline_structure_view && editor.parse_result().is_some(),
                total_size,
                line_starts,
                reveal_cursor_offset,
            )
        };
        let bytes_per_row_changed = self.last_bytes_per_row != Some(bytes_per_row) || self.last_max_bytes_per_row != Some(max_bytes_per_row);
        if bytes_per_row_changed {
            self.last_bytes_per_row = Some(bytes_per_row);
            self.last_max_bytes_per_row = Some(max_bytes_per_row);
            self.ascii_col_width = self.default_ascii_col_width(max_bytes_per_row);
            self.cursor_reveal_pending = true;
            self.row_cache.borrow_mut().clear();
        }
        let ascii_col_width = self.effective_ascii_col_width(max_bytes_per_row);

        let container = div()
            .id(("hex-view-root", cx.entity_id()))
            .relative()
            .flex()
            .flex_col()
            .bg(theme.background)
            .font_family(font_family.clone())
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .key_context(CONTEXT);

        let hex_cell_width = if self.hex_cell_width > 0.0 {
            px(self.hex_cell_width)
        } else {
            let measured = measure_hex_cell_width(window, font(font_family.clone()), font_size);
            self.hex_cell_width = f32::from(measured);
            measured
        };
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        self.ensure_header_probe(max_bytes_per_row, hex_case);
        let probe_text_len = self.cached_header_probe.as_ref().expect("header probe cached").source.text.len();
        let total_data_width = f32::from(hex_grid_width(probe_text_len, hex_cell_width));
        self.hex_content_width = total_data_width;
        if bytes_per_row_changed || self.hex_col_width <= 0.0 {
            self.hex_col_width = total_data_width;
        }
        let max_hex_scroll = (total_data_width - self.hex_col_width).max(0.0);
        self.scroll.hex_scroll_x = self.scroll.hex_scroll_x.clamp(0.0, max_hex_scroll);
        let layout = self.current_layout_with(max_bytes_per_row, is_struct_mode, cx);
        self.scroll.outer_scroll_x = self.scroll.outer_scroll_x.clamp(0.0, layout.outer_max);
        self.scroll.outer_scroll_handle.set_offset(point(-px(self.scroll.outer_scroll_x), px(0.0)));

        // Keep the cursor visible using the same fixed grid as the paint pass.
        let should_reveal_cursor = self.cursor_reveal_pending || self.last_cursor_offset != Some(reveal_cursor_offset);
        if should_reveal_cursor {
            let row = Editor::find_line_index(reveal_cursor_offset, &line_starts);
            let line_offset = line_starts.get(row).unwrap_or(0);
            let next_offset = line_starts.get(row + 1).unwrap_or(total_size);
            let cursor_layout = {
                let editor = self.editor.read(cx);
                if editor.is_folded(line_offset) {
                    None
                } else {
                    let chunk_len = next_offset.saturating_sub(line_offset).min(64);
                    editor.document.read().ok().map(|doc| {
                        let source = build_hex_text_source(
                            doc.buffer.get_range(line_offset, chunk_len),
                            line_offset,
                            self.radix,
                            self.group_size,
                            self.is_big_endian,
                            hex_case,
                        );
                        (reveal_cursor_offset.saturating_sub(line_offset), source)
                    })
                }
            };
            if let Some((cursor_in_row, source)) = cursor_layout {
                let cursor_range = source
                    .groups
                    .iter()
                    .find(|group| group.chunk_start <= cursor_in_row && cursor_in_row < group.chunk_end)
                    .map(|group| {
                        (
                            f32::from(hex_grid_x(group.text_start, hex_cell_width)),
                            f32::from(hex_grid_x(group.text_end, hex_cell_width)),
                        )
                    })
                    .or_else(|| {
                        if cursor_in_row == source.groups.last().map(|group| group.chunk_end).unwrap_or(0)
                            && reveal_cursor_offset == total_size
                            && next_offset == total_size
                        {
                            let end = source.groups.last().map(|group| hex_grid_x(group.text_end, hex_cell_width))?;
                            let end = f32::from(end);
                            Some((end, end))
                        } else {
                            None
                        }
                    });

                if let Some((cursor_left, cursor_right)) = cursor_range {
                    self.scroll
                        .reveal_cursor(cursor_left, cursor_right, self.hex_col_width, max_hex_scroll, &layout);
                }
            }
            self.last_cursor_offset = Some(reveal_cursor_offset);
            self.cursor_reveal_pending = false;
        }
        let is_hex_clipped_left = self.scroll.hex_scroll_x > 1.0;
        let is_hex_clipped_right = self.scroll.hex_scroll_x < max_hex_scroll - 1.0;
        let is_ascii_clipped_left = self.scroll.ascii_scroll_x > 1.0;
        let is_ascii_clipped_right = layout.ascii.map(|column| self.scroll.ascii_scroll_x < column.inner_max - 1.0).unwrap_or(false);
        let is_comment_clipped_left = self.scroll.comment_scroll_x > 1.0;
        let is_comment_clipped_right = self.scroll.comment_scroll_x < layout.comment.inner_max - 1.0;
        let is_desc_clipped_left = self.scroll.desc_scroll_x > 1.0;
        let is_desc_clipped_right = layout
            .description
            .map(|column| self.scroll.desc_scroll_x < column.inner_max - 1.0)
            .unwrap_or(false);

        let header = if self.show_header {
            let cursor_in_row = cursor_in_row_offset(reveal_cursor_offset, total_size, &line_starts);
            let probe = self.cached_header_probe.as_ref().expect("header probe cached");
            let total_groups = probe.source.groups.len();
            let mut hex_cols = Vec::with_capacity(probe.col_labels.len());
            for (i, group) in probe.source.groups.iter().enumerate() {
                let label = probe.col_labels[i].clone();
                let group_start = f32::from(hex_grid_x(group.text_start, hex_cell_width));
                let group_end = f32::from(hex_grid_x(group.text_end, hex_cell_width));
                let is_cursor_col = is_cursor_header_column(cursor_in_row, i, *group, total_groups);
                let col_color = if is_cursor_col { theme.foreground } else { theme.muted_foreground };
                hex_cols.push(
                    div()
                        .absolute()
                        .left(px(group_start - self.scroll.hex_scroll_x))
                        .top_0()
                        .h_full()
                        .w(px(group_end - group_start))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_center()
                        .text_xs()
                        .text_color(col_color)
                        .child(label),
                );
            }

            let can_switch_edit_col = self.show_ascii && !is_struct_mode;
            let flash_epoch = self.column_focus_flash_epoch;
            let flash_is_hex = can_switch_edit_col && self.column_focus_flash == Some(EditColumn::Hex);
            let flash_is_ascii = can_switch_edit_col && self.column_focus_flash == Some(EditColumn::Ascii);
            let flash_primary = theme.primary;

            let comment_header_el = |width: f32, can_scroll_left: bool, can_scroll_right: bool, theme: &gpui_kit::component::Theme| {
                h_flex()
                    .h(px(header_height))
                    .w(px(width + SECTION_GAP))
                    .child(
                        div()
                            .w(px(width))
                            .h_full()
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .relative()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .when(can_scroll_left, |el| el.pl(px(16.0)))
                            .child("Comment")
                            .when(can_scroll_left, |el| el.child(header_clip_indicator(true, theme)))
                            .when(can_scroll_right, |el| el.child(header_clip_indicator(false, theme)))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                    if event.click_count >= 2 {
                                        this.auto_fit_column(ResizingColumn::Comment, cx);
                                    }
                                }),
                            ),
                    )
                    .child(
                        div()
                            .w(px(SECTION_GAP))
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor(CursorStyle::ResizeLeftRight)
                            .hover(|s| s.bg(theme.drag_border.opacity(0.25)))
                            .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                    if event.click_count >= 2 {
                                        this.resizing_column = None;
                                        this.auto_fit_column(ResizingColumn::Comment, cx);
                                    } else {
                                        this.resizing_column = Some((ResizingColumn::Comment, event.position.x.into(), this.comment_col_width));
                                        cx.notify();
                                    }
                                }),
                            ),
                    )
            };

            div()
                .flex()
                .flex_row()
                .items_center()
                .h(px(header_height))
                .bg(theme.sidebar)
                .border_b_1()
                .border_color(theme.border)
                .font_family(font_family.clone())
                .pl_2()
                .pr(px(VERTICAL_SCROLLBAR_WIDTH))
                .min_w_0()
                .overflow_hidden()
                .child(if is_struct_mode || self.show_offset {
                    h_flex()
                        .flex_shrink_0()
                        .h(px(header_height))
                        .w(px(self.address_col_width + SECTION_GAP))
                        .child(
                            div()
                                .w(px(self.address_col_width))
                                .h_full()
                                .flex()
                                .items_center()
                                .overflow_hidden()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Address")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                        if event.click_count >= 2 {
                                            this.auto_fit_column(ResizingColumn::Address, cx);
                                        }
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .w(px(SECTION_GAP))
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor(CursorStyle::ResizeLeftRight)
                                .hover(|s| s.bg(theme.drag_border.opacity(0.25)))
                                .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                        if event.click_count >= 2 {
                                            this.resizing_column = None;
                                            this.auto_fit_column(ResizingColumn::Address, cx);
                                        } else {
                                            this.resizing_column = Some((ResizingColumn::Address, event.position.x.into(), this.address_col_width));
                                            cx.notify();
                                        }
                                    }),
                                ),
                        )
                        .into_any_element()
                } else {
                    div().flex_shrink_0().w(px(SECTION_GAP)).into_any_element()
                })
                .child(
                    h_flex().flex_1().min_w_0().h(px(header_height)).overflow_hidden().relative().child(
                        h_flex()
                            .relative()
                            .left(px(-self.scroll.outer_scroll_x))
                            .h(px(header_height))
                            .flex_shrink_0()
                            .child(
                                h_flex()
                                    .h(px(header_height))
                                    .flex_shrink_0()
                                    .w(px(self.hex_col_width + SECTION_GAP))
                                    .child(
                                        div()
                                            .w(px(self.hex_col_width))
                                            .h(px(header_height))
                                            .overflow_hidden()
                                            .relative()
                                            .when(can_switch_edit_col, |el| el.cursor_pointer())
                                            .children(hex_cols)
                                            .when(flash_is_hex, |el| {
                                                el.child(header_focus_flash_overlay("hex-col-flash", flash_epoch, flash_primary))
                                            })
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                    if event.click_count >= 2 {
                                                        this.auto_fit_column(ResizingColumn::Hex, cx);
                                                    } else if can_switch_edit_col && !this.input.is_hex() {
                                                        this.focus_handle.focus(window, cx);
                                                        this.commit_pending_hex_with_zero(cx);
                                                        this.set_active_column_with_flash(EditColumn::Hex, cx);
                                                        this.pause_cursor_blink(cx);
                                                        cx.notify();
                                                    }
                                                }),
                                            )
                                            .when(is_hex_clipped_left, |el| el.child(header_clip_indicator(true, theme)))
                                            .when(is_hex_clipped_right, |el| el.child(header_clip_indicator(false, theme))),
                                    )
                                    .child(
                                        div()
                                            .w(px(SECTION_GAP))
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor(CursorStyle::ResizeLeftRight)
                                            .hover(|s| s.bg(theme.drag_border.opacity(0.25)))
                                            .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                                    if event.click_count >= 2 {
                                                        this.resizing_column = None;
                                                        this.auto_fit_column(ResizingColumn::Hex, cx);
                                                    } else {
                                                        this.resizing_column = Some((ResizingColumn::Hex, event.position.x.into(), this.hex_col_width));
                                                        cx.notify();
                                                    }
                                                }),
                                            ),
                                    ),
                            )
                            .when(self.show_ascii, |el| {
                                let label = self.encoding.label();
                                el.child(
                                    h_flex()
                                        .h(px(header_height))
                                        .w(px(ascii_col_width + SECTION_GAP))
                                        .child(
                                            div()
                                                .w(px(ascii_col_width))
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .overflow_hidden()
                                                .relative()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .when(can_switch_edit_col, |el| el.cursor_pointer())
                                                .when(is_ascii_clipped_left, |el| el.pl(px(16.0)))
                                                .child(label)
                                                .when(flash_is_ascii, |el| {
                                                    el.child(header_focus_flash_overlay("ascii-col-flash", flash_epoch, flash_primary))
                                                })
                                                .when(is_ascii_clipped_left, |el| el.child(header_clip_indicator(true, theme)))
                                                .when(is_ascii_clipped_right, |el| el.child(header_clip_indicator(false, theme)))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                                        if event.click_count >= 2 {
                                                            this.auto_fit_column(ResizingColumn::Ascii, cx);
                                                        } else if can_switch_edit_col && !this.input.is_ascii() {
                                                            this.focus_handle.focus(window, cx);
                                                            this.commit_pending_hex_with_zero(cx);
                                                            this.set_active_column_with_flash(EditColumn::Ascii, cx);
                                                            this.pause_cursor_blink(cx);
                                                            cx.notify();
                                                        }
                                                    }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .w(px(SECTION_GAP))
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor(CursorStyle::ResizeLeftRight)
                                                .hover(|s| s.bg(theme.drag_border.opacity(0.25)))
                                                .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                                        if event.click_count >= 2 {
                                                            this.resizing_column = None;
                                                            this.auto_fit_column(ResizingColumn::Ascii, cx);
                                                        } else {
                                                            this.resizing_column = Some((ResizingColumn::Ascii, event.position.x.into(), ascii_col_width));
                                                            cx.notify();
                                                        }
                                                    }),
                                                ),
                                        ),
                                )
                            })
                            .when(is_struct_mode, |el| {
                                el.child(
                                    h_flex()
                                        .h(px(header_height))
                                        .w(px(self.desc_col_width + SECTION_GAP))
                                        .child(
                                            div()
                                                .w(px(self.desc_col_width))
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .overflow_hidden()
                                                .relative()
                                                .when(is_desc_clipped_left, |el| el.pl(px(16.0)))
                                                .child("Description")
                                                .when(is_desc_clipped_left, |el| el.child(header_clip_indicator(true, theme)))
                                                .when(is_desc_clipped_right, |el| el.child(header_clip_indicator(false, theme)))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                                        if event.click_count >= 2 {
                                                            this.auto_fit_column(ResizingColumn::Description, cx);
                                                        }
                                                    }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .w(px(SECTION_GAP))
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor(CursorStyle::ResizeLeftRight)
                                                .hover(|s| s.bg(theme.drag_border.opacity(0.25)))
                                                .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                                        if event.click_count >= 2 {
                                                            this.resizing_column = None;
                                                            this.auto_fit_column(ResizingColumn::Description, cx);
                                                        } else {
                                                            this.resizing_column =
                                                                Some((ResizingColumn::Description, event.position.x.into(), this.desc_col_width));
                                                            cx.notify();
                                                        }
                                                    }),
                                                ),
                                        ),
                                )
                            })
                            .child(comment_header_el(
                                self.comment_col_width,
                                is_comment_clipped_left,
                                is_comment_clipped_right,
                                theme,
                            )),
                    ),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let bounds_view = view.clone();
        let list_bounds_view = view.clone();

        container
            .track_focus(&self.focus_handle(cx))
            .on_scroll(cx.listener(Self::on_scroll))
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_group_left))
            .on_action(cx.listener(Self::move_group_right))
            .on_action(cx.listener(Self::vi_move_left))
            .on_action(cx.listener(Self::vi_move_right))
            .on_action(cx.listener(Self::vi_move_up))
            .on_action(cx.listener(Self::vi_move_down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_group_left))
            .on_action(cx.listener(Self::select_group_right))
            .on_action(cx.listener(Self::toggle_edit_column))
            .on_action(cx.listener(Self::vi_select_left))
            .on_action(cx.listener(Self::vi_select_right))
            .on_action(cx.listener(Self::vi_select_up))
            .on_action(cx.listener(Self::vi_select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(|this, _: &AppSelectAll, window, cx| {
                this.select_all(&SelectAll, window, cx);
            }))
            .on_action(cx.listener(Self::set_radix_hex))
            .on_action(cx.listener(Self::set_radix_dec))
            .on_action(cx.listener(Self::set_radix_oct))
            .on_action(cx.listener(Self::set_radix_bin))
            .on_action(cx.listener(Self::set_group_size_1))
            .on_action(cx.listener(Self::set_group_size_2))
            .on_action(cx.listener(Self::set_group_size_4))
            .on_action(cx.listener(Self::set_group_size_8))
            .on_action(cx.listener(Self::set_byte_order_le))
            .on_action(cx.listener(Self::set_byte_order_be))
            .on_action(cx.listener(Self::toggle_byte_order))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::copy_as_hexdump))
            .on_action(cx.listener(Self::copy_as_cpp_array))
            .on_action(cx.listener(Self::copy_as_hex_stream))
            .on_action(cx.listener(Self::copy_as_hex_spaces))
            .on_action(cx.listener(Self::copy_as_printable_text))
            .on_action(cx.listener(Self::copy_as_base64))
            .on_action(cx.listener(Self::copy_as_escaped_string))
            .on_action(cx.listener(Self::copy_as_binary))
            .on_action(cx.listener(Self::copy_as_rust_array))
            .on_action(cx.listener(Self::copy_as_json_array))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_page_up))
            .on_action(cx.listener(Self::select_page_down))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::trigger_search))
            .on_action(cx.listener(Self::trigger_search_next))
            .on_action(cx.listener(Self::trigger_search_prev))
            .on_action(cx.listener(Self::add_custom_break))
            .on_action(cx.listener(Self::remove_custom_break_backward))
            .on_action(cx.listener(Self::remove_custom_break_forward))
            .on_action(cx.listener(Self::remove_custom_breaks_in_range))
            .on_action(cx.listener(Self::join_line))
            .on_action(cx.listener(Self::clear_all_custom_breaks))
            .on_action(cx.listener(Self::show_all_bookmarks))
            .on_action(cx.listener(Self::hide_all_bookmarks))
            .on_action(cx.listener(Self::toggle_hide_unbookmarked))
            .on_action(cx.listener(Self::unfold_bookmark_at_cursor))
            .on_action(cx.listener(|this, _: &ToggleBookmarkRed, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Red, window, cx)))
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkOrange, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Orange, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkYellow, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Yellow, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkGreen, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Green, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkCyan, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Cyan, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkBlue, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Blue, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkPurple, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Purple, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkPink, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Pink, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ShowOnlyBookmarkRed, window, cx| this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Red, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowOnlyBookmarkOrange, window, cx| {
                this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Orange, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowOnlyBookmarkYellow, window, cx| {
                this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Yellow, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ShowOnlyBookmarkGreen, window, cx| {
                    this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Green, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &ShowOnlyBookmarkCyan, window, cx| this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Cyan, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ShowOnlyBookmarkBlue, window, cx| this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Blue, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowOnlyBookmarkPurple, window, cx| {
                this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Purple, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ShowOnlyBookmarkPink, window, cx| this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Pink, window, cx)),
            )
            .on_action(cx.listener(Self::bookmark_red))
            .on_action(cx.listener(Self::bookmark_orange))
            .on_action(cx.listener(Self::bookmark_yellow))
            .on_action(cx.listener(Self::bookmark_green))
            .on_action(cx.listener(Self::bookmark_cyan))
            .on_action(cx.listener(Self::bookmark_blue))
            .on_action(cx.listener(Self::bookmark_purple))
            .on_action(cx.listener(Self::bookmark_pink))
            .on_action(cx.listener(Self::clear_bookmark))
            .on_action(cx.listener(Self::clear_all_bookmarks))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_hover(cx.listener(|this, hovered: &bool, _window, cx| {
                if !*hovered && this.hovered_desc_tooltip.is_some() {
                    this.hovered_desc_tooltip = None;
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.focus_handle.focus(window, cx);
                    this.pause_cursor_blink(cx);
                    if this.hovered_desc_tooltip.is_some() {
                        this.hovered_desc_tooltip = None;
                    }

                    // スクロールバー領域（右端12px）でのクリック判定
                    if let Some(list_b) = this.list_bounds.get() {
                        if event.position.y >= list_b.bottom() {
                            return;
                        }
                        let click_x = f32::from(event.position.x);
                        let bar_x = f32::from(list_b.right()) - VERTICAL_SCROLLBAR_WIDTH;
                        if click_x >= bar_x {
                            let click_y = f32::from(event.position.y);
                            let list_top = f32::from(list_b.top());
                            let rel_y = click_y - list_top;
                            let list_h = f32::from(list_b.size.height);
                            let total_rows = this.effective_total_rows(cx);

                            if let Some(geom) =
                                crate::ui::components::scrollbar::calculate_scrollbar_geometry(list_h, this.scroll.scroll_offset, total_rows, this.row_height())
                            {
                                let max_thumb_top = (list_h - geom.thumb_height).max(0.0);
                                if rel_y >= geom.thumb_top && rel_y <= geom.thumb_top + geom.thumb_height {
                                    this.scroll.is_dragging_scrollbar = true;
                                    this.scroll.scrollbar_drag_start_y = click_y;
                                    this.scroll.scrollbar_drag_start_row = this.scroll.scroll_offset;
                                } else {
                                    let target_thumb_top = (rel_y - geom.thumb_height / 2.0).clamp(0.0, max_thumb_top);
                                    let new_ratio = if max_thumb_top > 0.0 {
                                        target_thumb_top as f64 / max_thumb_top as f64
                                    } else {
                                        0.0
                                    };
                                    let new_row = (new_ratio * geom.max_top_row as f64).round() as usize;
                                    this.scroll_to_row(new_row, cx);
                                    this.scroll.is_dragging_scrollbar = true;
                                    this.scroll.scrollbar_drag_start_y = click_y;
                                    this.scroll.scrollbar_drag_start_row = new_row;
                                }
                                cx.notify();
                                return;
                            }
                        }
                    }

                    if let Some(edit_target) = this.edit_target_from_point(event.position, window, cx) {
                        this.commit_pending_hex_with_zero(cx);
                        this.set_active_column_with_flash(edit_target.column(), cx);
                        let target_pos = edit_target.offset();
                        let is_ascii = matches!(edit_target, EditTarget::Ascii { .. });
                        let insert_mode = InsertModeState::is_enabled(cx);

                        if !event.modifiers.shift && event.click_count >= 3 {
                            let row_range = {
                                let editor = this.editor.read(cx);
                                let line_map = editor.line_starts();
                                crate::core::editor::CursorModel::visual_row_bounds(target_pos, &line_map, editor.total_size())
                                    .filter(|(start, end)| start < end)
                                    .map(|(start, end)| start..end)
                            };
                            if let Some(range) = row_range {
                                this.is_selecting = false;
                                this.mouse_selection_anchor = None;
                                this.editor.update(cx, |editor, cx| {
                                    editor.set_selection_range(range.clone());
                                    cx.notify();
                                });
                                cx.emit(HexViewEvent::SelectionChanged {
                                    start: Some(range.start),
                                    end: Some(range.end.saturating_sub(1)),
                                });
                                cx.emit(HexViewEvent::CursorMoved(range.start));
                                return;
                            }
                        } else if !event.modifiers.shift && event.click_count == 2 {
                            let dbl_range = {
                                let editor = this.editor.read(cx);
                                let total = editor.total_size();
                                if editor.structure.show_inline_structure_view
                                    && !editor.structure.is_parse_in_progress()
                                    && let Some(parse_res) = editor.parse_result()
                                    && !parse_res.is_live()
                                    && let Some(leaf) = parse_res.find_deepest_leaf_at(target_pos)
                                    && leaf.size > 0
                                {
                                    Some(leaf.offset..leaf.offset.saturating_add(leaf.size).min(total))
                                } else if is_ascii {
                                    editor
                                        .document
                                        .read()
                                        .ok()
                                        .and_then(|doc| crate::core::editor::cursor::ascii_word_range(doc.buffer.data(), target_pos))
                                } else {
                                    crate::core::editor::cursor::hex_double_click_range(target_pos, editor.options.group_size, total)
                                }
                            };
                            if let Some(range) = dbl_range
                                && range.start < range.end
                            {
                                this.is_selecting = false;
                                this.mouse_selection_anchor = None;
                                this.editor.update(cx, |editor, cx| {
                                    editor.set_selection_range(range.clone());
                                    cx.notify();
                                });
                                cx.emit(HexViewEvent::SelectionChanged {
                                    start: Some(range.start),
                                    end: Some(range.end.saturating_sub(1)),
                                });
                                cx.emit(HexViewEvent::CursorMoved(range.start));
                                return;
                            }
                        }

                        let (selection_anchor, total_size, is_read_only, clicked_byte) = {
                            let editor = this.editor.read(cx);
                            let byte = editor.document.read().ok().and_then(|doc| doc.buffer.data().get(target_pos).copied());
                            (editor.selection().anchor(), editor.total_size(), editor.is_read_only(), byte)
                        };
                        let (char_start, char_end) = if is_ascii {
                            let editor = this.editor.read(cx);
                            if let Ok(doc) = editor.document.read() {
                                let range = this.encoding.char_range_at(doc.buffer.data(), target_pos);
                                (range.start, range.end)
                            } else {
                                (target_pos, target_pos.saturating_add(1))
                            }
                        } else {
                            (target_pos, target_pos.saturating_add(1))
                        };

                        this.mouse_selection_anchor = if event.modifiers.shift {
                            Some(selection_anchor)
                        } else if is_ascii {
                            Some(char_start)
                        } else {
                            Some(target_pos)
                        };
                        this.is_selecting = true;
                        this.editor.update(cx, |editor, cx| {
                            if event.modifiers.shift {
                                if is_ascii {
                                    let (anchor, active) =
                                        crate::core::editor::cursor::ascii_shift_click_selection(selection_anchor, char_start, char_end, total_size);
                                    editor.set_selection(anchor, active);
                                    editor.cursor.offset = if active > anchor { active.saturating_sub(1) } else { active };
                                } else {
                                    editor.continue_drag(selection_anchor, target_pos);
                                }
                            } else if is_ascii && char_end > char_start && !insert_mode {
                                editor.set_selection_range(char_start..char_end);
                            } else {
                                editor.set_cursor_offset_exact(target_pos);
                            }
                            cx.notify();
                        });

                        if !event.modifiers.shift
                            && !insert_mode
                            && !is_read_only
                            && this.radix == crate::core::radix::DisplayRadix::Hexadecimal
                            && let EditTarget::Hex { offset, nibble: 1 } = edit_target
                            && let Some(original_byte) = clicked_byte
                        {
                            this.input.set_low_nibble_target(offset, original_byte);
                            cx.notify();
                        }
                    } else if let Some(target_pos) = this.offset_from_point(event.position, window, cx) {
                        let (is_struct_collapse_toggle, struct_collapse_target, is_desc_field_selection) = {
                            let editor = this.editor.read(cx);
                            let line_starts = editor.line_starts();
                            let layout = this.current_layout(cx);
                            let mut is_toggle = false;
                            let mut collapse_key = None;
                            let mut selected_field_range = None;

                            if editor.structure.show_inline_structure_view
                                && !editor.structure.is_parse_in_progress()
                                && let Some(parse_res) = editor.parse_result()
                                && !parse_res.is_live()
                                && let Some(list_bounds) = this.list_bounds.get()
                            {
                                let row_height = this.row_height();
                                let rel_y = f32::from(event.position.y - list_bounds.top()).max(0.0);
                                let top_overscroll_px = this.scroll.top_overscroll_rows as f32 * row_height;
                                if rel_y >= top_overscroll_px {
                                    let row_offset_in_view = ((rel_y - top_overscroll_px) / row_height).floor() as usize;
                                    let row_idx = this.scroll.scroll_offset + row_offset_in_view;
                                    if row_idx < line_starts.len() {
                                        let line_offset = line_starts.get(row_idx).unwrap_or(0);
                                        let next_offset = if row_idx + 1 < line_starts.len() {
                                            line_starts.get(row_idx + 1).unwrap_or(line_offset)
                                        } else {
                                            editor.total_size()
                                        };
                                        let chunk_len = next_offset.saturating_sub(line_offset);
                                        let base_x = f32::from(list_bounds.left()) + 8.0;
                                        let relative_x = f32::from(event.position.x) - base_x;
                                        let world_x = relative_x + this.scroll.outer_scroll_x;
                                        let in_desc = layout.description.is_some_and(|col| world_x >= col.start);

                                        let is_collapsed_container_row = parse_res.find_container_structs_starting_at(line_offset, chunk_len).iter().any(|c| {
                                            !c.is_instance
                                                && c.offset == line_offset
                                                && (c.size == chunk_len || chunk_len == c.size.min(editor.total_size().saturating_sub(line_offset)))
                                                && editor.structure.is_struct_collapsed(&c.collapse_key, &c.id)
                                        });

                                        if chunk_len == 0 {
                                            let containers: Vec<_> = parse_res
                                                .find_container_structs_starting_at(line_offset, 1)
                                                .iter()
                                                .filter(|container| !container.is_instance && container.size > 0)
                                                .cloned()
                                                .collect();
                                            let mut same_offset_rows = 0;
                                            let mut previous_row = row_idx;
                                            while previous_row > 0 && line_starts.get(previous_row - 1) == Some(line_offset) {
                                                same_offset_rows += 1;
                                                previous_row -= 1;
                                            }
                                            if let Some(c) = containers.get(same_offset_rows) {
                                                is_toggle = true;
                                                collapse_key = Some(c.collapse_key.clone());
                                            }
                                        } else if is_collapsed_container_row {
                                            let containers = parse_res.find_container_structs_starting_at(line_offset, chunk_len);
                                            if let Some(c) = containers.iter().find(|c| {
                                                !c.is_instance
                                                    && c.offset == line_offset
                                                    && (c.size == chunk_len || chunk_len == c.size.min(editor.total_size().saturating_sub(line_offset)))
                                                    && editor.structure.is_struct_collapsed(&c.collapse_key, &c.id)
                                            }) {
                                                is_toggle = true;
                                                collapse_key = Some(c.collapse_key.clone());
                                            }
                                        } else if in_desc {
                                            let containers = parse_res.find_container_structs_starting_at(line_offset, chunk_len);
                                            if let Some(c) = containers.iter().find(|c| {
                                                !c.is_instance && c.offset == line_offset && editor.structure.is_struct_collapsed(&c.collapse_key, &c.id)
                                            }) {
                                                is_toggle = true;
                                                collapse_key = Some(c.collapse_key.clone());
                                            } else if event.click_count >= 2 {
                                                let leaves = parse_res.find_leaf_fields_starting_at(line_offset, chunk_len);
                                                let leaf = leaves.first().or_else(|| parse_res.find_deepest_leaf_at(target_pos));
                                                if let Some(l) = leaf {
                                                    selected_field_range = Some(l.offset..l.offset + l.size);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            (is_toggle, collapse_key, selected_field_range)
                        };

                        if is_struct_collapse_toggle && let Some(key) = struct_collapse_target {
                            this.editor.update(cx, |editor, cx| {
                                editor.toggle_struct_collapsed(&key);
                                cx.notify();
                            });
                            this.notify_document_changed(cx);
                            return;
                        }

                        if this.editor.read(cx).is_folded(target_pos) {
                            let unfolded = this.editor.update(cx, |editor, cx| {
                                let res = editor.unfold_bookmark_at(target_pos);
                                if res {
                                    cx.notify();
                                }
                                res
                            });
                            if unfolded {
                                this.notify_document_changed(cx);
                                return;
                            }
                        }

                        if let Some(range) = is_desc_field_selection {
                            this.is_selecting = false;
                            this.mouse_selection_anchor = None;
                            this.editor.update(cx, |editor, cx| {
                                editor.set_selection_range(range.clone());
                                cx.notify();
                            });
                            if range.start < range.end {
                                cx.emit(HexViewEvent::SelectionChanged {
                                    start: Some(range.start),
                                    end: Some(range.end.saturating_sub(1)),
                                });
                            }
                            cx.emit(HexViewEvent::CursorMoved(range.start));
                            return;
                        }

                        let selection_anchor = {
                            let editor = this.editor.read(cx);
                            editor.selection().anchor()
                        };
                        this.mouse_selection_anchor = if event.modifiers.shift { Some(selection_anchor) } else { Some(target_pos) };
                        this.is_selecting = true;
                        this.clear_pending_hex_input();
                        this.editor.update(cx, |editor, cx| {
                            if event.modifiers.shift {
                                editor.continue_drag(selection_anchor, target_pos);
                            } else {
                                editor.set_cursor_offset_exact(target_pos);
                            }
                            cx.notify();
                        });
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.focus_handle.focus(window, cx);
                    this.pause_cursor_blink(cx);
                    if this.list_bounds.get().map(|bounds| event.position.y >= bounds.bottom()).unwrap_or(false) {
                        return;
                    }
                    if let Some(edit_target) = this.edit_target_from_point(event.position, window, cx) {
                        this.commit_pending_hex_with_zero(cx);
                        this.set_active_column_with_flash(edit_target.column(), cx);
                        let target_pos = edit_target.offset();
                        let is_ascii = matches!(edit_target, EditTarget::Ascii { .. });
                        let (char_start, char_end) = if is_ascii {
                            let editor = this.editor.read(cx);
                            if let Ok(doc) = editor.document.read() {
                                let range = this.encoding.char_range_at(doc.buffer.data(), target_pos);
                                (range.start, range.end)
                            } else {
                                (target_pos, target_pos.saturating_add(1))
                            }
                        } else {
                            (target_pos, target_pos.saturating_add(1))
                        };
                        let insert_mode = InsertModeState::is_enabled(cx);
                        this.editor.update(cx, |editor, cx| {
                            let in_selection = editor
                                .selection_range()
                                .is_some_and(|range| target_pos >= range.start && target_pos < range.end);
                            if !in_selection {
                                if is_ascii && !insert_mode && char_end > char_start {
                                    editor.set_selection_range(char_start..char_end);
                                } else {
                                    editor.set_cursor_offset_exact(target_pos);
                                }
                                cx.notify();
                            }
                        });
                    } else if let Some(target_pos) = this.offset_from_point(event.position, window, cx) {
                        this.clear_pending_hex_input();
                        this.editor.update(cx, |editor, cx| {
                            let in_selection = editor
                                .selection_range()
                                .is_some_and(|range| target_pos >= range.start && target_pos < range.end);
                            if !in_selection {
                                editor.set_cursor_offset(target_pos);
                                cx.notify();
                            }
                        });
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                let handle_offset = (-f32::from(this.scroll.outer_scroll_handle.offset().x)).max(0.0);
                if (handle_offset - this.scroll.outer_scroll_x).abs() > 0.01 {
                    let layout = this.current_layout(cx);
                    this.sync_outer_scroll_from_handle(layout, cx);
                }

                if let Some(list_b) = this.list_bounds.get() {
                    let pos = event.position;
                    let is_in_bar = pos.x >= list_b.right() - px(12.0) && pos.x <= list_b.right() && pos.y >= list_b.top() && pos.y <= list_b.bottom();
                    if this.scroll.scrollbar_hovered != is_in_bar {
                        this.scroll.scrollbar_hovered = is_in_bar;
                        cx.notify();
                    }
                }

                if let Some((col, start_x, start_w)) = this.resizing_column {
                    this.hovered_desc_tooltip = None;
                    let current_x: f32 = event.position.x.into();
                    let delta = current_x - start_x;
                    match col {
                        ResizingColumn::Address => {
                            this.address_col_width = (start_w + delta).max(60.0);
                        }
                        ResizingColumn::Hex => {
                            this.hex_col_width = (start_w + delta).max(100.0);
                        }
                        ResizingColumn::Ascii => {
                            this.ascii_col_width = (start_w + delta).max(MIN_ASCII_COLUMN_WIDTH);
                        }
                        ResizingColumn::Description => {
                            this.desc_col_width = (start_w + delta).max(80.0);
                        }
                        ResizingColumn::Comment => {
                            this.comment_col_width = (start_w + delta).max(80.0);
                        }
                    }
                    this.cursor_reveal_pending = true;
                    this.clamp_scroll_offsets(cx);
                    cx.notify();
                    return;
                }
                if this.is_selecting && event.dragging() {
                    this.hovered_desc_tooltip = None;
                    let row_height = this.row_height();
                    if let Some(list_b) = this.list_bounds.get() {
                        let y = f32::from(event.position.y);
                        let list_top = f32::from(list_b.top());
                        let list_bottom = f32::from(list_b.bottom());
                        if y < list_top {
                            let rows_up = ((list_top - y) / row_height).ceil() as usize;
                            let new_row = this.scroll.scroll_offset.saturating_sub(rows_up.min(5));
                            this.scroll_to_row(new_row, cx);
                        } else if y > list_bottom {
                            let rows_down = ((y - list_bottom) / row_height).ceil() as usize;
                            let total_rows = this.effective_total_rows(cx);
                            let max_top_row = total_rows.saturating_sub(1);
                            let new_row = (this.scroll.scroll_offset + rows_down.min(5)).min(max_top_row);
                            this.scroll_to_row(new_row, cx);
                        }
                    }

                    let target_pos = this.offset_from_point(event.position, window, cx).or_else(|| {
                        if let Some(list_b) = this.list_bounds.get()
                            && f32::from(event.position.y) >= f32::from(list_b.top())
                        {
                            let editor = this.editor.read(cx);
                            Some(editor.total_size())
                        } else {
                            None
                        }
                    });

                    if let Some(target_pos) = target_pos {
                        let mouse_selection_anchor = this.mouse_selection_anchor;
                        let is_ascii = this.input.is_ascii();
                        let insert_mode = InsertModeState::is_enabled(cx);
                        let target_drag_pos = if is_ascii && !insert_mode {
                            let editor = this.editor.read(cx);
                            if let Ok(doc) = editor.document.read() {
                                let char_range = this.encoding.char_range_at(doc.buffer.data(), target_pos);
                                let anchor = mouse_selection_anchor.unwrap_or(editor.cursor.offset);
                                if target_pos >= anchor { char_range.end } else { char_range.start }
                            } else {
                                target_pos
                            }
                        } else {
                            target_pos
                        };
                        this.editor.update(cx, |editor, cx| {
                            let anchor = mouse_selection_anchor.unwrap_or(editor.cursor.offset);
                            let prev_selection = editor.selection();
                            let prev_cursor = editor.cursor.offset;
                            editor.continue_drag(anchor, target_drag_pos);
                            if editor.selection() != prev_selection || editor.cursor.offset != prev_cursor {
                                cx.notify();
                            }
                        });
                    }
                    return;
                }

                // Hover tooltip for truncated Description cells in inline structure mode
                let next_tooltip = if !this.scroll.is_dragging_scrollbar
                    && let Some(root_b) = this.bounds.get()
                    && let Some(list_b) = this.list_bounds.get()
                    && event.position.y >= list_b.top()
                    && event.position.y < list_b.bottom()
                    && event.position.x < list_b.right() - px(VERTICAL_SCROLLBAR_WIDTH)
                {
                    let layout = this.current_layout(cx);
                    let base_x = f32::from(list_b.left()) + 8.0;
                    let relative_x = f32::from(event.position.x) - base_x;
                    let world_x = relative_x + this.scroll.outer_scroll_x;
                    if let Some(desc_col) = layout.description
                        && relative_x >= layout.fixed_width
                        && world_x >= desc_col.start
                        && world_x < desc_col.end()
                    {
                        let row_height = this.row_height();
                        let rel_y = f32::from(event.position.y - list_b.top());
                        let top_overscroll_px = this.scroll.top_overscroll_rows as f32 * row_height;
                        if rel_y >= top_overscroll_px {
                            let row_offset_in_view = ((rel_y - top_overscroll_px) / row_height).floor() as usize;
                            let row_idx = this.scroll.scroll_offset + row_offset_in_view;
                            let cache = this.row_cache.borrow();
                            cache.get(row_idx).and_then(|cached| cached.desc.as_ref()).and_then(|desc| {
                                if desc.desc_text_width > this.desc_col_width + 1.0 && !desc.full_text.is_empty() {
                                    let root_w = f32::from(root_b.size.width);
                                    let root_h = f32::from(root_b.size.height);
                                    let desc_screen_left = (base_x + desc_col.start - this.scroll.outer_scroll_x - f32::from(root_b.left())).max(8.0);
                                    let tooltip_x = desc_screen_left.min((root_w - 320.0).max(8.0));
                                    let row_top_in_root =
                                        f32::from(list_b.top() - root_b.top()) + (row_offset_in_view + this.scroll.top_overscroll_rows) as f32 * row_height;
                                    let tooltip_y = if row_top_in_root + row_height + 44.0 > root_h {
                                        (row_top_in_root - 32.0).max(4.0)
                                    } else {
                                        row_top_in_root + row_height + 2.0
                                    };
                                    Some((row_idx, desc.full_text.clone(), point(px(tooltip_x), px(tooltip_y))))
                                } else {
                                    None
                                }
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                let tooltip_changed = match (&this.hovered_desc_tooltip, &next_tooltip) {
                    (Some((r1, t1, _)), Some((r2, t2, _))) => r1 != r2 || t1 != t2,
                    (None, None) => false,
                    _ => true,
                };
                if tooltip_changed {
                    this.hovered_desc_tooltip = next_tooltip;
                    cx.notify();
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(
                canvas(
                    move |bounds, _window, cx| {
                        bounds_view.update(cx, |this, _cx| {
                            this.bounds.set(Some(bounds));
                        });
                    },
                    |_bounds, _prepaint, _window, _cx| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(header)
            .child(div().flex_1().w_full().min_w_0().min_h_0().relative().overflow_hidden().child({
                let editor_entity = self.editor.clone();
                let scroll_offset = self.scroll.scroll_offset;
                let top_overscroll_rows = self.scroll.top_overscroll_rows;
                let outer_scroll_x = self.scroll.outer_scroll_x;
                let hex_scroll_x = self.scroll.hex_scroll_x;
                let ascii_scroll_x = self.scroll.ascii_scroll_x;
                let desc_scroll_x = self.scroll.desc_scroll_x;
                let comment_scroll_x = self.scroll.comment_scroll_x;
                let address_col_width = self.address_col_width;
                let hex_col_width = self.hex_col_width;
                let desc_col_width = self.desc_col_width;
                let comment_col_width = self.comment_col_width;
                let show_offset = self.show_offset;
                let show_ascii = self.show_ascii;
                let encoding = self.encoding;
                let radix = self.radix;
                let group_size = self.group_size;
                let is_big_endian = self.is_big_endian;
                let active_column = self.input.active_column();
                let pending_hex_digit = self.input.pending_hex_digit();
                let low_nibble_offset = self.input.low_nibble_offset();
                let highlights = self.highlights.clone();
                let highlights_version = self.highlights_version;
                let effective_highlights_cache = self.effective_highlights_cache.clone();
                let is_dragging_scrollbar = self.scroll.is_dragging_scrollbar;
                let scrollbar_hovered = self.scroll.scrollbar_hovered;
                let scrollbar_view = view.clone();
                let custom_category_palette = self.category_palette;
                let cached_desc_palette = self.cached_desc_palette.clone();
                let cached_active_leaf_field = self.cached_active_leaf_field.clone();
                let row_cache = self.row_cache.clone();
                let paint_row_scratch = self.paint_row_scratch.clone();

                canvas(
                    move |bounds, _window, cx| {
                        list_bounds_view.update(cx, |this, _cx| {
                            let changed = this.list_bounds.get() != Some(bounds);
                            this.list_bounds.set(Some(bounds));
                            if changed {
                                _cx.notify();
                            }
                        });
                    },
                    move |bounds, _prepaint, window, cx| {
                        window.on_mouse_event({
                            let scrollbar_view = scrollbar_view.clone();
                            move |event: &MouseMoveEvent, phase, _window, cx| {
                                if !phase.bubble() || !event.dragging() {
                                    return;
                                }

                                scrollbar_view.update(cx, |this, cx| {
                                    this.update_scrollbar_drag(f32::from(event.position.y), cx);
                                });
                            }
                        });
                        window.on_mouse_event({
                            let scrollbar_view = scrollbar_view.clone();
                            move |event: &MouseUpEvent, phase, _window, cx| {
                                if !phase.bubble() || event.button != MouseButton::Left {
                                    return;
                                }

                                scrollbar_view.update(cx, |this, cx| {
                                    if this.scroll.is_dragging_scrollbar {
                                        this.scroll.is_dragging_scrollbar = false;
                                        cx.notify();
                                    }
                                });
                            }
                        });

                        let (
                            parse_result,
                            collapsed_structs,
                            structure_layout_version,
                            is_parsing_structure,
                            bookmark_items,
                            bookmark_version,
                            doc_arc,
                            line_starts,
                            cursor_offset,
                            insert_cursor_offset,
                            min_sel,
                            max_sel,
                        ) = {
                            let editor = editor_entity.read(cx);
                            let (min_sel, max_sel) = editor
                                .selection_range()
                                .map(|range| (range.start, range.end.saturating_sub(1)))
                                .unwrap_or((usize::MAX, usize::MIN));
                            (
                                if editor.structure.show_inline_structure_view {
                                    editor.parse_result()
                                } else {
                                    None
                                },
                                editor.structure.collapsed_struct_ids.clone(),
                                editor.structure.layout_version(),
                                editor.structure.is_parse_in_progress(),
                                editor.bookmarks().snapshot_arc(),
                                editor.bookmarks().version(),
                                editor.document.clone(),
                                editor.line_starts(),
                                editor.cursor.offset,
                                editor.insert_cursor_offset(),
                                min_sel,
                                max_sel,
                            )
                        };
                        let doc = doc_arc.read().expect("document read lock");

                        let list_h = f32::from(bounds.size.height);
                        let visible_rows = (list_h / row_height).ceil() as usize + 1;
                        let top_row = scroll_offset.min(total_rows.saturating_sub(1));
                        let end_row = (top_row + visible_rows).min(total_rows);
                        let total_size = doc.buffer.len();
                        let has_folded_bookmarks = doc.has_folded_regions();
                        let has_address_gaps = doc.address_map.has_gaps();
                        let vp_start = line_starts.get(top_row).unwrap_or(total_size);
                        let vp_end = if end_row < line_starts.len() {
                            line_starts.get(end_row).unwrap_or(total_size)
                        } else {
                            total_size
                        };
                        let mut hl_cache = effective_highlights_cache.borrow_mut();
                        hl_cache.ensure_updated(bookmark_version, &bookmark_items, highlights_version, &highlights);
                        let mut hl_sweep = hl_cache.viewport_sweep(vp_start, vp_end);
                        let category_palette = custom_category_palette.unwrap_or_else(|| {
                            let colors = *cx.global::<ByteCategoryColors>();
                            ByteCategoryPalette::from_colors(colors, cx.theme())
                        });
                        let desc_palette = {
                            let theme = cx.theme();
                            let key = (theme.mode, theme.background, theme.foreground);
                            if let Some((m, bg, fg, pal)) = cached_desc_palette.get() {
                                if m == key.0 && bg == key.1 && fg == key.2 {
                                    pal
                                } else {
                                    let pal = DescriptionPalette::from_theme(theme);
                                    cached_desc_palette.set(Some((key.0, key.1, key.2, pal)));
                                    pal
                                }
                            } else {
                                let pal = DescriptionPalette::from_theme(theme);
                                cached_desc_palette.set(Some((key.0, key.1, key.2, pal)));
                                pal
                            }
                        };
                        let active_cursor_offset = if insert_mode { insert_cursor_offset } else { cursor_offset };
                        let mut active_leaf_cache = cached_active_leaf_field.borrow_mut();
                        let active_leaf_field = active_leaf_cache.resolve(parse_result.as_ref(), active_cursor_offset);
                        let ascii_cursor_char_range = if active_column == EditColumn::Ascii && !insert_mode {
                            Some(encoding.char_range_at(doc.buffer.data(), cursor_offset))
                        } else {
                            None
                        };

                        // Pre-bucket visible comment bookmarks once per frame by display row
                        let visible_comment_bookmarks =
                            collect_visible_comment_refs(&bookmark_items, &line_starts, total_size, top_row, end_row, vp_start, vp_end);
                        let mut comment_cursor = 0usize;

                        let theme = cx.theme();
                        let theme_colors = RowThemeColors::from_theme(theme, is_focused);
                        let row_font = font(font_family.clone());
                        let parse_result_ptr = parse_result.as_ref().map(|p| Arc::as_ptr(p) as usize).unwrap_or(0);
                        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
                        let mut cache_borrow = row_cache.borrow_mut();
                        let mut scratch_borrow = paint_row_scratch.borrow_mut();
                        scratch_borrow.fade_runs.clear();
                        cache_borrow.validate(RowRenderCacheKey {
                            generation: doc.history.state_id(),
                            layout_version: doc.layout_version(),
                            bytes_per_row: line_starts.bytes_per_row(),
                            structure_layout_version,
                            is_parsing_structure,
                            show_offset,
                            show_ascii,
                            parse_result_ptr,
                            font_size,
                            font_family: &font_family,
                            radix,
                            group_size,
                            is_big_endian,
                            hex_case,
                            encoding,
                            theme_bg: theme_colors.bg_color_theme,
                            theme_fg: theme_colors.fg_color,
                        });

                        for (k, row_idx) in (top_row..end_row).enumerate() {
                            let row_y = bounds.top() + px((k + top_overscroll_rows) as f32 * row_height);
                            if row_y >= bounds.bottom() {
                                break;
                            }
                            let Some((row_offset, row_next_offset)) = row_byte_range(&line_starts, row_idx, total_size) else {
                                continue;
                            };
                            let active_row_highlights = hl_sweep.advance_to(row_offset, row_next_offset);
                            while visible_comment_bookmarks.get(comment_cursor).is_some_and(|c| c.row_idx < row_idx) {
                                comment_cursor += 1;
                            }
                            let mut comment_end = comment_cursor;
                            while visible_comment_bookmarks.get(comment_end).is_some_and(|c| c.row_idx == row_idx) {
                                comment_end += 1;
                            }
                            let row_bookmark_comments = &visible_comment_bookmarks[comment_cursor..comment_end];
                            comment_cursor = comment_end;
                            let row_bounds = Bounds::new(point(bounds.left(), row_y), size(bounds.size.width, px(row_height)));
                            paint_hex_row(
                                RowPaintParams {
                                    row_idx,
                                    bounds: row_bounds,
                                    doc: &doc,
                                    line_starts: &line_starts,
                                    parse_result: parse_result.as_deref(),
                                    is_parsing_structure,
                                    collapsed_structs: &collapsed_structs,
                                    encoding,
                                    radix,
                                    group_size,
                                    is_big_endian,
                                    hex_case,
                                    category_palette,
                                    cursor_offset,
                                    insert_cursor_offset,
                                    min_sel,
                                    max_sel,
                                    highlights: active_row_highlights,
                                    bookmark_comments: row_bookmark_comments,
                                    show_offset,
                                    show_ascii,
                                    ascii_col_width,
                                    ascii_scroll_x,
                                    is_focused,
                                    insert_mode,
                                    active_column,
                                    ascii_cursor_char_range: ascii_cursor_char_range.clone(),
                                    cursor_visible,
                                    pending_hex_digit,
                                    low_nibble_offset,
                                    outer_scroll_x,
                                    hex_scroll_x,
                                    desc_scroll_x,
                                    comment_scroll_x,
                                    address_col_width,
                                    hex_col_width,
                                    hex_cell_width,
                                    desc_col_width,
                                    comment_col_width,
                                    font: &row_font,
                                    font_size,
                                    row_height,
                                    ascii_cell_width,
                                    desc_palette,
                                    active_leaf_field,
                                    has_folded_bookmarks,
                                    has_address_gaps,
                                    theme_colors: &theme_colors,
                                    row_cache: &mut cache_borrow,
                                    scratch: &mut scratch_borrow,
                                },
                                window,
                                cx,
                            );
                        }

                        flush_column_fade_runs(&scratch_borrow.fade_runs, theme_colors.bg_color_theme, window);

                        let theme = cx.theme();
                        paint_scrollbar(bounds, top_row, total_rows, row_height, is_dragging_scrollbar, scrollbar_hovered, theme, window);
                    },
                )
                .size_full()
            }))
            .child(if layout.outer_max > 0.01 {
                let theme = cx.theme();
                div()
                    .h(px(HORIZONTAL_SCROLLBAR_HEIGHT))
                    .flex_shrink_0()
                    .flex()
                    .child(div().w(px(8.0 + layout.fixed_width)))
                    .child(
                        div().flex_1().mr(px(VERTICAL_SCROLLBAR_WIDTH)).relative().child(
                            Scrollbar::horizontal(&self.scroll.outer_scroll_handle)
                                .mode(ScrollbarMode::Always)
                                .scroll_size(size(px(layout.content_width), px(0.0)))
                                .styles(|_| crate::ui::components::scrollbar::common_scrollbar_styles(theme)),
                        ),
                    )
                    .into_any_element()
            } else {
                div().h(px(0.0)).into_any_element()
            })
            .when_some(self.hovered_desc_tooltip.clone(), |el, (_, tooltip_text, tooltip_pos)| {
                let theme = cx.theme();
                el.child(
                    div()
                        .absolute()
                        .left(tooltip_pos.x)
                        .top(tooltip_pos.y)
                        .max_w(px(480.0))
                        .px_2()
                        .py_1()
                        .rounded_md()
                        .bg(theme.popover)
                        .text_color(theme.popover_foreground)
                        .border_1()
                        .border_color(theme.border)
                        .shadow_md()
                        .text_xs()
                        .font_family(self.font_family_prop.clone())
                        .child(tooltip_text),
                )
            })
            .context_menu({
                let focus_handle = self.focus_handle.clone();
                let editor = self.editor.clone();
                let view = view.clone();
                move |menu, window, cx| {
                    let (is_read_only, can_undo, can_redo, has_selection, can_copy, has_structure, can_switch_column) = {
                        let ed = editor.read(cx);
                        let view_ref = view.read(cx);
                        let can_copy = view_ref.copy_range(cx).is_some();
                        let has_structure = ed.ksy_definition().is_some() || ed.parse_result().is_some();
                        let is_struct_mode = ed.structure.show_inline_structure_view && ed.parse_result().is_some();
                        (
                            ed.is_read_only(),
                            ed.can_undo(),
                            ed.can_redo(),
                            ed.has_selection(),
                            can_copy,
                            has_structure,
                            view_ref.show_ascii && !is_struct_mode,
                        )
                    };
                    menu.action_context(focus_handle.clone())
                        .menu_with_disabled("Undo", Box::new(Undo), is_read_only || !can_undo)
                        .menu_with_disabled("Redo", Box::new(Redo), is_read_only || !can_redo)
                        .menu_with_disabled("Cut", Box::new(Cut), is_read_only || !has_selection)
                        .menu_with_disabled("Copy", Box::new(Copy), !can_copy)
                        .submenu("Copy As", window, cx, move |menu, _window, _cx| {
                            menu.menu_with_disabled("Hex Dump", Box::new(CopyAsHexDump), !can_copy)
                                .menu_with_disabled("C++ Array", Box::new(CopyAsCppArray), !can_copy)
                                .menu_with_disabled("Hex Stream", Box::new(CopyAsHexStream), !can_copy)
                                .menu_with_disabled("Hex with Spaces", Box::new(CopyAsHexSpaces), !can_copy)
                                .menu_with_disabled("Printable Text", Box::new(CopyAsPrintableText), !can_copy)
                                .menu_with_disabled("Base64", Box::new(CopyAsBase64), !can_copy)
                                .menu_with_disabled("Escaped String", Box::new(CopyAsEscapedString), !can_copy)
                                .menu_with_disabled("Binary", Box::new(CopyAsBinary), !can_copy)
                                .menu_with_disabled("Rust Array", Box::new(CopyAsRustArray), !can_copy)
                                .menu_with_disabled("JSON Array", Box::new(CopyAsJsonArray), !can_copy)
                        })
                        .menu_with_disabled("Paste", Box::new(Paste), is_read_only)
                        .menu_with_disabled("Fill Selection...", Box::new(FillSelection), is_read_only || !has_selection)
                        .separator()
                        .submenu("Send to Scratchpad", window, cx, move |menu, _window, _cx| {
                            menu.menu_with_icon("Offset", IconName::Hash, Box::new(crate::actions::InsertActiveOffsetOnly))
                                .menu_with_icon("Hex Bytes", IconName::Binary, Box::new(crate::actions::InsertActiveHexBytes))
                                .menu_with_icon("Text / String", IconName::FileText, Box::new(crate::actions::InsertActiveText))
                        })
                        .separator()
                        .submenu("Bookmark", window, cx, move |menu, _window, _cx| {
                            menu.menu("Red", Box::new(BookmarkRed))
                                .menu("Orange", Box::new(BookmarkOrange))
                                .menu("Yellow", Box::new(BookmarkYellow))
                                .menu("Green", Box::new(BookmarkGreen))
                                .menu("Cyan", Box::new(BookmarkCyan))
                                .menu("Blue", Box::new(BookmarkBlue))
                                .menu("Purple", Box::new(BookmarkPurple))
                                .menu("Pink", Box::new(BookmarkPink))
                                .separator()
                                .menu("Clear Bookmark", Box::new(ClearBookmark))
                                .menu("Clear All Bookmarks", Box::new(ClearAllBookmarks))
                                .separator()
                                .menu("Show Bookmarks Panel", Box::new(ShowBookmarksTab))
                                .menu("Export Bookmarks...", Box::new(ExportBookmarks))
                                .menu("Import Bookmarks...", Box::new(ImportBookmarks))
                        })
                        .submenu("Bookmark Visibility", window, cx, move |menu, window, cx| {
                            menu.menu("Show All Bookmarks", Box::new(ShowAllBookmarks))
                                .menu("Hide All Bookmarks", Box::new(HideAllBookmarks))
                                .separator()
                                .menu("Show Only Bookmarked Regions", Box::new(ToggleHideUnbookmarked))
                                .separator()
                                .menu("Unfold at Cursor", Box::new(UnfoldBookmarkAtCursor))
                                .separator()
                                .submenu("Toggle by Color", window, cx, move |m, _window, _cx| {
                                    m.menu("Red", Box::new(ToggleBookmarkRed))
                                        .menu("Orange", Box::new(ToggleBookmarkOrange))
                                        .menu("Yellow", Box::new(ToggleBookmarkYellow))
                                        .menu("Green", Box::new(ToggleBookmarkGreen))
                                        .menu("Cyan", Box::new(ToggleBookmarkCyan))
                                        .menu("Blue", Box::new(ToggleBookmarkBlue))
                                        .menu("Purple", Box::new(ToggleBookmarkPurple))
                                        .menu("Pink", Box::new(ToggleBookmarkPink))
                                })
                                .submenu("Show Only Color", window, cx, move |m, _window, _cx| {
                                    m.menu("Only Red", Box::new(ShowOnlyBookmarkRed))
                                        .menu("Only Orange", Box::new(ShowOnlyBookmarkOrange))
                                        .menu("Only Yellow", Box::new(ShowOnlyBookmarkYellow))
                                        .menu("Only Green", Box::new(ShowOnlyBookmarkGreen))
                                        .menu("Only Cyan", Box::new(ShowOnlyBookmarkCyan))
                                        .menu("Only Blue", Box::new(ShowOnlyBookmarkBlue))
                                        .menu("Only Purple", Box::new(ShowOnlyBookmarkPurple))
                                        .menu("Only Pink", Box::new(ShowOnlyBookmarkPink))
                                })
                        })
                        .submenu("Structure", window, cx, move |menu, _window, _cx| {
                            menu.menu("Toggle Inline Structure View", Box::new(ToggleInlineStructureView))
                                .menu("Load Structure Definition...", Box::new(LoadStructureDefinition))
                                .menu_with_disabled("Clear Structure Definition", Box::new(ClearStructureDefinition), !has_structure)
                                .separator()
                                .menu("Show Structure Panel", Box::new(ShowStructureTab))
                        })
                        .separator()
                        .menu("Find / Replace...", Box::new(ToggleSearch))
                        .menu("Go to Address...", Box::new(crate::actions::ToggleGoToAddress))
                        .menu_with_disabled("Switch Hex / ASCII Column", Box::new(crate::actions::ToggleEditColumn), !can_switch_column)
                        .menu("Select All", Box::new(SelectAll))
                        .separator()
                        .menu("Break Line", Box::new(AddCustomBreak))
                        .menu("Join Lines", Box::new(JoinLine))
                        .menu_with_disabled("Reset Breaks in Selection", Box::new(RemoveCustomBreaksInRange), !has_selection)
                        .menu("Reset Layout", Box::new(ClearAllCustomBreaks))
                }
            })
    }
}
