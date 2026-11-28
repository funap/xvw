use crate::actions::*;
use crate::app_state::InsertModeState;
use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::format::{HexCase, decode_uint_value_with_case, format_binary_repr, format_size_friendly, format_text_repr};
use crate::core::radix::{ByteGroupSize, DisplayRadix};
use crate::ui::icon::IconName;
use crate::ui::views::diff_view::DiffView;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, DropdownMenu as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::status_bar::StatusBar as KitStatusBar;
use gpui_kit::component::{ActiveTheme, Sizable as _, Size, StyledExt, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::*;

#[derive(Clone, PartialEq, Action)]
#[action(namespace = status_bar, no_json)]
struct CopyStatusText {
    value: String,
    description: String,
}

pub enum StatusBarEvent {
    #[allow(dead_code)]
    ToggleLeftPanel,
}

pub struct StatusBar {
    focus_handle: FocusHandle,
    active_editor: Option<WeakEntity<Editor>>,
    editor_subscription: Option<Subscription>,
    active_diff_view: Option<WeakEntity<DiffView>>,
    diff_subscription: Option<Subscription>,
    _insert_mode_subscription: Subscription,
    _hex_case_subscription: Subscription,
}

/// Resolves the encoding shown in the status bar, giving precedence to an active diff tab
/// over any fallback background editor.
pub(crate) fn resolve_status_bar_encoding(diff_encoding: Option<Encoding>, editor_encoding: Option<Encoding>) -> Option<Encoding> {
    diff_encoding.or(editor_encoding)
}

impl EventEmitter<StatusBarEvent> for StatusBar {}

impl StatusBar {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let _insert_mode_subscription = cx.observe_global::<InsertModeState>(|_, cx| {
            cx.notify();
        });
        let _hex_case_subscription = cx.observe_global::<HexCase>(|_, cx| {
            cx.notify();
        });
        Self {
            focus_handle,
            active_editor: None,
            editor_subscription: None,
            active_diff_view: None,
            diff_subscription: None,
            _insert_mode_subscription,
            _hex_case_subscription,
        }
    }

    pub fn set_active_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        self.editor_subscription = None;
        self.active_editor = editor.as_ref().map(|e| e.downgrade());
        if let Some(editor) = editor {
            self.editor_subscription = Some(cx.observe(&editor, |_, _, cx| {
                cx.notify();
            }));
        }
        cx.notify();
    }

    /// Sets the currently active diff view so the status bar reflects and updates with its encoding.
    pub fn set_active_diff_view(&mut self, diff_view: Option<Entity<DiffView>>, cx: &mut Context<Self>) {
        self.diff_subscription = None;
        self.active_diff_view = diff_view.as_ref().map(|d| d.downgrade());
        if let Some(diff_view) = diff_view {
            self.diff_subscription = Some(cx.observe(&diff_view, |_, _, cx| {
                cx.notify();
            }));
        }
        cx.notify();
    }

    fn on_copy_status_text(&mut self, action: &CopyStatusText, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(Notification::info(format!("Copied {}", action.description)), cx);
    }
}

impl Render for StatusBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let active_diff_view = self.active_diff_view.as_ref().and_then(|d| d.upgrade());
        let active_editor = active_diff_view
            .as_ref()
            .map(|d| d.read(cx).active_side_editor())
            .or_else(|| self.active_editor.as_ref().and_then(|e| e.upgrade()));
        let has_editor = active_editor.is_some();
        let is_diff_primary = active_diff_view.is_some();

        let (total_size, cursor_addr) = if let Some(editor) = &active_editor {
            let editor = editor.read(cx);
            (Some(editor.total_size()), editor.cursor_address())
        } else {
            (None, 0)
        };

        let (current_byte_info, raw_byte_val) = if let Some(editor) = &active_editor {
            let editor = editor.read(cx);
            let doc = editor.document.read().ok();

            let target = if editor.has_selection() {
                if let Some(range) = editor.selected_range_or_cursor() {
                    let len = range.len();
                    if len <= 8 { Some((range.start, len)) } else { None }
                } else {
                    Some((editor.cursor.offset, editor.options.group_size.byte_count().min(8)))
                }
            } else {
                Some((editor.cursor.offset, editor.options.group_size.byte_count().min(8)))
            };

            if let (Some(d), Some((offset, len))) = (doc, target) {
                let slice = d.buffer.get_range(offset, len);
                if slice.len() == len && len > 0 {
                    let (uint_val, hex_str) = decode_uint_value_with_case(slice, editor.options.is_big_endian, hex_case);
                    let text_repr = format_text_repr(slice, editor.options.encoding);
                    let bin_repr = format_binary_repr(uint_val, len);
                    let display_str = format!("Val: {} ({}, {}, {})", hex_str, uint_val, text_repr, bin_repr);
                    let copy_str = format!("{} ({})", hex_str, uint_val);
                    (Some(display_str), Some(copy_str))
                } else {
                    (None, None)
                }
            } else {
                (None, None)
            }
        } else {
            (None, None)
        };

        let position_info = active_editor.as_ref().map(|editor| {
            let editor = editor.read(cx);
            if editor.has_selection() {
                if let Some(range) = editor.selected_range_or_cursor() {
                    let len = range.len();
                    let end_inclusive = range.end.saturating_sub(1);
                    let start_addr = editor.offset_to_address(range.start);
                    let end_addr = editor.offset_to_address(end_inclusive);
                    let byte_word = if len == 1 { "byte" } else { "bytes" };
                    let text = format!(
                        "{} - {} ({} | {} {})",
                        hex_case.format_prefixed(start_addr),
                        hex_case.format_prefixed(end_addr),
                        hex_case.format_prefixed(len),
                        len,
                        byte_word
                    );
                    let copy_val = format!("{}..{}", hex_case.format_prefixed(start_addr), hex_case.format_prefixed(end_addr));
                    (text, copy_val)
                } else {
                    let cur_addr = editor.cursor_address();
                    (
                        format!("{} ({})", hex_case.format_prefixed(cur_addr), cur_addr),
                        hex_case.format_prefixed(cur_addr),
                    )
                }
            } else {
                let cur_addr = editor.cursor_address();
                (
                    format!("{} ({})", hex_case.format_prefixed(cur_addr), cur_addr),
                    hex_case.format_prefixed(cur_addr),
                )
            }
        });

        let is_read_only = active_editor.as_ref().map(|e| e.read(cx).is_read_only()).unwrap_or(false);
        let file_mode_label = if is_read_only { "Read-only" } else { "Writable" };
        let file_mode_icon = if is_read_only { IconName::PenOff } else { IconName::PenLine };
        let insert_mode = InsertModeState::is_enabled(cx);
        let edit_mode_label = if insert_mode { "Insert" } else { "Overwrite" };

        let current_radix = active_editor.as_ref().map(|e| e.read(cx).options.radix);
        let current_group_size = active_editor.as_ref().map(|e| e.read(cx).options.group_size);
        let current_big_endian = active_editor.as_ref().map(|e| e.read(cx).options.is_big_endian);

        let radix_badge_info = current_radix.map(|r| r.short_label());

        let grouping_badge_info = active_editor.as_ref().map(|editor| {
            let editor = editor.read(cx);
            let group_str = editor.options.group_size.short_label();
            let endian_str = if editor.options.group_size.byte_count() > 1 {
                if editor.options.is_big_endian { " BE" } else { " LE" }
            } else {
                ""
            };
            format!("{}{}", group_str, endian_str)
        });

        let current_encoding = resolve_status_bar_encoding(
            active_diff_view.as_ref().map(|d| d.read(cx).encoding()),
            active_editor.as_ref().map(|e| e.read(cx).options.encoding),
        );
        let encoding_info = current_encoding.map(Encoding::label);

        let status_focus_handle = self.focus_handle.clone();

        let left_section = div()
            .flex()
            .items_center()
            .min_w_0()
            .gap_2()
            .text_xs()
            .when_some(position_info, |el, (position_text, position_copy_val)| {
                let focus_for_menu = status_focus_handle.clone();
                let copy_str_click = position_copy_val.clone();
                let offset_hex = hex_case.format_prefixed(cursor_addr);
                let offset_dec = format!("{}", cursor_addr);
                let pos_str = position_copy_val;
                el.child(
                    div()
                        .id("status-position")
                        .flex_shrink_0()
                        .font_medium()
                        .text_color(theme.foreground)
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme.primary))
                        .tooltip(|_window, cx| {
                            cx.new(|_| gpui_kit::component::tooltip::Tooltip::new("Click to copy address • Right-click for options"))
                                .into()
                        })
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy_str_click.clone()));
                            window.push_notification(Notification::info(format!("Copied address {}", copy_str_click)), cx);
                        })
                        .context_menu(move |menu, _window, _cx| {
                            let off_h = offset_hex.clone();
                            let off_d = offset_dec.clone();
                            let p_str = pos_str.clone();
                            menu.action_context(focus_for_menu.clone())
                                .menu_with_icon("Go to Address...", IconName::Search, Box::new(ToggleGoToAddress))
                                .separator()
                                .menu_with_icon("Go to Beginning", IconName::ChevronUp, Box::new(GoToBeginning))
                                .menu_with_icon("Go to End", IconName::ChevronDown, Box::new(GoToEnd))
                                .separator()
                                .menu_with_icon(
                                    format!("Copy Address ({})", p_str),
                                    IconName::Copy,
                                    Box::new(CopyStatusText {
                                        description: format!("address {}", p_str),
                                        value: p_str,
                                    }),
                                )
                                .menu_with_icon(
                                    format!("Copy Hex Address ({})", off_h),
                                    IconName::Hash,
                                    Box::new(CopyStatusText {
                                        description: format!("hex address {}", off_h),
                                        value: off_h,
                                    }),
                                )
                                .menu_with_icon(
                                    format!("Copy Dec Address ({})", off_d),
                                    IconName::Hash,
                                    Box::new(CopyStatusText {
                                        description: format!("decimal address {}", off_d),
                                        value: off_d,
                                    }),
                                )
                        })
                        .child(position_text),
                )
            })
            .when(!has_editor, |el| {
                let label = if is_diff_primary { "Diff View" } else { "Ready" };
                el.child(div().text_color(theme.muted_foreground).child(label))
            })
            .when_some(current_byte_info, |el, val_str| {
                let raw_copy = raw_byte_val.unwrap_or_default();
                el.child(div().w_px().h_3().bg(theme.border).flex_shrink_0()).child(
                    div()
                        .id("status-val-pill")
                        .min_w_0()
                        .truncate()
                        .whitespace_nowrap()
                        .text_color(theme.muted_foreground)
                        .cursor_pointer()
                        .hover(|s| s.text_color(theme.foreground))
                        .tooltip(|_window, cx| cx.new(|_| gpui_kit::component::tooltip::Tooltip::new("Click to copy value")).into())
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            if !raw_copy.is_empty() {
                                cx.write_to_clipboard(ClipboardItem::new_string(raw_copy.clone()));
                                window.push_notification(Notification::info(format!("Copied value {}", raw_copy)), cx);
                            }
                        })
                        .child(val_str),
                )
            });

        let right_section = div()
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap_2()
            .text_xs()
            .when_some(total_size, |el, size| {
                el.child(
                    div()
                        .id("status-size-pill")
                        .px_2()
                        .py_0p5()
                        .rounded_sm()
                        .cursor_pointer()
                        .text_color(theme.muted_foreground)
                        .hover(|s| s.bg(theme.muted).text_color(theme.foreground))
                        .tooltip(|_window, cx| {
                            cx.new(|_| gpui_kit::component::tooltip::Tooltip::new("Click to copy exact file size in bytes"))
                                .into()
                        })
                        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(size.to_string()));
                            window.push_notification(Notification::info(format!("Copied size ({size} bytes)")), cx);
                        })
                        .child(format!("Size: {}", format_size_friendly(size))),
                )
            })
            .when(has_editor, |el| {
                let edit_mode_btn = Button::new("status-edit-mode-btn")
                    .label(edit_mode_label)
                    .ghost()
                    .with_size(Size::XSmall)
                    .when(insert_mode, |btn| btn.text_color(theme.warning))
                    .tooltip(if insert_mode {
                        "Insert Mode (typing inserts bytes and shifts offsets). Click to switch to Overwrite Mode"
                    } else {
                        "Overwrite Mode. Click to switch to Insert Mode"
                    })
                    .on_click(cx.listener(|_, _, window, cx| {
                        window.dispatch_action(Box::new(ToggleInsertMode), cx);
                    }));

                el.child(
                    Button::new("status-file-mode-btn")
                        .icon(file_mode_icon)
                        .label(file_mode_label)
                        .ghost()
                        .with_size(Size::XSmall)
                        .tooltip(if is_read_only {
                            "Read-only. Click to make this file writable"
                        } else {
                            "Writable. Click to make this file read-only"
                        })
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(ToggleReadOnly), cx);
                        })),
                )
                .child(div().w_px().h_3().bg(theme.border))
                .child(edit_mode_btn)
            })
            .when_some(radix_badge_info, |el, radix_label| {
                el.child(div().w_px().h_3().bg(theme.border)).child(
                    Button::new("status-radix-btn")
                        .label(radix_label)
                        .ghost()
                        .with_size(Size::XSmall)
                        .tooltip("Click to change Radix (Hex, Dec, Oct, Bin)")
                        .dropdown_menu_with_anchor(Anchor::BottomRight, move |menu, _window, _cx| {
                            menu.menu_with_check("Hexadecimal (16)", current_radix == Some(DisplayRadix::Hexadecimal), Box::new(SetRadixHex))
                                .menu_with_check("Decimal (10)", current_radix == Some(DisplayRadix::Decimal), Box::new(SetRadixDec))
                                .menu_with_check("Octal (8)", current_radix == Some(DisplayRadix::Octal), Box::new(SetRadixOct))
                                .menu_with_check("Binary (2)", current_radix == Some(DisplayRadix::Binary), Box::new(SetRadixBin))
                        }),
                )
            })
            .when_some(grouping_badge_info, |el, grouping_label| {
                el.child(div().w_px().h_3().bg(theme.border)).child(
                    Button::new("status-grouping-btn")
                        .label(grouping_label)
                        .ghost()
                        .with_size(Size::XSmall)
                        .tooltip("Click to change Grouping & Byte Order")
                        .dropdown_menu_with_anchor(Anchor::BottomRight, move |menu, window, cx| {
                            menu.menu_with_check("1 Byte (8-bit)", current_group_size == Some(ByteGroupSize::One), Box::new(SetGroupSize1))
                                .menu_with_check("2 Bytes (16-bit)", current_group_size == Some(ByteGroupSize::Two), Box::new(SetGroupSize2))
                                .menu_with_check("4 Bytes (32-bit)", current_group_size == Some(ByteGroupSize::Four), Box::new(SetGroupSize4))
                                .menu_with_check("8 Bytes (64-bit)", current_group_size == Some(ByteGroupSize::Eight), Box::new(SetGroupSize8))
                                .separator()
                                .submenu("Byte Order", window, cx, move |menu, _window, _cx| {
                                    menu.menu_with_check("Little Endian", current_big_endian == Some(false), Box::new(SetByteOrderLittleEndian))
                                        .menu_with_check("Big Endian", current_big_endian == Some(true), Box::new(SetByteOrderBigEndian))
                                })
                        }),
                )
            })
            .when_some(encoding_info, |el, enc| {
                el.when(has_editor, |d| d.child(div().w_px().h_3().bg(theme.border))).child(
                    Button::new("status-encoding-btn")
                        .label(enc)
                        .ghost()
                        .with_size(Size::XSmall)
                        .tooltip("Click to change Text Encoding")
                        .dropdown_menu_with_anchor(Anchor::BottomRight, move |menu, window, cx| {
                            let menu = Encoding::primary_encodings().iter().copied().fold(menu, |menu, encoding| {
                                menu.menu_with_check(encoding.label(), Some(encoding) == current_encoding, Box::new(SetEncoding { encoding }))
                            });
                            let menu = menu.separator();
                            Encoding::secondary_categories().iter().fold(menu, |menu, (cat, encs)| {
                                menu.submenu(cat.label(), window, cx, move |menu, _window, _cx| {
                                    encs.iter().copied().fold(menu, |menu, encoding| {
                                        menu.menu_with_check(encoding.label(), Some(encoding) == current_encoding, Box::new(SetEncoding { encoding }))
                                    })
                                })
                            })
                        }),
                )
            });

        div()
            .id("status-bar-container")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_copy_status_text))
            .w_full()
            .context_menu(move |menu, window, cx| {
                let menu = if has_editor {
                    menu.menu_with_icon("Find...", IconName::Search, Box::new(ToggleSearch))
                        .menu_with_check("Read-only", is_read_only, Box::new(ToggleReadOnly))
                        .separator()
                        .submenu("Radix", window, cx, move |menu, _window, _cx| {
                            menu.menu_with_check("Hexadecimal (16)", current_radix == Some(DisplayRadix::Hexadecimal), Box::new(SetRadixHex))
                                .menu_with_check("Decimal (10)", current_radix == Some(DisplayRadix::Decimal), Box::new(SetRadixDec))
                                .menu_with_check("Octal (8)", current_radix == Some(DisplayRadix::Octal), Box::new(SetRadixOct))
                                .menu_with_check("Binary (2)", current_radix == Some(DisplayRadix::Binary), Box::new(SetRadixBin))
                        })
                        .submenu("Grouping", window, cx, move |menu, _window, _cx| {
                            menu.menu_with_check("1 Byte (8-bit)", current_group_size == Some(ByteGroupSize::One), Box::new(SetGroupSize1))
                                .menu_with_check("2 Bytes (16-bit)", current_group_size == Some(ByteGroupSize::Two), Box::new(SetGroupSize2))
                                .menu_with_check("4 Bytes (32-bit)", current_group_size == Some(ByteGroupSize::Four), Box::new(SetGroupSize4))
                                .menu_with_check("8 Bytes (64-bit)", current_group_size == Some(ByteGroupSize::Eight), Box::new(SetGroupSize8))
                        })
                        .submenu("Byte Order", window, cx, move |menu, _window, _cx| {
                            menu.menu_with_check("Little Endian", current_big_endian == Some(false), Box::new(SetByteOrderLittleEndian))
                                .menu_with_check("Big Endian", current_big_endian == Some(true), Box::new(SetByteOrderBigEndian))
                        })
                        .submenu("Encoding", window, cx, move |menu, window, cx| {
                            let menu = Encoding::primary_encodings().iter().copied().fold(menu, |menu, encoding| {
                                menu.menu_with_check(encoding.label(), Some(encoding) == current_encoding, Box::new(SetEncoding { encoding }))
                            });
                            let menu = menu.separator();
                            Encoding::secondary_categories().iter().fold(menu, |menu, (cat, encs)| {
                                menu.submenu(cat.label(), window, cx, move |menu, _window, _cx| {
                                    encs.iter().copied().fold(menu, |menu, encoding| {
                                        menu.menu_with_check(encoding.label(), Some(encoding) == current_encoding, Box::new(SetEncoding { encoding }))
                                    })
                                })
                            })
                        })
                        .separator()
                } else {
                    menu
                };
                menu.menu_with_icon("Toggle Left Panel", IconName::PanelLeft, Box::new(ToggleLeftPanel))
                    .menu_with_icon("Toggle Scratchpad", IconName::PanelRight, Box::new(ToggleRightPanel))
            })
            .child(
                KitStatusBar::new()
                    .font_family(cx.global::<Appearance>().font_family.clone())
                    .left(left_section)
                    .right(right_section),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Encoding, resolve_status_bar_encoding};

    #[test]
    fn test_resolve_status_bar_encoding_prefers_active_diff() {
        assert_eq!(
            resolve_status_bar_encoding(Some(Encoding::ShiftJis), Some(Encoding::Ascii)),
            Some(Encoding::ShiftJis)
        );
        assert_eq!(resolve_status_bar_encoding(Some(Encoding::Utf8), None), Some(Encoding::Utf8));
    }

    #[test]
    fn test_resolve_status_bar_encoding_falls_back_to_editor_or_none() {
        assert_eq!(resolve_status_bar_encoding(None, Some(Encoding::Utf16Le)), Some(Encoding::Utf16Le));
        assert_eq!(resolve_status_bar_encoding(None, None), None);
    }
}
