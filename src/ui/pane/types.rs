use gpui_kit::prelude::*;
use gpui_kit::*;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use crate::core::document::Document;
use crate::core::editor::Editor;
use crate::ui::panels::diff_panel::DiffPanel;
use crate::ui::panels::editor_panel::EditorPanel;
use crate::ui::panels::settings_panel::SettingsPanel;
use crate::ui::panels::visual_map_panel::VisualMapPanel;

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

    /// Returns the underlying EditorPanel entity if this tab hosts an editor panel.
    fn editor_panel(&self) -> Option<Entity<EditorPanel>> {
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

    /// Returns true if this tab represents a Settings panel.
    fn is_settings(&self) -> bool {
        false
    }
}

impl WorkspaceTab for Entity<EditorPanel> {
    fn title(&self, cx: &App) -> String {
        let path = self.read(cx).path(cx);
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.read(cx).editor().read(cx).document.read().map(|d| d.is_dirty()).unwrap_or(false)
    }

    fn is_read_only(&self, cx: &App) -> bool {
        self.read(cx).editor().read(cx).document.read().map(|d| d.is_read_only()).unwrap_or(false)
    }

    fn path(&self, cx: &App) -> Option<PathBuf> {
        Some(self.read(cx).path(cx))
    }

    fn render(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn editor(&self, cx: &App) -> Option<Entity<Editor>> {
        Some(self.read(cx).editor())
    }

    fn editor_panel(&self) -> Option<Entity<EditorPanel>> {
        Some(self.clone())
    }

    fn document(&self, cx: &App) -> Option<Arc<RwLock<Document>>> {
        Some(self.read(cx).editor().read(cx).document.clone())
    }

    fn create_split(&self, window: &mut Window, cx: &mut App) -> Option<TabContent> {
        let new_editor_panel = self.update(cx, |ep, cx| ep.create_split_clone(window, cx));
        Some(TabContent::new(new_editor_panel))
    }
}

impl WorkspaceTab for Entity<DiffPanel> {
    fn title(&self, cx: &App) -> String {
        let dp = self.read(cx);
        let left_name = dp
            .left_path()
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Left".to_string());
        let right_name = dp
            .right_path()
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Right".to_string());
        format!("Diff: {} ↔ {}", left_name, right_name)
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn render(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn create_split(&self, window: &mut Window, cx: &mut App) -> Option<TabContent> {
        let (left_doc, right_doc) = {
            let dp = self.read(cx);
            (dp.left_document.clone(), dp.right_document.clone())
        };
        let new_diff = cx.new(|cx| DiffPanel::new(left_doc, right_doc, window, cx));
        Some(TabContent::new(new_diff))
    }
}

impl WorkspaceTab for Entity<SettingsPanel> {
    fn title(&self, _cx: &App) -> String {
        "Settings".to_string()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn render(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn create_split(&self, window: &mut Window, cx: &mut App) -> Option<TabContent> {
        let new_settings = cx.new(|cx| SettingsPanel::new(window, cx));
        Some(TabContent::new(new_settings))
    }

    fn is_settings(&self) -> bool {
        true
    }
}

impl WorkspaceTab for Entity<VisualMapPanel> {
    fn title(&self, _cx: &App) -> String {
        "Visual Map".to_string()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn render(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn create_split(&self, _window: &mut Window, cx: &mut App) -> Option<TabContent> {
        let ed = self.read(cx).editor.clone();
        let new_vm = cx.new(|cx| VisualMapPanel::new(ed, cx));
        Some(TabContent::new(new_vm))
    }
}

/// Polymorphic container holding any tab content that implements `WorkspaceTab`.
#[derive(Clone)]
pub struct TabContent(Arc<dyn WorkspaceTab>);

#[allow(dead_code)]
impl TabContent {
    /// Creates a new `TabContent` wrapping any `WorkspaceTab`.
    pub fn new(tab: impl WorkspaceTab) -> Self {
        Self(Arc::new(tab))
    }

    /// Helper constructor for an editor panel.
    pub fn from_editor(panel: Entity<EditorPanel>) -> Self {
        Self::new(panel)
    }

    /// Helper constructor for a diff panel.
    pub fn from_diff(panel: Entity<DiffPanel>) -> Self {
        Self::new(panel)
    }

    /// Helper constructor for a settings panel.
    pub fn from_settings(panel: Entity<SettingsPanel>) -> Self {
        Self::new(panel)
    }

    /// Helper constructor for a visual map panel.
    pub fn from_visual_map(panel: Entity<VisualMapPanel>) -> Self {
        Self::new(panel)
    }

    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.focus_handle(cx)
    }

    pub fn editor(&self, cx: &App) -> Option<Entity<Editor>> {
        self.0.editor(cx)
    }

    pub fn editor_panel(&self) -> Option<Entity<EditorPanel>> {
        self.0.editor_panel()
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

    pub fn is_settings(&self) -> bool {
        self.0.is_settings()
    }
}

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
