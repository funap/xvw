use crate::ui::icon::IconName;
use gpui_kit::component::{Icon, StyledExt as _, h_flex, theme::Theme, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{Animation, AnimationExt as _, AnyElement, Div, ElementId, Hsla, IntoElement, ParentElement, SharedString, Styled, div, px};
use std::time::Duration;

/// Duration for the transient header highlight flash when focus moves to a panel or column.
pub const FOCUS_FLASH_DURATION: Duration = Duration::from_millis(800);

/// Maps normalized focus-flash animation progress (`0.0..=1.0`) to highlight opacity (`1.0..=0.0`).
/// Holds briefly at full opacity so the focus move registers clearly, then smoothly fades out.
#[inline]
pub fn focus_flash_alpha(progress: f32) -> f32 {
    let p = progress.clamp(0.0, 1.0);
    const HOLD_PORTION: f32 = 0.2;
    if p <= HOLD_PORTION {
        1.0
    } else {
        let fade_t = (p - HOLD_PORTION) / (1.0 - HOLD_PORTION);
        let remaining = 1.0 - fade_t;
        remaining * remaining
    }
}

/// Creates an absolute header overlay that briefly highlights the title area and its bottom bar
/// when focus arrives, then smoothly fades out to transparent.
pub fn header_focus_flash_overlay(id: impl Into<ElementId>, primary: Hsla) -> impl IntoElement {
    div()
        .absolute()
        .inset_0()
        .with_animation(id, Animation::new(FOCUS_FLASH_DURATION), move |el, progress| {
            let alpha = focus_flash_alpha(progress);
            el.bg(primary.opacity(alpha * 0.10))
                .child(div().absolute().left_0().right_0().bottom_0().h(px(2.0)).bg(primary.opacity(alpha)))
        })
}

/// Returns the header text color based on the focus state.
/// When focused, it returns `theme.foreground`. When not focused, it returns `theme.muted_foreground`.
pub fn header_text_color(focused: bool, theme: &Theme) -> Hsla {
    if focused { theme.foreground } else { theme.muted_foreground }
}

/// Creates a standardized panel container div with sizing and background.
pub fn panel_container(_is_focused: bool, theme: &Theme) -> Div {
    v_flex().size_full().min_w_0().min_h_0().overflow_hidden().bg(theme.sidebar)
}

/// Creates a standardized panel header toolbar with fixed height, border, focus flash indicator, and truncating title.
pub fn panel_header(title: impl Into<SharedString>, is_focused: bool, theme: &Theme, badge: Option<AnyElement>, actions: Option<AnyElement>) -> Div {
    let mut title_part = h_flex().flex_1().min_w_0().items_center().gap_2().child(
        div()
            .min_w_0()
            .truncate()
            .whitespace_nowrap()
            .text_xs()
            .font_semibold()
            .text_color(header_text_color(is_focused, theme))
            .child(title.into()),
    );

    if let Some(b) = badge {
        title_part = title_part.child(div().flex_shrink_0().child(b));
    }

    let mut header = h_flex()
        .relative()
        .justify_between()
        .items_center()
        .gap_2()
        .h(px(34.0))
        .flex_shrink_0()
        .px_3()
        .border_b_1()
        .border_color(theme.border)
        .bg(theme.sidebar)
        .when(is_focused, |el| el.child(header_focus_flash_overlay("panel-focus-flash", theme.primary)))
        .child(title_part);

    if let Some(act) = actions {
        header = header.child(div().flex_shrink_0().child(act));
    }

    header
}

/// Creates a standardized count/status badge for panel headers.
pub fn panel_badge(count_or_text: impl Into<SharedString>, theme: &Theme) -> Div {
    div()
        .px_1p5()
        .py_0p5()
        .rounded_sm()
        .bg(theme.secondary)
        .whitespace_nowrap()
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
        .child(Icon::new(icon).size(px(28.0)).text_color(theme.muted_foreground))
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

#[cfg(test)]
mod tests {
    use super::focus_flash_alpha;

    #[test]
    fn test_focus_flash_alpha_holds_then_fades_smoothly() {
        assert_eq!(focus_flash_alpha(-0.1), 1.0);
        assert_eq!(focus_flash_alpha(0.0), 1.0);
        assert_eq!(focus_flash_alpha(0.2), 1.0);
        let mid = focus_flash_alpha(0.6);
        assert!(mid > 0.0 && mid < 1.0);
        assert!(focus_flash_alpha(0.8) < mid);
        assert_eq!(focus_flash_alpha(1.0), 0.0);
        assert_eq!(focus_flash_alpha(1.5), 0.0);
    }
}
