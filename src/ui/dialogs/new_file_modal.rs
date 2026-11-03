use crate::core::new_file::{format_fill_preview, format_size_preview, parse_buffer_size, parse_fill_byte};
use crate::ui::icon::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable, Icon, Sizable, Size, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

pub fn init(_cx: &mut App) {}

pub struct NewFileModal {
    pub size_input: Entity<InputState>,
    pub fill_input: Entity<InputState>,
    pub parsed_size: Result<usize, String>,
    pub parsed_fill: Result<u8, String>,
}

impl NewFileModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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

    pub fn is_valid(&self) -> bool {
        self.parsed_size.is_ok() && self.parsed_fill.is_ok()
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
}

impl Render for NewFileModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        v_flex()
            .gap_4()
            .child(self.render_size_section(&theme, cx))
            .child(self.render_fill_section(&theme, cx))
    }
}

/// Displays the New Binary File dialog.
pub fn open_new_file_dialog(window: &mut Window, cx: &mut App, on_confirm: impl Fn(usize, u8, &mut Window, &mut App) + 'static) {
    if window.has_active_dialog(cx) {
        return;
    }

    let on_confirm = std::rc::Rc::new(on_confirm);
    let view = cx.new(|cx| NewFileModal::new(window, cx));

    // Focus size input initially
    view.update(cx, |v, cx| {
        v.size_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    });

    let on_confirm_for_click = on_confirm.clone();
    let on_confirm_for_ok = on_confirm.clone();
    let view_for_dialog = view.clone();
    let view_for_content = view.clone();
    let view_for_ok = view.clone();

    window.open_dialog(cx, move |dialog, _window, cx| {
        let theme = cx.theme();
        let is_valid = view_for_dialog.read(cx).is_valid();

        let on_confirm_btn = on_confirm_for_click.clone();
        let view_btn = view_for_dialog.clone();

        dialog
            .w(px(460.0))
            .title(
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
            .content({
                let view = view_for_content.clone();
                move |content, _window, _cx| content.child(view.clone())
            })
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new("cancel-btn")
                            .label("Cancel")
                            .ghost()
                            .with_size(Size::Small)
                            .on_click(|_, window, cx| {
                                window.close_dialog(cx);
                            }),
                    )
                    .child(
                        Button::new("create-btn")
                            .label("Create")
                            .primary()
                            .with_size(Size::Small)
                            .disabled(!is_valid)
                            .on_click(move |_, window, cx| {
                                let (size, fill) = {
                                    let v = view_btn.read(cx);
                                    match (v.parsed_size.as_ref(), v.parsed_fill.as_ref()) {
                                        (Ok(s), Ok(f)) => (*s, *f),
                                        _ => return,
                                    }
                                };
                                window.close_dialog(cx);
                                on_confirm_btn(size, fill, window, cx);
                            }),
                    ),
            )
            .on_ok({
                let on_confirm_ok = on_confirm_for_ok.clone();
                let view_ok = view_for_ok.clone();
                move |_, window, cx| {
                    let v = view_ok.read(cx);
                    if let (Ok(size), Ok(fill)) = (&v.parsed_size, &v.parsed_fill) {
                        let size = *size;
                        let fill = *fill;
                        on_confirm_ok(size, fill, window, cx);
                        true
                    } else {
                        false
                    }
                }
            })
    });
}

#[cfg(test)]
mod tests {
    use crate::core::new_file::{parse_buffer_size, parse_fill_byte};

    #[test]
    fn test_new_file_preset_values_are_valid() {
        assert!(parse_buffer_size("0").is_ok());
        assert!(parse_buffer_size("256").is_ok());
        assert!(parse_buffer_size("1KB").is_ok());
        assert!(parse_buffer_size("4KB").is_ok());
        assert!(parse_buffer_size("64KB").is_ok());
        assert!(parse_buffer_size("1MB").is_ok());

        assert!(parse_fill_byte("0x00").is_ok());
        assert!(parse_fill_byte("0xFF").is_ok());
        assert!(parse_fill_byte("0x20").is_ok());
    }
}
