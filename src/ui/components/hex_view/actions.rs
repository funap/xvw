use super::types::CONTEXT;
use crate::actions::{
    AddCustomBreak, ClearAllCustomBreaks, Copy, CopyAsHexDump, Cut, GoToBeginning, GoToEnd, JoinLine, Paste, Redo, SearchNext, SearchPrev, ToggleGoToAddress,
    ToggleSearch, Undo, UnfoldBookmarkAtCursor,
};
use gpui_kit::*;

actions!(
    hex_view,
    [
        MoveLeft,
        MoveRight,
        MoveUp,
        MoveDown,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        PageUp,
        PageDown,
        Home,
        End,
        SelectPageUp,
        SelectPageDown,
        SelectHome,
        SelectEnd,
        TriggerSearch,
        TriggerSearchNext,
        TriggerSearchPrev,
        ViMoveLeft,
        ViMoveRight,
        ViMoveUp,
        ViMoveDown,
        ViSelectLeft,
        ViSelectRight,
        ViSelectUp,
        ViSelectDown
    ]
);

pub fn init(cx: &mut App) {
    cx.bind_keys([
        // Navigation keys
        KeyBinding::new("left", MoveLeft, Some(CONTEXT)),
        KeyBinding::new("right", MoveRight, Some(CONTEXT)),
        KeyBinding::new("up", MoveUp, Some(CONTEXT)),
        KeyBinding::new("down", MoveDown, Some(CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(CONTEXT)),
        KeyBinding::new("shift-up", SelectUp, Some(CONTEXT)),
        KeyBinding::new("shift-down", SelectDown, Some(CONTEXT)),
        KeyBinding::new("pageup", PageUp, Some(CONTEXT)),
        KeyBinding::new("pagedown", PageDown, Some(CONTEXT)),
        KeyBinding::new("home", Home, Some(CONTEXT)),
        KeyBinding::new("end", End, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-home", GoToBeginning, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-home", GoToBeginning, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-end", GoToEnd, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-end", GoToEnd, Some(CONTEXT)),
        KeyBinding::new("shift-pageup", SelectPageUp, Some(CONTEXT)),
        KeyBinding::new("shift-pagedown", SelectPageDown, Some(CONTEXT)),
        KeyBinding::new("shift-home", SelectHome, Some(CONTEXT)),
        KeyBinding::new("shift-end", SelectEnd, Some(CONTEXT)),
        // Clipboard & Selection
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-a", SelectAll, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-a", SelectAll, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", Copy, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", Copy, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-c", CopyAsHexDump, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-c", CopyAsHexDump, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-x", Cut, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-x", Cut, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-v", Paste, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-v", Paste, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-z", Undo, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-z", Undo, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-z", Redo, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-z", Redo, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-y", Redo, Some(CONTEXT)),
        // Vi-like navigation remains available in the Hex column without
        // stealing printable characters from the ASCII column.
        KeyBinding::new("h", ViMoveLeft, Some(CONTEXT)),
        KeyBinding::new("l", ViMoveRight, Some(CONTEXT)),
        KeyBinding::new("k", ViMoveUp, Some(CONTEXT)),
        KeyBinding::new("j", ViMoveDown, Some(CONTEXT)),
        KeyBinding::new("shift-h", ViSelectLeft, Some(CONTEXT)),
        KeyBinding::new("shift-l", ViSelectRight, Some(CONTEXT)),
        KeyBinding::new("shift-k", ViSelectUp, Some(CONTEXT)),
        KeyBinding::new("shift-j", ViSelectDown, Some(CONTEXT)),
        // Vi-like search commands
        KeyBinding::new("/", TriggerSearch, Some(CONTEXT)),
        KeyBinding::new("n", TriggerSearchNext, Some(CONTEXT)),
        KeyBinding::new("shift-n", TriggerSearchPrev, Some(CONTEXT)),
        // Standard search shortcuts
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-f", ToggleSearch, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-f", ToggleSearch, Some(CONTEXT)),
        KeyBinding::new("f3", SearchNext, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-g", SearchNext, Some(CONTEXT)),
        KeyBinding::new("shift-f3", SearchPrev, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-g", SearchPrev, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-g", SearchPrev, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-l", ToggleGoToAddress, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-l", ToggleGoToAddress, Some(CONTEXT)),
        KeyBinding::new("ctrl-g", ToggleGoToAddress, Some(CONTEXT)),
        // Custom line breaks & joins
        KeyBinding::new("enter", AddCustomBreak, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-j", JoinLine, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-j", JoinLine, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-backspace", ClearAllCustomBreaks, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-backspace", ClearAllCustomBreaks, Some(CONTEXT)),
        // Bookmark Visibility
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-]", UnfoldBookmarkAtCursor, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-]", UnfoldBookmarkAtCursor, Some(CONTEXT)),
    ]);
}
