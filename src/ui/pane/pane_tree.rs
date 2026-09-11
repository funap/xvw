use gpui_kit::component::ActiveTheme;
use gpui_kit::component::resizable::{h_resizable, resizable_panel, v_resizable};
use gpui_kit::prelude::*;
use gpui_kit::*;

use super::editor_group::{EditorGroup, EditorGroupEvent};
use super::types::{DropPlacement, SplitDirection, TabContent, TabDrag, TabItem};
use crate::core::editor::Editor;

pub enum PaneNode {
    Leaf { id: usize, group: Entity<EditorGroup> },
    HSplit { id: usize, left: usize, right: usize },
    VSplit { id: usize, top: usize, bottom: usize },
}

pub enum PaneTreeEvent {
    ActiveEditorChanged,
}

type PaneParent = (usize, bool);

struct PaneLocation {
    node_id: usize,
    parent: Option<PaneParent>,
    grandparent: Option<PaneParent>,
}

#[allow(dead_code)]
pub struct PaneTree {
    /// The pane tree is stored as an arena so replacing or dropping a layout
    /// never walks a recursive `Box<PaneNode>` chain on the UI stack.
    nodes: Vec<Option<PaneNode>>,
    root: Option<usize>,
    active_group_id: Option<usize>,
    next_group_id: usize,
    next_split_id: usize,
    next_tab_id: usize,
}

#[allow(dead_code)]
impl PaneTree {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: None,
            active_group_id: None,
            next_group_id: 1,
            next_split_id: 1,
            next_tab_id: 1,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.root.is_none()
    }

    pub fn active_group_id(&self) -> Option<usize> {
        self.active_group_id
    }

    pub fn active_group(&self, _cx: &App) -> Option<Entity<EditorGroup>> {
        let id = self.active_group_id?;
        self.find_group(id)
    }

    pub fn active_editor(&self, cx: &App) -> Option<Entity<Editor>> {
        self.active_group(cx)?.read(cx).active_editor(cx)
    }

    pub fn active_tab_as<T: Clone + 'static>(&self, cx: &App) -> Option<T> {
        self.active_group(cx)?.read(cx).active_tab_as::<T>()
    }

    pub fn find_group(&self, group_id: usize) -> Option<Entity<EditorGroup>> {
        let root = self.root?;
        let mut stack = vec![root];

        while let Some(node_id) = stack.pop() {
            let Some(node) = self.nodes.get(node_id).and_then(Option::as_ref) else {
                continue;
            };
            match node {
                PaneNode::Leaf { id, group } => {
                    if *id == group_id {
                        return Some(group.clone());
                    }
                }
                PaneNode::HSplit { left, right, .. } => {
                    // Push right first so the traversal keeps the original
                    // left-to-right search order without using call frames.
                    stack.push(*right);
                    stack.push(*left);
                }
                PaneNode::VSplit { top, bottom, .. } => {
                    stack.push(*bottom);
                    stack.push(*top);
                }
            }
        }

        None
    }

    pub fn all_groups(&self) -> Vec<Entity<EditorGroup>> {
        let mut groups = Vec::new();
        let Some(root) = self.root else {
            return groups;
        };

        let mut stack = vec![root];
        while let Some(node_id) = stack.pop() {
            let Some(node) = self.nodes.get(node_id).and_then(Option::as_ref) else {
                continue;
            };
            match node {
                PaneNode::Leaf { group, .. } => groups.push(group.clone()),
                PaneNode::HSplit { left, right, .. } => {
                    stack.push(*right);
                    stack.push(*left);
                }
                PaneNode::VSplit { top, bottom, .. } => {
                    stack.push(*bottom);
                    stack.push(*top);
                }
            }
        }

        groups
    }

    fn create_group(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<EditorGroup> {
        let id = self.next_group_id;
        self.next_group_id += 1;

        let group = cx.new(|cx| EditorGroup::new(id, cx));

        cx.subscribe_in(&group, window, move |this, _group, event: &EditorGroupEvent, window, cx| match event {
            EditorGroupEvent::Focused => {
                this.set_active_group(id, cx);
            }
            EditorGroupEvent::TabChanged => {
                this.sync_active_states(cx);
                cx.emit(PaneTreeEvent::ActiveEditorChanged);
                cx.notify();
            }
            EditorGroupEvent::Split { direction, new_content } => {
                this.split_group(id, *direction, new_content.clone(), window, cx);
            }
            EditorGroupEvent::CloseTab(_) => {
                this.sync_active_states(cx);
                cx.emit(PaneTreeEvent::ActiveEditorChanged);
                cx.notify();
            }
            EditorGroupEvent::CloseGroup => {
                this.remove_group(id, window, cx);
            }
            EditorGroupEvent::DropTab { drag, target_index } => {
                this.move_tab(*drag, id, *target_index, window, cx);
            }
            EditorGroupEvent::SplitWithDrop { drag, placement } => {
                this.split_with_drop(*drag, id, *placement, window, cx);
            }
        })
        .detach();

        group
    }

    fn insert_node(&mut self, node: PaneNode) -> usize {
        let node_id = self.nodes.len();
        self.nodes.push(Some(node));
        node_id
    }

    fn take_leaf(&mut self, node_id: usize, group_id: usize) -> Option<Entity<EditorGroup>> {
        let node = self.nodes.get_mut(node_id)?.take()?;
        match node {
            PaneNode::Leaf { id, group } if id == group_id => Some(group),
            node => {
                self.nodes[node_id] = Some(node);
                None
            }
        }
    }

    fn set_child(&mut self, parent_id: usize, is_first_child: bool, child_id: usize) -> bool {
        let Some(Some(parent)) = self.nodes.get_mut(parent_id) else {
            return false;
        };

        match parent {
            PaneNode::HSplit { left, right, .. } | PaneNode::VSplit { top: left, bottom: right, .. } => {
                if is_first_child {
                    *left = child_id;
                } else {
                    *right = child_id;
                }
                true
            }
            PaneNode::Leaf { .. } => false,
        }
    }

    fn sibling_of(&self, parent_id: usize, is_first_child: bool) -> Option<usize> {
        let parent = self.nodes.get(parent_id)?.as_ref()?;
        match parent {
            PaneNode::HSplit { left, right, .. } | PaneNode::VSplit { top: left, bottom: right, .. } => Some(if is_first_child { *right } else { *left }),
            PaneNode::Leaf { .. } => None,
        }
    }

    fn find_group_location(&self, group_id: usize) -> Option<PaneLocation> {
        struct Visit {
            node_id: usize,
            parent: Option<PaneParent>,
            grandparent: Option<PaneParent>,
        }

        let root = self.root?;
        let mut work = vec![Visit {
            node_id: root,
            parent: None,
            grandparent: None,
        }];

        while let Some(Visit { node_id, parent, grandparent }) = work.pop() {
            let Some(node) = self.nodes.get(node_id).and_then(Option::as_ref) else {
                continue;
            };
            match node {
                PaneNode::Leaf { id, .. } => {
                    if *id == group_id {
                        return Some(PaneLocation { node_id, parent, grandparent });
                    }
                }
                PaneNode::HSplit { left, right, .. } | PaneNode::VSplit { top: left, bottom: right, .. } => {
                    work.push(Visit {
                        node_id: *right,
                        parent: Some((node_id, false)),
                        grandparent: parent,
                    });
                    work.push(Visit {
                        node_id: *left,
                        parent: Some((node_id, true)),
                        grandparent: parent,
                    });
                }
            }
        }

        None
    }

    /// Replaces one leaf with a split without moving or rebuilding the rest of
    /// the arena. The existing group remains the first or second child and
    /// the new group occupies the other side.
    fn split_group_node(
        &mut self,
        target_group_id: usize,
        new_group_id: usize,
        new_group: &Entity<EditorGroup>,
        split_id: usize,
        horizontal: bool,
        new_group_is_first: bool,
    ) -> bool {
        let Some(PaneLocation { node_id: target_node_id, .. }) = self.find_group_location(target_group_id) else {
            return false;
        };
        let Some(target_group) = self.take_leaf(target_node_id, target_group_id) else {
            return false;
        };

        let target_leaf_id = self.insert_node(PaneNode::Leaf {
            id: target_group_id,
            group: target_group,
        });
        let new_leaf_id = self.insert_node(PaneNode::Leaf {
            id: new_group_id,
            group: new_group.clone(),
        });

        let (first_child, second_child) = if new_group_is_first {
            (new_leaf_id, target_leaf_id)
        } else {
            (target_leaf_id, new_leaf_id)
        };
        let split = if horizontal {
            PaneNode::HSplit {
                id: split_id,
                left: first_child,
                right: second_child,
            }
        } else {
            PaneNode::VSplit {
                id: split_id,
                top: first_child,
                bottom: second_child,
            }
        };
        self.nodes[target_node_id] = Some(split);
        true
    }

    pub fn open_tab(&mut self, content: TabContent, window: &mut Window, cx: &mut Context<Self>) -> usize {
        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;

        let tab = TabItem::new(tab_id, content);

        let target_group = if let Some(active_id) = self.active_group_id
            && let Some(g) = self.find_group(active_id)
        {
            g
        } else if let Some(first_g) = self.all_groups().first().cloned() {
            first_g
        } else {
            let new_group = self.create_group(window, cx);
            let group_id = new_group.read(cx).id;
            let node_id = self.insert_node(PaneNode::Leaf {
                id: group_id,
                group: new_group.clone(),
            });
            self.root = Some(node_id);
            self.active_group_id = Some(group_id);
            new_group
        };

        let target_group_id = target_group.read(cx).id;
        self.active_group_id = Some(target_group_id);

        target_group.update(cx, |g, cx| {
            g.add_tab(tab, true, window, cx);
        });

        self.sync_active_states(cx);
        cx.notify();

        tab_id
    }

    pub fn split_group(&mut self, target_group_id: usize, direction: SplitDirection, content: TabContent, window: &mut Window, cx: &mut Context<Self>) {
        if self.find_group(target_group_id).is_none() {
            return;
        }

        let new_group = self.create_group(window, cx);
        let new_group_id = new_group.read(cx).id;

        let tab_id = self.next_tab_id;
        self.next_tab_id += 1;
        let tab = TabItem::new(tab_id, content);

        let split_id = self.next_split_id;
        self.next_split_id += 1;

        let split_created = self.split_group_node(
            target_group_id,
            new_group_id,
            &new_group,
            split_id,
            matches!(direction, SplitDirection::Horizontal),
            false,
        );
        if !split_created {
            return;
        }

        // Install the new leaf before adding its first tab. `add_tab` focuses
        // the tab and emits `TabChanged`; subscribers must see a complete pane
        // tree when that event is flushed by GPUI.
        self.active_group_id = Some(new_group_id);
        self.sync_active_states(cx);
        new_group.update(cx, |g, cx| {
            g.add_tab(tab, true, window, cx);
        });
        cx.notify();
    }

    pub fn split_with_drop(&mut self, drag: TabDrag, target_group_id: usize, placement: DropPlacement, window: &mut Window, cx: &mut Context<Self>) {
        let from_group = self.find_group(drag.from_group_id);
        let Some(from_group) = from_group else {
            return;
        };

        let removed_tab = from_group.update(cx, |g, cx| g.remove_tab_by_id(drag.tab_id, window, cx));
        let Some(tab) = removed_tab else {
            return;
        };

        // Check if from_group is now empty
        let from_group_empty = from_group.read(cx).is_empty();

        match placement {
            DropPlacement::Center => {
                if let Some(target_group) = self.find_group(target_group_id) {
                    target_group.update(cx, |g, cx| {
                        g.add_tab(tab, true, window, cx);
                    });
                } else {
                    // The target can disappear between drag start and drop.
                    // Keep the removed tab usable instead of dropping it.
                    from_group.update(cx, |g, cx| {
                        g.add_tab(tab, true, window, cx);
                    });
                    return;
                }
            }
            DropPlacement::Left | DropPlacement::Right | DropPlacement::Top | DropPlacement::Bottom => {
                let new_group = self.create_group(window, cx);
                let new_group_id = new_group.read(cx).id;

                let split_id = self.next_split_id;
                self.next_split_id += 1;

                let horizontal = matches!(placement, DropPlacement::Left | DropPlacement::Right);
                let new_group_is_first = matches!(placement, DropPlacement::Left | DropPlacement::Top);
                let split_created = self.split_group_node(target_group_id, new_group_id, &new_group, split_id, horizontal, new_group_is_first);
                if !split_created {
                    // The target can disappear between drag start and drop.
                    // Keep the removed tab usable instead of dropping it.
                    from_group.update(cx, |g, cx| {
                        g.add_tab(tab, true, window, cx);
                    });
                    return;
                }
                self.active_group_id = Some(new_group_id);
                self.sync_active_states(cx);
                new_group.update(cx, |g, cx| {
                    g.add_tab(tab, true, window, cx);
                });
            }
        }

        if from_group_empty {
            self.remove_group(drag.from_group_id, window, cx);
        }

        self.sync_active_states(cx);
        cx.notify();
    }

    pub fn move_tab(&mut self, drag: TabDrag, to_group_id: usize, target_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let from_group = self.find_group(drag.from_group_id);
        let to_group = self.find_group(to_group_id);

        if let (Some(from_g), Some(to_g)) = (from_group, to_group) {
            let removed_tab = from_g.update(cx, |g, cx| g.remove_tab_by_id(drag.tab_id, window, cx));

            if let Some(tab) = removed_tab {
                to_g.update(cx, |g, cx| {
                    g.insert_tab(target_index, tab, true, window, cx);
                });
                self.active_group_id = Some(to_group_id);

                if from_g.read(cx).is_empty() {
                    self.remove_group(drag.from_group_id, window, cx);
                }
            }
        }

        self.sync_active_states(cx);
        cx.notify();
    }

    pub fn remove_group(&mut self, group_id: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(PaneLocation { node_id, parent, grandparent }) = self.find_group_location(group_id) {
            match parent {
                None => {
                    // The only leaf in the tree is being removed.
                    if self.root == Some(node_id) {
                        self.root = None;
                        self.nodes[node_id] = None;
                    }
                }
                Some((parent_id, is_first_child)) => {
                    let Some(survivor_id) = self.sibling_of(parent_id, is_first_child) else {
                        return;
                    };

                    let layout_updated = if let Some((grandparent_id, parent_is_first_child)) = grandparent {
                        self.set_child(grandparent_id, parent_is_first_child, survivor_id)
                    } else if self.root == Some(parent_id) {
                        self.root = Some(survivor_id);
                        true
                    } else {
                        false
                    };

                    if layout_updated {
                        // The parent split and target leaf are now unreachable.
                        // Clearing slots drops only a flat node value.
                        self.nodes[node_id] = None;
                        self.nodes[parent_id] = None;
                    }
                }
            }
        }

        if self.active_group_id == Some(group_id) {
            self.active_group_id = self.all_groups().first().map(|g| g.read(cx).id);
            if let Some(group) = self.active_group(cx)
                && let Some(tab) = group.read(cx).active_tab()
            {
                tab.focus_handle(cx).focus(window, cx);
            }
        }

        self.sync_active_states(cx);
        cx.emit(PaneTreeEvent::ActiveEditorChanged);
        cx.notify();
    }

    pub fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(group) = self.active_group(cx) {
            let tab_id = group.read(cx).active_tab().map(|t| t.id);
            if let Some(tab_id) = tab_id {
                group.update(cx, |g, cx| {
                    g.close_tab(tab_id, window, cx);
                });
            }
        }
    }

    pub fn set_active_group(&mut self, group_id: usize, cx: &mut Context<Self>) {
        if self.active_group_id == Some(group_id) {
            return;
        }

        self.active_group_id = Some(group_id);
        self.sync_active_states(cx);
        cx.emit(PaneTreeEvent::ActiveEditorChanged);
        cx.notify();
    }

    fn sync_active_states(&self, cx: &mut Context<Self>) {
        let active_id = self.active_group_id;
        for group in self.all_groups() {
            let is_active = Some(group.read(cx).id) == active_id;
            group.update(cx, |g, cx| {
                if g.is_active_group != is_active {
                    g.is_active_group = is_active;
                    cx.notify();
                }
            });
        }
    }
}

fn render_node(tree: &PaneTree) -> AnyElement {
    enum Work {
        Visit(usize),
        Combine { id: usize, horizontal: bool, child_count: usize },
    }

    let Some(root) = tree.root else {
        return div().size_full().into_any_element();
    };

    let mut work = vec![Work::Visit(root)];
    let mut elements = Vec::new();

    while let Some(item) = work.pop() {
        match item {
            Work::Visit(node_id) => {
                let node = tree
                    .nodes
                    .get(node_id)
                    .and_then(Option::as_ref)
                    .expect("reachable pane node must exist in the arena");
                match node {
                    PaneNode::Leaf { group, .. } => {
                        elements.push(wrap_pane_content(group.clone().into_any_element()));
                    }
                    PaneNode::HSplit { id, left, right } => {
                        let mut children = Vec::new();
                        collect_same_axis_children(tree, *left, *right, true, &mut children);
                        let child_count = children.len();
                        work.push(Work::Combine {
                            id: *id,
                            horizontal: true,
                            child_count,
                        });
                        for child in children.into_iter().rev() {
                            work.push(Work::Visit(child));
                        }
                    }
                    PaneNode::VSplit { id, top, bottom } => {
                        let mut children = Vec::new();
                        collect_same_axis_children(tree, *top, *bottom, false, &mut children);
                        let child_count = children.len();
                        work.push(Work::Combine {
                            id: *id,
                            horizontal: false,
                            child_count,
                        });
                        for child in children.into_iter().rev() {
                            work.push(Work::Visit(child));
                        }
                    }
                }
            }
            Work::Combine { id, horizontal, child_count } => {
                let mut children = Vec::with_capacity(child_count);
                for _ in 0..child_count {
                    children.push(elements.pop().expect("pane rendering must produce a child element"));
                }
                children.reverse();

                let content = if horizontal {
                    h_resizable(ElementId::NamedInteger("h-split".into(), id as u64))
                        .children(children.into_iter().map(|child| resizable_panel().child(child)))
                        .into_any_element()
                } else {
                    v_resizable(ElementId::NamedInteger("v-split".into(), id as u64))
                        .children(children.into_iter().map(|child| resizable_panel().child(child)))
                        .into_any_element()
                };
                elements.push(wrap_pane_content(content));
            }
        }
    }

    elements.pop().expect("pane rendering must produce a root element")
}

/// Collects a contiguous same-axis split chain without adding recursive call
/// frames. Repeated Split Down/Right operations therefore become one
/// resizable group instead of a deeply nested element tree.
fn collect_same_axis_children(tree: &PaneTree, first: usize, second: usize, horizontal: bool, children: &mut Vec<usize>) {
    let mut pending = vec![second, first];

    while let Some(node_id) = pending.pop() {
        let node = tree
            .nodes
            .get(node_id)
            .and_then(Option::as_ref)
            .expect("reachable pane node must exist in the arena");
        match (horizontal, node) {
            (true, PaneNode::HSplit { left, right, .. }) => {
                pending.push(*right);
                pending.push(*left);
            }
            (false, PaneNode::VSplit { top, bottom, .. }) => {
                pending.push(*bottom);
                pending.push(*top);
            }
            _ => children.push(node_id),
        }
    }
}

fn wrap_pane_content(content: AnyElement) -> AnyElement {
    div().size_full().min_w_0().min_h_0().overflow_hidden().child(content).into_any_element()
}

impl EventEmitter<PaneTreeEvent> for PaneTree {}

impl Render for PaneTree {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.root.is_some() {
            div()
                .id("pane-tree-root")
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(render_node(self))
        } else {
            div()
                .id("pane-tree-empty")
                .size_full()
                .flex()
                .justify_center()
                .items_center()
                .bg(cx.theme().background)
                .child(div().text_xl().text_color(cx.theme().muted_foreground).child("Nothing is open"))
        }
    }
}
