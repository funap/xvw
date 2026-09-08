use gpui_kit::prelude::*;
use gpui_kit::*;

use super::Workspace;
use crate::actions::*;
use crate::core::encoding::Encoding;
use crate::ui::components::activity_bar::Activity;
use crate::ui::pane::{SplitDirection, TabContent};
use crate::ui::panels::left_panel::LeftPanelTab;

impl Workspace {
    pub(crate) fn on_action_select_all(&mut self, action: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.select_all(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_go_to_beginning(&mut self, action: &GoToBeginning, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.go_to_beginning(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_go_to_end(&mut self, action: &GoToEnd, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.go_to_end(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_toggle_search(&mut self, action: &ToggleSearch, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.toggle_search(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_toggle_goto_address(&mut self, action: &ToggleGoToAddress, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.toggle_goto_address(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_search_next(&mut self, action: &SearchNext, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.search_next(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_search_prev(&mut self, action: &SearchPrev, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.search_prev(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy(&mut self, action: &Copy, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_as_hexdump(&mut self, action: &CopyAsHexDump, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy_as_hexdump(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_as_cpp_array(&mut self, action: &CopyAsCppArray, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy_as_cpp_array(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_as_hex_stream(&mut self, action: &CopyAsHexStream, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy_as_hex_stream(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_as_hex_spaces(&mut self, action: &CopyAsHexSpaces, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy_as_hex_spaces(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_as_printable_text(&mut self, action: &CopyAsPrintableText, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy_as_printable_text(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_as_base64(&mut self, action: &CopyAsBase64, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy_as_base64(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_as_escaped_string(&mut self, action: &CopyAsEscapedString, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy_as_escaped_string(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_as_binary(&mut self, action: &CopyAsBinary, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy_as_binary(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_as_rust_array(&mut self, action: &CopyAsRustArray, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy_as_rust_array(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_as_json_array(&mut self, action: &CopyAsJsonArray, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.copy_as_json_array(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_copy_path(&mut self, _: &CopyPath, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            let path = editor.read(cx).document.read().ok().map(|d| d.path().to_path_buf());
            if let Some(path) = path {
                cx.write_to_clipboard(ClipboardItem::new_string(path.to_string_lossy().to_string()));
            }
        }
    }

    pub(crate) fn on_action_copy_file_name(&mut self, _: &CopyFileName, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            let path = editor.read(cx).document.read().ok().map(|d| d.path().to_path_buf());
            if let Some(path) = path
                && let Some(name) = path.file_name()
            {
                cx.write_to_clipboard(ClipboardItem::new_string(name.to_string_lossy().to_string()));
            }
        }
    }

    pub(crate) fn on_action_bookmark_red(&mut self, action: &BookmarkRed, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.bookmark_red(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_bookmark_orange(&mut self, action: &BookmarkOrange, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.bookmark_orange(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_bookmark_yellow(&mut self, action: &BookmarkYellow, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.bookmark_yellow(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_bookmark_green(&mut self, action: &BookmarkGreen, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.bookmark_green(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_bookmark_cyan(&mut self, action: &BookmarkCyan, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.bookmark_cyan(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_bookmark_blue(&mut self, action: &BookmarkBlue, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.bookmark_blue(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_bookmark_purple(&mut self, action: &BookmarkPurple, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.bookmark_purple(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_bookmark_pink(&mut self, action: &BookmarkPink, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.bookmark_pink(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_clear_bookmark(&mut self, action: &ClearBookmark, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.clear_bookmark(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_clear_all_bookmarks(&mut self, action: &ClearAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.clear_all_bookmarks(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_show_all_bookmarks(&mut self, action: &ShowAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.show_all_bookmarks(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_hide_all_bookmarks(&mut self, action: &HideAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.hide_all_bookmarks(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_toggle_hide_unbookmarked(&mut self, action: &ToggleHideUnbookmarked, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.toggle_hide_unbookmarked(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_unfold_bookmark_at_cursor(&mut self, action: &UnfoldBookmarkAtCursor, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.unfold_bookmark_at_cursor(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_add_custom_break(&mut self, action: &AddCustomBreak, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.add_custom_break(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_remove_custom_break_backward(&mut self, action: &RemoveCustomBreakBackward, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.remove_custom_break_backward(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_remove_custom_break_forward(&mut self, action: &RemoveCustomBreakForward, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.remove_custom_break_forward(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_join_line(&mut self, action: &JoinLine, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.join_line(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_clear_all_custom_breaks(&mut self, action: &ClearAllCustomBreaks, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.clear_all_custom_breaks(action, window, cx);
            });
        }
    }

    pub(crate) fn on_action_set_encoding(&mut self, action: &SetEncoding, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_encoding(action.encoding);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_set_encoding_ascii(&mut self, _: &SetEncodingAscii, window: &mut Window, cx: &mut Context<Self>) {
        self.on_action_set_encoding(&SetEncoding { encoding: Encoding::Ascii }, window, cx);
    }

    pub(crate) fn on_action_set_encoding_utf8(&mut self, _: &SetEncodingUtf8, window: &mut Window, cx: &mut Context<Self>) {
        self.on_action_set_encoding(&SetEncoding { encoding: Encoding::Utf8 }, window, cx);
    }

    pub(crate) fn on_action_set_encoding_utf16le(&mut self, _: &SetEncodingUtf16Le, window: &mut Window, cx: &mut Context<Self>) {
        self.on_action_set_encoding(&SetEncoding { encoding: Encoding::Utf16Le }, window, cx);
    }

    pub(crate) fn on_action_set_encoding_utf16be(&mut self, _: &SetEncodingUtf16Be, window: &mut Window, cx: &mut Context<Self>) {
        self.on_action_set_encoding(&SetEncoding { encoding: Encoding::Utf16Be }, window, cx);
    }

    pub(crate) fn on_action_set_radix_hex(&mut self, _: &SetRadixHex, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_radix(crate::core::radix::DisplayRadix::Hexadecimal);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_set_radix_dec(&mut self, _: &SetRadixDec, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_radix(crate::core::radix::DisplayRadix::Decimal);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_set_radix_oct(&mut self, _: &SetRadixOct, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_radix(crate::core::radix::DisplayRadix::Octal);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_set_radix_bin(&mut self, _: &SetRadixBin, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_radix(crate::core::radix::DisplayRadix::Binary);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_set_group_size_1(&mut self, _: &SetGroupSize1, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_group_size(crate::core::radix::ByteGroupSize::One);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_set_group_size_2(&mut self, _: &SetGroupSize2, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_group_size(crate::core::radix::ByteGroupSize::Two);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_set_group_size_4(&mut self, _: &SetGroupSize4, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_group_size(crate::core::radix::ByteGroupSize::Four);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_set_group_size_8(&mut self, _: &SetGroupSize8, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_group_size(crate::core::radix::ByteGroupSize::Eight);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_set_byte_order_le(&mut self, _: &SetByteOrderLittleEndian, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_is_big_endian(false);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_set_byte_order_be(&mut self, _: &SetByteOrderBigEndian, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.set_is_big_endian(true);
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_toggle_byte_order(&mut self, _: &ToggleByteOrder, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.toggle_byte_order();
                cx.notify();
            });
        }
        if let Some(panel) = self.active_editor_panel(cx) {
            panel.update(cx, |panel, cx| {
                panel.focus_hex_view(&FocusHexView, window, cx);
            });
        }
        cx.notify();
    }

    pub(crate) fn on_action_clear_structure_definition(&mut self, _: &ClearStructureDefinition, _: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor(cx) else {
            return;
        };
        let document_path = editor.read(cx).document.read().ok().map(|document| document.path().to_path_buf());
        editor.update(cx, |editor, cx| {
            editor.clear_structure_definition();
            cx.notify();
        });

        if let Some(path) = document_path {
            let service = crate::app_state::AppState::global(cx).document_service.clone();
            service.notify_document_changed(&path, cx);
        }

        // The panels observe the editor entity. Clearing and re-binding the
        // same editor here is redundant and can re-enter the Structure Panel
        // while its action handler is still being dispatched.
        cx.notify();
    }

    pub(crate) fn on_action_toggle_inline_structure_view(&mut self, _: &crate::actions::ToggleInlineStructureView, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            editor.update(cx, |editor, cx| {
                editor.toggle_inline_structure_view();
                cx.notify();
            });
        }
    }

    pub(crate) fn on_action_close_active_panel(&mut self, _: &CloseActivePanel, window: &mut Window, cx: &mut Context<Self>) {
        self.pane_tree.update(cx, |tree, cx| {
            tree.close_active_tab(window, cx);
        });
        self.sync_active_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn on_action_activate_next_tab(&mut self, _: &ActivateNextTab, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(group) = self.pane_tree.read(cx).active_group(cx) {
            group.update(cx, |g, cx| {
                g.activate_next_tab(window, cx);
            });
        }
    }

    pub(crate) fn on_action_activate_previous_tab(&mut self, _: &ActivatePreviousTab, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(group) = self.pane_tree.read(cx).active_group(cx) {
            group.update(cx, |g, cx| {
                g.activate_previous_tab(window, cx);
            });
        }
    }

    pub(crate) fn on_action_activate_tab(&mut self, action: &ActivateTab, window: &mut Window, cx: &mut Context<Self>) {
        if action.index > 0 {
            let zero_based = action.index - 1;
            if let Some(group) = self.pane_tree.read(cx).active_group(cx) {
                group.update(cx, |g, cx| {
                    g.activate_tab(zero_based, window, cx);
                });
            }
        }
    }

    pub(crate) fn on_action_close_other_tabs(&mut self, _: &CloseOtherTabs, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(group) = self.pane_tree.read(cx).active_group(cx) {
            let active_id = group.read(cx).active_tab().map(|t| t.id);
            if let Some(active_id) = active_id {
                let tab_ids: Vec<_> = group.read(cx).tabs.iter().map(|t| t.id).collect();
                for id in tab_ids {
                    if id != active_id {
                        group.update(cx, |g, cx| {
                            g.close_tab(id, window, cx);
                        });
                    }
                }
            }
        }
        self.sync_active_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn on_action_close_tabs_to_right(&mut self, _: &CloseTabsToRight, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(group) = self.pane_tree.read(cx).active_group(cx) {
            let active_id = group.read(cx).active_tab().map(|t| t.id);
            if let Some(active_id) = active_id {
                group.update(cx, |g, cx| {
                    g.close_tabs_to_right(active_id, window, cx);
                });
            }
        }
        self.sync_active_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn on_action_close_saved_tabs(&mut self, _: &CloseSavedTabs, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(group) = self.pane_tree.read(cx).active_group(cx) {
            group.update(cx, |g, cx| {
                g.close_saved_tabs(window, cx);
            });
        }
        self.sync_active_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn on_action_close_all_tabs(&mut self, _: &CloseAllTabs, window: &mut Window, cx: &mut Context<Self>) {
        self.pane_tree = cx.new(|_| crate::ui::pane::PaneTree::new());
        self.sync_active_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn on_action_reveal_in_explorer(&mut self, _: &RevealInExplorer, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor(cx) {
            let path = editor.read(cx).document.read().ok().map(|d| d.path().to_path_buf());
            if let Some(path) = path {
                crate::ui::style::reveal_in_file_explorer(&path);
            }
        }
    }

    pub(crate) fn on_action_split_right(&mut self, _: &SplitRight, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(group) = self.pane_tree.read(cx).active_group(cx) {
            group.update(cx, |g, cx| {
                g.split_active_tab(SplitDirection::Horizontal, window, cx);
            });
        }
    }

    pub(crate) fn on_action_split_down(&mut self, _: &SplitDown, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(group) = self.pane_tree.read(cx).active_group(cx) {
            group.update(cx, |g, cx| {
                g.split_active_tab(SplitDirection::Vertical, window, cx);
            });
        }
    }

    pub(crate) fn on_action_open_settings(&mut self, _: &OpenSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings_panel(window, cx);
    }

    pub(crate) fn on_action_open_about(&mut self, _: &OpenAbout, window: &mut Window, cx: &mut Context<Self>) {
        self.open_about_dialog(window, cx);
    }

    pub(crate) fn open_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use crate::ui::panels::settings_panel::SettingsPanel;

        // Check if settings is already open in any group
        for group in self.pane_tree.read(cx).all_groups() {
            for (idx, tab) in group.read(cx).tabs.iter().enumerate() {
                if tab.content.is_settings() {
                    group.update(cx, |g, cx| {
                        g.activate_tab(idx, window, cx);
                    });
                    self.pane_tree.update(cx, |tree, cx| {
                        tree.set_active_group(group.read(cx).id, cx);
                    });
                    self.sync_active_editor(window, cx);
                    cx.notify();
                    return;
                }
            }
        }

        let settings_panel = cx.new(|cx| SettingsPanel::new(window, cx));
        let content = TabContent::from_settings(settings_panel);
        self.pane_tree.update(cx, |tree, cx| {
            tree.open_tab(content, window, cx);
        });
        self.sync_active_editor(window, cx);
        cx.notify();
    }

    pub(crate) fn on_action_open_visual_map(&mut self, _: &OpenVisualMap, window: &mut Window, cx: &mut Context<Self>) {
        self.select_activity(Activity::Map, window, cx);
    }

    pub(crate) fn on_action_toggle_left_panel(&mut self, _: &ToggleLeftPanel, window: &mut Window, cx: &mut Context<Self>) {
        self.set_left_panel_visible(!self.is_left_panel_visible, window, cx);
    }

    pub(crate) fn on_action_toggle_search_panel(&mut self, _: &crate::actions::ToggleSearchPanel, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_left_panel_visible || self.left_panel.read(cx).active_tab != LeftPanelTab::Search {
            self.left_panel.update(cx, |p, cx| {
                p.set_tab(LeftPanelTab::Search, cx);
            });
            self.set_left_panel_visible(true, window, cx);
        }
        let focus_handle = self.left_panel.read(cx).search_panel.read(cx).focus_handle(cx);
        focus_handle.focus(window, cx);
    }

    pub(crate) fn on_action_show_files_tab(&mut self, _: &ShowFilesTab, window: &mut Window, cx: &mut Context<Self>) {
        self.select_activity(Activity::Files, window, cx);
    }

    pub(crate) fn on_action_show_strings_tab(&mut self, _: &ShowStringsTab, window: &mut Window, cx: &mut Context<Self>) {
        self.select_activity(Activity::Strings, window, cx);
    }

    pub(crate) fn on_action_show_structure_tab(&mut self, _: &ShowStructureTab, window: &mut Window, cx: &mut Context<Self>) {
        self.select_activity(Activity::Structure, window, cx);
    }

    pub(crate) fn on_action_show_checksum_tab(&mut self, _: &ShowChecksumTab, window: &mut Window, cx: &mut Context<Self>) {
        self.select_activity(Activity::Checksum, window, cx);
    }

    pub(crate) fn on_action_show_bookmarks_tab(&mut self, _: &ShowBookmarksTab, window: &mut Window, cx: &mut Context<Self>) {
        self.select_activity(Activity::Bookmarks, window, cx);
    }

    pub(crate) fn select_activity(&mut self, activity: Activity, window: &mut Window, cx: &mut Context<Self>) {
        let tab = match activity {
            Activity::Files => LeftPanelTab::Files,
            Activity::Search => LeftPanelTab::Search,
            Activity::Strings => LeftPanelTab::Strings,
            Activity::Structure => LeftPanelTab::Structure,
            Activity::Inspector => LeftPanelTab::Inspector,
            Activity::Map => LeftPanelTab::Map,
            Activity::Checksum => LeftPanelTab::Checksum,
            Activity::Bookmarks => LeftPanelTab::Bookmarks,
        };

        let current_tab = self.left_panel.read(cx).active_tab;

        if self.is_left_panel_visible && current_tab == tab {
            self.set_left_panel_visible(false, window, cx);
        } else {
            self.left_panel.update(cx, |p, cx| {
                p.set_tab(tab, cx);
            });
            self.set_left_panel_visible(true, window, cx);
            let focus_handle = self.left_panel.read(cx).focus_handle(cx);
            focus_handle.focus(window, cx);
        }
    }
}
