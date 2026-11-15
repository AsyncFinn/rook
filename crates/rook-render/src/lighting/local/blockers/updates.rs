//! Taking the scene's parts and terrain as they change.

use super::*;

impl Blockers {
    /// Forget the parts (the scene is replaced); the terrain stays, and is
    /// marked afresh with the parts that come.
    pub(in crate::lighting::local) fn clear(&mut self) {
        self.blockers.clear();
        self.slack.clear();
        self.held.clear();
        self.columns.clear();
        self.changed = true;
        self.grid = None;
        self.moved.clear();
        self.stale.clear();
        self.is_stale.clear();
    }

    /// Take the parts at `changed` (indices into `scene.parts`) as they
    /// are now, or, while the blockers aren't `used`, note them for later.
    pub(in crate::lighting::local) fn update(
        &mut self,
        scene: &Scene,
        changed: &[usize],
        used: bool,
    ) {
        if self.blockers.len() < scene.parts.len() {
            self.blockers.resize(scene.parts.len(), None);
            self.slack.resize(scene.parts.len(), Slack::default());
            self.is_stale.resize(scene.parts.len(), false);
        }
        if !used {
            for &i in changed {
                if !std::mem::replace(&mut self.is_stale[i], true) {
                    self.stale.push(i);
                }
            }
            return;
        }
        let stale = std::mem::take(&mut self.stale);
        for &i in stale.iter().chain(changed) {
            self.is_stale[i] = false;
            let blocker = self.blocker(&scene.parts[i], scene);
            self.take(i, blocker);
        }
        // More changes than parts (or nothing marking the voxels for a
        // while): marking afresh is cheaper.
        if self.moved.len() > self.blockers.len()
            && let Some(grid) = &mut self.grid
        {
            grid.pending = pieces(grid.span()).collect();
            self.moved.clear();
            self.touched = Touched::All;
        }
    }

    /// Take part `i` as `blocker`, or keep it where it was marked while it
    /// moves within its slack.
    pub(super) fn take(&mut self, i: usize, blocker: Option<Blocker>) {
        let span = self.grid.as_ref().map(Grid::span);
        if let (Some(from), Some(to), Some(span)) = (&self.blockers[i], &blocker, span)
            && self.slack[i].holds(from, to, span)
        {
            if from == to {
                self.held.remove(&i);
            } else {
                self.held.insert(i, *to);
            }
            return;
        }
        self.held.remove(&i);
        if self.blockers[i] != blocker {
            // A part that moves is likely to again: its slack is worth
            // working out.
            self.slack[i] = match (&self.blockers[i], &blocker, span) {
                (Some(from), Some(to), Some(span)) if Slack::worth(from, to) => Slack::of(to, span),
                _ => Slack::default(),
            };
            self.set(i, blocker);
        }
    }

    /// Mark part `i` as `blocker` from now on.
    pub(super) fn set(&mut self, i: usize, blocker: Option<Blocker>) {
        // Only a part in the volume, before or after, changes a voxel: one
        // moving where the volume doesn't reach asks for no marking, and
        // is taken where it is if the volume comes onto it.
        match self.grid.as_ref().map(Grid::span) {
            Some(span) => {
                let within = |b: &Option<Blocker>| {
                    (b.as_ref()).is_some_and(|b| clip(b.centers(), span).is_some())
                };
                if within(&self.blockers[i]) || within(&blocker) {
                    self.moved.push((self.blockers[i], blocker));
                    self.changed = true;
                }
            }
            None => self.changed = true,
        }
        // A part moving within the columns it stood in (most moving parts,
        // frame to frame) keeps its place in them: removing it walks each
        // column's list.
        let columns = |b: &Option<Blocker>| b.as_ref().map(|b| columns_of(b.centers()));
        if columns(&self.blockers[i]) != columns(&blocker) {
            if let Some(b) = &self.blockers[i] {
                self.columns.remove(i, b);
            }
            if let Some(b) = &blocker {
                self.columns.insert(i, b);
            }
        }
        self.blockers[i] = blocker;
        rook_types::count!(LightSourcesChanged, 1);
    }

    /// Before the volume moves: its slacks covered only the voxels it
    /// held, so parts held within theirs take where they are now.
    pub(super) fn release_held(&mut self) {
        let mut held: Vec<(usize, Blocker)> = self.held.drain().collect();
        held.sort_unstable_by_key(|&(i, _)| i);
        for (i, now) in held {
            self.slack[i] = Slack::default();
            self.set(i, Some(now));
        }
    }

    /// Take the terrain's voxels the scene sends: all of them, which marks
    /// every voxel afresh (as a place loads, or Workspace's Terrain is
    /// another), or the boxes of them an edit changed, which mark only
    /// those again.
    pub(in crate::lighting::local) fn update_terrain(&mut self, voxels: TerrainVoxels) {
        match self.terrain.take(voxels) {
            Taken::Whole => {
                if let Some(grid) = &mut self.grid {
                    grid.pending = pieces(grid.span()).collect();
                    self.moved.clear();
                    self.touched = Touched::All;
                }
                self.changed = true;
            }
            // An edit where the volume doesn't reach changes no voxel: it's
            // read where it is if the volume comes onto it.
            Taken::Edits => {
                let span = self.grid.as_ref().map(Grid::span);
                self.changed |= span.is_none_or(|span| self.terrain.reaches(span));
            }
        }
    }
}
