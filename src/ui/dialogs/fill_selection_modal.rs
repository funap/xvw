use std::ops::Range;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable, Icon, Sizable, Size, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::core::appearance::Appearance;
use crate::core::fill::{FillPattern, RandomMode, SequentialWidth, parse_pattern_hex, parse_pattern_text, parse_step_val, parse_u64_val};
use crate::core::new_file::parse_fill_byte;
use crate::core::radix::ByteOrder;
use crate::ui::icon::IconName;

#[derive(Clone, PartialEq, Action)]
pub struct ConfirmFill;

#[derive(Clone, PartialEq, Action)]
pub struct CancelFill;

const CONTEXT: &str = "FillSelectionModal";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", ConfirmFill, Some(CONTEXT)),
        KeyBinding::new("escape", CancelFill, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FillTab {
    #[default]
    SingleByte,
    Pattern,
    Sequential,
    Random,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PatternFormat {
    #[default]
    Hex,
    Text,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FillSelectionModalEvent {
    Fill(FillPattern),
    Cancel,
}

pub struct FillSelectionModal {
    focus_handle: FocusHandle,
    range: Range<usize>,
    active_tab: FillTab,

    // Single Byte
    byte_input: Entity<InputState>,
    parsed_byte: Result<u8, String>,

    // Pattern
    pattern_format: PatternFormat,
    pattern_input: Entity<InputState>,
    parsed_pattern: Result<Vec<u8>, String>,

    // Sequential
    seq_width: SequentialWidth,
    seq_order: ByteOrder,
    seq_start_input: Entity<InputState>,
    seq_step_input: Entity<InputState>,
    parsed_seq_start: Result<u64, String>,
    parsed_seq_step: Result<u64, String>,

    // Random
    random_mode: RandomMode,
    random_preview_sample: Vec<u8>,
}

impl EventEmitter<FillSelectionModalEvent> for FillSelectionModal {}

impl FillSelectionModal {
    pub fn new(range: Range<usize>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        // 1. Single byte input
        let byte_input = cx.new(|cx| InputState::new(window, cx).placeholder("0x00, 0xFF, 0x90, 255, ' '..."));
        byte_input.update(cx, |input, cx| {
            input.set_value("0x00", window, cx);
        });

        // 2. Pattern input
        let pattern_input = cx.new(|cx| InputState::new(window, cx).placeholder("DE AD BE EF, deadbeef..."));
        pattern_input.update(cx, |input, cx| {
            input.set_value("DE AD BE EF", window, cx);
        });

        // 3. Sequential inputs
        let seq_start_input = cx.new(|cx| InputState::new(window, cx).placeholder("0, 0x00, 0x1000..."));
        let seq_step_input = cx.new(|cx| InputState::new(window, cx).placeholder("1, 2, -1, 0x10..."));
        seq_start_input.update(cx, |input, cx| {
            input.set_value("0", window, cx);
        });
        seq_step_input.update(cx, |input, cx| {
            input.set_value("1", window, cx);
        });

        let this = Self {
            focus_handle,
            range,
            active_tab: FillTab::SingleByte,

            byte_input: byte_input.clone(),
            parsed_byte: parse_fill_byte("0x00"),

            pattern_format: PatternFormat::Hex,
            pattern_input: pattern_input.clone(),
            parsed_pattern: parse_pattern_hex("DE AD BE EF"),

            seq_width: SequentialWidth::U8,
            seq_order: ByteOrder::LittleEndian,
            seq_start_input: seq_start_input.clone(),
            seq_step_input: seq_step_input.clone(),
            parsed_seq_start: parse_u64_val("0"),
            parsed_seq_step: parse_step_val("1"),

            random_mode: RandomMode::Pseudo,
            random_preview_sample: FillPattern::Random { mode: RandomMode::Pseudo }.generate(16),
        };

        // Subscribe to byte input changes
        cx.subscribe(&byte_input, |this, input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let val = input.read(cx).value().to_string();
                this.parsed_byte = parse_fill_byte(&val);
                cx.notify();
            }
        })
        .detach();

        // Subscribe to pattern input changes
        cx.subscribe(&pattern_input, |this, _input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                this.reparse_pattern(cx);
            }
        })
        .detach();

        // Subscribe to sequential start input changes
        cx.subscribe(&seq_start_input, |this, input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let val = input.read(cx).value().to_string();
                this.parsed_seq_start = parse_u64_val(&val);
                cx.notify();
            }
        })
        .detach();

        // Subscribe to sequential step input changes
        cx.subscribe(&seq_step_input, |this, input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let val = input.read(cx).value().to_string();
                this.parsed_seq_step = parse_step_val(&val);
                cx.notify();
            }
        })
        .detach();

        this
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.active_tab {
            FillTab::SingleByte => {
                self.byte_input.update(cx, |input, cx| input.focus(window, cx));
            }
            FillTab::Pattern => {
                self.pattern_input.update(cx, |input, cx| input.focus(window, cx));
            }
            FillTab::Sequential => {
                self.seq_start_input.update(cx, |input, cx| input.focus(window, cx));
            }
            FillTab::Random => {
                self.focus_handle.focus(window, cx);
            }
        }
    }

    fn reparse_pattern(&mut self, cx: &mut Context<Self>) {
        let val = self.pattern_input.read(cx).value().to_string();
        self.parsed_pattern = match self.pattern_format {
            PatternFormat::Hex => parse_pattern_hex(&val),
            PatternFormat::Text => parse_pattern_text(&val),
        };
        cx.notify();
    }

    pub fn set_single_byte_preset(&mut self, preset: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.byte_input.update(cx, |input, cx| {
            input.set_value(preset, window, cx);
        });
        self.parsed_byte = parse_fill_byte(preset);
        cx.notify();
    }

    pub fn set_pattern_preset(&mut self, preset: &'static str, format: PatternFormat, window: &mut Window, cx: &mut Context<Self>) {
        self.pattern_format = format;
        self.pattern_input.update(cx, |input, cx| {
            input.set_value(preset, window, cx);
        });
        self.parsed_pattern = match format {
            PatternFormat::Hex => parse_pattern_hex(preset),
            PatternFormat::Text => parse_pattern_text(preset),
        };
        cx.notify();
    }

    pub fn set_seq_preset(&mut self, start: &'static str, step: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.seq_start_input.update(cx, |input, cx| {
            input.set_value(start, window, cx);
        });
        self.seq_step_input.update(cx, |input, cx| {
            input.set_value(step, window, cx);
        });
        self.parsed_seq_start = parse_u64_val(start);
        self.parsed_seq_step = parse_step_val(step);
        cx.notify();
    }

    fn regenerate_random_preview(&mut self) {
        self.random_preview_sample = FillPattern::Random { mode: self.random_mode }.generate(16);
    }

    fn is_current_valid(&self) -> bool {
        match self.active_tab {
            FillTab::SingleByte => self.parsed_byte.is_ok(),
            FillTab::Pattern => self.parsed_pattern.as_ref().is_ok_and(|p| !p.is_empty()),
            FillTab::Sequential => self.parsed_seq_start.is_ok() && self.parsed_seq_step.is_ok(),
            FillTab::Random => true,
        }
    }

    fn current_error_message(&self) -> Option<String> {
        match self.active_tab {
            FillTab::SingleByte => self.parsed_byte.as_ref().err().cloned(),
            FillTab::Pattern => self.parsed_pattern.as_ref().err().cloned(),
            FillTab::Sequential => self
                .parsed_seq_start
                .as_ref()
                .err()
                .cloned()
                .or_else(|| self.parsed_seq_step.as_ref().err().cloned()),
            FillTab::Random => None,
        }
    }

    pub fn submit(&mut self, cx: &mut Context<Self>) {
        if !self.is_current_valid() {
            return;
        }

        let pattern = match self.active_tab {
            FillTab::SingleByte => {
                let byte = match &self.parsed_byte {
                    Ok(b) => *b,
                    Err(_) => return,
                };
                FillPattern::SingleByte(byte)
            }
            FillTab::Pattern => {
                let bytes = match &self.parsed_pattern {
                    Ok(p) => p.clone(),
                    Err(_) => return,
                };
                FillPattern::Pattern(bytes)
            }
            FillTab::Sequential => {
                let start = match &self.parsed_seq_start {
                    Ok(s) => *s,
                    Err(_) => return,
                };
                let step = match &self.parsed_seq_step {
                    Ok(st) => *st,
                    Err(_) => return,
                };
                FillPattern::Sequential {
                    width: self.seq_width,
                    start,
                    step,
                    byte_order: self.seq_order,
                }
            }
            FillTab::Random => FillPattern::Random { mode: self.random_mode },
        };

        cx.emit(FillSelectionModalEvent::Fill(pattern));
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(FillSelectionModalEvent::Cancel);
    }

    fn render_header(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let range_len = self.range.len();
        let range_str = format!(
            "0x{:08X} .. 0x{:08X} ({} bytes / 0x{:X})",
            self.range.start, self.range.end, range_len, range_len
        );

        h_flex()
            .justify_between()
            .items_center()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.tab_bar)
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(Icon::new(IconName::Replace).size_4().text_color(theme.primary))
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_sm().child("Fill Selection"))
                    .child(
                        div()
                            .text_xs()
                            .px_2()
                            .py_0p5()
                            .rounded_md()
                            .bg(theme.muted)
                            .text_color(theme.muted_foreground)
                            .child(range_str),
                    ),
            )
            .child(
                Button::new("close-fill-modal")
                    .icon(IconName::Close)
                    .ghost()
                    .with_size(Size::XSmall)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.cancel(cx);
                    })),
            )
    }

    fn render_tab_buttons(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let tab_item = |tab: FillTab, label: &'static str, cx: &mut Context<Self>| {
            let is_active = self.active_tab == tab;
            let theme = theme.clone();
            div()
                .id(ElementId::Name(format!("tab-btn-{label}").into()))
                .flex_1()
                .py_1p5()
                .px_2()
                .rounded_md()
                .text_xs()
                .font_weight(if is_active { FontWeight::SEMIBOLD } else { FontWeight::NORMAL })
                .text_align(TextAlign::Center)
                .cursor_pointer()
                .when(is_active, |el| el.bg(theme.primary).text_color(theme.primary_foreground))
                .when(!is_active, |el| {
                    el.bg(theme.muted)
                        .text_color(theme.foreground)
                        .hover(|h| h.bg(theme.accent).text_color(theme.accent_foreground))
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| {
                        this.active_tab = tab;
                        if tab == FillTab::Random {
                            this.regenerate_random_preview();
                        }
                        this.focus(window, cx);
                        cx.notify();
                    }),
                )
                .child(label)
        };

        h_flex()
            .gap_1p5()
            .p_1()
            .bg(theme.tab_bar)
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .child(tab_item(FillTab::SingleByte, "Single Byte", cx))
            .child(tab_item(FillTab::Pattern, "Pattern", cx))
            .child(tab_item(FillTab::Sequential, "Sequential", cx))
            .child(tab_item(FillTab::Random, "Random", cx))
    }

    fn render_single_byte_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let preview_text = match &self.parsed_byte {
            Ok(b) => {
                let ascii = if b.is_ascii_graphic() || *b == b' ' {
                    format!("'{}'", *b as char)
                } else {
                    "non-printable".to_string()
                };
                format!("Hex: 0x{b:02X} | Dec: {b} | Bin: 0b{b:08b} | ASCII: {ascii}")
            }
            Err(e) => format!("Error: {e}"),
        };

        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).child("Byte Value"))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(preview_text)),
            )
            .child(Input::new(&self.byte_input).cleanable(true))
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .items_center()
                    .child(div().text_xs().text_color(theme.muted_foreground).mr_1().child("Presets:"))
                    .child(
                        Button::new("fill-00")
                            .label("0x00 (Zeros)")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_single_byte_preset("0x00", window, cx);
                            })),
                    )
                    .child(
                        Button::new("fill-ff")
                            .label("0xFF (0xFF)")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_single_byte_preset("0xFF", window, cx);
                            })),
                    )
                    .child(
                        Button::new("fill-90")
                            .label("0x90 (NOP)")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_single_byte_preset("0x90", window, cx);
                            })),
                    )
                    .child(
                        Button::new("fill-20")
                            .label("0x20 (Spaces)")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_single_byte_preset("0x20", window, cx);
                            })),
                    ),
            )
    }

    fn render_pattern_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let is_hex = self.pattern_format == PatternFormat::Hex;

        let preview_text = match &self.parsed_pattern {
            Ok(bytes) => {
                let hex_repr: Vec<String> = bytes.iter().take(8).map(|b| format!("{b:02X}")).collect();
                let dots = if bytes.len() > 8 { "..." } else { "" };
                format!("Length: {} bytes | {}{dots}", bytes.len(), hex_repr.join(" "))
            }
            Err(e) => format!("Error: {e}"),
        };

        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().text_xs().font_weight(FontWeight::MEDIUM).child("Format:"))
                            .child(
                                Button::new("format-hex")
                                    .label("Hex Bytes")
                                    .with_size(Size::XSmall)
                                    .when(is_hex, |b| b.primary())
                                    .when(!is_hex, |b| b.ghost())
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.pattern_format = PatternFormat::Hex;
                                        this.pattern_input.update(cx, |input, cx| {
                                            if input.value() == "NULL" {
                                                input.set_value("DE AD BE EF", window, cx);
                                            }
                                            input.set_placeholder("DE AD BE EF, deadbeef...", window, cx);
                                            input.focus(window, cx);
                                        });
                                        this.reparse_pattern(cx);
                                    })),
                            )
                            .child(
                                Button::new("format-text")
                                    .label("Text String")
                                    .with_size(Size::XSmall)
                                    .when(!is_hex, |b| b.primary())
                                    .when(is_hex, |b| b.ghost())
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.pattern_format = PatternFormat::Text;
                                        this.pattern_input.update(cx, |input, cx| {
                                            if input.value() == "DE AD BE EF" {
                                                input.set_value("NULL", window, cx);
                                            }
                                            input.set_placeholder("NULL, TEST, text...", window, cx);
                                            input.focus(window, cx);
                                        });
                                        this.reparse_pattern(cx);
                                    })),
                            ),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(preview_text)),
            )
            .child(Input::new(&self.pattern_input).cleanable(true))
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .items_center()
                    .child(div().text_xs().text_color(theme.muted_foreground).mr_1().child("Presets:"))
                    .when(is_hex, |el| {
                        el.child(
                            Button::new("pat-zeros")
                                .label("00 00 00 00")
                                .ghost()
                                .with_size(Size::XSmall)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_pattern_preset("00 00 00 00", PatternFormat::Hex, window, cx);
                                })),
                        )
                        .child(
                            Button::new("pat-ff")
                                .label("FF FF FF FF")
                                .ghost()
                                .with_size(Size::XSmall)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_pattern_preset("FF FF FF FF", PatternFormat::Hex, window, cx);
                                })),
                        )
                        .child(
                            Button::new("pat-00ff")
                                .label("00 FF")
                                .ghost()
                                .with_size(Size::XSmall)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_pattern_preset("00 FF", PatternFormat::Hex, window, cx);
                                })),
                        )
                        .child(
                            Button::new("pat-55aa")
                                .label("55 AA")
                                .ghost()
                                .with_size(Size::XSmall)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_pattern_preset("55 AA", PatternFormat::Hex, window, cx);
                                })),
                        )
                        .child(
                            Button::new("pat-deadbeef")
                                .label("DE AD BE EF")
                                .ghost()
                                .with_size(Size::XSmall)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_pattern_preset("DE AD BE EF", PatternFormat::Hex, window, cx);
                                })),
                        )
                    })
                    .when(!is_hex, |el| {
                        el.child(
                            Button::new("pat-null")
                                .label("NULL")
                                .ghost()
                                .with_size(Size::XSmall)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_pattern_preset("NULL", PatternFormat::Text, window, cx);
                                })),
                        )
                        .child(
                            Button::new("pat-test")
                                .label("TEST")
                                .ghost()
                                .with_size(Size::XSmall)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_pattern_preset("TEST", PatternFormat::Text, window, cx);
                                })),
                        )
                        .child(
                            Button::new("pat-debug")
                                .label("DEBUG")
                                .ghost()
                                .with_size(Size::XSmall)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_pattern_preset("DEBUG", PatternFormat::Text, window, cx);
                                })),
                        )
                        .child(
                            Button::new("pat-abcd")
                                .label("ABCD")
                                .ghost()
                                .with_size(Size::XSmall)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_pattern_preset("ABCD", PatternFormat::Text, window, cx);
                                })),
                        )
                        .child(
                            Button::new("pat-divider")
                                .label("----")
                                .ghost()
                                .with_size(Size::XSmall)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.set_pattern_preset("----", PatternFormat::Text, window, cx);
                                })),
                        )
                    }),
            )
    }

    fn render_sequential_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let width_btn = |w: SequentialWidth, label: &'static str, cx: &mut Context<Self>| {
            let is_sel = self.seq_width == w;
            Button::new(ElementId::Name(format!("width-{label}").into()))
                .label(label)
                .with_size(Size::XSmall)
                .when(is_sel, |b| b.primary())
                .when(!is_sel, |b| b.ghost())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.seq_width = w;
                    cx.notify();
                }))
        };

        let order_btn = |order: ByteOrder, label: &'static str, cx: &mut Context<Self>| {
            let is_sel = self.seq_order == order;
            Button::new(ElementId::Name(format!("order-{label}").into()))
                .label(label)
                .with_size(Size::XSmall)
                .when(is_sel, |b| b.primary())
                .when(!is_sel, |b| b.ghost())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.seq_order = order;
                    cx.notify();
                }))
        };

        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .child(div().text_xs().font_weight(FontWeight::MEDIUM).child("Width:"))
                            .child(width_btn(SequentialWidth::U8, SequentialWidth::U8.label(), cx))
                            .child(width_btn(SequentialWidth::U16, SequentialWidth::U16.label(), cx))
                            .child(width_btn(SequentialWidth::U32, SequentialWidth::U32.label(), cx))
                            .child(width_btn(SequentialWidth::U64, SequentialWidth::U64.label(), cx)),
                    )
                    .when(self.seq_width != SequentialWidth::U8, |el| {
                        el.child(
                            h_flex()
                                .gap_1p5()
                                .items_center()
                                .child(div().text_xs().font_weight(FontWeight::MEDIUM).child("Endian:"))
                                .child(order_btn(ByteOrder::LittleEndian, "LE", cx))
                                .child(order_btn(ByteOrder::BigEndian, "BE", cx)),
                        )
                    }),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(div().text_xs().font_weight(FontWeight::MEDIUM).child("Start Value"))
                            .child(Input::new(&self.seq_start_input).cleanable(true)),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .gap_1()
                            .child(div().text_xs().font_weight(FontWeight::MEDIUM).child("Step / Increment"))
                            .child(Input::new(&self.seq_step_input).cleanable(true)),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .items_center()
                    .child(div().text_xs().text_color(theme.muted_foreground).mr_1().child("Presets:"))
                    .child(
                        Button::new("seq-p1")
                            .label("+1 (Ascending)")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_seq_preset("0", "1", window, cx);
                            })),
                    )
                    .child(
                        Button::new("seq-m1")
                            .label("-1 (Descending)")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_seq_preset("255", "-1", window, cx);
                            })),
                    )
                    .child(
                        Button::new("seq-p2")
                            .label("+2 (Even)")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_seq_preset("0", "2", window, cx);
                            })),
                    )
                    .child(
                        Button::new("seq-p16")
                            .label("+16 (0x10)")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_seq_preset("0", "16", window, cx);
                            })),
                    ),
            )
    }

    fn render_random_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let is_pseudo = self.random_mode == RandomMode::Pseudo;

        let mode_btn = |mode: RandomMode, label: &'static str, cx: &mut Context<Self>| {
            let is_sel = self.random_mode == mode;
            Button::new(ElementId::Name(format!("rand-{label}").into()))
                .label(label)
                .with_size(Size::Small)
                .when(is_sel, |b| b.primary())
                .when(!is_sel, |b| b.ghost())
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.random_mode = mode;
                    this.regenerate_random_preview();
                    cx.notify();
                }))
        };

        v_flex()
            .gap_3()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(div().text_xs().font_weight(FontWeight::MEDIUM).child("Generator Mode:"))
                            .child(mode_btn(RandomMode::Pseudo, RandomMode::Pseudo.label(), cx))
                            .child(mode_btn(RandomMode::Cryptographic, RandomMode::Cryptographic.label(), cx)),
                    )
                    .child(
                        Button::new("refresh-sample")
                            .label("New Sample")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.regenerate_random_preview();
                                cx.notify();
                            })),
                    ),
            )
            .child(
                div()
                    .p_2p5()
                    .rounded_md()
                    .bg(theme.muted)
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .when(is_pseudo, |el| {
                        el.child("Fast 64-bit pseudo-random numbers (XorShift). Best suited for high-speed filling, testing, and general binary generation.")
                    })
                    .when(!is_pseudo, |el| {
                        el.child("Cryptographically secure randomness generated via OS entropy pool. Best suited for security testing and data sanitization.")
                    }),
            )
    }

    fn render_preview_error(&self, theme: &gpui_kit::component::Theme, error: &str) -> impl IntoElement {
        let is_random = self.active_tab == FillTab::Random;
        let title = if is_random { "Sample Output" } else { "Fill Preview" };
        let icon = if is_random { IconName::Sparkles } else { IconName::Eye };

        v_flex()
            .h(px(126.0))
            .justify_between()
            .p_3()
            .rounded_md()
            .bg(theme.muted)
            .border_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .child(Icon::new(icon).size_3p5().text_color(theme.muted_foreground))
                            .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).child(title)),
                    )
                    .child(div().text_xs().text_color(theme.danger).child("Invalid Input")),
            )
            .child(
                div()
                    .p_2()
                    .rounded_md()
                    .bg(theme.tab_bar)
                    .border_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.danger)
                    .child(format!("Error: {error}")),
            )
            .child(div().text_xs().text_color(theme.muted_foreground).child("Please check the input value."))
    }

    fn render_preview(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let font_family = cx.global::<Appearance>().font_family.clone();
        let total_len = self.range.len();

        let pattern_res = match self.active_tab {
            FillTab::SingleByte => match &self.parsed_byte {
                Ok(b) => Ok(FillPattern::SingleByte(*b)),
                Err(e) => Err(e.clone()),
            },
            FillTab::Pattern => match &self.parsed_pattern {
                Ok(bytes) => {
                    if bytes.is_empty() {
                        Err("Pattern cannot be empty".to_string())
                    } else {
                        Ok(FillPattern::Pattern(bytes.clone()))
                    }
                }
                Err(e) => Err(e.clone()),
            },
            FillTab::Sequential => {
                let start = match &self.parsed_seq_start {
                    Ok(s) => *s,
                    Err(e) => return self.render_preview_error(theme, e).into_any_element(),
                };
                let step = match &self.parsed_seq_step {
                    Ok(st) => *st,
                    Err(e) => return self.render_preview_error(theme, e).into_any_element(),
                };
                Ok(FillPattern::Sequential {
                    width: self.seq_width,
                    start,
                    step,
                    byte_order: self.seq_order,
                })
            }
            FillTab::Random => Ok(FillPattern::Random { mode: self.random_mode }),
        };

        let pattern = match pattern_res {
            Ok(p) => p,
            Err(e) => return self.render_preview_error(theme, &e).into_any_element(),
        };

        if total_len == 0 {
            return self.render_preview_error(theme, "No bytes selected (range is empty)").into_any_element();
        }

        let preview_len = total_len.min(16);
        let sample_bytes: Vec<u8> = if self.active_tab == FillTab::Random {
            self.random_preview_sample[..preview_len.min(self.random_preview_sample.len())].to_vec()
        } else {
            pattern.generate(preview_len)
        };

        let mut hex_parts = Vec::with_capacity(preview_len + 1);
        let mut text_parts = Vec::with_capacity(preview_len + 1);

        for (i, &b) in sample_bytes.iter().enumerate() {
            if i == 8 {
                hex_parts.push(String::new());
                text_parts.push(" ".to_string());
            }
            hex_parts.push(format!("{b:02X}"));
            let ch = if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' };
            text_parts.push(ch.to_string());
        }

        let has_more = total_len > 16;
        let hex_display = if has_more {
            format!("{} ...", hex_parts.join(" "))
        } else {
            hex_parts.join(" ")
        };

        let text_display = if has_more {
            format!("{} ...", text_parts.concat())
        } else {
            text_parts.concat()
        };

        let is_random = self.active_tab == FillTab::Random;
        let title = if is_random { "Sample Output" } else { "Fill Preview" };
        let icon = if is_random { IconName::Sparkles } else { IconName::Eye };

        let range_info = if is_random {
            if total_len <= 16 {
                format!("Sample ({total_len} bytes)")
            } else {
                format!("Sample of first 16 bytes (total: {total_len})")
            }
        } else if total_len <= 16 {
            format!("{total_len} bytes (0x{total_len:X})")
        } else {
            format!("First 16 of {total_len} bytes (0x{total_len:X})")
        };

        let note = match &pattern {
            FillPattern::SingleByte(b) => {
                let ascii_repr = if b.is_ascii_graphic() || *b == b' ' {
                    format!(" ('{}')", *b as char)
                } else {
                    String::new()
                };
                format!("Fills {total_len} bytes with 0x{b:02X}{ascii_repr}")
            }
            FillPattern::Pattern(bytes) => {
                format!("Repeats {}-byte pattern across {total_len} bytes", bytes.len())
            }
            FillPattern::Sequential {
                width,
                start,
                step,
                byte_order,
            } => {
                let order_str = if *width == SequentialWidth::U8 {
                    ""
                } else if !byte_order.is_big_endian() {
                    ", LE"
                } else {
                    ", BE"
                };
                format!("Sequential {} (start: {start}, step: {step:+}{order_str})", width.label())
            }
            FillPattern::Random { mode } => {
                format!("Sample data ({} mode) — each fill operation generates a fresh random sequence", mode.label())
            }
        };

        v_flex()
            .h(px(126.0))
            .justify_between()
            .p_3()
            .rounded_md()
            .bg(theme.muted)
            .border_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        h_flex()
                            .gap_1p5()
                            .items_center()
                            .child(Icon::new(icon).size_3p5().text_color(theme.muted_foreground))
                            .child(div().text_xs().font_weight(FontWeight::SEMIBOLD).child(title)),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(range_info)),
            )
            .child(
                v_flex()
                    .p_2()
                    .rounded_md()
                    .bg(theme.tab_bar)
                    .border_1()
                    .border_color(theme.border)
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .w(px(38.0))
                                    .font_family(font_family.clone())
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("HEX"),
                            )
                            .child(div().font_family(font_family.clone()).text_xs().text_color(theme.foreground).child(hex_display)),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(
                                div()
                                    .w(px(38.0))
                                    .font_family(font_family.clone())
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child("TEXT"),
                            )
                            .child(div().font_family(font_family).text_xs().text_color(theme.foreground).child(text_display)),
                    ),
            )
            .child(div().text_xs().text_color(theme.muted_foreground).child(note))
            .into_any_element()
    }

    fn render_footer(&self, theme: &gpui_kit::component::Theme, is_valid: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let error_msg = self.current_error_message();

        h_flex()
            .justify_between()
            .items_center()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.tab_bar)
            .child(div().text_xs().text_color(theme.danger).child(error_msg.unwrap_or_default()))
            .child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(
                        Button::new("cancel-btn")
                            .label("Cancel")
                            .ghost()
                            .with_size(Size::Small)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.cancel(cx);
                            })),
                    )
                    .child(
                        Button::new("fill-btn")
                            .label("Fill")
                            .primary()
                            .with_size(Size::Small)
                            .disabled(!is_valid)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.submit(cx);
                            })),
                    ),
            )
    }
}

impl Render for FillSelectionModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_valid = self.is_current_valid();

        v_flex()
            .id("fill-selection-modal")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ConfirmFill, _, cx| {
                this.submit(cx);
            }))
            .on_action(cx.listener(|this, _: &CancelFill, _, cx| {
                this.cancel(cx);
            }))
            .w(px(520.0))
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .rounded_lg()
            .shadow_xl()
            .overflow_hidden()
            .child(self.render_header(&theme, cx))
            .child(
                v_flex()
                    .p_4()
                    .gap_3()
                    .child(self.render_tab_buttons(&theme, cx))
                    .child(div().h(px(148.0)).child(match self.active_tab {
                        FillTab::SingleByte => self.render_single_byte_section(&theme, cx).into_any_element(),
                        FillTab::Pattern => self.render_pattern_section(&theme, cx).into_any_element(),
                        FillTab::Sequential => self.render_sequential_section(&theme, cx).into_any_element(),
                        FillTab::Random => self.render_random_section(&theme, cx).into_any_element(),
                    }))
                    .child(self.render_preview(&theme, cx)),
            )
            .child(self.render_footer(&theme, is_valid, cx))
    }
}
