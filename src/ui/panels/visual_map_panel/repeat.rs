//! Asynchronous auto-repeat action scheduler for increment/decrement buttons.

use gpui_kit::prelude::*;
use gpui_kit::*;
use std::time::Duration;

pub const REPEAT_INITIAL_DELAY: Duration = Duration::from_millis(350);
pub const REPEAT_MIN_INTERVAL: Duration = Duration::from_millis(15);
pub const REPEAT_MED_INTERVAL: Duration = Duration::from_millis(30);
pub const REPEAT_BASE_INTERVAL: Duration = Duration::from_millis(50);

/// Spawns an accelerated repeat loop that invokes `action` repeatedly while it returns `true`.
pub fn spawn_repeat_action<T: 'static, F>(cx: &mut Context<T>, mut action: F) -> Task<()>
where
    F: FnMut(&mut T, &mut Context<T>) -> bool + 'static,
{
    cx.spawn(async move |this, cx| {
        tokio::time::sleep(REPEAT_INITIAL_DELAY).await;
        let mut count = 0;
        loop {
            let interval = if count < 10 {
                REPEAT_BASE_INTERVAL
            } else if count < 30 {
                REPEAT_MED_INTERVAL
            } else {
                REPEAT_MIN_INTERVAL
            };

            let should_continue = this.update(cx, |this, cx| action(this, cx)).unwrap_or(false);

            if !should_continue {
                break;
            }

            count += 1;
            tokio::time::sleep(interval).await;
        }
    })
}
