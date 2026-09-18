use crate::core::checksum::{ChecksumAlgorithm, ChecksumCategory, ChecksumResults};
use crate::core::editor::Editor;
use crate::ui::appearance::Appearance;
use crate::ui::icon::IconName;
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{ActiveTheme as _, button::Button, button::ButtonVariants, h_flex, v_flex};
use gpui_kit::component::{Disableable, Icon, Selectable, Sizable, Size, StyledExt as _, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::ops::Range;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum CalculationRange {
    Selection,
    EntireFile,
}

pub(crate) fn format_byte_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.2} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
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

    fn copy_value(&mut self, action: &CopyValue, window: &mut Window, cx: &mut Context<Self>) {
        if !action.value.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
            window.push_notification(Notification::info("Copied checksum value to clipboard"), cx);
        }
    }

    fn copy_row(&mut self, action: &CopyRow, window: &mut Window, cx: &mut Context<Self>) {
        if !action.text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(action.text.clone()));
            window.push_notification(Notification::info("Copied checksum row to clipboard"), cx);
        }
    }

    fn copy_all_checksums(&mut self, action: &CopyAllChecksums, window: &mut Window, cx: &mut Context<Self>) {
        if !action.text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(action.text.clone()));
            window.push_notification(Notification::info("Copied all checksums to clipboard"), cx);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_row(
        algo: ChecksumAlgorithm,
        res: &ChecksumResults,
        all_text: Option<String>,
        font_family: &str,
        view: &Entity<Self>,
        window: &mut Window,
        theme: &gpui_kit::component::Theme,
    ) -> Stateful<Div> {
        let label = algo.label();
        let val_str = res.format_hex(algo);
        let copy_val_for_click = val_str.clone();
        let copy_val_for_btn = val_str.clone();
        let val_for_right_click = val_str.clone();
        let all_copy = all_text.clone();

        let label_el = h_flex().flex_shrink_0().items_center().gap_1p5().child(
            div()
                .text_xs()
                .font_medium()
                .whitespace_nowrap()
                .text_color(theme.muted_foreground)
                .child(label),
        );

        let value_text_el = div()
            .flex_1()
            .text_right()
            .font_family(font_family.to_string())
            .text_xs()
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .text_color(theme.foreground)
            .child(val_str);

        let copy_button = Button::new(SharedString::from(format!("checksum-copy-{}", label)))
            .ghost()
            .icon(IconName::Copy)
            .with_size(Size::XSmall)
            .tooltip("Copy Value")
            .on_click(move |_, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy_val_for_btn.clone()));
                window.push_notification(Notification::info(format!("Copied {} value", label)), cx);
            });

        let right_container = h_flex()
            .flex_1()
            .justify_end()
            .items_center()
            .gap_1()
            .overflow_hidden()
            .min_w_0()
            .child(value_text_el)
            .child(copy_button);

        let row = h_flex()
            .id(label)
            .w_full()
            .justify_between()
            .items_center()
            .gap_2()
            .py_1()
            .pl_3()
            .pr_1()
            .rounded_sm()
            .cursor_pointer()
            .hover(|style| style.bg(theme.muted.opacity(0.4)))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy_val_for_click.clone()));
                window.push_notification(Notification::info(format!("Copied {} value", label)), cx);
            })
            .on_mouse_down(
                MouseButton::Right,
                window.listener_for(view, move |this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                    this.selected_row = Some((label, val_for_right_click.clone(), all_copy.clone()));
                    cx.notify();
                }),
            );

        row.child(label_el).child(right_container)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_crypto_card(
        algo: ChecksumAlgorithm,
        res: &ChecksumResults,
        all_text: Option<String>,
        font_family: &str,
        view: &Entity<Self>,
        window: &mut Window,
        theme: &gpui_kit::component::Theme,
    ) -> Stateful<Div> {
        let label = algo.label();
        let copy_str = res.format_hex(algo);
        let copy_val_for_click = copy_str.clone();
        let copy_val_for_btn = copy_str.clone();
        let val_for_right_click = copy_str.clone();
        let all_copy = all_text.clone();

        let hex_display = if algo == ChecksumAlgorithm::Sha256 && copy_str.len() == 64 {
            v_flex()
                .w_full()
                .min_w_0()
                .child(
                    div()
                        .w_full()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(copy_str[0..32].to_string()),
                )
                .child(
                    div()
                        .w_full()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(copy_str[32..64].to_string()),
                )
        } else {
            v_flex()
                .w_full()
                .min_w_0()
                .child(div().w_full().overflow_hidden().text_ellipsis().whitespace_nowrap().child(copy_str.clone()))
        };

        let label_el = h_flex().flex_shrink_0().items_center().gap_1p5().child(
            div()
                .text_xs()
                .font_medium()
                .whitespace_nowrap()
                .text_color(theme.muted_foreground)
                .child(label),
        );

        let copy_button = Button::new(SharedString::from(format!("checksum-copy-{}", label)))
            .ghost()
            .icon(IconName::Copy)
            .with_size(Size::XSmall)
            .tooltip("Copy Value")
            .on_click(move |_, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy_val_for_btn.clone()));
                window.push_notification(Notification::info(format!("Copied {} value", label)), cx);
            });

        let header_row = h_flex().w_full().justify_between().items_center().child(label_el).child(copy_button);

        let card = v_flex()
            .id(label)
            .w_full()
            .min_w_0()
            .py_1()
            .pl_3()
            .pr_1()
            .gap_1()
            .rounded_sm()
            .cursor_pointer()
            .hover(|style| style.bg(theme.muted.opacity(0.4)))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(copy_val_for_click.clone()));
                window.push_notification(Notification::info(format!("Copied {} value", label)), cx);
            })
            .on_mouse_down(
                MouseButton::Right,
                window.listener_for(view, move |this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                    this.selected_row = Some((label, val_for_right_click.clone(), all_copy.clone()));
                    cx.notify();
                }),
            );

        card.child(header_row)
            .child(hex_display.font_family(font_family.to_string()).text_xs().text_color(theme.foreground))
    }
}

impl ChecksumPanel {
    fn render_header(&self, is_focused: bool, all_formatted: Option<String>, theme: &gpui_kit::component::Theme) -> Div {
        let header_actions = all_formatted.map(|all_str| {
            Button::new("copy_all_checksums_header")
                .ghost()
                .icon(IconName::Copy)
                .with_size(Size::XSmall)
                .tooltip("Copy All Checksums")
                .on_click(move |_, window, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(all_str.clone()));
                    window.push_notification(Notification::info("Copied all checksums to clipboard"), cx);
                })
                .into_any_element()
        });

        let header_badge = if self.is_calculating {
            Some(crate::ui::panels::panel_badge("Calculating...", theme).into_any_element())
        } else {
            None
        };

        crate::ui::panels::panel_header("CHECKSUM & HASH", is_focused, theme, header_badge, header_actions)
    }

    fn render_segmented_control(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> Div {
        let is_selection = self.calculation_range == CalculationRange::Selection;
        h_flex()
            .p_0p5()
            .rounded_md()
            .bg(theme.muted.opacity(0.4))
            .items_center()
            .gap_0p5()
            .child(
                div()
                    .id("seg_selection")
                    .px_2p5()
                    .py_1()
                    .rounded_sm()
                    .text_xs()
                    .cursor_pointer()
                    .when(is_selection, |this| {
                        this.bg(theme.background).text_color(theme.foreground).font_semibold().shadow_xs()
                    })
                    .when(!is_selection, |this| {
                        this.text_color(theme.muted_foreground).hover(|s| s.text_color(theme.foreground))
                    })
                    .child("Selection")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            if this.calculation_range != CalculationRange::Selection {
                                this.calculation_range = CalculationRange::Selection;
                                this.results = None;
                                if this.auto_calculate {
                                    this.trigger_calculation(cx);
                                } else {
                                    cx.notify();
                                }
                            }
                        }),
                    ),
            )
            .child(
                div()
                    .id("seg_entire_file")
                    .px_2p5()
                    .py_1()
                    .rounded_sm()
                    .text_xs()
                    .cursor_pointer()
                    .when(!is_selection, |this| {
                        this.bg(theme.background).text_color(theme.foreground).font_semibold().shadow_xs()
                    })
                    .when(is_selection, |this| {
                        this.text_color(theme.muted_foreground).hover(|s| s.text_color(theme.foreground))
                    })
                    .child("Entire File")
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            if this.calculation_range != CalculationRange::EntireFile {
                                this.calculation_range = CalculationRange::EntireFile;
                                this.results = None;
                                if this.auto_calculate {
                                    this.trigger_calculation(cx);
                                } else {
                                    cx.notify();
                                }
                            }
                        }),
                    ),
            )
    }

    fn render_action_controls(&self, data_len: usize, cx: &mut Context<Self>) -> Div {
        h_flex()
            .items_center()
            .gap_1p5()
            .child(
                Button::new("auto_calc_toggle")
                    .label("Auto")
                    .ghost()
                    .selected(self.auto_calculate)
                    .with_size(Size::XSmall)
                    .tooltip(if self.auto_calculate {
                        "Auto-calculate is ON (computes on selection change for < 1MB)"
                    } else {
                        "Auto-calculate is OFF"
                    })
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
                    .icon(IconName::Calculator)
                    .primary()
                    .with_size(Size::XSmall)
                    .disabled(data_len == 0 || self.is_calculating)
                    .tooltip("Compute checksums for target range")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.trigger_calculation(cx);
                    })),
            )
    }

    fn render_target_card(
        &self,
        data_len: usize,
        start_addr: usize,
        end_addr: usize,
        font_family: &str,
        theme: &gpui_kit::component::Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        if self.editor.is_none() {
            h_flex()
                .w_full()
                .px_2p5()
                .py_1p5()
                .rounded_md()
                .bg(theme.muted.opacity(0.2))
                .items_center()
                .gap_2()
                .child(Icon::new(IconName::Binary).size(px(14.0)).text_color(theme.muted_foreground))
                .child(div().text_xs().text_color(theme.muted_foreground).child("No active file"))
        } else if data_len == 0 {
            h_flex()
                .w_full()
                .px_2p5()
                .py_1p5()
                .rounded_md()
                .bg(theme.muted.opacity(0.2))
                .justify_between()
                .items_center()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::Binary).size(px(14.0)).text_color(theme.muted_foreground))
                        .child(div().text_xs().text_color(theme.muted_foreground).child("No bytes selected")),
                )
                .child(
                    Button::new("switch_to_file")
                        .label("Use Entire File")
                        .ghost()
                        .with_size(Size::XSmall)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.calculation_range = CalculationRange::EntireFile;
                            this.results = None;
                            if this.auto_calculate {
                                this.trigger_calculation(cx);
                            } else {
                                cx.notify();
                            }
                        })),
                )
        } else {
            h_flex()
                .w_full()
                .px_2p5()
                .py_1p5()
                .rounded_md()
                .bg(theme.muted.opacity(0.2))
                .border_1()
                .border_color(theme.border.opacity(0.4))
                .items_center()
                .gap_1p5()
                .child(Icon::new(IconName::Binary).size(px(13.0)).text_color(theme.muted_foreground))
                .child(
                    div()
                        .font_family(font_family.to_string())
                        .text_xs()
                        .text_color(theme.foreground)
                        .whitespace_nowrap()
                        .overflow_hidden()
                        .text_ellipsis()
                        .child(format!("0x{:08X} .. 0x{:08X}", start_addr, end_addr)),
                )
        }
    }

    fn render_empty_state(&self, data_len: usize, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> AnyElement {
        let (icon, title, msg, action) = if self.editor.is_none() {
            (IconName::Hash, "No Active File", Some("Open a binary file to compute checksums"), None)
        } else if data_len == 0 {
            (
                IconName::SquareMousePointer,
                "Selection Empty",
                Some("Select bytes in hex view or switch to Entire File"),
                Some(
                    Button::new("empty_switch_file")
                        .label("Switch to Entire File")
                        .primary()
                        .with_size(Size::Small)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.calculation_range = CalculationRange::EntireFile;
                            this.results = None;
                            if this.auto_calculate {
                                this.trigger_calculation(cx);
                            } else {
                                cx.notify();
                            }
                        }))
                        .into_any_element(),
                ),
            )
        } else if self.auto_calculate && data_len > 1_000_000 {
            (
                IconName::SlidersHorizontal,
                "Large Data Range",
                Some("Data exceeds 1MB. Click Calculate to compute."),
                Some(
                    Button::new("empty_calc_large")
                        .label(format!("Calculate ({})", format_byte_size(data_len)))
                        .primary()
                        .with_size(Size::Small)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.trigger_calculation(cx);
                        }))
                        .into_any_element(),
                ),
            )
        } else {
            (
                IconName::Calculator,
                "Ready to Compute",
                Some("Click Calculate to compute checksums"),
                Some(
                    Button::new("empty_calc_ready")
                        .label("Calculate Checksums")
                        .primary()
                        .with_size(Size::Small)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.trigger_calculation(cx);
                        }))
                        .into_any_element(),
                ),
            )
        };

        crate::ui::panels::panel_empty_state(icon, title, msg, action, theme).into_any_element()
    }
}

impl Render for ChecksumPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_focused = self.focus_handle.is_focused(window);

        let mut data_len = 0;
        let mut start_addr = 0;
        let mut end_addr = 0;

        if let Some(editor_entity) = &self.editor {
            let editor = editor_entity.read(cx);
            let total_len = editor.total_size();

            let range = match self.calculation_range {
                CalculationRange::Selection => selected_range_for_checksum(editor).unwrap_or(editor.cursor.offset..editor.cursor.offset),
                CalculationRange::EntireFile => 0..total_len,
            };
            data_len = range.len();
            if data_len > 0 {
                let end_inclusive = range.end.saturating_sub(1);
                start_addr = editor.offset_to_address(range.start);
                end_addr = editor.offset_to_address(end_inclusive);
            }
        }

        let all_formatted = self.format_all_results();
        let header = self.render_header(is_focused, all_formatted.clone(), &theme);
        let font_family = cx.global::<Appearance>().font_family.clone();

        let top_toolbar = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .gap_2()
            .child(self.render_segmented_control(&theme, cx))
            .child(self.render_action_controls(data_len, cx));

        let target_card = self.render_target_card(data_len, start_addr, end_addr, &font_family, &theme, cx);

        let control_card = v_flex()
            .p_2p5()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
            .child(top_toolbar)
            .child(target_card);

        let results_container = if self.is_calculating {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2()
                .p_6()
                .child(Icon::new(IconName::LoaderCircle).size(px(24.0)).text_color(theme.primary))
                .child(div().text_sm().font_medium().text_color(theme.foreground).child("Computing checksums..."))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("Processing {} of data", format_byte_size(data_len))),
                )
                .into_any_element()
        } else if let Some(res) = &self.results {
            let all_opt = all_formatted.clone();
            let view = cx.entity().clone();

            let mut rows = v_flex().flex_1().min_w_0().py_2().pl_2().pr_1().gap_0p5();

            for &cat in ChecksumCategory::ALL {
                let cat_algos: Vec<ChecksumAlgorithm> = ChecksumAlgorithm::ALL.iter().copied().filter(|a| a.category() == cat).collect();
                if cat_algos.is_empty() {
                    continue;
                }

                rows = rows.child(crate::ui::panels::panel_section_header(cat.label(), &theme));

                for algo in cat_algos {
                    if cat == ChecksumCategory::Crypto {
                        rows = rows.child(Self::render_crypto_card(algo, res, all_opt.clone(), &font_family, &view, window, &theme));
                    } else {
                        rows = rows.child(Self::render_row(algo, res, all_opt.clone(), &font_family, &view, window, &theme));
                    }
                }
            }

            rows.overflow_y_scrollbar().into_any_element()
        } else {
            self.render_empty_state(data_len, &theme, cx)
        };

        let view = cx.entity().clone();
        let context_view = view.clone();
        let context_focus_handle = self.focus_handle.clone();
        let container = crate::ui::panels::panel_container(is_focused, &theme);

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
            .child(control_card)
            .child(results_container)
    }
}

impl Focusable for ChecksumPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::format_byte_size;

    #[test]
    fn test_format_byte_size() {
        assert_eq!(format_byte_size(0), "0 B");
        assert_eq!(format_byte_size(512), "512 B");
        assert_eq!(format_byte_size(1024), "1.0 KB");
        assert_eq!(format_byte_size(1536), "1.5 KB");
        assert_eq!(format_byte_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_byte_size(5 * 1024 * 1024), "5.00 MB");
        assert_eq!(format_byte_size(1024 * 1024 * 1024), "1.00 GB");
    }
}
