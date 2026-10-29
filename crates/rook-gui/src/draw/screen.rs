//! The screen drawn in pieces, one per stacked object (a CanvasGroup's
//! holding its group), each kept until what it draws changes
//! (docs/design/architecture.md#redrawing-what-changed).

use std::sync::Arc;

use rook_dom::{Dom, InstanceId};
use rook_types::GuiQuad;
use rustc_hash::{FxHashMap, FxHashSet};

use super::{Draw, Text, group, messages};
use crate::images::Images;
use crate::layout::{GUI_INSET, Layout, top_of};
use crate::order::{self, Clip, Stacked};

/// One ScreenGui's stack, bottom first.
#[derive(Debug)]
struct Collector {
    root: InstanceId,
    stack: Vec<Stacked>,
    /// Where each object is in `stack`.
    at: FxHashMap<InstanceId, usize>,
    /// The CanvasGroup drawing each object a group holds.
    group_of: FxHashMap<InstanceId, InstanceId>,
}

impl Collector {
    fn new(dom: &Dom, layout: &Layout, root: InstanceId) -> Self {
        let stack = order::stack(dom, layout, root, Clip::NONE);
        let mut c = Self {
            root,
            at: FxHashMap::default(),
            group_of: FxHashMap::default(),
            stack,
        };
        c.index(dom);
        c
    }

    fn index(&mut self, dom: &Dom) {
        self.at = (self.stack.iter().enumerate())
            .map(|(i, s)| (s.id, i))
            .collect();
        self.group_of.clear();
        for (i, g) in self.stack.iter().enumerate() {
            if dom.class_name(g.id) != "CanvasGroup" {
                continue;
            }
            for s in &self.stack[i + 1..] {
                if dom.is_ancestor_of(g.id, s.id) && !self.group_of.contains_key(&s.id) {
                    self.group_of.insert(s.id, g.id);
                }
            }
        }
    }

    /// The object whose piece draws `id`: itself, or the group holding it.
    fn drawer(&self, id: InstanceId) -> InstanceId {
        self.group_of.get(&id).copied().unwrap_or(id)
    }
}

/// The screen as last drawn, and what changed since.
#[derive(Debug, Default)]
pub(crate) struct Screen {
    /// The shown ScreenGuis' stacks in drawing order, once stacked.
    collectors: Option<Vec<Collector>>,
    /// Each drawn object's piece, already below the top bar.
    pieces: FxHashMap<InstanceId, Arc<[GuiQuad]>>,
    /// ScreenGuis laid out again, to stack again.
    restack: FxHashSet<InstanceId>,
    /// Objects placed again on their own, and objects whose look alone
    /// changed (or a UI component's, drawn by its parent).
    placed: FxHashSet<InstanceId>,
    repaint: FxHashSet<InstanceId>,
    /// The Hints and Messages over it all, once drawn.
    messages: Option<Arc<[GuiQuad]>>,
    /// The pieces last given, which a renderer tells apart by pointer.
    drawn: Option<Arc<[Arc<[GuiQuad]>]>>,
}

impl Screen {
    /// Draw everything again.
    pub(crate) fn forget(&mut self) {
        *self = Self::default();
    }

    /// The tree from `top` was laid out again.
    pub(crate) fn restack(&mut self, top: InstanceId) {
        self.restack.insert(top);
        self.drawn = None;
    }

    /// `id` was placed again on its own, with what it holds.
    pub(crate) fn placed(&mut self, id: InstanceId) {
        self.placed.insert(id);
        self.drawn = None;
    }

    /// `id`'s look changed.
    pub(crate) fn repaint(&mut self, id: InstanceId) {
        self.repaint.insert(id);
        self.drawn = None;
    }

    /// A Hint or Message changed, came or went.
    pub(crate) fn repaint_messages(&mut self) {
        self.messages = None;
        self.drawn = None;
    }

    /// The pieces the local player's ScreenGuis draw, bottom first, then
    /// the Hints and Messages among `messages` that show, over them.
    pub(crate) fn draw(
        &mut self,
        dom: &Dom,
        layout: &Layout,
        (text, images): (&mut Text<'_>, &mut Images),
        focus: Option<InstanceId>,
        messages: &FxHashSet<InstanceId>,
    ) -> Arc<[Arc<[GuiQuad]>]> {
        if let Some(drawn) = &self.drawn {
            return drawn.clone();
        }
        self.apply(dom, layout);
        let collectors = self.collectors.as_ref().expect("stacked above");
        let mut draw = Draw {
            dom,
            layout,
            text,
            images,
            out: &mut Vec::new(),
            fields: false,
            focus,
            canvas: GuiQuad::NO_CLIP,
        };
        let mut list: Vec<Arc<[GuiQuad]>> = Vec::new();
        for c in collectors {
            for (i, s) in c.stack.iter().enumerate() {
                if c.group_of.contains_key(&s.id) {
                    continue;
                }
                let piece = self.pieces.entry(s.id).or_insert_with(|| {
                    let mut quads = Vec::new();
                    std::mem::swap(draw.out, &mut quads);
                    if dom.class_name(s.id) == "CanvasGroup" {
                        let held: Vec<Stacked> = (c.stack[i + 1..].iter())
                            .filter(|h| c.group_of.get(&h.id) == Some(&s.id))
                            .copied()
                            .collect();
                        group::emit(&mut draw, s, &held);
                    } else {
                        draw.emit(s);
                    }
                    std::mem::swap(draw.out, &mut quads);
                    below_top_bar(&mut quads);
                    quads.into()
                });
                if !piece.is_empty() {
                    list.push(piece.clone());
                }
            }
        }
        let bars = self.messages.get_or_insert_with(|| {
            let mut quads = Vec::new();
            messages::emit(dom, messages, draw.text, &mut quads);
            quads.into()
        });
        if !bars.is_empty() {
            list.push(bars.clone());
        }
        let drawn: Arc<[Arc<[GuiQuad]>]> = list.into();
        self.drawn = Some(drawn.clone());
        drawn
    }

    /// Bring the stacks and pieces up to date with what changed.
    fn apply(&mut self, dom: &Dom, layout: &Layout) {
        let mut restack = std::mem::take(&mut self.restack);
        let placed = std::mem::take(&mut self.placed);
        let repaint = std::mem::take(&mut self.repaint);
        let Some(collectors) = &mut self.collectors else {
            self.pieces.clear();
            self.collectors = Some(stack_all(dom, layout, &FxHashSet::default(), Vec::new()));
            return;
        };
        for &id in &placed {
            if !place(dom, layout, collectors, &mut self.pieces, id) {
                restack.extend(top_of(dom, id));
            }
        }
        if !restack.is_empty() {
            restack_kept(dom, layout, collectors, &mut self.pieces, &restack);
        }
        for id in repaint {
            // A UI component (UICorner, UIGradient, UIStroke) draws as
            // part of its parent.
            let mut at = Some(id);
            while let Some(i) = at {
                if let Some(c) = collectors.iter().find(|c| c.at.contains_key(&i)) {
                    self.pieces.remove(&c.drawer(i));
                    break;
                }
                at = dom.parent(i);
            }
        }
    }
}

/// Stack the ScreenGuis in `restack` again, keeping the pieces of the
/// objects whose entries (and the group drawing them) didn't change.
fn restack_kept(
    dom: &Dom,
    layout: &Layout,
    collectors: &mut Vec<Collector>,
    pieces: &mut FxHashMap<InstanceId, Arc<[GuiQuad]>>,
    restack: &FxHashSet<InstanceId>,
) {
    let mut before: FxHashMap<InstanceId, (Stacked, InstanceId)> = FxHashMap::default();
    for c in collectors.iter().filter(|c| restack.contains(&c.root)) {
        before.extend(c.stack.iter().map(|s| (s.id, (*s, c.drawer(s.id)))));
    }
    *collectors = stack_all(dom, layout, restack, std::mem::take(collectors));
    for c in collectors.iter().filter(|c| restack.contains(&c.root)) {
        for s in &c.stack {
            let drawer = c.drawer(s.id);
            if before.remove(&s.id) != Some((*s, drawer)) {
                pieces.remove(&drawer);
            }
        }
    }
    // No longer shown: their pieces, and the groups that drew them.
    for (id, (_, drawer)) in before {
        pieces.remove(&id);
        pieces.remove(&drawer);
    }
}

/// Stack the shown ScreenGuis, keeping those in `kept` not in `restack`.
fn stack_all(
    dom: &Dom,
    layout: &Layout,
    restack: &FxHashSet<InstanceId>,
    kept: Vec<Collector>,
) -> Vec<Collector> {
    let mut kept: FxHashMap<InstanceId, Collector> = (kept.into_iter())
        .filter(|c| !restack.contains(&c.root))
        .map(|c| (c.root, c))
        .collect();
    (order::screens(dom).into_iter())
        .map(|root| {
            kept.remove(&root)
                .unwrap_or_else(|| Collector::new(dom, layout, root))
        })
        .collect()
}

/// Place `id`'s stack entries again where `layout` puts them now, and drop
/// the pieces that changed. False when what shows changed, so its
/// ScreenGui must be stacked again.
fn place(
    dom: &Dom,
    layout: &Layout,
    collectors: &mut [Collector],
    pieces: &mut FxHashMap<InstanceId, Arc<[GuiQuad]>>,
    id: InstanceId,
) -> bool {
    let Some(c) = collectors.iter_mut().find(|c| c.at.contains_key(&id)) else {
        // Not drawn before: shown now, unless something above it hides it
        // (a spinner on a hidden loading screen), or in no ScreenGui at all.
        return hidden_above(dom, id);
    };
    let Some(now) = order::restacked(dom, layout, &c.stack[c.at[&id]]) else {
        return false;
    };
    let mut changed = Vec::new();
    for s in now {
        let Some(&i) = c.at.get(&s.id) else {
            return false;
        };
        if c.stack[i] != s {
            c.stack[i] = s;
            changed.push(s.id);
        }
    }
    for id in changed {
        pieces.remove(&c.drawer(id));
    }
    true
}

/// Whether a GUI object above `id`, up to its LayerCollector, is hidden.
fn hidden_above(dom: &Dom, id: InstanceId) -> bool {
    let mut at = dom.parent(id);
    while let Some(a) = at.filter(|&a| dom.is_a(a, "GuiObject") || dom.class_name(a) == "Folder") {
        if !crate::layout::visible(dom, a) {
            return true;
        }
        at = dom.parent(a);
    }
    false
}

/// Laid out in GUI coordinates, drawn in the window's, below the top bar:
/// the quads, what clips them, and the corners and gradients they carry,
/// which are placed in the same pixels.
fn below_top_bar(quads: &mut [GuiQuad]) {
    for q in quads {
        q.center.y += GUI_INSET;
        q.clip[1] += GUI_INSET;
        q.clip[3] += GUI_INSET;
        if let Some(shape) = &mut q.shape {
            shape.center.y += GUI_INSET;
        }
        if let Some(gradient) = &mut q.gradient {
            gradient.origin.y += GUI_INSET;
        }
    }
}
