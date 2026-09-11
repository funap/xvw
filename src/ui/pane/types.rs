use gpui_kit::*;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::core::document::Document;
use crate::core::editor::Editor;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabDrag {
    pub from_group_id: usize,
    pub tab_id: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SplitDirection {
    Horizontal,
    Vertical,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropPlacement {
    Left,
    Right,
    Top,
    Bottom,
    Center,
}

/// Trait representing a polymorphic tab panel in the workspace pane system.
pub trait WorkspaceTab: 'static {
    /// Returns reference to Any for downcasting.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Returns the tab title for display in the tab bar.
    fn title(&self, cx: &App) -> String;

    /// Returns the focus handle for this tab view.
    fn focus_handle(&self, cx: &App) -> FocusHandle;

    /// Returns whether the tab content has unsaved modifications.
    fn is_dirty(&self, cx: &App) -> bool {
        let _ = cx;
        false
    }

    /// Returns whether the tab content is read-only.
    fn is_read_only(&self, cx: &App) -> bool {
        let _ = cx;
        false
    }

    /// Returns the file path associated with this tab, if any.
    fn path(&self, cx: &App) -> Option<PathBuf> {
        let _ = cx;
        None
    }

    /// Renders the tab panel content.
    fn render(&self) -> AnyElement;

    /// Returns the underlying Editor entity if this tab hosts an editor.
    fn editor(&self, cx: &App) -> Option<Entity<Editor>> {
        let _ = cx;
        None
    }

    /// Returns the underlying Document if this tab hosts a document.
    fn document(&self, cx: &App) -> Option<Arc<RwLock<Document>>> {
        let _ = cx;
        None
    }

    /// Creates a clone of this tab content suitable for a split pane.
    fn create_split(&self, window: &mut Window, cx: &mut App) -> Option<TabContent> {
        let _ = (window, cx);
        None
    }
}

/// Polymorphic container holding any tab content that implements `WorkspaceTab`.
#[derive(Clone)]
pub struct TabContent(Arc<dyn WorkspaceTab>);

impl TabContent {
    /// Creates a new `TabContent` wrapping any `WorkspaceTab`.
    pub fn new(tab: impl WorkspaceTab) -> Self {
        Self(Arc::new(tab))
    }

    /// Downcasts the inner tab to a concrete type if it matches.
    pub fn downcast<T: Clone + 'static>(&self) -> Option<T> {
        self.0.as_any().downcast_ref::<T>().cloned()
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.focus_handle(cx)
    }

    pub fn editor(&self, cx: &App) -> Option<Entity<Editor>> {
        self.0.editor(cx)
    }

    pub fn document(&self, cx: &App) -> Option<Arc<RwLock<Document>>> {
        self.0.document(cx)
    }

    pub fn path(&self, cx: &App) -> Option<PathBuf> {
        self.0.path(cx)
    }

    pub fn title(&self, cx: &App) -> String {
        self.0.title(cx)
    }

    pub fn is_dirty(&self, cx: &App) -> bool {
        self.0.is_dirty(cx)
    }

    pub fn is_read_only(&self, cx: &App) -> bool {
        self.0.is_read_only(cx)
    }

    pub fn render(&self) -> AnyElement {
        self.0.render()
    }

    pub fn create_split(&self, window: &mut Window, cx: &mut App) -> Option<TabContent> {
        self.0.create_split(window, cx)
    }
}

#[derive(Clone)]
pub struct TabItem {
    pub id: usize,
    pub content: TabContent,
}

impl TabItem {
    pub fn new(id: usize, content: TabContent) -> Self {
        Self { id, content }
    }

    pub fn title(&self, cx: &App) -> String {
        self.content.title(cx)
    }

    pub fn path(&self, cx: &App) -> Option<PathBuf> {
        self.content.path(cx)
    }

    pub fn is_dirty(&self, cx: &App) -> bool {
        self.content.is_dirty(cx)
    }

    pub fn is_read_only(&self, cx: &App) -> bool {
        self.content.is_read_only(cx)
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.content.focus_handle(cx)
    }
}
