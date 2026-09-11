use crate::ui::icon::IconName;
use gpui_kit::component::{Icon, StyledExt as _, h_flex, theme::Theme, v_flex};
use gpui_kit::{AnyElement, Div, Hsla, ParentElement, SharedString, Styled, div, px};

/// Returns the header text color based on the focus state.
/// When focused, it returns `theme.foreground`. When not focused, it returns `theme.muted_foreground`.
pub fn header_text_color(focused: bool, theme: &Theme) -> Hsla {
    if focused { theme.foreground } else { theme.muted_foreground }
}

/// Creates a standardized panel container div with sizing and background.
pub fn panel_container(_is_focused: bool, theme: &Theme) -> Div {
    v_flex().size_full().min_w_0().min_h_0().overflow_hidden().bg(theme.sidebar)
}

/// Creates a standardized panel header toolbar with fixed height, border, and uppercase title.
pub fn panel_header(title: impl Into<SharedString>, is_focused: bool, theme: &Theme, badge: Option<AnyElement>, actions: Option<AnyElement>) -> Div {
    let mut title_part = h_flex().items_center().gap_2().child(
        div()
            .text_xs()
            .font_semibold()
            .text_color(header_text_color(is_focused, theme))
            .child(title.into()),
    );

    if let Some(b) = badge {
        title_part = title_part.child(b);
    }

    let mut header = h_flex()
        .justify_between()
        .items_center()
        .h(px(34.0))
        .flex_shrink_0()
        .px_3()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.sidebar)
        .child(title_part);

    if let Some(act) = actions {
        header = header.child(h_flex().items_center().gap_1().child(act));
    }

    header
}

/// Creates a standardized count/status badge for panel headers.
pub fn panel_badge(count_or_text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .px_1p5()
        .py_0p5()
        .rounded_sm()
        .bg(theme.muted.opacity(0.6))
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(count_or_text.into())
}

/// Creates a standardized empty / blank state layout for panels.
pub fn panel_empty_state(
    icon: IconName,
    title: impl Into<SharedString>,
    description: Option<impl Into<SharedString>>,
    action: Option<AnyElement>,
    theme: &Theme,
) -> Div {
    let mut container = v_flex()
        .size_full()
        .pt_10()
        .items_center()
        .px_4()
        .gap_2p5()
        .child(Icon::new(icon).size(px(28.0)).text_color(theme.muted_foreground.opacity(0.4)))
        .child(div().text_xs().font_medium().text_color(theme.foreground).child(title.into()));

    if let Some(desc) = description {
        container = container.child(div().text_xs().text_center().text_color(theme.muted_foreground).child(desc.into()));
    }

    if let Some(act) = action {
        container = container.child(div().mt_2().w_full().child(act));
    }

    container
}

/// Creates a standardized section header inside panel bodies.
pub fn panel_section_header(label: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .mt_3()
        .mb_1()
        .px_3()
        .text_xs()
        .font_semibold()
        .text_color(theme.muted_foreground)
        .child(label.into())
}
