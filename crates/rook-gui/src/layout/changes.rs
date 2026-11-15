//! Laying out what changed: whole trees, from relayout boundaries, objects
//! placed again on their own, and text laid out again in place.

use super::*;

impl Layout {
    /// Lay out the GUI scripts see: every player's PlayerGui, StarterGui,
    /// CoreGui (what Roblox's CoreScripts draw) and the world GUIs in
    /// `world`. `previous` is the last layout, where
    /// hidden children of a UI layout stay; `turning` says where each
    /// turning UIPageLayout's view is.
    pub(crate) fn screens(
        dom: &Dom,
        (previous, turning): (&Layout, &Turning),
        fonts: &mut Fonts,
        world: &[InstanceId],
    ) -> Self {
        let mut pass = Pass::new(dom, (previous, turning), fonts, Layout::default());
        let players = dom.find_service("Players");
        let holders = players
            .into_iter()
            .flat_map(|p| dom.children(p).iter().copied())
            .filter_map(|p| dom.find_first_child_of_class(p, "PlayerGui"))
            .chain(dom.find_service("StarterGui"))
            .chain(dom.find_service("CoreGui"));
        for holder in holders {
            for root in through_folders(dom, holder, |id| dom.is_a(id, "GuiBase2d")) {
                if world.binary_search(&root).is_err() {
                    pass.root(root);
                }
            }
        }
        for &root in world {
            pass.root(root);
        }
        pass.out
    }

    /// Forget where `ids` were laid out (they left their tree, or are
    /// being freed). Their text stays, reused if it's laid out unchanged.
    pub(crate) fn drop(&mut self, ids: &FxHashSet<InstanceId>) {
        for id in ids {
            self.placed.remove(id);
            self.arranged.remove(id);
            self.pages.remove(id);
            self.scrolls.remove(id);
            self.world.remove(id);
        }
    }

    pub(crate) fn contains(&self, id: InstanceId) -> bool {
        self.placed.contains_key(&id) || self.arranged.contains_key(&id)
    }

    /// Lay out the trees from `tops` again (a world GUI the camera
    /// resized, a tree whose properties changed), leaving the rest as it
    /// is. What `tops` hold must be what they held when last laid out.
    pub(crate) fn redo(&mut self, dom: &Dom, tops: &[InstanceId], (fonts, turning): Fonted<'_>) {
        for top in tops {
            self.world.remove(top);
        }
        let previous = Layout::default();
        let mut pass = Pass::new(dom, (&previous, turning), fonts, std::mem::take(self));
        for &top in tops {
            pass.root(top);
        }
        *self = pass.out;
    }

    /// What to place again for changes to `ids`: for each, the nearest
    /// object, itself or a GUI object holding it, that can be placed
    /// again on its own (laid out, its parent placing it by `Position`
    /// and `Size` alone, nothing growing to hold it), as browsers lay out
    /// again from a relayout boundary; and the ids with none, whose trees
    /// must lay out again. None of the first is inside another.
    pub(crate) fn alone(
        &self,
        dom: &Dom,
        ids: impl IntoIterator<Item = InstanceId>,
    ) -> (Vec<InstanceId>, Vec<InstanceId>) {
        let mut placed = Vec::new();
        let mut whole = Vec::new();
        for id in ids {
            let mut at = id;
            loop {
                if self.free(dom, at) {
                    placed.push(at);
                    break;
                }
                match gui_parent(dom, at).filter(|&p| dom.is_a(p, "GuiObject")) {
                    Some(p) => at = p,
                    None => {
                        whole.push(id);
                        break;
                    }
                }
            }
        }
        placed.sort_unstable();
        placed.dedup();
        let set: FxHashSet<InstanceId> = placed.iter().copied().collect();
        placed.retain(|&b| !dom.ancestors(b).any(|a| set.contains(&a)));
        (placed, whole)
    }

    /// Whether `id` can be placed again on its own (see [`Layout::alone`]).
    pub(super) fn free(&self, dom: &Dom, id: InstanceId) -> bool {
        let free = |id: InstanceId| {
            let Some(parent) = gui_parent(dom, id) else {
                return false;
            };
            if !self.placed.contains_key(&id)
                || !self.placed.contains_key(&parent)
                || dom.class_name(parent) == "ScrollingFrame"
                || arranger(dom, parent).is_some()
                || dom
                    .enum_item_property(parent, "AutomaticSize")
                    .is_some_and(|a| a != "None")
            {
                return false;
            }
            // A UITableLayout places its rows' cells.
            let table = gui_parent(dom, parent)
                .and_then(|g| arranger(dom, g))
                .is_some_and(|l| dom.class_name(l) == "UITableLayout");
            if table {
                return false;
            }
            // A ScrollingFrame's automatic canvas reaches what all its
            // GUI holds, up to the next ScrollingFrame in.
            let mut at = gui_parent(dom, parent);
            while let Some(a) = at {
                if dom.class_name(a) == "ScrollingFrame" {
                    return dom.enum_item_property(a, "AutomaticCanvasSize") == Some("None");
                }
                at = gui_parent(dom, a);
            }
            true
        };
        free(id)
    }

    /// Place `ids` again, with what they hold, where their parents put
    /// them now; each must be one [`Layout::alone`] allows.
    pub(crate) fn place_again(
        &mut self,
        dom: &Dom,
        ids: &[InstanceId],
        (fonts, turning): Fonted<'_>,
    ) {
        let previous = Layout::default();
        let mut pass = Pass::new(dom, (&previous, turning), fonts, std::mem::take(self));
        for &id in ids {
            pass.place_alone(id);
        }
        *self = pass.out;
    }

    pub(crate) fn waiting_texts(&self) -> impl Iterator<Item = InstanceId> + '_ {
        (self.texts.iter())
            .filter(|(_, laid)| laid.waited)
            .map(|(&id, _)| id)
    }

    /// Lay out the text of `ids` again where it is, for a change to how
    /// its glyphs look alone: their objects keep their size and place.
    pub(crate) fn restyle_texts(
        &mut self,
        dom: &Dom,
        ids: &[InstanceId],
        (fonts, turning): Fonted<'_>,
    ) {
        let previous = Layout::default();
        let mut pass = Pass::new(dom, (&previous, turning), fonts, std::mem::take(self));
        for &id in ids {
            if let Some(size) = pass.out.placed.get(&id).map(|p| p.local) {
                pass.lay_out_text(id, size);
            }
        }
        *self = pass.out;
    }
}
