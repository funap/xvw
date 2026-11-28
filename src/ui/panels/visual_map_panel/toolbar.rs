//! Toolbar controls and configuration sections for the visual map panel.

use super::VisualMapPanel;
use crate::core::appearance::Appearance;
use crate::core::visual_map::VisualMapColorMode as ColorMode;
use crate::ui::icon::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Disableable, Icon, Sizable, Size, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

impl VisualMapPanel {
    pub(crate) fn start_offset_repeat(&mut self, is_increment: bool, cx: &mut Context<Self>) {
        if is_increment {
            self.increment_header_offset(cx);
        } else {
            self.decrement_header_offset(cx);
        }

        self._offset_repeat_task = Some(super::repeat::spawn_repeat_action(cx, move |this, cx| {
            let max = this.max_header_offset();
            if is_increment {
                if this.header_offset < max {
                    this.increment_header_offset(cx);
                    this.header_offset < max
                } else {
                    false
                }
            } else if this.header_offset > 0 {
                this.decrement_header_offset(cx);
                this.header_offset > 0
            } else {
                false
            }
        }));
    }

    pub(crate) fn stop_offset_repeat(&mut self) {
        self._offset_repeat_task = None;
    }

    pub(crate) fn start_width_repeat(&mut self, is_increment: bool, cx: &mut Context<Self>) {
        if is_increment {
            self.increment_width(cx);
        } else {
            self.decrement_width(cx);
        }

        self._width_repeat_task = Some(super::repeat::spawn_repeat_action(cx, move |this, cx| {
            if is_increment {
                this.increment_width(cx);
                this.cols < 4096
            } else {
                this.decrement_width(cx);
                this.cols > this.min_cols()
            }
        }));
    }

    pub(crate) fn stop_width_repeat(&mut self) {
        self._width_repeat_task = None;
    }

    pub(crate) fn render_offset_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> AnyElement {
        let muted_color = theme.muted_foreground;
        let font_family = cx.global::<Appearance>().font_family.clone();
        let max_offset = self.max_header_offset();
        let is_at_min = self.header_offset == 0;
        let is_at_max = self.header_offset >= max_offset;

        let mut dec_btn = Button::new("dec_offset")
            .label("-")
            .ghost()
            .with_size(Size::XSmall)
            .tooltip("Decrease header offset (-1 B)");
        if is_at_min {
            dec_btn = dec_btn.disabled(true);
        } else {
            dec_btn = dec_btn
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.start_offset_repeat(false, cx);
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| {
                        this.stop_offset_repeat();
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| {
                        this.stop_offset_repeat();
                    }),
                );
        }

        let mut inc_btn = Button::new("inc_offset")
            .label("+")
            .ghost()
            .with_size(Size::XSmall)
            .tooltip("Increase header offset (+1 B)");
        if is_at_max {
            inc_btn = inc_btn.disabled(true);
        } else {
            inc_btn = inc_btn
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.start_offset_repeat(true, cx);
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| {
                        this.stop_offset_repeat();
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| {
                        this.stop_offset_repeat();
                    }),
                );
        }

        let mut reset_btn = Button::new("reset_offset")
            .label("0")
            .ghost()
            .with_size(Size::XSmall)
            .tooltip("Reset offset to 0");
        if is_at_min {
            reset_btn = reset_btn.disabled(true);
        } else {
            reset_btn = reset_btn.on_click(cx.listener(|this, _, _, cx| {
                this.set_header_offset(0, cx);
            }));
        }

        let right_flex = h_flex()
            .items_center()
            .gap_1()
            .child(reset_btn)
            .child(dec_btn)
            .child(
                div()
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .bg(theme.muted)
                    .font_family(font_family)
                    .text_xs()
                    .font_semibold()
                    .text_color(theme.foreground)
                    .child(format!("+{} B", self.header_offset)),
            )
            .child(inc_btn);

        h_flex()
            .justify_between()
            .items_center()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_1p5()
                    .child(Icon::new(IconName::SlidersHorizontal).size(px(13.0)).text_color(muted_color))
                    .child(div().text_xs().font_medium().text_color(muted_color).child("Offset")),
            )
            .child(right_flex)
            .into_any_element()
    }

    pub(crate) fn render_width_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> AnyElement {
        let muted_color = theme.muted_foreground;
        let font_family = cx.global::<Appearance>().font_family.clone();
        let is_at_min = self.cols <= self.min_cols();
        let is_at_max = self.cols >= 4096;

        let mut dec_btn = Button::new("dec_w").label("-").ghost().with_size(Size::XSmall).tooltip("Decrease width");
        if is_at_min {
            dec_btn = dec_btn.disabled(true);
        } else {
            dec_btn = dec_btn
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.start_width_repeat(false, cx);
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| {
                        this.stop_width_repeat();
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| {
                        this.stop_width_repeat();
                    }),
                );
        }

        let mut inc_btn = Button::new("inc_w").label("+").ghost().with_size(Size::XSmall).tooltip("Increase width");
        if is_at_max {
            inc_btn = inc_btn.disabled(true);
        } else {
            inc_btn = inc_btn
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.start_width_repeat(true, cx);
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| {
                        this.stop_width_repeat();
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _, _, _| {
                        this.stop_width_repeat();
                    }),
                );
        }

        h_flex()
            .justify_between()
            .items_center()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_1p5()
                    .child(Icon::new(IconName::SlidersHorizontal).size(px(13.0)).text_color(muted_color))
                    .child(div().text_xs().font_medium().text_color(muted_color).child("Width")),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(dec_btn)
                    .child(
                        div()
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(theme.muted)
                            .font_family(font_family)
                            .text_xs()
                            .font_semibold()
                            .text_color(theme.foreground)
                            .child(if self.color_mode == ColorMode::Planar4bpp {
                                let tiles = (self.cols / 8).max(1);
                                format!("{} px ({} tiles, {} B)", self.cols, tiles, tiles * 32)
                            } else if self.color_mode.is_rgb() {
                                let bpp = self.color_mode.bytes_per_pixel();
                                format!("{} px ({} B)", self.cols, self.cols * bpp)
                            } else if self.color_mode.is_sub_byte() {
                                let ppb = self.color_mode.pixels_per_byte();
                                format!("{} px ({} B)", self.cols, self.cols.div_ceil(ppb))
                            } else {
                                format!("{} B", self.cols)
                            }),
                    )
                    .child(inc_btn),
            )
            .into_any_element()
    }

    pub(crate) fn render_scale_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> AnyElement {
        let muted_color = theme.muted_foreground;
        let pixel_button = |preset: usize, label: &'static str, cx: &mut Context<Self>| {
            let is_selected = self.pixel_size == preset;
            let mut btn = Button::new(("p_preset", preset)).label(label).with_size(Size::XSmall);
            if is_selected {
                btn = btn.primary();
            } else {
                btn = btn.ghost();
            }
            btn.on_click(cx.listener(move |this, _, _, cx| {
                this.pixel_size = preset;
                this.update_scrollbar(cx);
                this.cached_image.borrow_mut().take();
                cx.notify();
            }))
        };

        h_flex()
            .justify_between()
            .items_center()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_1p5()
                    .child(Icon::new(IconName::Grid2x2).size(px(13.0)).text_color(muted_color))
                    .child(div().text_xs().font_medium().text_color(muted_color).child("Scale")),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .justify_end()
                    .gap_1()
                    .child(pixel_button(1, "x1", cx))
                    .child(pixel_button(2, "x2", cx))
                    .child(pixel_button(4, "x4", cx))
                    .child(pixel_button(8, "x8", cx)),
            )
            .into_any_element()
    }

    pub(crate) fn render_palette_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> AnyElement {
        let muted_color = theme.muted_foreground;
        let current_mode = self.color_mode;
        let panel_weak = cx.entity().downgrade();

        let dropdown = Button::new("visual_map_palette_dropdown")
            .label(current_mode.label())
            .outline()
            .dropdown_caret(true)
            .with_size(Size::XSmall)
            .tooltip("Select color palette mode")
            .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                let add_modes = |menu: gpui_kit::component::menu::PopupMenu, modes: &[ColorMode]| {
                    modes.iter().copied().fold(menu, |menu, mode| {
                        let panel_weak = panel_weak.clone();
                        menu.item(PopupMenuItem::new(mode.label()).checked(mode == current_mode).on_click(move |_, _, cx| {
                            if let Some(panel) = panel_weak.upgrade() {
                                panel.update(cx, |this, cx| {
                                    this.set_color_mode(mode, cx);
                                });
                            }
                        }))
                    })
                };

                let menu = menu.label("Analysis");
                let menu = add_modes(menu, &[ColorMode::DataCategory, ColorMode::Grayscale, ColorMode::Rainbow, ColorMode::Entropy]);
                let menu = menu.separator().label("Indexed / Bitplane");
                let menu = add_modes(
                    menu,
                    &[
                        ColorMode::Mono1bpp,
                        ColorMode::Indexed2bpp,
                        ColorMode::Indexed4bpp,
                        ColorMode::Planar4bpp,
                        ColorMode::Vga256,
                    ],
                );
                let menu = menu.separator().label("RGB / RGBA");
                add_modes(
                    menu,
                    &[
                        ColorMode::Rgb565,
                        ColorMode::Rgb555,
                        ColorMode::Rgb888,
                        ColorMode::Bgr888,
                        ColorMode::Rgba,
                        ColorMode::Argb,
                        ColorMode::Bgra,
                    ],
                )
            });

        h_flex()
            .justify_between()
            .items_center()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_1p5()
                    .child(Icon::new(IconName::Palette).size(px(13.0)).text_color(muted_color))
                    .child(div().text_xs().font_medium().text_color(muted_color).child("Palette")),
            )
            .child(dropdown)
            .into_any_element()
    }

    pub(crate) fn render_entropy_window_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> AnyElement {
        let muted_color = theme.muted_foreground;
        let window_button = |preset: usize, label: &'static str, cx: &mut Context<Self>| {
            let is_selected = self.entropy_window == preset;
            let mut btn = Button::new(("w_preset", preset)).label(label).with_size(Size::XSmall);
            if is_selected {
                btn = btn.primary();
            } else {
                btn = btn.ghost();
            }
            btn.on_click(cx.listener(move |this, _, _, cx| {
                this.entropy_window = preset;
                this.cached_image.borrow_mut().take();
                cx.notify();
            }))
        };

        h_flex()
            .justify_between()
            .items_center()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_1p5()
                    .child(Icon::new(IconName::SlidersHorizontal).size(px(13.0)).text_color(muted_color))
                    .child(div().text_xs().font_medium().text_color(muted_color).child("Window")),
            )
            .child(
                h_flex()
                    .flex_wrap()
                    .justify_end()
                    .gap_1()
                    .child(window_button(64, "64B", cx))
                    .child(window_button(128, "128B", cx))
                    .child(window_button(256, "256B", cx))
                    .child(window_button(512, "512B", cx))
                    .child(window_button(1024, "1K", cx)),
            )
            .into_any_element()
    }

    pub(crate) fn render_endian_section(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> AnyElement {
        let muted_color = theme.muted_foreground;
        let is_big_endian = self.is_big_endian;
        let endian_button = |be: bool, label: &'static str, id_str: &'static str, cx: &mut Context<Self>| {
            let is_selected = is_big_endian == be;
            let mut btn = Button::new(id_str).label(label).with_size(Size::XSmall);
            if is_selected {
                btn = btn.primary();
            } else {
                btn = btn.ghost();
            }
            btn.on_click(cx.listener(move |this, _, _, cx| {
                if this.is_big_endian != be {
                    this.is_big_endian = be;
                    this.cached_image.borrow_mut().take();
                    cx.notify();
                }
            }))
        };

        h_flex()
            .justify_between()
            .items_center()
            .gap_2()
            .child(
                h_flex()
                    .items_center()
                    .gap_1p5()
                    .child(Icon::new(IconName::SlidersHorizontal).size(px(13.0)).text_color(muted_color))
                    .child(div().text_xs().font_medium().text_color(muted_color).child("Endian")),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(endian_button(false, "LE", "vm_endian_le", cx))
                    .child(endian_button(true, "BE", "vm_endian_be", cx)),
            )
            .into_any_element()
    }

    pub(crate) fn render_toolbar(&self, theme: &gpui_kit::component::Theme, cx: &mut Context<Self>) -> AnyElement {
        let mut toolbar = v_flex()
            .p_2()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
            .child(self.render_width_section(theme, cx))
            .child(self.render_offset_section(theme, cx))
            .child(self.render_scale_section(theme, cx))
            .child(self.render_palette_section(theme, cx));

        if self.color_mode == ColorMode::Entropy {
            toolbar = toolbar.child(self.render_entropy_window_section(theme, cx));
        } else if self.color_mode.is_rgb_16() {
            toolbar = toolbar.child(self.render_endian_section(theme, cx));
        }

        toolbar.into_any_element()
    }
}
