//! Mesh files and images loading in the background, and what parts draw
//! meanwhile.

use super::*;

/// What a part's mesh file last settled on.
#[derive(Debug, Clone, Copy)]
pub(super) enum Settled {
    /// A loaded mesh.
    File(MeshHash),
    /// The look of an empty `MeshId`.
    Empty,
}

/// How a mesh file draws right now.
pub(super) enum FileDraw {
    /// Its mesh, or the last one this mesh loaded.
    Mesh(MeshHash),
    /// The look of an empty `MeshId`, which this mesh had last.
    Empty,
    Nothing,
}

impl SceneCache {
    /// Take the files that finished loading and redraw the parts waiting
    /// for them; with `wait`, first wait until none is loading. Returns the
    /// entries that changed.
    pub fn poll_files(&mut self, dom: &Dom, wait: bool) -> Vec<usize> {
        let finished = self.files.poll(wait);
        if finished.is_empty() {
            return Vec::new();
        }
        profiling::scope!("SceneCache::poll_files");
        let mut parts = HashSet::new();
        for (key, loaded) in finished {
            match loaded {
                Some(Loaded::Mesh(hash, mesh)) => {
                    self.scene.meshes.insert(hash, mesh);
                }
                Some(Loaded::Image(hash, image)) => {
                    self.scene.images.insert(hash, image);
                }
                None => {}
            }
            if self.sky_files.remove(&key) {
                self.lighting_changed = true;
            }
            parts.extend(self.waiting.remove(&key).unwrap_or_default());
        }
        if self.lighting_changed {
            self.scene.lighting = self.lighting(dom);
        }
        let mut changed = Vec::new();
        for id in parts {
            if self.index.contains_key(&id) {
                self.place(dom, id, true, &mut changed);
            }
        }
        changed
    }

    /// A file's state, noting that `part` waits for it while it loads.
    pub(super) fn file_state(&mut self, part: InstanceId, url: &str, want: Want) -> FileState {
        let Some(r) = ContentRef::parse(url) else {
            return FileState::Failed;
        };
        self.file_state_of(part, r, want)
    }

    /// [`SceneCache::file_state`] for a parsed reference.
    pub(super) fn file_state_of(
        &mut self,
        part: InstanceId,
        r: ContentRef,
        want: Want,
    ) -> FileState {
        let state = self.files.state(&r, want);
        if state == FileState::Loading {
            self.waiting.entry((r, want)).or_default().insert(part);
        }
        state
    }

    pub(super) fn settle(&mut self, part: InstanceId, mesh: InstanceId, settled: Settled) {
        self.shown.insert(part, Shown { mesh, settled });
    }

    /// How a part's mesh file `url` draws, by the rules observed in Studio
    /// 0.742 (2026-10-13, ROADMAP M4 "File meshes (step 3)"): a new mesh
    /// draws nothing until its file arrives, and nothing if it fails to
    /// load; when a mesh's `MeshId` changes, what it drew before (a mesh, or
    /// the empty-`MeshId` box) stays until the new file arrives, and stays
    /// for good if the new one fails. The file is asked for the first time
    /// any part needs it.
    pub(super) fn file(&mut self, part: InstanceId, mesh: InstanceId, url: &str) -> FileDraw {
        if let FileState::Loaded(hash) = self.file_state(part, url, Want::Mesh) {
            let hash = MeshHash(hash);
            self.settle(part, mesh, Settled::File(hash));
            return FileDraw::Mesh(hash);
        }
        match self.shown.get(&part).filter(|s| s.mesh == mesh) {
            Some(Shown {
                settled: Settled::File(hash),
                ..
            }) => FileDraw::Mesh(*hash),
            Some(Shown {
                settled: Settled::Empty,
                ..
            }) => FileDraw::Empty,
            None => FileDraw::Nothing,
        }
    }
}
