use crate::core::encoding::Encoding;
use crate::core::format::HexCase;
use crate::core::search::{SearchMode, TextSearchFlags};
use crate::ui::components::EventInterceptionExt;
use crate::ui::icon::IconName;
use crate::ui::panels::search_panel::SearchModeExt;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable, Size,
    button::{Button, ButtonVariants},
    input::{self, Input, InputState},
};
use gpui_kit::prelude::*;
use gpui_kit::{App, Context, Entity, EventEmitter, Focusable, IntoElement, KeyBinding, Render, Task, Window, actions, div, px};

use super::history::InputHistory;

const MAX_HISTORY_ENTRIES: usize = 30;
pub(super) const MAX_PREFILL_SELECTION_BYTES: usize = 64;

actions!(
    search_bar,
    [SearchBarEnter, SearchBarShiftEnter, SearchDismiss, SearchHistoryPrev, SearchHistoryNext]
);

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SearchMatchStatus {
    #[default]
    None,
    Scanning,
    Count {
        current: Option<usize>,
        total: usize,
        truncated: bool,
    },
    Error(String),
}

pub enum SearchBarEvent {
    IncrementalSearch {
        query: String,
        mode: SearchMode,
        flags: TextSearchFlags,
    },
    Next {
        query: String,
        mode: SearchMode,
        flags: TextSearchFlags,
    },
    Prev {
        query: String,
        mode: SearchMode,
        flags: TextSearchFlags,
    },
    Replace {
        query: String,
        mode: SearchMode,
        flags: TextSearchFlags,
        replacement: String,
    },
    ReplaceAll {
        query: String,
        mode: SearchMode,
        flags: TextSearchFlags,
        replacement: String,
    },
    Dismiss,
}

/// Formats selected document bytes for pre-filling the Find input (capped at `MAX_PREFILL_SELECTION_BYTES`).
pub(super) fn format_selection_for_find(bytes: &[u8], mode: SearchMode, encoding: Encoding, hex_case: HexCase) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_PREFILL_SELECTION_BYTES {
        return None;
    }
    match mode {
        SearchMode::Hex => {
            let digits = hex_case.digits();
            let mut out = String::with_capacity(bytes.len() * 3);
            for (i, &b) in bytes.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push(digits[(b >> 4) as usize] as char);
                out.push(digits[(b & 0x0F) as usize] as char);
            }
            Some(out)
        }
        SearchMode::Text => {
            let mut decoded = String::new();
            let mut pos = 0;
            while pos < bytes.len() {
                let (ch, len) = encoding.decode_char_at(bytes, pos)?;
                if len == 0 || ch.is_control() || ch == '\u{FFFD}' {
                    return None;
                }
                decoded.push(ch);
                pos += len;
            }
            if decoded.is_empty() { None } else { Some(decoded) }
        }
    }
}

/// Formats a compact match counter label for the Find bar.
pub(super) fn format_match_status_label(status: &SearchMatchStatus) -> Option<String> {
    match status {
        SearchMatchStatus::None | SearchMatchStatus::Error(_) => None,
        SearchMatchStatus::Scanning => Some("...".to_string()),
        SearchMatchStatus::Count { total: 0, .. } => Some("No results".to_string()),
        SearchMatchStatus::Count {
            current: Some(idx),
            total,
            truncated: false,
        } => Some(format!("{}/{}", idx + 1, total)),
        SearchMatchStatus::Count {
            current: Some(idx),
            total,
            truncated: true,
        } => Some(format!("{}/{}+", idx + 1, total)),
        SearchMatchStatus::Count {
            current: None,
            total,
            truncated: false,
        } => Some(format!("{total} found")),
        SearchMatchStatus::Count {
            current: None,
            total,
            truncated: true,
        } => Some(format!("{total}+")),
    }
}

fn replace_placeholder(mode: SearchMode) -> &'static str {
    match mode {
        SearchMode::Hex => "Replace with hex (e.g. 90 90)...",
        SearchMode::Text => "Replace with text...",
    }
}

pub struct SearchBar {
    input: Entity<InputState>,
    replace_input: Entity<InputState>,
    mode: SearchMode,
    flags: TextSearchFlags,
    replace_visible: bool,
    is_read_only: bool,
    match_status: SearchMatchStatus,
    wrapped_notice: Option<&'static str>,
    replace_error: Option<String>,
    find_history: InputHistory,
    replace_history: InputHistory,
    suppress_find_history_reset: bool,
    suppress_replace_history_reset: bool,
    debounce_task: Option<Task<()>>,
}

impl EventEmitter<SearchBarEvent> for SearchBar {}

impl SearchBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(SearchMode::Hex.placeholder()));
        let replace_input = cx.new(|cx| InputState::new(window, cx).placeholder(replace_placeholder(SearchMode::Hex)));

        // Subscribe to find input changes with debouncing
        cx.subscribe(&input, |this, input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                if !this.suppress_find_history_reset {
                    this.find_history.reset_navigation();
                }
                this.wrapped_notice = None;
                this.replace_error = None;
                let query = input.read(cx).value().to_string();
                let mode = this.mode;
                let flags = this.flags;

                // Cancel previous debounce task
                this.debounce_task = None;

                // Empty query updates immediately without waiting for debounce
                if query.is_empty() {
                    this.match_status = SearchMatchStatus::None;
                    cx.emit(SearchBarEvent::IncrementalSearch { query, mode, flags });
                    cx.notify();
                    return;
                }

                // Start new debounce task (200ms)
                let task = cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(200)).await;
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |_this, cx| {
                            cx.emit(SearchBarEvent::IncrementalSearch { query, mode, flags });
                            cx.notify();
                        });
                    }
                });
                this.debounce_task = Some(task);
                cx.notify();
            }
        })
        .detach();

        cx.subscribe(&replace_input, |this, _input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                if !this.suppress_replace_history_reset {
                    this.replace_history.reset_navigation();
                }
                this.replace_error = None;
                cx.notify();
            }
        })
        .detach();

        Self {
            input,
            replace_input,
            mode: SearchMode::Hex,
            flags: TextSearchFlags::default(),
            replace_visible: false,
            is_read_only: false,
            match_status: SearchMatchStatus::None,
            wrapped_notice: None,
            replace_error: None,
            find_history: InputHistory::new(MAX_HISTORY_ENTRIES),
            replace_history: InputHistory::new(MAX_HISTORY_ENTRIES),
            suppress_find_history_reset: false,
            suppress_replace_history_reset: false,
            debounce_task: None,
        }
    }

    pub fn focus_and_select_all(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    pub fn focus_replace_and_select_all(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    pub fn set_query_and_select_all(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.debounce_task = None;
        self.wrapped_notice = None;
        self.replace_error = None;
        self.suppress_find_history_reset = true;
        self.input.update(cx, |input, cx| {
            input.set_value(query.to_string(), window, cx);
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        self.suppress_find_history_reset = false;
        self.find_history.reset_navigation();
        // Cancel the debounced timer started by set_value's Change event and emit immediately.
        self.debounce_task = None;
        cx.emit(SearchBarEvent::IncrementalSearch {
            query: query.to_string(),
            mode: self.mode,
            flags: self.flags,
        });
        cx.notify();
    }

    pub fn is_find_focused(&self, window: &Window, cx: &App) -> bool {
        self.input.read(cx).focus_handle(cx).is_focused(window)
    }

    pub fn is_replace_focused(&self, window: &Window, cx: &App) -> bool {
        self.replace_input.read(cx).focus_handle(cx).is_focused(window)
    }

    pub fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    pub fn mode(&self) -> SearchMode {
        self.mode
    }

    pub fn flags(&self) -> TextSearchFlags {
        self.flags
    }

    pub fn replace_visible(&self) -> bool {
        self.replace_visible
    }

    pub fn set_replace_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.replace_visible != visible {
            self.replace_visible = visible;
            if !visible {
                self.replace_error = None;
            }
            cx.notify();
        }
    }

    pub fn set_read_only(&mut self, is_read_only: bool, cx: &mut Context<Self>) {
        if self.is_read_only != is_read_only {
            self.is_read_only = is_read_only;
            cx.notify();
        }
    }

    pub fn set_match_status(&mut self, status: SearchMatchStatus, cx: &mut Context<Self>) {
        if self.match_status != status {
            self.match_status = status;
            cx.notify();
        }
    }

    pub fn set_wrapped_notice(&mut self, notice: Option<&'static str>, cx: &mut Context<Self>) {
        if self.wrapped_notice != notice {
            self.wrapped_notice = notice;
            cx.notify();
        }
    }

    pub fn set_replace_error(&mut self, error: Option<String>, cx: &mut Context<Self>) {
        if self.replace_error != error {
            self.replace_error = error;
            cx.notify();
        }
    }

    fn emit_next(&mut self, cx: &mut Context<Self>) {
        self.debounce_task = None;
        let query = self.input.read(cx).value().to_string();
        self.find_history.push(&query);
        cx.emit(SearchBarEvent::Next {
            query,
            mode: self.mode,
            flags: self.flags,
        });
    }

    fn emit_prev(&mut self, cx: &mut Context<Self>) {
        self.debounce_task = None;
        let query = self.input.read(cx).value().to_string();
        self.find_history.push(&query);
        cx.emit(SearchBarEvent::Prev {
            query,
            mode: self.mode,
            flags: self.flags,
        });
    }

    fn emit_replace(&mut self, cx: &mut Context<Self>) {
        if self.is_read_only {
            return;
        }
        self.debounce_task = None;
        let query = self.input.read(cx).value().to_string();
        let replacement = self.replace_input.read(cx).value().to_string();
        self.find_history.push(&query);
        self.replace_history.push(&replacement);
        cx.emit(SearchBarEvent::Replace {
            query,
            mode: self.mode,
            flags: self.flags,
            replacement,
        });
    }

    fn emit_replace_all(&mut self, cx: &mut Context<Self>) {
        if self.is_read_only {
            return;
        }
        self.debounce_task = None;
        let query = self.input.read(cx).value().to_string();
        let replacement = self.replace_input.read(cx).value().to_string();
        self.find_history.push(&query);
        self.replace_history.push(&replacement);
        cx.emit(SearchBarEvent::ReplaceAll {
            query,
            mode: self.mode,
            flags: self.flags,
            replacement,
        });
    }

    fn on_mode_change(&mut self, mode: SearchMode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = mode;
        self.debounce_task = None;
        self.wrapped_notice = None;
        self.replace_error = None;
        self.input.update(cx, |input, cx| {
            input.set_placeholder(mode.placeholder(), window, cx);
        });
        self.replace_input.update(cx, |input, cx| {
            input.set_placeholder(replace_placeholder(mode), window, cx);
        });
        let query = self.input.read(cx).value().to_string();
        cx.emit(SearchBarEvent::IncrementalSearch {
            query,
            mode: self.mode,
            flags: self.flags,
        });
        cx.notify();
    }

    fn toggle_case_insensitive(&mut self, cx: &mut Context<Self>) {
        self.flags.case_insensitive = !self.flags.case_insensitive;
        self.debounce_task = None;
        self.wrapped_notice = None;
        let query = self.input.read(cx).value().to_string();
        cx.emit(SearchBarEvent::IncrementalSearch {
            query,
            mode: self.mode,
            flags: self.flags,
        });
        cx.notify();
    }

    fn toggle_regex(&mut self, cx: &mut Context<Self>) {
        self.flags.regex = !self.flags.regex;
        self.debounce_task = None;
        self.wrapped_notice = None;
        let query = self.input.read(cx).value().to_string();
        cx.emit(SearchBarEvent::IncrementalSearch {
            query,
            mode: self.mode,
            flags: self.flags,
        });
        cx.notify();
    }

    fn navigate_history(&mut self, older: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_find_focused(window, cx) {
            let current = self.input.read(cx).value().to_string();
            let entry = if older { self.find_history.prev(&current) } else { self.find_history.next() }.map(str::to_string);
            if let Some(value) = entry {
                self.suppress_find_history_reset = true;
                self.input.update(cx, |input, cx| {
                    input.set_value(value.clone(), window, cx);
                });
                self.suppress_find_history_reset = false;
                self.debounce_task = None;
                cx.emit(SearchBarEvent::IncrementalSearch {
                    query: value,
                    mode: self.mode,
                    flags: self.flags,
                });
                cx.notify();
            }
        } else if self.is_replace_focused(window, cx) {
            let current = self.replace_input.read(cx).value().to_string();
            let entry = if older {
                self.replace_history.prev(&current)
            } else {
                self.replace_history.next()
            }
            .map(str::to_string);
            if let Some(value) = entry {
                self.suppress_replace_history_reset = true;
                self.replace_input.update(cx, |input, cx| {
                    input.set_value(value, window, cx);
                });
                self.suppress_replace_history_reset = false;
                cx.notify();
            }
        }
    }
}

const GUTTER_WIDTH: f32 = 24.0;
const ACTION_COL_WIDTH: f32 = 128.0;
const REPLACE_ALL_WIDTH: f32 = 44.0;
const COL_GAP: f32 = 6.0;

impl Render for SearchBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_query_empty = self.input.read(cx).value().is_empty();
        let match_label = format_match_status_label(&self.match_status);
        let is_zero_matches = matches!(self.match_status, SearchMatchStatus::Count { total: 0, .. });
        let query_error = match &self.match_status {
            SearchMatchStatus::Error(msg) => Some(msg.clone()),
            _ => None,
        };
        let replace_disabled = self.is_read_only || is_query_empty || query_error.is_some();
        let replace_toggle_tooltip = if cfg!(target_os = "macos") {
            "Toggle Replace (Option+Cmd+F)"
        } else {
            "Toggle Replace (Ctrl+H)"
        };

        let header_strip = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .px_2()
            .py_1()
            .bg(theme.secondary.opacity(0.35))
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .p(px(2.0))
                            .rounded_md()
                            .bg(theme.background)
                            .border_1()
                            .border_color(theme.border)
                            .child(
                                if self.mode == SearchMode::Hex {
                                    Button::new("hex_mode").label("Hex").primary().with_size(Size::XSmall)
                                } else {
                                    Button::new("hex_mode").label("Hex").ghost().with_size(Size::XSmall)
                                }
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.on_mode_change(SearchMode::Hex, window, cx);
                                })),
                            )
                            .child(
                                if self.mode == SearchMode::Text {
                                    Button::new("text_mode").label("Text").primary().with_size(Size::XSmall)
                                } else {
                                    Button::new("text_mode").label("Text").ghost().with_size(Size::XSmall)
                                }
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.on_mode_change(SearchMode::Text, window, cx);
                                })),
                            ),
                    )
                    .when(self.mode == SearchMode::Text, |el| {
                        let match_case = !self.flags.case_insensitive;
                        let regex_enabled = self.flags.regex;
                        el.child(div().w(px(1.0)).h(px(14.0)).bg(theme.border))
                            .child(
                                if match_case {
                                    Button::new("match_case").label("Aa").primary().with_size(Size::XSmall)
                                } else {
                                    Button::new("match_case").label("Aa").ghost().with_size(Size::XSmall)
                                }
                                .tooltip(if match_case {
                                    "Match Case: On (click for case-insensitive)"
                                } else {
                                    "Match Case: Off (click for case-sensitive)"
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_case_insensitive(cx);
                                })),
                            )
                            .child(
                                if regex_enabled {
                                    Button::new("regex_mode").label(".*").primary().with_size(Size::XSmall)
                                } else {
                                    Button::new("regex_mode").label(".*").ghost().with_size(Size::XSmall)
                                }
                                .tooltip("Use Regular Expression (bytes)")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_regex(cx);
                                })),
                            )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .when_some(self.wrapped_notice, |el, notice| {
                        el.child(
                            div()
                                .px_1p5()
                                .py(px(1.0))
                                .rounded_sm()
                                .bg(theme.secondary)
                                .text_xs()
                                .whitespace_nowrap()
                                .text_color(theme.muted_foreground)
                                .child(notice),
                        )
                    })
                    .when_some(match_label, |el, label| {
                        el.child(
                            div()
                                .px_1p5()
                                .py(px(1.0))
                                .rounded_sm()
                                .when(is_zero_matches, |d| d.bg(theme.danger.opacity(0.12)))
                                .text_xs()
                                .whitespace_nowrap()
                                .text_color(if is_zero_matches { theme.danger } else { theme.muted_foreground })
                                .child(label),
                        )
                    })
                    .child(
                        Button::new("close")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .tooltip("Close search (Esc)")
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(SearchBarEvent::Dismiss);
                            })),
                    ),
            );

        let find_row = div()
            .flex()
            .items_center()
            .gap(px(COL_GAP))
            .child(
                div().w(px(GUTTER_WIDTH)).flex_shrink_0().flex().items_center().justify_center().child(
                    Button::new("toggle_replace")
                        .ghost()
                        .xsmall()
                        .icon(if self.replace_visible {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .tooltip(replace_toggle_tooltip)
                        .on_click(cx.listener(|this, _, window, cx| {
                            let next = !this.replace_visible;
                            this.set_replace_visible(next, cx);
                            if next {
                                this.focus_replace_and_select_all(window, cx);
                            } else {
                                this.focus_and_select_all(window, cx);
                            }
                        })),
                ),
            )
            .child(
                div().flex_1().min_w_0().child(
                    Input::new(&self.input)
                        .small()
                        .prefix(Icon::new(IconName::Search).size_3p5().text_color(theme.muted_foreground))
                        .cleanable(true),
                ),
            )
            .child(
                div()
                    .w(px(ACTION_COL_WIDTH))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(
                        div().flex_1().child(
                            Button::new("prev")
                                .w_full()
                                .small()
                                .icon(IconName::ChevronUp)
                                .tooltip("Previous match (Shift+Enter)")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.emit_prev(cx);
                                })),
                        ),
                    )
                    .child(
                        div().flex_1().child(
                            Button::new("next")
                                .w_full()
                                .small()
                                .icon(IconName::ChevronDown)
                                .tooltip("Next match (Enter)")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.emit_next(cx);
                                })),
                        ),
                    ),
            );

        let replace_row = self.replace_visible.then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(COL_GAP))
                .child(div().w(px(GUTTER_WIDTH)).flex_shrink_0())
                .child(
                    div().flex_1().min_w_0().child(
                        Input::new(&self.replace_input)
                            .small()
                            .prefix(Icon::new(IconName::Replace).size_3p5().text_color(theme.muted_foreground))
                            .cleanable(true),
                    ),
                )
                .child(
                    div()
                        .w(px(ACTION_COL_WIDTH))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(
                            div().flex_1().child(
                                Button::new("replace_one")
                                    .w_full()
                                    .label("Replace")
                                    .small()
                                    .disabled(replace_disabled)
                                    .tooltip(if self.is_read_only {
                                        "File is read-only"
                                    } else {
                                        "Replace current match (Enter in replace field)"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.emit_replace(cx);
                                    })),
                            ),
                        )
                        .child(
                            div().w(px(REPLACE_ALL_WIDTH)).flex_shrink_0().child(
                                Button::new("replace_all")
                                    .w_full()
                                    .label("All")
                                    .small()
                                    .disabled(replace_disabled)
                                    .tooltip(if self.is_read_only {
                                        "File is read-only"
                                    } else {
                                        "Replace all matches (single Undo step)"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.emit_replace_all(cx);
                                    })),
                            ),
                        ),
                )
        });

        let feedback_row = if let Some(err) = query_error.or_else(|| self.replace_error.clone()) {
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .pl(px(GUTTER_WIDTH + COL_GAP))
                    .text_xs()
                    .text_color(theme.danger)
                    .child(Icon::new(IconName::CircleAlert).size_3().text_color(theme.danger))
                    .child(err),
            )
        } else if self.replace_visible && self.is_read_only {
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .pl(px(GUTTER_WIDTH + COL_GAP))
                    .text_xs()
                    .text_color(theme.warning)
                    .child(Icon::new(IconName::TriangleAlert).size_3().text_color(theme.warning))
                    .child("Read-only mode is enabled — unlock read-only to replace bytes"),
            )
        } else {
            None
        };

        div()
            .flex()
            .flex_col()
            .w(px(500.0))
            .max_w_full()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .rounded_lg()
            .shadow_xl()
            .overflow_hidden()
            .key_context("SearchBar")
            .occlude_and_block_input()
            .on_action(cx.listener(|this, _: &SearchBarEnter, window, cx| {
                if this.is_replace_focused(window, cx) {
                    this.emit_replace(cx);
                } else {
                    this.emit_next(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &SearchBarShiftEnter, _, cx| {
                this.emit_prev(cx);
            }))
            .on_action(cx.listener(|this, _: &crate::actions::SearchNext, _, cx| {
                this.emit_next(cx);
            }))
            .on_action(cx.listener(|this, _: &crate::actions::SearchPrev, _, cx| {
                this.emit_prev(cx);
            }))
            .on_action(cx.listener(|_, _: &SearchDismiss, _, cx| {
                cx.emit(SearchBarEvent::Dismiss);
            }))
            .on_action(cx.listener(|this, _: &SearchHistoryPrev, window, cx| {
                this.navigate_history(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &SearchHistoryNext, window, cx| {
                this.navigate_history(false, window, cx);
            }))
            .child(header_strip)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .p_2()
                    .child(find_row)
                    .children(replace_row)
                    .children(feedback_row),
            )
    }
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", SearchBarEnter, Some("SearchBar")),
        KeyBinding::new("shift-enter", SearchBarShiftEnter, Some("SearchBar")),
        KeyBinding::new("escape", SearchDismiss, Some("SearchBar")),
        KeyBinding::new("up", SearchHistoryPrev, Some("SearchBar > Input")),
        KeyBinding::new("down", SearchHistoryNext, Some("SearchBar > Input")),
    ]);
}
