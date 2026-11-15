use crate::*;
use content::arrivals::ARRIVALS_BUDGET;
use content::report::Wanter;
use rook_content::Failure;

/// How far the camera moves before the files still loading are ranked
/// again by their distance from it, in studs.
const RERANK: f32 = 16.0;

/// What a part's mesh file last settled on.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Settled {
    File(MeshHash),
    /// The look of an empty `MeshId`.
    Empty,
}

/// How a mesh file draws right now.
pub(crate) enum FileDraw {
    /// Its mesh, or the last one this mesh loaded.
    Mesh(MeshHash),
    /// The look of an empty `MeshId`, which this mesh had last.
    Empty,
    Nothing,
}

impl SceneCache {
    /// Take the files that finished loading and redraw the parts waiting
    /// for them, within the frame's budget for arrivals; with `wait`, first
    /// wait until none is loading, and redraw them all. Returns the entries
    /// that changed.
    pub(crate) fn poll_files(&mut self, dom: &Dom, wait: bool) -> Vec<usize> {
        let finished = self.files.poll(wait);
        let mut changed = Vec::new();
        if !finished.is_empty() {
            self.take_finished(dom, finished, &mut changed);
        }
        if !self.arrivals.is_empty() {
            profiling::scope!("SceneCache::arrivals");
            let budget = (!wait).then_some(ARRIVALS_BUDGET);
            let mut arrivals = std::mem::take(&mut self.arrivals);
            arrivals.drain(budget, |id| {
                if self.index.contains_key(&id) {
                    self.place(dom, id, true, &mut changed);
                }
            });
            self.arrivals = arrivals;
        }
        if self.files.idle() && self.arrivals.is_empty() {
            self.report.flush(dom);
        }
        changed
    }

    /// Store finished files, and queue the parts waiting for them nearest
    /// the camera first (effects and terrain, one each, redraw now).
    fn take_finished(
        &mut self,
        dom: &Dom,
        finished: Vec<(FileRef, Result<Loaded, Failure>)>,
        changed: &mut Vec<usize>,
    ) {
        profiling::scope!("SceneCache::poll_files");
        let mut parts = FxHashSet::default();
        for (key, loaded) in finished {
            let waiting = self.waiting.remove(&key).unwrap_or_default();
            let sky = self.sky_files.remove(&key);
            let gui = self.gui_waiting.contains_key(&key);
            match loaded {
                Ok(Loaded::Mesh(hash, mesh)) => {
                    self.scene.meshes.insert(hash, mesh);
                }
                Ok(Loaded::Image(hash, image)) => {
                    self.images.add(hash, image);
                    let info = self.images.info(hash).expect("just added");
                    self.gui_file_done(&key, Some(crate::gui::gui_image(hash, info)));
                }
                Err(failure) => {
                    for &part in &waiting {
                        self.report.add(Wanter::Part(part), &key, &failure);
                    }
                    if sky {
                        self.report.add(Wanter::Sky, &key, &failure);
                    }
                    if gui {
                        self.report.add(Wanter::Gui, &key, &failure);
                    }
                    self.gui_file_done(&key, None);
                }
            }
            if sky {
                self.lighting_changed = true;
            }
            parts.extend(waiting);
        }
        if self.lighting_changed {
            self.scene.lighting = self.lighting(dom);
        }
        let effects: Vec<InstanceId> = parts
            .iter()
            .copied()
            .filter(|&id| self.effects.has(id))
            .collect();
        if !effects.is_empty() {
            for id in &effects {
                parts.remove(id);
            }
            self.with_effects(|cache, drawn, images| {
                cache.reread_ids(dom, &effects, drawn, images);
            });
        }
        if let Some(&terrain) = parts.iter().find(|&&id| self.is_terrain(id)) {
            parts.remove(&terrain);
            changed.extend(self.refresh_terrain(dom));
        }
        let mut queued: Vec<(f32, InstanceId)> = parts
            .into_iter()
            .filter_map(|id| {
                let i = *self.index.get(&id)?;
                Some((self.scene.parts[i].position.distance(self.focus), id))
            })
            .collect();
        queued.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        for (_, id) in queued {
            self.arrivals.push(id);
        }
    }

    /// A file's state, noting that `part` waits for it while it loads.
    pub(crate) fn file_state(&mut self, part: InstanceId, url: &str, want: Want) -> FileState {
        let Some(r) = ContentRef::parse(url) else {
            return FileState::Failed;
        };
        self.file_state_of(part, r, want)
    }

    pub(crate) fn file_state_of(
        &mut self,
        part: InstanceId,
        r: ContentRef,
        want: Want,
    ) -> FileState {
        // Terrain is under the camera wherever it is.
        let priority = if self.is_terrain(part) {
            0.0
        } else {
            self.focus.distance(self.near)
        };
        let state = self.files.state(&r, want, priority);
        let key = (r, want);
        match state {
            FileState::Loading => {
                self.waiting.entry(key).or_default().insert(part);
            }
            // A file that failed before: this part didn't load either.
            FileState::Failed => {
                if let Some(failure) = self.files.failure(&key) {
                    self.report.add(Wanter::Part(part), &key, failure);
                }
            }
            FileState::Loaded(_) => {}
        }
        state
    }

    /// Follow the camera: once it has moved far enough, rank the files
    /// still loading again by how near it their nearest waiting part is, so
    /// what's around it loads first.
    pub(crate) fn follow_camera(&mut self, dom: &Dom, camera: Vec3) {
        self.focus = camera;
        if self.waiting.is_empty() || camera.distance(self.ranked_from) < RERANK {
            return;
        }
        profiling::scope!("SceneCache::follow_camera");
        self.ranked_from = camera;
        for (key, parts) in &self.waiting {
            let nearest = parts
                .iter()
                .filter_map(|&id| {
                    if self.is_terrain(id) {
                        return Some(0.0);
                    }
                    let cf = dom.cframe_property(id, "CFrame")?;
                    Some(camera.distance(cf.position))
                })
                .reduce(f32::min);
            if let Some(priority) = nearest {
                self.files.set_priority(key, priority);
            }
        }
    }

    pub(crate) fn settle(&mut self, part: InstanceId, mesh: InstanceId, settled: Settled) {
        self.shown.insert(part, Shown { mesh, settled });
    }

    /// How a part's mesh file `url` draws, by the rules observed in Studio
    /// 0.742 (2026-10-13, docs/behavior/meshes/file-meshes.md): a new mesh
    /// draws nothing until its file arrives, and nothing if it fails to
    /// load; when a mesh's `MeshId` changes, what it drew before (a mesh, or
    /// the empty-`MeshId` box) stays until the new file arrives, and stays
    /// for good if the new one fails. The file is asked for the first time
    /// any part needs it.
    pub(crate) fn file(&mut self, part: InstanceId, mesh: InstanceId, url: &str) -> FileDraw {
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
