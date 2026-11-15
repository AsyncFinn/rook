//! Terrain chunks meshed, placed and dropped as voxels change, the
//! Terrain is swapped or its look is reread.

use super::*;

impl SceneCache {
    /// Add the terrain's entries to the (rebuilt) scene, meshing it first
    /// if its voxels aren't the ones meshed last.
    pub(crate) fn add_terrain(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::add_terrain");
        let terrain = workspace_terrain(dom);
        self.terrain.instance = terrain;
        for mesh in self.terrain.grass.reset() {
            self.scene.meshes.remove(&mesh);
        }
        // The scene's entries are gone; the meshes stay.
        for chunk in self.terrain.chunks.values_mut() {
            chunk.entries.clear();
        }
        self.read_terrain_look(dom);
        // The rebuilt scene places every chunk.
        self.take_voxels(dom, false);
        self.read_looks(dom);
        let mut keys: Vec<LevelChunk> = self.terrain.chunks.keys().copied().collect();
        keys.sort_unstable_by_key(chunk_order);
        self.place_chunks(&keys);
    }

    /// Note the frame's changes to the Terrain's voxels, which the next
    /// [`SceneCache::take_voxels`] meshes (the scene may be rebuilt first).
    pub(crate) fn note_voxels(&mut self, changes: &RenderChanges) {
        let t = &mut self.terrain;
        t.whole |= changes.terrain_voxels;
        let meshed = t.meshed;
        t.edits.extend(
            changes
                .terrain_edits
                .iter()
                .filter(|(id, _)| Some(*id) == meshed)
                .map(|(_, bounds)| *bounds),
        );
    }

    /// Mesh what changed of the Terrain's voxels since they were meshed:
    /// only the chunks the edits reached, or what differs from a whole
    /// new grid. With `place`, those chunks are swapped in the scene and
    /// the entries that changed returned.
    pub(super) fn take_voxels(&mut self, dom: &Dom, place: bool) -> Vec<usize> {
        let t = &mut self.terrain;
        let whole = std::mem::take(&mut t.whole) || t.meshed != t.instance;
        let edits = std::mem::take(&mut t.edits);
        if !whole && edits.is_empty() {
            return Vec::new();
        }
        t.meshed = t.instance;
        let empty = Grid::new();
        let voxels = t.instance.and_then(|i| dom.voxels(i)).unwrap_or(&empty);
        let materials = t.materials.clone();
        let edits = (!whole).then_some(&edits[..]);
        let mut changed = self.mesh_terrain(voxels, edits, place);
        // A material new to the terrain needs its look read.
        if place && self.terrain.materials != materials {
            changed.extend(self.refresh_terrain(dom));
        }
        changed
    }

    /// Workspace's Terrain is another instance: stop drawing the old one's
    /// chunks and blades and mesh the new one's voxels, leaving the rest
    /// of the scene as it is. Returns the entries that changed.
    pub(super) fn swap_terrain(&mut self, dom: &Dom) -> Vec<usize> {
        self.terrain.background.wait();
        let mut changed = Vec::new();
        let chunks: Vec<LevelChunk> = self.terrain.chunks.keys().copied().collect();
        for key in chunks {
            changed.extend(self.drop_chunk(key));
        }
        for i in self.terrain.grass.entries() {
            self.release(i);
            changed.push(i);
        }
        for mesh in self.terrain.grass.reset() {
            self.scene.meshes.remove(&mesh);
        }
        let t = &mut self.terrain;
        t.instance = workspace_terrain(dom);
        // The new one's voxels are taken whole.
        t.edits.clear();
        t.levels = Arc::default();
        t.shown = None;
        t.epoch += 1;
        changed.extend(self.take_voxels(dom, true));
        changed
    }

    /// Scripts changed the Terrain (its voxels or other properties),
    /// `Workspace.GlobalWind`, or which Terrain Workspace holds: remesh
    /// what the voxel edits reached and, with `look` or for another
    /// Terrain, read the waves, wind, colors and grass again and redraw
    /// the terrain's entries and blades. Returns the entries that changed.
    pub(crate) fn terrain_changed(&mut self, dom: &Dom, look: bool) -> Vec<usize> {
        let same = workspace_terrain(dom) == self.terrain.instance;
        let mut changed = if same {
            self.take_voxels(dom, true)
        } else {
            self.swap_terrain(dom)
        };
        // An edit alone changes no look.
        if same && !look {
            return changed;
        }
        self.read_terrain_look(dom);
        changed.extend(self.refresh_terrain(dom));
        let color = self.terrain.grass.color;
        for i in self.terrain.grass.entries() {
            self.scene.parts[i].color = color;
            changed.push(i);
        }
        changed
    }

    /// Redraw the terrain (its textures arrived, or its colors changed):
    /// the entries that changed.
    pub(crate) fn refresh_terrain(&mut self, dom: &Dom) -> Vec<usize> {
        self.read_looks(dom);
        let mut changed = Vec::new();
        for chunk in self.terrain.chunks.values() {
            for (s, &i) in chunk.surfaces.iter().zip(&chunk.entries) {
                self.scene.parts[i] = self.terrain.looks.entry(s);
                changed.push(i);
            }
        }
        changed
    }

    /// Give the chunks at `keys` entries in the scene. Returns them.
    pub(super) fn place_chunks(&mut self, keys: &[LevelChunk]) -> Vec<usize> {
        let mut placed = Vec::new();
        for key in keys {
            let Some(chunk) = self.terrain.chunks.get(key) else {
                continue;
            };
            let entries: Vec<PartInstance> = chunk
                .surfaces
                .iter()
                .map(|s| self.terrain.looks.entry(s))
                .collect();
            let indices: Vec<usize> = entries
                .into_iter()
                .map(|e| self.take_index(e, Pose::default()))
                .collect();
            placed.extend(&indices);
            self.terrain.chunks.get_mut(key).expect("placed").entries = indices;
        }
        placed
    }

    /// Stop drawing the chunk at `key`, freeing its meshes. Returns its
    /// entries.
    pub(super) fn drop_chunk(&mut self, key: LevelChunk) -> Vec<usize> {
        let Some(chunk) = self.terrain.chunks.remove(&key) else {
            return Vec::new();
        };
        for s in &chunk.surfaces {
            self.scene.meshes.remove(&s.mesh);
            self.terrain.dropped.push(s.mesh);
        }
        for &i in &chunk.entries {
            self.release(i);
        }
        if let Some(tops) = self.terrain.tops.remove(&key) {
            for tile in tops.keys() {
                if let Some(chunks) = self.terrain.tile_chunks.get_mut(tile) {
                    chunks.retain(|c| *c != key);
                    if chunks.is_empty() {
                        self.terrain.tile_chunks.remove(tile);
                    }
                }
            }
            self.terrain.stale_tiles.extend(tops.keys());
        }
        chunk.entries
    }
}
