use std::ops::Range;

use crate::core::address_map::AddressMap;
use crate::core::format::HexCase;
use crate::core::goto::{GotoParseError, GotoRadix, ParsedGotoOffset, parse_goto_offset_with_context};
use crate::ui::components::EventInterceptionExt;
use crate::ui::icon::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, Size,
    button::{Button, ButtonVariants},
    input::{self, Input, InputState},
};
use gpui_kit::prelude::*;
use gpui_kit::{App, Context, Entity, EventEmitter, IntoElement, KeyBinding, Render, Window, actions, div, px};

use super::history::InputHistory;

const MAX_GOTO_HISTORY_ENTRIES: usize = 30;

actions!(goto_offset_bar, [GotoJump, GotoJumpExtend, GotoDismiss, GotoHistoryPrev, GotoHistoryNext]);

pub enum GotoBarEvent {
    Jump { offset: usize, extend_selection: bool },
    SelectRange { range: Range<usize> },
    Dismiss,
}

pub struct GotoOffsetBar {
    input: Entity<InputState>,
    radix: GotoRadix,
    current_cursor: usize,
    total_size: usize,
    bytes_per_row: usize,
    address_map: AddressMap,
    parsed_result: Option<Result<ParsedGotoOffset, GotoParseError>>,
    history: InputHistory,
    suppress_history_reset: bool,
}

impl EventEmitter<GotoBarEvent> for GotoOffsetBar {}

impl GotoOffsetBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Go to address or range..."));

        // Subscribe to input changes
        cx.subscribe(&input, |this, input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                if !this.suppress_history_reset {
                    this.history.reset_navigation();
                }
                let query = input.read(cx).value().to_string();
                this.update_parsed_result(&query, cx);
            }
        })
        .detach();

        Self {
            input,
            radix: GotoRadix::Hex,
            current_cursor: 0,
            total_size: 0,
            bytes_per_row: 16,
            address_map: AddressMap::default(),
            parsed_result: None,
            history: InputHistory::new(MAX_GOTO_HISTORY_ENTRIES),
            suppress_history_reset: false,
        }
    }

    pub fn set_context_info(&mut self, current_cursor: usize, total_size: usize, bytes_per_row: usize, address_map: AddressMap, cx: &mut Context<Self>) {
        self.current_cursor = current_cursor;
        self.total_size = total_size;
        self.bytes_per_row = bytes_per_row.max(1);
        self.address_map = address_map;
        let query = self.input.read(cx).value().to_string();
        self.update_parsed_result(&query, cx);
    }

    pub fn set_bytes_per_row(&mut self, bytes_per_row: usize, cx: &mut Context<Self>) {
        let effective = bytes_per_row.max(1);
        if self.bytes_per_row != effective {
            self.bytes_per_row = effective;
            let query = self.input.read(cx).value().to_string();
            self.update_parsed_result(&query, cx);
        }
    }

    fn set_radix(&mut self, radix: GotoRadix, cx: &mut Context<Self>) {
        if self.radix != radix {
            self.radix = radix;
            let query = self.input.read(cx).value().to_string();
            self.update_parsed_result(&query, cx);
        }
    }

    fn update_parsed_result(&mut self, query: &str, cx: &mut Context<Self>) {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.parsed_result = None;
        } else {
            self.parsed_result = Some(parse_goto_offset_with_context(
                trimmed,
                self.current_cursor,
                self.total_size,
                self.radix,
                &self.address_map,
                self.bytes_per_row,
            ));
        }
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    fn navigate_history(&mut self, older: bool, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.input.read(cx).value().to_string();
        let entry = if older { self.history.prev(&current) } else { self.history.next() }.map(str::to_string);
        if let Some(value) = entry {
            self.suppress_history_reset = true;
            self.input.update(cx, |input, cx| {
                input.set_value(value.clone(), window, cx);
            });
            self.suppress_history_reset = false;
            self.update_parsed_result(&value, cx);
        }
    }

    pub fn execute_jump(&mut self, extend_selection: bool, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().to_string();
        let trimmed = query.trim();
        if let Ok(parsed) = parse_goto_offset_with_context(trimmed, self.current_cursor, self.total_size, self.radix, &self.address_map, self.bytes_per_row) {
            self.history.push(trimmed);
            if let Some(range) = parsed.selection_range {
                cx.emit(GotoBarEvent::SelectRange { range });
            } else {
                cx.emit(GotoBarEvent::Jump {
                    offset: parsed.target_offset,
                    extend_selection,
                });
            }
        }
    }
}

impl Render for GotoOffsetBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let hint_text = match self.radix {
            GotoRadix::Hex => "Hex default — e.g. 100, 100..1FF, +20, 50%, L10",
            GotoRadix::Dec => "Dec default — e.g. 256, 0x100..0x1FF, +32, 50%, L10",
        };
        let bottom_info = match &self.parsed_result {
            None => div().px_1().text_xs().text_color(theme.muted_foreground).child(hint_text),
            Some(Ok(parsed)) => {
                let text = if let Some(range) = &parsed.selection_range {
                    let len = range.len();
                    let byte_word = if len == 1 { "byte" } else { "bytes" };
                    if len == 0 {
                        format!(
                            "Range: {}..{} (0 bytes)",
                            hex_case.format_prefixed(parsed.target_offset),
                            hex_case.format_prefixed(parsed.target_offset)
                        )
                    } else {
                        let start_addr = self.address_map.offset_to_address(range.start);
                        let end_addr = self.address_map.offset_to_address(range.end.saturating_sub(1));
                        format!(
                            "Range: {}..{} ({} | {} {})",
                            hex_case.format_prefixed(start_addr),
                            hex_case.format_prefixed(end_addr),
                            hex_case.format_prefixed(len),
                            len,
                            byte_word
                        )
                    }
                } else {
                    let target_addr = self.address_map.offset_to_address(parsed.target_offset);
                    format!("Target: {} ({} dec)", hex_case.format_prefixed(target_addr), target_addr)
                };
                let max_offset = self.total_size.saturating_sub(1);
                let max_addr = self.address_map.offset_to_address(max_offset);
                let display_text = if parsed.is_out_of_bounds {
                    format!("{text} ⚠ (max {})", hex_case.format_prefixed(max_addr))
                } else {
                    text
                };
                div()
                    .px_1()
                    .text_xs()
                    .text_color(if parsed.is_out_of_bounds { theme.warning } else { theme.foreground })
                    .child(display_text)
            }
            Some(Err(err)) => div().px_1().text_xs().text_color(theme.danger).child(format!("{}", err)),
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
                    .gap_2()
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
                                if self.radix == GotoRadix::Hex {
                                    Button::new("goto_radix_hex").label("Hex").primary().with_size(Size::XSmall)
                                } else {
                                    Button::new("goto_radix_hex").label("Hex").ghost().with_size(Size::XSmall)
                                }
                                .tooltip("Interpret bare numbers as hexadecimal")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_radix(GotoRadix::Hex, cx);
                                })),
                            )
                            .child(
                                if self.radix == GotoRadix::Dec {
                                    Button::new("goto_radix_dec").label("Dec").primary().with_size(Size::XSmall)
                                } else {
                                    Button::new("goto_radix_dec").label("Dec").ghost().with_size(Size::XSmall)
                                }
                                .tooltip("Interpret bare numbers as decimal")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.set_radix(GotoRadix::Dec, cx);
                                })),
                            ),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child("Go to Address")),
            )
            .child(
                Button::new("close")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Close)
                    .tooltip("Close (Esc)")
                    .on_click(cx.listener(|_, _, _, cx| {
                        cx.emit(GotoBarEvent::Dismiss);
                    })),
            );

        div()
            .flex()
            .flex_col()
            .w(px(460.0))
            .max_w_full()
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .rounded_lg()
            .shadow_xl()
            .overflow_hidden()
            .key_context("GotoOffsetBar")
            .occlude_and_block_input()
            .on_action(cx.listener(|this, _: &GotoJump, _, cx| {
                this.execute_jump(false, cx);
            }))
            .on_action(cx.listener(|this, _: &GotoJumpExtend, _, cx| {
                this.execute_jump(true, cx);
            }))
            .on_action(cx.listener(|_, _: &GotoDismiss, _, cx| {
                cx.emit(GotoBarEvent::Dismiss);
            }))
            .on_action(cx.listener(|this, _: &GotoHistoryPrev, window, cx| {
                this.navigate_history(true, window, cx);
            }))
            .on_action(cx.listener(|this, _: &GotoHistoryNext, window, cx| {
                this.navigate_history(false, window, cx);
            }))
            .child(header_strip)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .p_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .child(
                                div().flex_1().min_w_0().child(
                                    Input::new(&self.input)
                                        .small()
                                        .prefix(Icon::new(IconName::Hash).size_3p5().text_color(theme.muted_foreground))
                                        .cleanable(true),
                                ),
                            )
                            .child(
                                div()
                                    .w(px(56.0))
                                    .flex_shrink_0()
                                    .child(
                                        Button::new("go_jump")
                                            .w_full()
                                            .label("Go")
                                            .primary()
                                            .small()
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.execute_jump(false, cx);
                                            })),
                                    ),
                            ),
                    )
                    .child(bottom_info),
            )
    }
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", GotoJump, Some("GotoOffsetBar")),
        KeyBinding::new("shift-enter", GotoJumpExtend, Some("GotoOffsetBar")),
        KeyBinding::new("escape", GotoDismiss, Some("GotoOffsetBar")),
        KeyBinding::new("up", GotoHistoryPrev, Some("GotoOffsetBar > Input")),
        KeyBinding::new("down", GotoHistoryNext, Some("GotoOffsetBar > Input")),
    ]);
}
