use crate::core::appearance::Appearance;
use crate::core::checksum::{ChecksumAlgorithm, ChecksumResults};
use crate::core::editor::Editor;
use crate::ui::icon::IconName;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{ActiveTheme as _, button::Button, button::ButtonVariants, h_flex, v_flex};
use gpui_kit::component::{Disableable, Selectable, Sizable, Size};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CalculationRange {
    Selection,
    EntireFile,
}

fn selected_range_for_checksum(editor: &Editor) -> Option<Range<usize>> {
    if editor.has_selection() { editor.selected_range_or_cursor() } else { None }
}

const CONTEXT: &str = "ChecksumPanel";

#[derive(Clone, PartialEq, Action)]
#[action(namespace = checksum_panel, no_json)]
struct CopyValue {
    value: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = checksum_panel, no_json)]
struct CopyRow {
    text: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = checksum_panel, no_json)]
struct CopyAllChecksums {
    text: String,
}

pub struct ChecksumPanel {
    pub editor: Option<Entity<Editor>>,
    pub focus_handle: FocusHandle,
    pub calculation_range: CalculationRange,
    pub auto_calculate: bool,
    pub is_calculating: bool,
    pub results: Option<ChecksumResults>,
    pub selected_row: Option<(&'static str, String, Option<String>)>,
    _editor_subscription: Option<Subscription>,
    calculation_task: Option<Task<()>>,
}

impl ChecksumPanel {
    pub fn new(editor: Option<Entity<Editor>>, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let mut this = Self {
            editor: None,
            focus_handle,
            calculation_range: CalculationRange::Selection,
            auto_calculate: true,
            is_calculating: false,
            results: None,
            selected_row: None,
            _editor_subscription: None,
            calculation_task: None,
        };
        this.set_editor(editor, cx);
        this
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        self._editor_subscription = None;
        self.calculation_task = None;
        self.editor = editor.clone();
        self.results = None;
        self.is_calculating = false;

        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |this, _, cx| {
                this.on_editor_changed(cx);
            }));
            self.on_editor_changed(cx);
        }
        cx.notify();
    }

    fn on_editor_changed(&mut self, cx: &mut Context<Self>) {
        if self.auto_calculate {
            self.trigger_calculation(cx);
        } else {
            cx.notify();
        }
    }

    fn trigger_calculation(&mut self, cx: &mut Context<Self>) {
        let Some(editor_entity) = &self.editor else {
            self.results = None;
            self.is_calculating = false;
            cx.notify();
            return;
        };

        // Determine range and buffer parameters in a nested scope to free the immutable borrow on cx
        let (range, data) = {
            let editor = editor_entity.read(cx);
            let selected_range = if self.calculation_range == CalculationRange::Selection {
                selected_range_for_checksum(editor)
            } else {
                None
            };
            let doc = editor.document.read().expect("document read lock");
            let buffer = &doc.buffer;
            let total_len = buffer.len();

            let r = match self.calculation_range {
                CalculationRange::Selection => selected_range.unwrap_or(editor.cursor.offset..editor.cursor.offset),
                CalculationRange::EntireFile => 0..total_len,
            };

            let data_len = r.len();
            if data_len == 0 {
                (r, Vec::new())
            } else {
                (r.clone(), buffer.get_range(r.start, data_len).to_vec())
            }
        };

        let data_len = range.len();
        if data_len == 0 {
            self.results = None;
            self.is_calculating = false;
            cx.notify();
            return;
        }

        // If auto-calculating and data is > 1MB, skip automatic calculation to prevent lag
        if self.auto_calculate && data_len > 1_000_000 {
            self.results = None;
            self.is_calculating = false;
            cx.notify();
            return;
        }

        self.is_calculating = true;
        self.results = None;
        cx.notify();

        self.calculation_task = None;

        let start_offset = range.start;
        let end_offset = range.end;

        let task = cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move { ChecksumResults::compute(&data, start_offset, end_offset) })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.results = Some(results);
                    this.is_calculating = false;
                    cx.notify();
                });
            }
        });

        self.calculation_task = Some(task);
    }

    fn format_all_results(&self) -> Option<String> {
        self.results.as_ref().map(|res| res.format_all())
    }

    fn copy_value(&mut self, action: &CopyValue, _window: &mut Window, cx: &mut Context<Self>) {
        if !action.value.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        }
    }

    fn copy_row(&mut self, action: &CopyRow, _window: &mut Window, cx: &mut Context<Self>) {
        if !action.text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(action.text.clone()));
        }
    }

    fn copy_all_checksums(&mut self, action: &CopyAllChecksums, _window: &mut Window, cx: &mut Context<Self>) {
        if !action.text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(action.text.clone()));
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_row(
        label: &'static str,
        display_value: String,
        copy_value: String,
        all_text: Option<String>,
        font_family: &str,
        view: &Entity<Self>,
        window: &mut Window,
        theme: &gpui_kit::component::Theme,
    ) -> impl IntoElement {
        let copy_val_for_click = copy_value.clone();
        let val_for_right_click = copy_value.clone();
        let all_copy = all_text.clone();

        h_flex()
            .id(label)
            .w_full()
            .justify_between()
            .items_center()
            .py_1()
            .px_3()
            .rounded_sm()
            .cursor_pointer()
            .hover(|style| style.bg(theme.muted.opacity(0.4)))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy_val_for_click.clone()));
            })
            .on_mouse_down(
                MouseButton::Right,
                window.listener_for(view, move |this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                    this.selected_row = Some((label, val_for_right_click.clone(), all_copy.clone()));
                    cx.notify();
                }),
            )
            .child(div().flex_shrink_0().w(px(110.0)).text_xs().text_color(theme.muted_foreground).child(label))
            .child(
                h_flex()
                    .flex_1()
                    .justify_end()
                    .items_center()
                    .gap_1()
                    .overflow_hidden()
                    .min_w_0()
                    .child(
                        div()
                            .flex_1()
                            .text_right()
                            .font_family(font_family.to_string())
                            .text_xs()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .text_color(theme.foreground)
                            .child(display_value),
                    )
                    .child(
                        Button::new(label)
                            .ghost()
                            .icon(IconName::Copy)
                            .with_size(Size::XSmall)
                            .tooltip("Copy Value")
                            .on_click({
                                let copy_v = copy_value.clone();
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(copy_v.clone()));
                                }
                            }),
                    ),
            )
    }
}

impl Render for ChecksumPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_focused = self.focus_handle.is_focused(window);

        let all_formatted = self.format_all_results();
        let header_actions = if let Some(ref all_str) = all_formatted {
            let all_copy = all_str.clone();
            Some(
                Button::new("copy_all_checksums_header")
                    .ghost()
                    .icon(IconName::Copy)
                    .with_size(Size::XSmall)
                    .tooltip("Copy All Checksums")
                    .on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(all_copy.clone()));
                    })
                    .into_any_element(),
            )
        } else {
            None
        };

        // Header
        let header = crate::ui::style::panel_header("CHECKSUM & SUM", is_focused, theme, None, header_actions);

        // Context info
        let mut info_text = "No Active File".to_string();
        let mut range_desc = String::new();
        let mut data_len = 0;

        if let Some(editor_entity) = &self.editor {
            let editor = editor_entity.read(cx);
            let total_len = editor.total_size();
            info_text = format!("File Size: {} bytes", total_len);

            let range = match self.calculation_range {
                CalculationRange::Selection => selected_range_for_checksum(editor).unwrap_or(editor.cursor.offset..editor.cursor.offset),
                CalculationRange::EntireFile => 0..total_len,
            };
            data_len = range.len();
            if data_len > 0 {
                let end_inclusive = range.end.saturating_sub(1);
                let start_addr = editor.offset_to_address(range.start);
                let end_addr = editor.offset_to_address(end_inclusive);
                range_desc = format!("Range: 0x{:08X} - 0x{:08X} ({} bytes)", start_addr, end_addr, data_len);
            } else {
                range_desc = "No selection (0 bytes)".to_string();
            }
        }

        let font_family = cx.global::<Appearance>().font_family.clone();

        let info_section = v_flex()
            .p_2()
            .gap_1()
            .border_b_1()
            .border_color(theme.border)
            .child(div().text_xs().text_color(theme.muted_foreground).child(info_text))
            .child(div().text_xs().font_family(font_family.clone()).text_color(theme.foreground).child(range_desc));

        // Range selection
        let range_selector = h_flex()
            .p_2()
            .gap_2()
            .items_center()
            .child(
                Button::new("range_selection")
                    .label("Selection")
                    .ghost()
                    .selected(self.calculation_range == CalculationRange::Selection)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.calculation_range != CalculationRange::Selection {
                            this.calculation_range = CalculationRange::Selection;
                            this.results = None;
                            if this.auto_calculate {
                                this.trigger_calculation(cx);
                            } else {
                                cx.notify();
                            }
                        }
                    })),
            )
            .child(
                Button::new("range_entire_file")
                    .label("Entire File")
                    .ghost()
                    .selected(self.calculation_range == CalculationRange::EntireFile)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if this.calculation_range != CalculationRange::EntireFile {
                            this.calculation_range = CalculationRange::EntireFile;
                            this.results = None;
                            if this.auto_calculate {
                                this.trigger_calculation(cx);
                            } else {
                                cx.notify();
                            }
                        }
                    })),
            );

        // Buttons for calculation control
        let control_section = h_flex()
            .p_2()
            .gap_2()
            .items_center()
            .justify_between()
            .child(
                Button::new("auto_calc_toggle")
                    .label("Auto")
                    .ghost()
                    .selected(self.auto_calculate)
                    .tooltip("Automatically calculate when selection changes (for ranges < 1MB)")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.auto_calculate = !this.auto_calculate;
                        if this.auto_calculate && this.results.is_none() {
                            this.trigger_calculation(cx);
                        } else {
                            cx.notify();
                        }
                    })),
            )
            .child(
                Button::new("calc_button")
                    .label("Calculate")
                    .primary()
                    .disabled(data_len == 0 || self.is_calculating)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.trigger_calculation(cx);
                    })),
            );

        // Results or Status Area
        let results_container = if self.is_calculating {
            v_flex()
                .flex_1()
                .items_center()
                .pt_8()
                .p_4()
                .child(div().text_sm().text_color(theme.foreground).child("Calculating sums..."))
                .into_any_element()
        } else if let Some(res) = &self.results {
            let all_opt = all_formatted.clone();
            let view = cx.entity().clone();

            let mut rows = v_flex().flex_1().p_2();
            for &algo in ChecksumAlgorithm::ALL {
                let display_str = res.format_display(algo);
                let copy_str = res.format_hex(algo);
                rows = rows.child(Self::render_row(
                    algo.label(),
                    display_str,
                    copy_str,
                    all_opt.clone(),
                    &font_family,
                    &view,
                    window,
                    theme,
                ));
            }

            rows.overflow_y_scrollbar().into_any_element()
        } else {
            let (title, msg) = if self.editor.is_none() {
                ("No Active File", "Open a binary file to compute checksums")
            } else if data_len == 0 {
                ("Selection Empty", "Select bytes in hex view or switch to Entire File")
            } else if self.auto_calculate && data_len > 1_000_000 {
                ("Large Data Range", "Range exceeds 1MB. Click Calculate to compute.")
            } else {
                ("Ready to Compute", "Click Calculate to compute checksums")
            };

            crate::ui::style::panel_empty_state(IconName::Hash, title, Some(msg), None, theme).into_any_element()
        };

        let view = cx.entity().clone();
        let context_view = view.clone();
        let context_focus_handle = self.focus_handle.clone();
        let container = crate::ui::style::panel_container(is_focused, theme);

        container
            .id("checksum-panel")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::copy_value))
            .on_action(cx.listener(Self::copy_row))
            .on_action(cx.listener(Self::copy_all_checksums))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                }),
            )
            .context_menu(move |menu, _window, cx| {
                let selected = {
                    let this = context_view.read(cx);
                    this.selected_row.clone()
                };
                let Some((label, copy_val, all_opt)) = selected else {
                    return menu;
                };
                let row_copy = format!("{}: {}", label, copy_val);
                let mut menu = menu
                    .action_context(context_focus_handle.clone())
                    .menu_with_icon(format!("Copy Value ({})", copy_val), IconName::Copy, Box::new(CopyValue { value: copy_val }))
                    .menu_with_icon(format!("Copy Row ({})", row_copy), IconName::Copy, Box::new(CopyRow { text: row_copy }));
                if let Some(all) = all_opt {
                    menu = menu
                        .separator()
                        .menu_with_icon("Copy All Checksums", IconName::Copy, Box::new(CopyAllChecksums { text: all }));
                }
                menu
            })
            .child(header)
            .child(info_section)
            .child(range_selector)
            .child(control_section)
            .child(div().h_px().bg(theme.border))
            .child(results_container)
    }
}

impl Focusable for ChecksumPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
