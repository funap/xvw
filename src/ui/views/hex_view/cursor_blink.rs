use gpui_kit::{Context, Task};
use std::time::Duration;

const HEX_CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const HEX_CURSOR_PAUSE_DELAY: Duration = Duration::from_millis(300);

/// Keeps the Insert Mode cursor in sync with the blinking behavior of
/// `gpui-kit`'s text inputs.
pub(super) struct HexCursorBlink {
    visible: bool,
    paused: bool,
    active: bool,
    epoch: usize,
    _task: Task<()>,
}

impl HexCursorBlink {
    pub(super) fn new() -> Self {
        Self {
            visible: false,
            paused: false,
            active: false,
            epoch: 0,
            _task: Task::ready(()),
        }
    }

    pub(super) fn start(&mut self, cx: &mut Context<Self>) {
        if self.active && !self.paused {
            return;
        }
        self.active = true;
        self.paused = false;
        self.visible = false;
        let epoch = self.next_epoch();
        self.blink(epoch, cx);
    }

    pub(super) fn stop(&mut self, cx: &mut Context<Self>) {
        if !self.active && !self.paused {
            return;
        }
        self.active = false;
        self.paused = false;
        self.visible = false;
        self.next_epoch();
        self._task = Task::ready(());
        cx.notify();
    }

    fn next_epoch(&mut self) -> usize {
        self.epoch = self.epoch.wrapping_add(1);
        self.epoch
    }

    fn blink(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if !self.active || self.paused || epoch != self.epoch {
            if self.paused {
                self.visible = true;
            }
            return;
        }

        self.visible = !self.visible;
        cx.notify();

        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            tokio::time::sleep(HEX_CURSOR_BLINK_INTERVAL).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| this.blink(epoch, cx));
            }
        });
    }

    pub(super) fn visible(&self) -> bool {
        self.paused || self.visible
    }

    pub(super) fn pause(&mut self, cx: &mut Context<Self>) {
        self.active = true;
        self.paused = true;
        self.visible = true;
        cx.notify();

        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            tokio::time::sleep(HEX_CURSOR_PAUSE_DELAY).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.active && epoch == this.epoch {
                        this.paused = false;
                        this.blink(epoch, cx);
                    }
                });
            }
        });
    }
}
