use crate::core::checksum::{ChecksumAlgorithm, ChecksumCategory, ChecksumCompareStatus, ChecksumResults};
use crate::core::editor::Editor;
use crate::ui::appearance::Appearance;
use crate::ui::icon::IconName;
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::component::{ActiveTheme as _, button::Button, button::ButtonVariants, h_flex, v_flex};
use gpui_kit::component::{Disableable, Icon, Selectable, Sizable, Size, StyledExt as _, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::collections::HashMap;
use std::ops::Range;
use std::time::Duration;

/// Maximum byte length automatically computed without manual confirmation.
pub const AUTO_CALC_MAX_BYTES: usize = 1_000_000;

/// Maximum byte length computed synchronously on the foreground thread to avoid async 1-frame flicker.
pub const SYNC_CALC_MAX_BYTES: usize = 32 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CalculationRange {
    Selection,
    EntireFile,
}

/// Identifies the exact byte slice and document state for a checksum calculation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CalculationSignature {
    pub editor_id: EntityId,
    pub content_state: usize,
    pub total_len: usize,
    pub range_mode: CalculationRange,
    pub byte_range: Range<usize>,
}

struct CachedChecksumState {
    calculation_range: CalculationRange,
    auto_calculate: bool,
    results: Option<ChecksumResults>,
    computed_signature: Option<CalculationSignature>,
    last_observed_signature: Option<CalculationSignature>,
    selected_algorithm: Option<ChecksumAlgorithm>,
}

/// Events emitted by the checksum panel for workspace navigation and focus coordination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChecksumPanelEvent {
    NavigateTo { offset: usize, len: usize },
    FocusEditor,
}

impl EventEmitter<ChecksumPanelEvent> for ChecksumPanel {}

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

/// Resolves the effective byte range to hash for the given range mode, selection, and total file length.
/// Empty ranges are normalized to `0..0` so cursor movements without a selection do not change the range identity.
pub(crate) fn resolve_target_range(range_mode: CalculationRange, selected_range: Option<Range<usize>>, total_len: usize) -> Range<usize> {
    match range_mode {
        CalculationRange::Selection => {
            let range = selected_range.unwrap_or(0..0);
            let start = range.start.min(total_len);
            let end = range.end.min(total_len).max(start);
            if start == end { 0..0 } else { start..end }
        }
        CalculationRange::EntireFile => 0..total_len,
    }
}

/// Splits a hex string into fixed-size chunks for responsive multi-line wrapping without ellipsis truncation.
pub(crate) fn split_hex_chunks(hex: &str, chunk_size: usize) -> Vec<&str> {
    if hex.is_empty() || chunk_size == 0 {
        return Vec::new();
    }
    let mut chunks = Vec::with_capacity(hex.len().div_ceil(chunk_size));
    let mut start = 0;
    while start < hex.len() {
        let end = (start + chunk_size).min(hex.len());
        chunks.push(&hex[start..end]);
        start = end;
    }
    chunks
}

/// Truncates a long checksum value for compact context menu labels while preserving start and end digits.
pub(crate) fn truncate_menu_preview(value: &str, max_len: usize) -> String {
    if value.len() <= max_len || max_len < 8 {
        return value.to_string();
    }
    let head = (max_len - 3) / 2;
    let tail = max_len - 3 - head;
    format!("{}...{}", &value[..head], &value[value.len() - tail..])
}

fn selected_range_for_checksum(editor: &Editor) -> Option<Range<usize>> {
    if editor.has_selection() { editor.selected_range_or_cursor() } else { None }
}

const CONTEXT: &str = "ChecksumPanel";

actions!(checksum_panel, [Dismiss, FocusPanel, CopySelected, SelectPrevRow, SelectNextRow, ClearResults]);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Dismiss, Some("ChecksumPanel && !Input")),
        KeyBinding::new("escape", FocusPanel, Some("ChecksumPanel && Input")),
        KeyBinding::new("up", SelectPrevRow, Some("ChecksumPanel && !Input")),
        KeyBinding::new("down", SelectNextRow, Some("ChecksumPanel && !Input")),
        KeyBinding::new("enter", CopySelected, Some("ChecksumPanel && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", CopySelected, Some("ChecksumPanel && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", CopySelected, Some("ChecksumPanel && !Input")),
    ]);
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = checksum_panel, no_json)]
struct CopyValue {
    algo: ChecksumAlgorithm,
    value: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = checksum_panel, no_json)]
struct CopyRawHex {
    algo: ChecksumAlgorithm,
    value: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = checksum_panel, no_json)]
struct CopyRow {
    algo: ChecksumAlgorithm,
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
    pub compare_input: Entity<InputState>,
    pub calculation_range: CalculationRange,
    pub auto_calculate: bool,
    pub is_calculating: bool,
    pub results: Option<ChecksumResults>,
    pub selected_algorithm: Option<ChecksumAlgorithm>,
    last_copied_algorithm: Option<ChecksumAlgorithm>,
    is_panel_active: bool,
    calculation_generation: usize,
    computed_signature: Option<CalculationSignature>,
    calculating_signature: Option<CalculationSignature>,
    last_observed_signature: Option<CalculationSignature>,
    cached_states: HashMap<EntityId, CachedChecksumState>,
    _editor_subscription: Option<Subscription>,
    _compare_subscription: Option<Subscription>,
    calculation_task: Option<Task<()>>,
    copy_feedback_task: Option<Task<()>>,
}

impl ChecksumPanel {
    pub fn new(editor: Option<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let compare_input = cx.new(|cx| InputState::new(window, cx).placeholder("Verify checksum / hash..."));

        let compare_subscription = cx.subscribe_in(&compare_input, window, |_this, _, event: &input::InputEvent, _window, cx| {
            if matches!(event, input::InputEvent::Change | input::InputEvent::Focus | input::InputEvent::Blur) {
                cx.notify();
            }
        });

        let mut this = Self {
            editor: None,
            focus_handle,
            compare_input,
            calculation_range: CalculationRange::Selection,
            auto_calculate: true,
            is_calculating: false,
            results: None,
            selected_algorithm: None,
            last_copied_algorithm: None,
            is_panel_active: true,
            calculation_generation: 0,
            computed_signature: None,
            calculating_signature: None,
            last_observed_signature: None,
            cached_states: HashMap::new(),
            _editor_subscription: None,
            _compare_subscription: Some(compare_subscription),
            calculation_task: None,
            copy_feedback_task: None,
        };
        this.set_editor(editor, cx);
        this
    }

    /// Sets whether this panel is the currently active tab in the side panel.
    /// When activated, any pending calculation for the current editor state is synchronized immediately.
    pub fn set_panel_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.is_panel_active == active {
            return;
        }
        self.is_panel_active = active;
        if active {
            self.sync_with_editor(cx);
        }
        cx.notify();
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        let current_editor_id = self.editor.as_ref().map(Entity::entity_id);
        let next_editor_id = editor.as_ref().map(Entity::entity_id);
        if current_editor_id == next_editor_id {
            cx.notify();
            return;
        }

        self.cache_current_state();
        self._editor_subscription = None;
        self.cancel_in_flight_task();
        self.editor = editor.clone();
        self.results = None;
        self.computed_signature = None;
        self.last_observed_signature = None;
        self.selected_algorithm = None;
        self.last_copied_algorithm = None;

        if let Some(editor_id) = next_editor_id {
            self.restore_cached_state(editor_id);
        }

        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |this, _, cx| {
                this.on_editor_changed(cx);
            }));
            if self.is_panel_active {
                self.sync_with_editor(cx);
            }
        }
        cx.notify();
    }

    fn cache_current_state(&mut self) {
        let Some(editor) = &self.editor else {
            return;
        };
        self.cached_states.insert(
            editor.entity_id(),
            CachedChecksumState {
                calculation_range: self.calculation_range,
                auto_calculate: self.auto_calculate,
                results: self.results.take(),
                computed_signature: self.computed_signature.take(),
                last_observed_signature: self.last_observed_signature.take(),
                selected_algorithm: self.selected_algorithm,
            },
        );
    }

    fn restore_cached_state(&mut self, editor_id: EntityId) {
        let Some(cached) = self.cached_states.remove(&editor_id) else {
            return;
        };
        self.calculation_range = cached.calculation_range;
        self.auto_calculate = cached.auto_calculate;
        self.results = cached.results;
        self.computed_signature = cached.computed_signature;
        self.last_observed_signature = cached.last_observed_signature;
        self.selected_algorithm = cached.selected_algorithm;
    }

    pub(crate) fn current_signature(&self, cx: &App) -> Option<CalculationSignature> {
        let editor_entity = self.editor.as_ref()?;
        let editor = editor_entity.read(cx);
        let doc = editor.document.read().expect("document read lock");
        let total_len = doc.buffer.len();
        let selected_range = if self.calculation_range == CalculationRange::Selection {
            selected_range_for_checksum(editor)
        } else {
            None
        };
        let byte_range = resolve_target_range(self.calculation_range, selected_range, total_len);
        Some(CalculationSignature {
            editor_id: editor_entity.entity_id(),
            content_state: doc.history.state_id(),
            total_len,
            range_mode: self.calculation_range,
            byte_range,
        })
    }

    /// Returns true if `self.results` is present, but was computed for a different range or older buffer state.
    pub(crate) fn is_results_outdated(&self, current_sig: Option<&CalculationSignature>) -> bool {
        if self.results.is_none() {
            return false;
        }
        match (&self.computed_signature, current_sig) {
            (Some(computed), Some(current)) => computed != current,
            _ => true,
        }
    }

    fn cancel_in_flight_task(&mut self) {
        self.calculation_generation = self.calculation_generation.wrapping_add(1);
        self.calculation_task = None;
        self.calculating_signature = None;
        self.is_calculating = false;
    }

    fn invalidate_calculation(&mut self, cx: &App) {
        self.cancel_in_flight_task();
        self.results = None;
        self.computed_signature = None;
        self.last_observed_signature = self.current_signature(cx);
    }

    fn dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ChecksumPanelEvent::FocusEditor);
        window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
    }

    fn focus_panel(&mut self, _: &FocusPanel, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn clear_results_action(&mut self, _: &ClearResults, _: &mut Window, cx: &mut Context<Self>) {
        self.invalidate_calculation(cx);
        cx.notify();
    }

    fn select_prev_row(&mut self, _: &SelectPrevRow, _: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_none() {
            return;
        }
        let algos = ChecksumAlgorithm::ALL;
        let next_idx = match self.selected_algorithm.and_then(|a| algos.iter().position(|&x| x == a)) {
            Some(0) | None => algos.len().saturating_sub(1),
            Some(idx) => idx - 1,
        };
        self.selected_algorithm = algos.get(next_idx).copied();
        cx.notify();
    }

    fn select_next_row(&mut self, _: &SelectNextRow, _: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_none() {
            return;
        }
        let algos = ChecksumAlgorithm::ALL;
        let next_idx = match self.selected_algorithm.and_then(|a| algos.iter().position(|&x| x == a)) {
            Some(idx) if idx + 1 < algos.len() => idx + 1,
            _ => 0,
        };
        self.selected_algorithm = algos.get(next_idx).copied();
        cx.notify();
    }

    fn copy_selected(&mut self, _: &CopySelected, window: &mut Window, cx: &mut Context<Self>) {
        let Some(algo) = self.selected_algorithm else {
            return;
        };
        let Some(res) = &self.results else {
            return;
        };
        let value = res.format_hex(algo);
        self.perform_copy_algorithm_value(algo, value, window, cx);
    }

    fn on_editor_changed(&mut self, cx: &mut Context<Self>) {
        if !self.is_panel_active {
            return;
        }
        self.sync_with_editor(cx);
    }

    fn sync_with_editor(&mut self, cx: &mut Context<Self>) {
        let current_sig = self.current_signature(cx);
        if current_sig == self.last_observed_signature {
            // Even if signature hasn't changed, if auto_calculate is enabled and we haven't computed yet
            // (for example, when first activating the panel or turning Auto ON), compute if needed.
            if self.auto_calculate
                && self.computed_signature != current_sig
                && self.calculating_signature != current_sig
                && current_sig
                    .as_ref()
                    .is_some_and(|s| !s.byte_range.is_empty() && s.byte_range.len() <= AUTO_CALC_MAX_BYTES)
            {
                self.start_calculation(false, cx);
                return;
            }
            cx.notify();
            return;
        }

        self.last_observed_signature = current_sig;
        if self.auto_calculate {
            self.start_calculation(false, cx);
        } else {
            // When Auto is OFF, cancel any superseded background task, but keep existing results pinned
            // so the user can inspect or edit other regions while viewing the last computed checksums.
            if self.is_calculating && self.calculating_signature != self.last_observed_signature {
                self.cancel_in_flight_task();
            }
            cx.notify();
        }
    }

    /// Starts checksum calculation.
    /// When `force_manual` is true, the 1 MB automatic calculation limit is bypassed.
    pub fn start_calculation(&mut self, force_manual: bool, cx: &mut Context<Self>) {
        self.cancel_in_flight_task();

        let (Some(sig), Some(editor_entity)) = (self.current_signature(cx), self.editor.clone()) else {
            self.results = None;
            self.computed_signature = None;
            self.last_observed_signature = None;
            cx.notify();
            return;
        };

        self.last_observed_signature = Some(sig.clone());
        let data_len = sig.byte_range.len();
        if data_len == 0 {
            self.results = None;
            self.computed_signature = None;
            cx.notify();
            return;
        }

        // If auto-calculating and data is > 1MB, skip automatic calculation unless already computed for this signature.
        if !force_manual && data_len > AUTO_CALC_MAX_BYTES {
            if self.computed_signature.as_ref() != Some(&sig) {
                self.results = None;
                self.computed_signature = None;
            }
            cx.notify();
            return;
        }

        let buffer = {
            let editor = editor_entity.read(cx);
            let doc = editor.document.read().expect("document read lock");
            doc.buffer.clone()
        };

        // Compute small ranges synchronously to avoid 1-frame async loading flashes and preserve scroll state.
        if data_len <= SYNC_CALC_MAX_BYTES {
            let slice = buffer.get_range(sig.byte_range.start, data_len);
            let results = ChecksumResults::compute(slice, sig.byte_range.start, sig.byte_range.end);
            self.results = Some(results);
            self.computed_signature = Some(sig);
            self.is_calculating = false;
            cx.notify();
            return;
        }

        let generation = self.calculation_generation;
        self.is_calculating = true;
        self.calculating_signature = Some(sig.clone());
        cx.notify();

        let range = sig.byte_range.clone();
        let sig_for_done = sig;

        let task = cx.spawn(async move |this, cx| {
            let results = cx
                .background_executor()
                .spawn(async move {
                    let slice = buffer.get_range(range.start, range.len());
                    ChecksumResults::compute(slice, range.start, range.end)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.calculation_generation != generation {
                        return;
                    }
                    this.calculation_task = None;
                    this.calculating_signature = None;
                    this.is_calculating = false;
                    this.results = Some(results);
                    this.computed_signature = Some(sig_for_done);
                    cx.notify();
                });
            }
        });

        self.calculation_task = Some(task);
    }

    fn format_all_results(&self) -> Option<String> {
        self.results.as_ref().map(|res| res.format_all())
    }

    fn mark_copied_feedback(&mut self, algo: ChecksumAlgorithm, cx: &mut Context<Self>) {
        self.last_copied_algorithm = Some(algo);
        self.copy_feedback_task = None;
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1400)).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.last_copied_algorithm == Some(algo) {
                        this.last_copied_algorithm = None;
                        cx.notify();
                    }
                });
            }
        });
        self.copy_feedback_task = Some(task);
        cx.notify();
    }

    fn perform_copy_algorithm_value(&mut self, algo: ChecksumAlgorithm, value: String, window: &mut Window, cx: &mut Context<Self>) {
        if value.is_empty() {
            return;
        }
        cx.write_to_clipboard(ClipboardItem::new_string(value));
        self.mark_copied_feedback(algo, cx);
        window.push_notification(Notification::info(format!("Copied {} value", algo.label())), cx);
    }

    fn copy_value(&mut self, action: &CopyValue, window: &mut Window, cx: &mut Context<Self>) {
        self.perform_copy_algorithm_value(action.algo, action.value.clone(), window, cx);
    }

    fn copy_raw_hex(&mut self, action: &CopyRawHex, window: &mut Window, cx: &mut Context<Self>) {
        if !action.value.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
            self.mark_copied_feedback(action.algo, cx);
            window.push_notification(Notification::info(format!("Copied {} raw hex", action.algo.label())), cx);
        }
    }

    fn copy_row(&mut self, action: &CopyRow, window: &mut Window, cx: &mut Context<Self>) {
        if !action.text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(action.text.clone()));
            self.mark_copied_feedback(action.algo, cx);
            window.push_notification(Notification::info("Copied checksum row to clipboard"), cx);
        }
    }

    fn copy_all_checksums(&mut self, action: &CopyAllChecksums, window: &mut Window, cx: &mut Context<Self>) {
        if !action.text.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(action.text.clone()));
            window.push_notification(Notification::info("Copied all checksums to clipboard"), cx);
        }
    }

    fn navigate_to_range(&mut self, range: Range<usize>, select_in_editor: bool, cx: &mut Context<Self>) {
        if range.is_empty() {
            return;
        }
        if select_in_editor && let Some(ed) = &self.editor {
            ed.update(cx, |editor, cx| {
                editor.set_selection_range(range.clone());
                cx.notify();
            });
        }
        cx.emit(ChecksumPanelEvent::NavigateTo {
            offset: range.start,
            len: range.len(),
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn render_row(
        &self,
        algo: ChecksumAlgorithm,
        res: &ChecksumResults,
        is_matched: bool,
        font_family: &str,
        view: &Entity<Self>,
        window: &mut Window,
        theme: &gpui_kit::component::Theme,
    ) -> Stateful<Div> {
        let label = algo.label();
        let val_str = res.format_hex(algo);
        let copy_val_for_btn = val_str.clone();
        let copy_val_for_dblclick = val_str.clone();
        let is_selected = self.selected_algorithm == Some(algo);
        let is_copied = self.last_copied_algorithm == Some(algo);

        let mut label_el = h_flex().flex_shrink_0().items_center().gap_1p5().child(
            div()
                .text_xs()
                .font_medium()
                .whitespace_nowrap()
                .text_color(if is_matched {
                    theme.green
                } else if is_selected {
                    theme.foreground
                } else {
                    theme.muted_foreground
                })
                .child(label),
        );

        if is_matched {
            label_el = label_el.child(
                div()
                    .px_1()
                    .rounded_xs()
                    .bg(theme.green.opacity(0.18))
                    .text_color(theme.green)
                    .text_xs()
                    .font_semibold()
                    .child("MATCH"),
            );
        }

        let value_text_el = div()
            .flex_1()
            .text_right()
            .font_family(font_family.to_string())
            .text_xs()
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap()
            .text_color(if is_matched { theme.green } else { theme.foreground })
            .child(val_str);

        let copy_icon = if is_copied { IconName::Check } else { IconName::Copy };
        let copy_button = Button::new(SharedString::from(format!("checksum-copy-{}", label)))
            .ghost()
            .icon(copy_icon)
            .with_size(Size::XSmall)
            .tooltip(if is_copied { "Copied!" } else { "Copy Value" })
            .on_click(window.listener_for(view, move |this, _, window, cx| {
                this.selected_algorithm = Some(algo);
                this.perform_copy_algorithm_value(algo, copy_val_for_btn.clone(), window, cx);
            }));

        let right_container = h_flex()
            .flex_1()
            .justify_end()
            .items_center()
            .gap_1()
            .overflow_hidden()
            .min_w_0()
            .child(value_text_el)
            .child(copy_button);

        let row_bg = if is_matched {
            theme.green.opacity(0.12)
        } else if is_selected {
            theme.selection.opacity(0.45)
        } else {
            gpui::transparent_black()
        };

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
            .bg(row_bg)
            .when(is_matched, |el| el.border_1().border_color(theme.green.opacity(0.5)))
            .cursor_pointer()
            .hover(|style| style.bg(theme.muted.opacity(0.4)))
            .on_click(window.listener_for(view, move |this, event: &ClickEvent, window, cx| {
                this.focus_handle.focus(window, cx);
                this.selected_algorithm = Some(algo);
                if event.click_count() >= 2 {
                    this.perform_copy_algorithm_value(algo, copy_val_for_dblclick.clone(), window, cx);
                } else {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                window.listener_for(view, move |this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                    this.selected_algorithm = Some(algo);
                    cx.notify();
                }),
            );

        row.child(label_el).child(right_container)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_crypto_card(
        &self,
        algo: ChecksumAlgorithm,
        res: &ChecksumResults,
        is_matched: bool,
        font_family: &str,
        view: &Entity<Self>,
        window: &mut Window,
        theme: &gpui_kit::component::Theme,
    ) -> Stateful<Div> {
        let label = algo.label();
        let copy_str = res.format_hex(algo);
        let copy_val_for_btn = copy_str.clone();
        let copy_val_for_dblclick = copy_str.clone();
        let is_selected = self.selected_algorithm == Some(algo);
        let is_copied = self.last_copied_algorithm == Some(algo);

        // Render 16-character hex blocks with flex_wrap so all digits remain visible without ellipsis
        // at narrow sidebar widths (140px-250px) and collapse to fewer lines when widened.
        let chunks = split_hex_chunks(&copy_str, 16);
        let hex_display = h_flex()
            .w_full()
            .min_w_0()
            .flex_wrap()
            .justify_end()
            .text_right()
            .gap_x_1p5()
            .gap_y_0p5()
            .children(chunks.into_iter().map(|chunk| div().whitespace_nowrap().child(chunk.to_string())));

        let mut label_el = h_flex().flex_shrink_0().items_center().gap_1p5().child(
            div()
                .text_xs()
                .font_medium()
                .whitespace_nowrap()
                .text_color(if is_matched {
                    theme.green
                } else if is_selected {
                    theme.foreground
                } else {
                    theme.muted_foreground
                })
                .child(label),
        );

        if is_matched {
            label_el = label_el.child(
                div()
                    .px_1()
                    .rounded_xs()
                    .bg(theme.green.opacity(0.18))
                    .text_color(theme.green)
                    .text_xs()
                    .font_semibold()
                    .child("MATCH"),
            );
        }

        let copy_icon = if is_copied { IconName::Check } else { IconName::Copy };
        let copy_button = Button::new(SharedString::from(format!("checksum-copy-{}", label)))
            .ghost()
            .icon(copy_icon)
            .with_size(Size::XSmall)
            .tooltip(if is_copied { "Copied!" } else { "Copy Value" })
            .on_click(window.listener_for(view, move |this, _, window, cx| {
                this.selected_algorithm = Some(algo);
                this.perform_copy_algorithm_value(algo, copy_val_for_btn.clone(), window, cx);
            }));

        let header_row = h_flex().w_full().justify_between().items_center().child(label_el).child(copy_button);

        let card_bg = if is_matched {
            theme.green.opacity(0.12)
        } else if is_selected {
            theme.selection.opacity(0.45)
        } else {
            gpui::transparent_black()
        };

        let card = v_flex()
            .id(label)
            .w_full()
            .min_w_0()
            .py_1()
            .pl_3()
            .pr_1()
            .gap_1()
            .rounded_sm()
            .bg(card_bg)
            .when(is_matched, |el| el.border_1().border_color(theme.green.opacity(0.5)))
            .cursor_pointer()
            .hover(|style| style.bg(theme.muted.opacity(0.4)))
            .on_click(window.listener_for(view, move |this, event: &ClickEvent, window, cx| {
                this.focus_handle.focus(window, cx);
                this.selected_algorithm = Some(algo);
                if event.click_count() >= 2 {
                    this.perform_copy_algorithm_value(algo, copy_val_for_dblclick.clone(), window, cx);
                } else {
                    cx.notify();
                }
            }))
            .on_mouse_down(
                MouseButton::Right,
                window.listener_for(view, move |this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                    this.selected_algorithm = Some(algo);
                    cx.notify();
                }),
            );

        card.child(header_row)
            .child(
                hex_display
                    .font_family(font_family.to_string())
                    .text_xs()
                    .text_color(if is_matched { theme.green } else { theme.foreground }),
            )
    }
}

impl ChecksumPanel {
    fn render_header(
        &self,
        is_focused: bool,
        is_outdated: bool,
        all_formatted: Option<String>,
        theme: &gpui_kit::component::Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let can_clear = self.results.is_some() || self.is_calculating;
        let mut actions_row = h_flex().items_center().gap_1();

        if let Some(all_str) = all_formatted {
            actions_row = actions_row.child(
                Button::new("copy_all_checksums_header")
                    .ghost()
                    .icon(IconName::Copy)
                    .with_size(Size::XSmall)
                    .tooltip("Copy All Checksums")
                    .on_click(move |_, window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(all_str.clone()));
                        window.push_notification(Notification::info("Copied all checksums to clipboard"), cx);
                    }),
            );
        }

        actions_row = actions_row.child(
            Button::new("clear_checksums_header")
                .ghost()
                .icon(IconName::Eraser)
                .with_size(Size::XSmall)
                .tooltip("Clear results")
                .disabled(!can_clear)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.invalidate_calculation(cx);
                    cx.notify();
                })),
        );

        let header_badge = if self.is_calculating {
            Some(crate::ui::panels::panel_badge("Calculating...", theme).into_any_element())
        } else if is_outdated {
            Some(crate::ui::panels::panel_badge("Pinned", theme).into_any_element())
        } else {
            None
        };

        crate::ui::panels::panel_header("CHECKSUM & HASH", is_focused, theme, header_badge, Some(actions_row.into_any_element()))
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
                                this.invalidate_calculation(cx);
                                if this.auto_calculate {
                                    this.start_calculation(false, cx);
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
                                this.invalidate_calculation(cx);
                                if this.auto_calculate {
                                    this.start_calculation(false, cx);
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
                        "Auto-calculate is ON (computes automatically for ranges up to 1 MB)"
                    } else {
                        "Auto-calculate is OFF (results stay pinned until Calculate is clicked)"
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.auto_calculate = !this.auto_calculate;
                        if this.auto_calculate {
                            this.sync_with_editor(cx);
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
                        this.start_calculation(true, cx);
                    })),
            )
    }

    #[allow(clippy::too_many_arguments)]
    fn render_target_card(
        &self,
        total_len: usize,
        target_range: Range<usize>,
        start_addr: usize,
        end_addr: usize,
        is_outdated: bool,
        font_family: &str,
        theme: &gpui_kit::component::Theme,
        cx: &mut Context<Self>,
    ) -> Div {
        let data_len = target_range.len();
        let primary_card = if self.editor.is_none() {
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
                .into_any_element()
        } else if total_len == 0 {
            h_flex()
                .w_full()
                .px_2p5()
                .py_1p5()
                .rounded_md()
                .bg(theme.muted.opacity(0.2))
                .items_center()
                .gap_2()
                .child(Icon::new(IconName::Binary).size(px(14.0)).text_color(theme.muted_foreground))
                .child(div().text_xs().text_color(theme.muted_foreground).child("Empty file (0 B)"))
                .into_any_element()
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
                            this.invalidate_calculation(cx);
                            if this.auto_calculate {
                                this.start_calculation(false, cx);
                            } else {
                                cx.notify();
                            }
                        })),
                )
                .into_any_element()
        } else {
            let nav_range = target_range.clone();
            h_flex()
                .id("checksum_target_range_card")
                .w_full()
                .px_2p5()
                .py_1p5()
                .rounded_md()
                .bg(theme.muted.opacity(0.2))
                .border_1()
                .border_color(theme.border.opacity(0.4))
                .justify_between()
                .items_center()
                .gap_2()
                .cursor_pointer()
                .hover(|style| style.bg(theme.muted.opacity(0.35)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.navigate_to_range(nav_range.clone(), false, cx);
                }))
                .child(
                    h_flex()
                        .min_w_0()
                        .flex_1()
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
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .px_1p5()
                        .py_0p5()
                        .rounded_xs()
                        .bg(theme.muted.opacity(0.5))
                        .font_family(font_family.to_string())
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format_byte_size(data_len)),
                )
                .into_any_element()
        };

        let mut container = v_flex().w_full().gap_1p5().child(primary_card);

        if is_outdated
            && let Some(res) = &self.results
            && let Some(editor_entity) = &self.editor
        {
            let editor = editor_entity.read(cx);
            let res_start_addr = editor.offset_to_address(res.range_start);
            let res_end_addr = editor.offset_to_address(res.range_end.saturating_sub(1));
            let res_range = res.range_start..res.range_end;
            let res_size = format_byte_size(res.data_len);

            container = container.child(
                h_flex()
                    .id("checksum_pinned_range_banner")
                    .w_full()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .bg(theme.accent.opacity(0.12))
                    .border_1()
                    .border_color(theme.accent.opacity(0.35))
                    .justify_between()
                    .items_center()
                    .gap_1p5()
                    .cursor_pointer()
                    .hover(|s| s.bg(theme.accent.opacity(0.2)))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.navigate_to_range(res_range.clone(), true, cx);
                    }))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .overflow_hidden()
                            .text_ellipsis()
                            .whitespace_nowrap()
                            .font_family(font_family.to_string())
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("Showing 0x{:08X}..0x{:08X} ({})", res_start_addr, res_end_addr, res_size)),
                    )
                    .child(div().flex_shrink_0().text_xs().font_medium().text_color(theme.accent).child("Jump")),
            );
        }

        container
    }

    fn render_compare_bar(&self, compare_status: &ChecksumCompareStatus, theme: &gpui_kit::component::Theme) -> Div {
        let mut container = v_flex()
            .px_2p5()
            .py_2()
            .gap_1p5()
            .border_b_1()
            .border_color(theme.border)
            .child(Input::new(&self.compare_input).prefix(IconName::Search).cleanable(true).with_size(Size::XSmall));

        match compare_status {
            ChecksumCompareStatus::Empty => {}
            ChecksumCompareStatus::InvalidHex => {
                container = container.child(
                    h_flex()
                        .items_center()
                        .gap_1p5()
                        .px_1()
                        .child(Icon::new(IconName::TriangleAlert).size(px(12.0)).text_color(theme.red))
                        .child(div().text_xs().text_color(theme.red).child("Invalid hexadecimal characters in query")),
                );
            }
            ChecksumCompareStatus::NoMatch => {
                container = container.child(
                    h_flex()
                        .items_center()
                        .gap_1p5()
                        .px_1()
                        .child(Icon::new(IconName::Close).size(px(12.0)).text_color(theme.red))
                        .child(div().text_xs().text_color(theme.red).child("No matching checksum or hash")),
                );
            }
            ChecksumCompareStatus::Matched(algos) => {
                let names = algos.iter().map(|a| a.label()).collect::<Vec<_>>().join(", ");
                container = container.child(
                    h_flex()
                        .items_center()
                        .gap_1p5()
                        .px_1()
                        .child(Icon::new(IconName::Check).size(px(12.0)).text_color(theme.green))
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .text_xs()
                                .font_medium()
                                .text_color(theme.green)
                                .child(format!("Matched: {}", names)),
                        ),
                );
            }
        }

        container
    }

    fn render_empty_state(&self, total_len: usize, data_len: usize, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> AnyElement {
        let (icon, title, msg, action) = if self.editor.is_none() {
            (IconName::Hash, "No Active File", Some("Open a binary file to compute checksums"), None)
        } else if total_len == 0 {
            (IconName::Binary, "Empty File", Some("The active file contains 0 bytes"), None)
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
                            this.invalidate_calculation(cx);
                            if this.auto_calculate {
                                this.start_calculation(false, cx);
                            } else {
                                cx.notify();
                            }
                        }))
                        .into_any_element(),
                ),
            )
        } else if self.auto_calculate && data_len > AUTO_CALC_MAX_BYTES {
            (
                IconName::SlidersHorizontal,
                "Large Data Range",
                Some("Data exceeds 1 MB. Click Calculate to compute."),
                Some(
                    Button::new("empty_calc_large")
                        .label(format!("Calculate ({})", format_byte_size(data_len)))
                        .primary()
                        .with_size(Size::Small)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.start_calculation(true, cx);
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
                            this.start_calculation(true, cx);
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
        let is_input_focused = self.compare_input.read(cx).focus_handle(cx).is_focused(window);
        let is_focused = self.focus_handle.is_focused(window) || is_input_focused;

        let mut total_len = 0;
        let mut target_range = 0..0;
        let mut start_addr = 0;
        let mut end_addr = 0;

        if let Some(editor_entity) = &self.editor {
            let editor = editor_entity.read(cx);
            total_len = editor.total_size();
            let selected_range = if self.calculation_range == CalculationRange::Selection {
                selected_range_for_checksum(editor)
            } else {
                None
            };
            target_range = resolve_target_range(self.calculation_range, selected_range, total_len);
            if !target_range.is_empty() {
                let end_inclusive = target_range.end.saturating_sub(1);
                start_addr = editor.offset_to_address(target_range.start);
                end_addr = editor.offset_to_address(end_inclusive);
            }
        }

        let data_len = target_range.len();
        let current_sig = self.current_signature(cx);
        let is_outdated = self.is_results_outdated(current_sig.as_ref());

        let all_formatted = self.format_all_results();
        let header = self.render_header(is_focused, is_outdated, all_formatted, &theme, cx);
        let font_family = cx.global::<Appearance>().font_family.clone();

        let top_toolbar = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .gap_2()
            .child(self.render_segmented_control(&theme, cx))
            .child(self.render_action_controls(data_len, cx));

        let target_card = self.render_target_card(total_len, target_range, start_addr, end_addr, is_outdated, &font_family, &theme, cx);

        let control_card = v_flex()
            .p_2p5()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
            .child(top_toolbar)
            .child(target_card);

        let compare_query = self.compare_input.read(cx).value().to_string();
        let compare_status = self
            .results
            .as_ref()
            .map(|res| res.compare_query(&compare_query))
            .unwrap_or(ChecksumCompareStatus::Empty);

        let compare_bar = if self.results.is_some() {
            Some(self.render_compare_bar(&compare_status, &theme))
        } else {
            None
        };

        let results_container = if let Some(res) = &self.results {
            let view = cx.entity().clone();
            let matched_algos = match &compare_status {
                ChecksumCompareStatus::Matched(list) => list.as_slice(),
                _ => &[],
            };

            let mut rows = v_flex().flex_1().min_w_0().py_2().pl_2().pr_1().gap_0p5();

            for &cat in ChecksumCategory::ALL {
                let cat_algos: Vec<ChecksumAlgorithm> = ChecksumAlgorithm::ALL.iter().copied().filter(|a| a.category() == cat).collect();
                if cat_algos.is_empty() {
                    continue;
                }

                rows = rows.child(crate::ui::panels::panel_section_header(cat.label(), &theme));

                for algo in cat_algos {
                    let is_matched = matched_algos.contains(&algo);
                    if cat == ChecksumCategory::Crypto {
                        rows = rows.child(self.render_crypto_card(algo, res, is_matched, &font_family, &view, window, &theme));
                    } else {
                        rows = rows.child(self.render_row(algo, res, is_matched, &font_family, &view, window, &theme));
                    }
                }
            }

            rows.overflow_y_scrollbar().into_any_element()
        } else if self.is_calculating {
            v_flex()
                .flex_1()
                .items_center()
                .justify_center()
                .gap_2p5()
                .p_6()
                .child(Icon::new(IconName::LoaderCircle).size(px(24.0)).text_color(theme.primary))
                .child(div().text_sm().font_medium().text_color(theme.foreground).child("Computing checksums..."))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("Processing {} of data", format_byte_size(data_len))),
                )
                .child(
                    Button::new("cancel_checksum_calc")
                        .label("Cancel")
                        .ghost()
                        .with_size(Size::XSmall)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.cancel_in_flight_task();
                            cx.notify();
                        })),
                )
                .into_any_element()
        } else {
            self.render_empty_state(total_len, data_len, &theme, cx)
        };

        let view = cx.entity().clone();
        let context_view = view.clone();
        let context_focus_handle = self.focus_handle.clone();
        let container = crate::ui::panels::panel_container(is_focused, &theme);

        container
            .id("checksum-panel")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(Self::focus_panel))
            .on_action(cx.listener(Self::clear_results_action))
            .on_action(cx.listener(Self::select_prev_row))
            .on_action(cx.listener(Self::select_next_row))
            .on_action(cx.listener(Self::copy_selected))
            .on_action(cx.listener(Self::copy_value))
            .on_action(cx.listener(Self::copy_raw_hex))
            .on_action(cx.listener(Self::copy_row))
            .on_action(cx.listener(Self::copy_all_checksums))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !this.compare_input.read(cx).focus_handle(cx).is_focused(window) {
                        this.focus_handle.focus(window, cx);
                    }
                }),
            )
            .context_menu(move |menu, _window, cx| {
                let (selected_algo, results_opt) = {
                    let this = context_view.read(cx);
                    (this.selected_algorithm, this.results.clone())
                };
                let (Some(algo), Some(res)) = (selected_algo, results_opt) else {
                    return menu;
                };
                let label = algo.label();
                let copy_val = res.format_hex(algo);
                let raw_hex = res.format_raw_hex(algo);
                let all_text = res.format_all();
                let val_preview = truncate_menu_preview(&copy_val, 20);
                let row_copy = format!("{}: {}", label, copy_val);

                let mut menu = menu.action_context(context_focus_handle.clone()).menu_with_icon(
                    format!("Copy Value ({})", val_preview),
                    IconName::Copy,
                    Box::new(CopyValue { algo, value: copy_val.clone() }),
                );

                if copy_val.starts_with("0x") {
                    menu = menu.menu_with_icon(
                        format!("Copy Raw Hex ({})", truncate_menu_preview(&raw_hex, 20)),
                        IconName::Copy,
                        Box::new(CopyRawHex { algo, value: raw_hex }),
                    );
                }

                menu.menu_with_icon(
                    format!("Copy Row ({}: {})", label, val_preview),
                    IconName::Copy,
                    Box::new(CopyRow { algo, text: row_copy }),
                )
                .separator()
                .menu_with_icon("Copy All Checksums", IconName::Copy, Box::new(CopyAllChecksums { text: all_text }))
            })
            .child(header)
            .child(control_card)
            .children(compare_bar)
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
    use super::{CalculationRange, format_byte_size, resolve_target_range, split_hex_chunks, truncate_menu_preview};

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

    #[test]
    fn test_resolve_target_range_normalizes_empty_selection() {
        assert_eq!(resolve_target_range(CalculationRange::Selection, None, 100), 0..0);
        assert_eq!(resolve_target_range(CalculationRange::Selection, Some(42..42), 100), 0..0);
        assert_eq!(resolve_target_range(CalculationRange::Selection, Some(10..20), 100), 10..20);
        assert_eq!(resolve_target_range(CalculationRange::Selection, Some(90..150), 100), 90..100);
        assert_eq!(resolve_target_range(CalculationRange::EntireFile, None, 100), 0..100);
        assert_eq!(resolve_target_range(CalculationRange::EntireFile, Some(10..20), 0), 0..0);
    }

    #[test]
    fn test_split_hex_chunks() {
        let sha256_hex = "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592";
        let chunks = split_hex_chunks(sha256_hex, 16);
        assert_eq!(chunks, vec!["d7a8fbb307d78094", "69ca9abcb0082e4f", "8d5651e46d3cdb76", "2d02d0bf37c9e592"]);
    }

    #[test]
    fn test_truncate_menu_preview() {
        assert_eq!(truncate_menu_preview("0x12345678", 20), "0x12345678");
        let sha = "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592";
        let truncated = truncate_menu_preview(sha, 20);
        assert_eq!(truncated, "d7a8fbb3...f37c9e592");
        assert!(truncated.len() <= 20);
    }
}
