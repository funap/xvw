use std::ops::Range;

use crate::core::address_map::AddressMap;
use crate::core::goto::{GotoParseError, GotoRadix, ParsedGotoOffset, parse_goto_offset_with_map};
use crate::ui::icon::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon,
    button::{Button, ButtonVariants},
    input::{self, Input, InputState},
};
use gpui_kit::prelude::*;
use gpui_kit::*;

#[derive(Clone, PartialEq, Action)]
pub struct GotoJump;

#[derive(Clone, PartialEq, Action)]
pub struct GotoJumpExtend;

#[derive(Clone, PartialEq, Action)]
pub struct GotoDismiss;

pub enum GotoBarEvent {
    Jump { offset: usize, extend_selection: bool },
    SelectRange { range: Range<usize> },
    Dismiss,
}

pub struct GotoOffsetBar {
    input: Entity<InputState>,
    current_cursor: usize,
    total_size: usize,
    address_map: AddressMap,
    parsed_result: Option<Result<ParsedGotoOffset, GotoParseError>>,
}

impl EventEmitter<GotoBarEvent> for GotoOffsetBar {}

impl GotoOffsetBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Address or Range (e.g. 0x100, 0x100..0x1ff, +10, 50%)..."));

        // Subscribe to input changes
        cx.subscribe(&input, |this, input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let query = input.read(cx).value().to_string();
                this.update_parsed_result(&query, cx);
            }
        })
        .detach();

        Self {
            input,
            current_cursor: 0,
            total_size: 0,
            address_map: AddressMap::default(),
            parsed_result: None,
        }
    }

    pub fn set_context_info(&mut self, current_cursor: usize, total_size: usize, address_map: AddressMap, cx: &mut Context<Self>) {
        self.current_cursor = current_cursor;
        self.total_size = total_size;
        self.address_map = address_map;
        let query = self.input.read(cx).value().to_string();
        self.update_parsed_result(&query, cx);
    }

    fn update_parsed_result(&mut self, query: &str, cx: &mut Context<Self>) {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            self.parsed_result = None;
        } else {
            self.parsed_result = Some(parse_goto_offset_with_map(
                trimmed,
                self.current_cursor,
                self.total_size,
                GotoRadix::Dec,
                &self.address_map,
            ));
        }
        cx.notify();
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    }

    pub fn execute_jump(&mut self, extend_selection: bool, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().to_string();
        if let Ok(parsed) = parse_goto_offset_with_map(&query, self.current_cursor, self.total_size, GotoRadix::Dec, &self.address_map) {
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

        let preview_info = match &self.parsed_result {
            None => {
                let current_addr = self.address_map.offset_to_address(self.current_cursor);
                let text = format!("Pos: 0x{:X} / Size: 0x{:X}", current_addr, self.total_size);
                div().text_sm().text_color(theme.muted_foreground).child(text)
            }
            Some(Ok(parsed)) => {
                let text = if let Some(range) = &parsed.selection_range {
                    let len = range.len();
                    let byte_word = if len == 1 { "byte" } else { "bytes" };
                    if len == 0 {
                        format!("Range: 0x{:X}..0x{:X} (0 bytes)", parsed.target_offset, parsed.target_offset)
                    } else {
                        let start_addr = self.address_map.offset_to_address(range.start);
                        let end_addr = self.address_map.offset_to_address(range.end.saturating_sub(1));
                        format!("Range: 0x{:X}..0x{:X} (0x{:X} | {} {})", start_addr, end_addr, len, len, byte_word)
                    }
                } else {
                    let target_addr = self.address_map.offset_to_address(parsed.target_offset);
                    format!("Target: 0x{:X} ({} dec)", target_addr, target_addr)
                };
                let max_offset = self.total_size.saturating_sub(1);
                let max_addr = self.address_map.offset_to_address(max_offset);
                let warning = if parsed.is_out_of_bounds {
                    format!(" ⚠ (max 0x{:X})", max_addr)
                } else {
                    String::new()
                };
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_sm()
                    .text_color(if parsed.is_out_of_bounds { theme.yellow } else { theme.foreground })
                    .child(text)
                    .when(!warning.is_empty(), |el| el.child(div().text_color(theme.yellow).child(warning)))
            }
            Some(Err(err)) => div().text_sm().text_color(theme.red).child(format!("{}", err)),
        };

        div()
            .flex()
            .items_center()
            .gap_2()
            .p_2()
            .bg(theme.background)
            .border_b_1()
            .border_color(theme.border)
            .key_context("GotoOffsetBar")
            .on_action(cx.listener(|this, _: &GotoJump, _, cx| {
                this.execute_jump(false, cx);
            }))
            .on_action(cx.listener(|this, _: &GotoJumpExtend, _, cx| {
                this.execute_jump(true, cx);
            }))
            .on_action(cx.listener(|_, _: &GotoDismiss, _, cx| {
                cx.emit(GotoBarEvent::Dismiss);
            }))
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&self.input).prefix(Icon::new(IconName::Hash).size_3p5()).cleanable(true)),
            )
            .child(preview_info)
            .child(Button::new("go_jump").label("Go").primary().on_click(cx.listener(|this, _, _, cx| {
                this.execute_jump(false, cx);
            })))
            .child(Button::new("close").ghost().icon(IconName::Close).on_click(cx.listener(|_, _, _, cx| {
                cx.emit(GotoBarEvent::Dismiss);
            })))
    }
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", GotoJump, Some("GotoOffsetBar")),
        KeyBinding::new("shift-enter", GotoJumpExtend, Some("GotoOffsetBar")),
        KeyBinding::new("escape", GotoDismiss, Some("GotoOffsetBar")),
    ]);
}
