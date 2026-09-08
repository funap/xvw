use crate::core::new_file::{format_fill_preview, format_size_preview, parse_buffer_size, parse_fill_byte};
use crate::ui::icon::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable, Icon, Sizable, Size, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

#[derive(Clone, PartialEq, Action)]
pub struct ConfirmCreate;

#[derive(Clone, PartialEq, Action)]
pub struct CancelModal;

const CONTEXT: &str = "NewFileModal";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", ConfirmCreate, Some(CONTEXT)),
        KeyBinding::new("escape", CancelModal, Some(CONTEXT)),
    ]);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewFileModalEvent {
    Create { size: usize, fill_byte: u8 },
    Cancel,
}

pub struct NewFileModal {
    focus_handle: FocusHandle,
    size_input: Entity<InputState>,
    fill_input: Entity<InputState>,
    parsed_size: Result<usize, String>,
    parsed_fill: Result<u8, String>,
}

impl EventEmitter<NewFileModalEvent> for NewFileModal {}

impl NewFileModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let size_input = cx.new(|cx| InputState::new(window, cx).placeholder("1024, 0x400, 4KB, 1MB..."));
        let fill_input = cx.new(|cx| InputState::new(window, cx).placeholder("0x00, FF, 255, ' '..."));

        // Default values: 1024 bytes (1 KB), 0x00 fill
        size_input.update(cx, |input, cx| {
            input.set_value("1024", window, cx);
        });
        fill_input.update(cx, |input, cx| {
            input.set_value("0x00", window, cx);
        });

        let this = Self {
            focus_handle,
            size_input: size_input.clone(),
            fill_input: fill_input.clone(),
            parsed_size: parse_buffer_size("1024"),
            parsed_fill: parse_fill_byte("0x00"),
        };

        // Subscribe to size input changes
        cx.subscribe(&size_input, |this, input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let val = input.read(cx).value().to_string();
                this.parsed_size = parse_buffer_size(&val);
                cx.notify();
            }
        })
        .detach();

        // Subscribe to fill input changes
        cx.subscribe(&fill_input, |this, input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let val = input.read(cx).value().to_string();
                this.parsed_fill = parse_fill_byte(&val);
                cx.notify();
            }
        })
        .detach();

        this
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.size_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    }

    pub fn set_size_preset(&mut self, preset: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.size_input.update(cx, |input, cx| {
            input.set_value(preset, window, cx);
        });
        self.parsed_size = parse_buffer_size(preset);
        cx.notify();
    }

    pub fn set_fill_preset(&mut self, preset: &'static str, window: &mut Window, cx: &mut Context<Self>) {
        self.fill_input.update(cx, |input, cx| {
            input.set_value(preset, window, cx);
        });
        self.parsed_fill = parse_fill_byte(preset);
        cx.notify();
    }

    pub fn submit(&mut self, cx: &mut Context<Self>) {
        if let (Ok(size), Ok(fill_byte)) = (&self.parsed_size, &self.parsed_fill) {
            cx.emit(NewFileModalEvent::Create {
                size: *size,
                fill_byte: *fill_byte,
            });
        }
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(NewFileModalEvent::Cancel);
    }
}

impl NewFileModal {
    fn render_header(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .child(Icon::new(IconName::File).size(px(18.0)).text_color(theme.primary))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.foreground)
                            .child("New Binary File"),
                    ),
            )
            .child(
                Button::new("close-btn")
                    .icon(IconName::Close)
                    .ghost()
                    .with_size(Size::XSmall)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.cancel(cx);
                    })),
            )
    }

    fn render_size_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let size_preview_text = match &self.parsed_size {
            Ok(size) => format_size_preview(*size),
            Err(err) => err.clone(),
        };

        v_flex()
            .gap_1p5()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.foreground)
                            .child("Buffer Size"),
                    )
                    .child(div().text_xs().text_color(theme.muted_foreground).child(size_preview_text)),
            )
            .child(Input::new(&self.size_input).cleanable(true))
            .child(
                h_flex()
                    .gap_1()
                    .flex_wrap()
                    .items_center()
                    .child(div().text_xs().text_color(theme.muted_foreground).mr_1().child("Presets:"))
                    .child(
                        Button::new("size-0")
                            .label("0 B")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_size_preset("0", window, cx);
                            })),
                    )
                    .child(
                        Button::new("size-256")
                            .label("256 B")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_size_preset("256", window, cx);
                            })),
                    )
                    .child(
                        Button::new("size-1k")
                            .label("1 KB")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_size_preset("1KB", window, cx);
                            })),
                    )
                    .child(
                        Button::new("size-4k")
                            .label("4 KB")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_size_preset("4KB", window, cx);
                            })),
                    )
                    .child(
                        Button::new("size-64k")
                            .label("64 KB")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_size_preset("64KB", window, cx);
                            })),
                    )
                    .child(
                        Button::new("size-1m")
                            .label("1 MB")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_size_preset("1MB", window, cx);
                            })),
                    ),
            )
    }

    fn render_fill_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> impl IntoElement {
        let fill_preview_text = match &self.parsed_fill {
            Ok(fill) => format_fill_preview(*fill),
            Err(err) => err.clone(),
        };

        v_flex()
            .gap_1p5()
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .child(div().text_xs().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child("Fill Value"))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(fill_preview_text)),
            )
            .child(Input::new(&self.fill_input).cleanable(true))
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
                                this.set_fill_preset("0x00", window, cx);
                            })),
                    )
                    .child(
                        Button::new("fill-ff")
                            .label("0xFF (0xFF)")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_fill_preset("0xFF", window, cx);
                            })),
                    )
                    .child(
                        Button::new("fill-20")
                            .label("0x20 (Spaces)")
                            .ghost()
                            .with_size(Size::XSmall)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.set_fill_preset("0x20", window, cx);
                            })),
                    ),
            )
    }

    fn render_footer(&self, theme: &gpui_kit::component::Theme, is_valid: bool, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .justify_end()
            .items_center()
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(theme.border)
            .bg(theme.tab_bar)
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
                Button::new("create-btn")
                    .label("Create")
                    .primary()
                    .with_size(Size::Small)
                    .disabled(!is_valid)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.submit(cx);
                    })),
            )
    }
}

impl Render for NewFileModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_valid = self.parsed_size.is_ok() && self.parsed_fill.is_ok();

        v_flex()
            .id("new-file-modal")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &ConfirmCreate, _, cx| {
                this.submit(cx);
            }))
            .on_action(cx.listener(|this, _: &CancelModal, _, cx| {
                this.cancel(cx);
            }))
            .w(px(460.0))
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
                    .gap_4()
                    .child(self.render_size_section(&theme, cx))
                    .child(self.render_fill_section(&theme, cx)),
            )
            .child(self.render_footer(&theme, is_valid, cx))
    }
}
