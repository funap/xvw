use super::{ActivePalette, MAX_LEFT_PANEL_WIDTH, MIN_LEFT_PANEL_WIDTH, Workspace};
use crate::ui::components::EventInterceptionExt;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::resizable::{h_resizable, resizable_panel};
use gpui_kit::prelude::*;
use gpui_kit::*;

impl Render for Workspace {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .id("workspace")
            .key_context("Workspace")
            .on_action(cx.listener(Self::on_action_focus_hex_view))
            .on_action(cx.listener(Self::on_action_toggle_right_panel))
            .on_action(cx.listener(Self::on_action_new_file))
            .on_action(cx.listener(Self::on_action_new_empty_file))
            .on_action(cx.listener(Self::on_action_new_scratchpad))
            .on_action(cx.listener(Self::on_action_open_scratchpad_dialog))
            .on_action(cx.listener(Self::on_action_open_scratchpad_file))
            .on_action(cx.listener(Self::on_action_reveal_scratches_in_explorer))
            .on_action(cx.listener(Self::on_action_export_scratchpad_as))
            .on_action(cx.listener(Self::on_action_delete_current_scratchpad))
            .on_action(cx.listener(Self::on_action_quick_open))
            .on_action(cx.listener(Self::on_action_open_scratchpad_palette))
            .on_action(cx.listener(Self::on_action_toggle_scratchpad_mode))
            .on_action(cx.listener(Self::on_action_navigate_to_offset))
            .on_action(cx.listener(Self::on_action_open_file))
            .on_action(cx.listener(Self::on_action_save))
            .on_action(cx.listener(Self::on_action_save_as))
            .on_action(cx.listener(Self::on_action_toggle_read_only))
            .on_action(cx.listener(Self::on_action_close_active_panel))
            .on_action(cx.listener(Self::on_action_open_file_dialog))
            .on_action(cx.listener(Self::on_action_quit))
            .on_action(cx.listener(Self::on_action_select_all))
            .on_action(cx.listener(Self::on_action_go_to_beginning))
            .on_action(cx.listener(Self::on_action_go_to_end))
            .on_action(cx.listener(Self::on_action_navigate_back))
            .on_action(cx.listener(Self::on_action_navigate_forward))
            .on_action(cx.listener(Self::on_action_toggle_edit_column))
            .on_action(cx.listener(Self::on_action_toggle_goto_address))
            .on_action(cx.listener(Self::on_action_toggle_search))
            .on_action(cx.listener(Self::on_action_toggle_replace))
            .on_action(cx.listener(Self::on_action_toggle_search_panel))
            .on_action(cx.listener(Self::on_action_search_next))
            .on_action(cx.listener(Self::on_action_search_prev))
            .on_action(cx.listener(Self::on_action_undo))
            .on_action(cx.listener(Self::on_action_redo))
            .on_action(cx.listener(Self::on_action_cut))
            .on_action(cx.listener(Self::on_action_copy))
            .on_action(cx.listener(Self::on_action_paste))
            .on_action(cx.listener(Self::on_action_copy_as_hexdump))
            .on_action(cx.listener(Self::on_action_copy_as_cpp_array))
            .on_action(cx.listener(Self::on_action_copy_as_hex_stream))
            .on_action(cx.listener(Self::on_action_copy_as_hex_spaces))
            .on_action(cx.listener(Self::on_action_copy_as_printable_text))
            .on_action(cx.listener(Self::on_action_copy_as_base64))
            .on_action(cx.listener(Self::on_action_copy_as_escaped_string))
            .on_action(cx.listener(Self::on_action_copy_as_binary))
            .on_action(cx.listener(Self::on_action_copy_as_rust_array))
            .on_action(cx.listener(Self::on_action_copy_as_json_array))
            .on_action(cx.listener(Self::on_action_bookmark_red))
            .on_action(cx.listener(Self::on_action_bookmark_orange))
            .on_action(cx.listener(Self::on_action_bookmark_yellow))
            .on_action(cx.listener(Self::on_action_bookmark_green))
            .on_action(cx.listener(Self::on_action_bookmark_cyan))
            .on_action(cx.listener(Self::on_action_bookmark_blue))
            .on_action(cx.listener(Self::on_action_bookmark_purple))
            .on_action(cx.listener(Self::on_action_bookmark_pink))
            .on_action(cx.listener(Self::on_action_clear_bookmark))
            .on_action(cx.listener(Self::on_action_clear_all_bookmarks))
            .on_action(cx.listener(Self::on_action_add_custom_break))
            .on_action(cx.listener(Self::on_action_remove_custom_break_backward))
            .on_action(cx.listener(Self::on_action_remove_custom_break_forward))
            .on_action(cx.listener(Self::on_action_remove_custom_breaks_in_range))
            .on_action(cx.listener(Self::on_action_join_line))
            .on_action(cx.listener(Self::on_action_clear_all_custom_breaks))
            .on_action(cx.listener(Self::on_action_show_all_bookmarks))
            .on_action(cx.listener(Self::on_action_hide_all_bookmarks))
            .on_action(cx.listener(Self::on_action_toggle_hide_unbookmarked))
            .on_action(cx.listener(Self::on_action_unfold_bookmark_at_cursor))
            .on_action(cx.listener(Self::on_action_toggle_bookmark_red))
            .on_action(cx.listener(Self::on_action_toggle_bookmark_orange))
            .on_action(cx.listener(Self::on_action_toggle_bookmark_yellow))
            .on_action(cx.listener(Self::on_action_toggle_bookmark_green))
            .on_action(cx.listener(Self::on_action_toggle_bookmark_cyan))
            .on_action(cx.listener(Self::on_action_toggle_bookmark_blue))
            .on_action(cx.listener(Self::on_action_toggle_bookmark_purple))
            .on_action(cx.listener(Self::on_action_toggle_bookmark_pink))
            .on_action(cx.listener(Self::on_action_show_only_bookmark_red))
            .on_action(cx.listener(Self::on_action_show_only_bookmark_orange))
            .on_action(cx.listener(Self::on_action_show_only_bookmark_yellow))
            .on_action(cx.listener(Self::on_action_show_only_bookmark_green))
            .on_action(cx.listener(Self::on_action_show_only_bookmark_cyan))
            .on_action(cx.listener(Self::on_action_show_only_bookmark_blue))
            .on_action(cx.listener(Self::on_action_show_only_bookmark_purple))
            .on_action(cx.listener(Self::on_action_show_only_bookmark_pink))
            .on_action(cx.listener(Self::on_action_set_encoding))
            .on_action(cx.listener(Self::on_action_set_encoding_ascii))
            .on_action(cx.listener(Self::on_action_set_encoding_utf8))
            .on_action(cx.listener(Self::on_action_set_encoding_utf16le))
            .on_action(cx.listener(Self::on_action_set_encoding_utf16be))
            .on_action(cx.listener(Self::on_action_set_radix_hex))
            .on_action(cx.listener(Self::on_action_set_radix_dec))
            .on_action(cx.listener(Self::on_action_set_radix_oct))
            .on_action(cx.listener(Self::on_action_set_radix_bin))
            .on_action(cx.listener(Self::on_action_set_group_size_1))
            .on_action(cx.listener(Self::on_action_set_group_size_2))
            .on_action(cx.listener(Self::on_action_set_group_size_4))
            .on_action(cx.listener(Self::on_action_set_group_size_8))
            .on_action(cx.listener(Self::on_action_set_byte_order_le))
            .on_action(cx.listener(Self::on_action_set_byte_order_be))
            .on_action(cx.listener(Self::on_action_toggle_byte_order))
            .on_action(cx.listener(Self::on_action_open_diff))
            .on_action(cx.listener(Self::on_action_select_for_compare))
            .on_action(cx.listener(Self::on_action_compare_with_active_file))
            .on_action(cx.listener(Self::on_action_compare_open_files))
            .on_action(cx.listener(Self::on_action_compare_visible_panes))
            .on_action(cx.listener(Self::on_action_swap_diff_files))
            .on_action(cx.listener(Self::on_action_refresh_diff))
            .on_action(cx.listener(Self::on_action_next_difference))
            .on_action(cx.listener(Self::on_action_prev_difference))
            .on_action(cx.listener(Self::on_action_toggle_sync_scroll))
            .on_action(cx.listener(Self::on_action_toggle_left_panel))
            .on_action(cx.listener(Self::on_action_open_settings))
            .on_action(cx.listener(Self::on_action_open_about))
            .on_action(cx.listener(Self::on_action_open_visual_map))
            .on_action(cx.listener(Self::on_action_show_files_tab))
            .on_action(cx.listener(Self::on_action_show_search_tab))
            .on_action(cx.listener(Self::on_action_show_strings_tab))
            .on_action(cx.listener(Self::on_action_show_zlib_tab))
            .on_action(cx.listener(Self::on_action_show_structure_tab))
            .on_action(cx.listener(Self::on_action_show_inspector_tab))
            .on_action(cx.listener(Self::on_action_show_checksum_tab))
            .on_action(cx.listener(Self::on_action_show_bookmarks_tab))
            .on_action(cx.listener(Self::on_action_export_bookmarks))
            .on_action(cx.listener(Self::on_action_import_bookmarks))
            .on_action(cx.listener(Self::on_action_import_hex_or_mot))
            .on_action(cx.listener(Self::on_action_import_base64))
            .on_action(cx.listener(Self::on_action_export_base64))
            .on_action(cx.listener(Self::on_action_export_motorola_srec))
            .on_action(cx.listener(Self::on_action_export_intel_hex))
            .on_action(cx.listener(Self::on_action_export_raw_binary))
            .on_action(cx.listener(Self::on_action_load_structure_definition))
            .on_action(cx.listener(Self::on_action_load_structure_definition_from_history))
            .on_action(cx.listener(Self::on_action_remove_structure_definition_from_history))
            .on_action(cx.listener(Self::on_action_remove_file_from_history))
            .on_action(cx.listener(Self::on_action_clear_structure_definition))
            .on_action(cx.listener(Self::on_action_toggle_inline_structure_view))
            .on_action(cx.listener(Self::on_action_expand_all_structure))
            .on_action(cx.listener(Self::on_action_collapse_all_structure))
            .on_action(cx.listener(Self::on_action_open_folder))
            .on_action(cx.listener(Self::on_action_close_folder))
            .on_action(cx.listener(Self::on_action_activate_next_tab))
            .on_action(cx.listener(Self::on_action_activate_previous_tab))
            .on_action(cx.listener(Self::on_action_activate_tab))
            .on_action(cx.listener(Self::on_action_close_other_tabs))
            .on_action(cx.listener(Self::on_action_close_tabs_to_right))
            .on_action(cx.listener(Self::on_action_close_saved_tabs))
            .on_action(cx.listener(Self::on_action_close_all_tabs))
            .on_action(cx.listener(Self::on_action_copy_path))
            .on_action(cx.listener(Self::on_action_copy_file_name))
            .on_action(cx.listener(Self::on_action_reveal_in_explorer))
            .on_action(cx.listener(Self::on_action_split_right))
            .on_action(cx.listener(Self::on_action_split_down))
            .drag_over::<ExternalPaths>(|style, _paths, _window, cx| {
                let theme = cx.theme();
                style.border_2().border_color(theme.drag_border).bg(theme.drop_target)
            })
            .on_drop(cx.listener(move |this, external_paths: &ExternalPaths, window, cx| {
                for path in external_paths.paths() {
                    if path.is_file() {
                        let action = crate::actions::OpenFile::new(path.to_string_lossy().to_string());
                        this.on_action_open_file(&action, window, cx);
                    } else if path.is_dir() {
                        this.left_panel.update(cx, |p, cx| {
                            p.set_tab(crate::ui::panels::left_panel::LeftPanelTab::Files, cx);
                            p.file_tree.update(cx, |ft, cx| {
                                ft.set_root_path(path.clone(), cx);
                            });
                        });
                        this.set_left_panel_visible(true, window, cx);
                    }
                }
                cx.notify();
            }))
            .relative()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(self.title_bar.clone())
            .child(
                div()
                    .track_focus(&self.focus_handle)
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(self.activity_bar.clone())
                    .child({
                        let scratchpad_panel_width = cx
                            .try_global::<crate::settings::ScratchpadSettings>()
                            .map(|s| s.panel_width)
                            .unwrap_or(crate::settings::DEFAULT_SCRATCHPAD_PANEL_WIDTH);

                        let center_and_right = if self.is_right_panel_visible {
                            h_resizable("workspace-scratchpad-resize")
                                .with_state(&self.workspace_resizable_state)
                                .on_resize(move |state, _window, cx| {
                                    let sizes = state.read(cx).sizes();
                                    if let Some(right_size) = sizes.last() {
                                        let w = right_size.as_f32();
                                        if (crate::settings::MIN_SCRATCHPAD_PANEL_WIDTH..=crate::settings::MAX_SCRATCHPAD_PANEL_WIDTH).contains(&w)
                                            && cx.has_global::<crate::settings::ScratchpadSettings>()
                                        {
                                            let current_w = cx.global::<crate::settings::ScratchpadSettings>().panel_width;
                                            if (current_w - w).abs() > 1.0 {
                                                cx.update_global::<crate::settings::ScratchpadSettings, _>(|settings, _| {
                                                    settings.set_panel_width(w);
                                                });
                                                crate::settings::save_current(cx);
                                            }
                                        }
                                    }
                                })
                                .child(resizable_panel().child(self.pane_tree.clone()))
                                .child(
                                    resizable_panel()
                                        .size(px(scratchpad_panel_width))
                                        .size_range(px(crate::settings::MIN_SCRATCHPAD_PANEL_WIDTH)..px(crate::settings::MAX_SCRATCHPAD_PANEL_WIDTH))
                                        .flex_none()
                                        .child(
                                            div()
                                                .size_full()
                                                .min_w_0()
                                                .min_h_0()
                                                .overflow_hidden()
                                                .border_l_1()
                                                .border_color(theme.border)
                                                .child(self.scratchpad_panel.clone()),
                                        ),
                                )
                                .into_any_element()
                        } else {
                            self.pane_tree.clone().into_any_element()
                        };

                        let main_layout = if self.is_left_panel_visible {
                            let view = cx.entity().clone();
                            h_resizable("workspace-left-resize")
                                .with_state(&self.left_panel_resizable_state)
                                .on_resize(move |state, _window, cx| {
                                    let sizes = state.read(cx).sizes();
                                    if let Some(left_size) = sizes.first() {
                                        let w = left_size.as_f32();
                                        if (MIN_LEFT_PANEL_WIDTH..=MAX_LEFT_PANEL_WIDTH).contains(&w) {
                                            view.update(cx, |this, _| {
                                                this.left_panel_width = w;
                                            });
                                        }
                                    }
                                })
                                .child(
                                    resizable_panel()
                                        .size(px(self.left_panel_width))
                                        .size_range(px(MIN_LEFT_PANEL_WIDTH)..px(MAX_LEFT_PANEL_WIDTH))
                                        .flex_none()
                                        .child(self.left_panel.clone()),
                                )
                                .child(resizable_panel().child(center_and_right))
                                .into_any_element()
                        } else {
                            center_and_right
                        };

                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .size_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_hidden()
                            .child(main_layout)
                    }),
            )
            .child(self.status_bar.clone())
            .when_some(self.active_palette.clone(), |el, palette| {
                let content = match palette {
                    ActivePalette::QuickOpen(modal) => modal.into_any_element(),
                    ActivePalette::CommandPalette(modal) => modal.into_any_element(),
                };
                el.child(
                    div()
                        .absolute()
                        .inset_0()
                        .bg(theme.overlay)
                        .flex()
                        .items_start()
                        .justify_center()
                        .pt(px(80.0))
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.close_active_palette(window, cx);
                            }),
                        )
                        .block_scroll()
                        .child(content),
                )
            })
    }
}
