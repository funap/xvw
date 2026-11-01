use gpui_kit::{InteractiveElement, MouseButton};

/// Extension trait for GPUI interactive elements to block mouse clicks and scroll
/// events from bleeding through to underlying views (e.g. in modals, popups, and overlays).
pub trait EventInterceptionExt: InteractiveElement {
    /// Occludes underlying elements in hit testing and stops propagation for
    /// left-click, right-click, and scroll events.
    fn occlude_and_block_input(self) -> Self
    where
        Self: Sized,
    {
        self.occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_mouse_down(MouseButton::Right, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_scroll(|_, _, cx| {
                cx.stop_propagation();
            })
    }

    /// Stops propagation for scroll events only.
    fn block_scroll(self) -> Self
    where
        Self: Sized,
    {
        self.on_scroll(|_, _, cx| {
            cx.stop_propagation();
        })
    }
}

impl<E: InteractiveElement> EventInterceptionExt for E {}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::div;

    #[test]
    fn test_event_interception_trait_methods_compile() {
        let _element = div().occlude_and_block_input();
        let _element2 = div().block_scroll();
    }
}
