use crate::core::appearance::Appearance;
use crate::core::encoding::Encoding;
use crate::core::layout::{BytesPerRow, MAX_BYTES_PER_ROW, MIN_BYTES_PER_ROW};
use gpui_kit::component::{
    ActiveTheme, Sizable as _, Size, StyledExt,
    button::Button,
    dock::{Panel, PanelEvent},
    input::{self, Input, InputState, NumberInput},
    menu::{DropdownMenu as _, PopupMenuItem},
    theme::Theme,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    Action, Anchor, App, Context, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, ParentElement, Render, SharedString, Subscription, Window, div,
};

#[derive(Clone, PartialEq, Action)]
pub struct UpdateSettingInput;

pub struct SettingsPanel {
    focus_handle: FocusHandle,
    font_family_input: Entity<InputState>,
    font_size_input: Entity<InputState>,
    bytes_per_row_input: Entity<InputState>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        let font_family_input = cx.new(|cx| InputState::new(window, cx));
        let font_size_input = cx.new(|cx| InputState::new(window, cx));
        let bytes_per_row_input = cx.new(|cx| {
            InputState::new(window, cx)
                .step(1.0)
                .min(MIN_BYTES_PER_ROW as f64)
                .max(MAX_BYTES_PER_ROW as f64)
        });

        // The global must be retrieved after the entities have been created.
        let (family, size, bytes_per_row) = {
            let appearance = cx.global::<Appearance>();
            let bytes_per_row = cx.global::<BytesPerRow>().0;
            (appearance.font_family.clone(), appearance.font_size.to_string(), bytes_per_row.to_string())
        };

        // Set initial values
        font_family_input.update(cx, |input: &mut InputState, cx| {
            input.set_value(family, window, cx);
        });

        font_size_input.update(cx, |input: &mut InputState, cx| {
            input.set_value(size, window, cx);
        });

        bytes_per_row_input.update(cx, |input: &mut InputState, cx| {
            input.set_value(bytes_per_row, window, cx);
        });

        let mut subscriptions = Vec::new();

        subscriptions.push(cx.observe_global::<Appearance>(|_, cx| {
            cx.dispatch_action(&UpdateSettingInput);
        }));

        subscriptions.push(cx.observe_global::<BytesPerRow>(|_, cx| {
            cx.dispatch_action(&UpdateSettingInput);
        }));

        subscriptions.push(cx.observe_global::<Theme>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<Encoding>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.subscribe(&font_family_input, |_, input: Entity<InputState>, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let value = input.read(cx).value().to_string();
                cx.update_global::<Appearance, _>(|appearance, _| {
                    appearance.font_family = value;
                });
                crate::settings::save_current(cx);
            }
        }));

        subscriptions.push(cx.subscribe(&font_size_input, |_, input: Entity<InputState>, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let value = input.read(cx).value().to_string();
                if let Ok(size) = value.parse::<f32>() {
                    cx.update_global::<Appearance, _>(|appearance, _| {
                        appearance.font_size = size;
                    });
                    crate::settings::save_current(cx);
                }
            }
        }));

        subscriptions.push(
            cx.subscribe(&bytes_per_row_input, |_, input: Entity<InputState>, event: &input::InputEvent, cx| {
                if let input::InputEvent::Change = event {
                    let value = input.read(cx).value().to_string();
                    if let Ok(bpr) = value.parse::<usize>()
                        && (MIN_BYTES_PER_ROW..=MAX_BYTES_PER_ROW).contains(&bpr)
                    {
                        cx.update_global::<BytesPerRow, _>(|bytes_per_row, _| {
                            bytes_per_row.0 = bpr;
                        });
                        crate::settings::save_current(cx);
                    }
                }
            }),
        );

        Self {
            focus_handle,
            font_family_input,
            font_size_input,
            bytes_per_row_input,
            _subscriptions: subscriptions,
        }
    }

    fn on_action_update_setting_input(&mut self, _action: &UpdateSettingInput, window: &mut Window, cx: &mut Context<Self>) {
        let appearance = cx.global::<Appearance>();
        let family = appearance.font_family.clone();
        let size_str = appearance.font_size.to_string();
        let bytes_per_row_str = cx.global::<BytesPerRow>().0.to_string();

        self.font_family_input.update(cx, |input, cx| {
            if input.value() != family.as_str() {
                input.set_value(family, window, cx);
            }
        });
        self.font_size_input.update(cx, |input, cx| {
            if input.value() != size_str.as_str() {
                input.set_value(size_str, window, cx);
            }
        });
        self.bytes_per_row_input.update(cx, |input, cx| {
            if input.value() != bytes_per_row_str.as_str() {
                input.set_value(bytes_per_row_str, window, cx);
            }
        });
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let container = div().p_4().flex().flex_col().gap_6();

        container
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_update_setting_input))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().font_semibold().text_color(cx.theme().muted_foreground).child("Appearance"))
                    .child({
                        let active_theme_name = cx.theme().theme_name().clone();
                        let all_themes = crate::theme::all_theme_names(cx);

                        div().flex().items_center().gap_4().child(div().w_24().child("Theme")).child(
                            div().w_48().child(
                                Button::new("theme-selection")
                                    .label(active_theme_name.clone())
                                    .outline()
                                    .dropdown_caret(true)
                                    .with_size(Size::Small)
                                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                                        all_themes.iter().fold(menu, |menu, theme_name| {
                                            let is_active = theme_name == &active_theme_name;
                                            let name = theme_name.clone();
                                            menu.item(PopupMenuItem::new(name.clone()).checked(is_active).on_click(move |_, window, cx| {
                                                crate::theme::apply_theme_by_name(&name, Some(window), cx);
                                                crate::settings::save_current(cx);
                                            }))
                                        })
                                    }),
                            ),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().font_semibold().text_color(cx.theme().muted_foreground).child("Editor"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child(div().w_32().child("Font Family"))
                            .child(div().w_48().child(Input::new(&self.font_family_input))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child(div().w_32().child("Font Size"))
                            .child(div().w_48().child(Input::new(&self.font_size_input))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_4()
                            .child(div().w_32().child("Bytes Per Row"))
                            .child(div().w_48().child(NumberInput::new(&self.bytes_per_row_input))),
                    )
                    .child({
                        let default_encoding = *cx.global::<Encoding>();

                        div().flex().items_center().gap_4().child(div().w_32().child("Default Encoding")).child(
                            div().w_48().child(
                                Button::new("default-encoding")
                                    .label(default_encoding.label())
                                    .outline()
                                    .dropdown_caret(true)
                                    .with_size(Size::Small)
                                    .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| {
                                        Encoding::categories().iter().fold(menu, |menu, (cat, encs)| {
                                            menu.submenu(cat.label(), window, cx, move |menu, _window, _cx| {
                                                encs.iter().copied().fold(menu, |menu, encoding| {
                                                    menu.item(PopupMenuItem::new(encoding.label()).checked(encoding == default_encoding).on_click(
                                                        move |_, _, cx| {
                                                            cx.update_global::<Encoding, _>(|current, _| {
                                                                *current = encoding;
                                                            });
                                                            crate::settings::save_current(cx);
                                                        },
                                                    ))
                                                })
                                            })
                                        })
                                    }),
                            ),
                        )
                    }),
            )
    }
}

impl EventEmitter<PanelEvent> for SettingsPanel {}
impl Focusable for SettingsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
impl Panel for SettingsPanel {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Settings"
    }
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some("Settings".into())
    }
    fn zoom_control(&self, _: &App) -> Option<gpui_kit::component::dock::PanelControl> {
        None
    }
}

impl gpui_kit::base::dock::Panel for SettingsPanel {
    fn panel_name(&self) -> &'static str {
        "SettingsPanel"
    }
    fn closable(&self, _: &App) -> bool {
        true
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
    fn visible(&self, _: &App) -> bool {
        true
    }
    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if active {
            self.focus_handle.focus(window, cx);
        }
    }
    fn set_zoomed(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) {}
}
