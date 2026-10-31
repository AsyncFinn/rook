//! Keeping the rings of detail with the camera: each frame the rings
//! follow it (`rook_terrain::lod::Rings::follow`), and when one moved, the
//! chunks whose level changed are meshed in the background and swapped in
//! all at once, the old ones drawn until then, so the terrain never shows
//! a hole or a chunk drawn twice. Grass's blades grow on the drawn
//! chunks of the levels within their reach, so they stand on the ground
//! that's drawn, as Studio plants them on the triangles it draws.

use rook_terrain::lod::{LEVELS, nearest};

use super::background::{Job, mesh};
use super::*;

impl SceneCache {
    /// Install what the background mesher finished and follow `camera`
    /// with the rings, meshing what they now hold off the frame, or in
    /// place when `wait` (a screenshot waits for the terrain it shows).
    /// Returns the entries that changed.
    pub(crate) fn update_terrain(&mut self, camera: Vec3, wait: bool) -> Vec<usize> {
        let mut changed = Vec::new();
        let done = if wait {
            self.terrain.background.wait()
        } else {
            self.terrain.background.poll()
        };
        if let Some((job, meshes)) = done
            && job.epoch == self.terrain.epoch
        {
            changed.extend(self.install(&job, meshes, true));
        }
        if self.terrain.background.busy() {
            return changed;
        }
        let Some(target) = self.terrain.target.as_mut() else {
            return changed;
        };
        target.follow(camera);
        let target = *target;
        if self.terrain.shown == Some(target) {
            return changed;
        }
        profiling::scope!("SceneCache::update_terrain");
        let job = self.plan(target);
        let todo = self.todo(&job);
        if todo.is_empty() || wait {
            let meshes = mesh_in_place(&self.terrain.levels, &todo);
            changed.extend(self.install(&job, meshes, true));
            return changed;
        }
        let levels = self.terrain.levels.clone();
        if let Err(job) = self.terrain.background.start(&levels, todo.clone(), job) {
            let meshes = mesh_in_place(&levels, &todo);
            changed.extend(self.install(&job, meshes, true));
        }
        changed
    }

    /// What should be drawn with the rings at `rings`.
    pub(super) fn plan(&self, rings: Rings) -> Job {
        let levels = &self.terrain.levels;
        let mut draw = HashSet::new();
        for level in 0..LEVELS {
            for c in rings.chunks(level, &levels[usize::from(level)], self.terrain.rows) {
                draw.insert((level, c));
            }
        }
        Job {
            epoch: self.terrain.epoch,
            rings,
            draw,
        }
    }

    /// The chunks `job` draws that aren't meshed.
    pub(super) fn todo(&self, job: &Job) -> Vec<LevelChunk> {
        let mut todo: Vec<LevelChunk> = job
            .draw
            .iter()
            .filter(|k| !self.terrain.chunks.contains_key(k))
            .copied()
            .collect();
        todo.sort_unstable_by_key(|(l, c)| (*l, c.x, c.y, c.z));
        todo
    }

    /// Draw what `job` asked for with `meshes`, its chunks newly meshed:
    /// add those with their Grass tops, and drop the chunks it doesn't
    /// draw. Places the new chunks' entries in the scene when `place` (a
    /// rebuilt scene places every chunk itself). Returns the entries that
    /// changed.
    pub(super) fn install(&mut self, job: &Job, meshes: Vec<ChunkMesh>, place: bool) -> Vec<usize> {
        let mut changed = Vec::new();
        let mut added = Vec::new();
        let mut tiles = HashSet::new();
        for m in meshes {
            let key = (m.level, m.chunk);
            if !job.draw.contains(&key) || self.terrain.chunks.contains_key(&key) {
                continue;
            }
            if grows_grass(m.level) {
                let mut tops = GrassTops::default();
                add_grass_tops(&m, &mut tops);
                if !tops.is_empty() {
                    tiles.extend(tops.keys());
                    self.terrain.tops.insert(key, tops);
                }
            }
            let meshed = self.add_chunk_meshes(m);
            self.terrain.chunks.insert(key, meshed);
            added.push(key);
        }
        let gone: Vec<LevelChunk> = self
            .terrain
            .chunks
            .keys()
            .filter(|k| !job.draw.contains(k))
            .copied()
            .collect();
        for key in gone {
            changed.extend(self.drop_chunk(key));
        }
        tiles.extend(self.terrain.stale_tiles.drain());
        if place {
            added.sort_unstable_by_key(|(l, c)| (*l, c.x, c.y, c.z));
            changed.extend(self.place_chunks(&added));
        }
        if !tiles.is_empty() {
            changed.extend(self.grass_tops_changed(&tiles));
        }
        self.terrain.shown = Some(job.rings);
        changed.sort_unstable();
        changed.dedup();
        changed
    }
}

/// Whether chunks drawn at `level` can come within the blades' reach of
/// the camera, so blades grow on them.
fn grows_grass(level: u8) -> bool {
    nearest(level) < GRASS_REACH
}

/// Mesh `chunks` on the frame's own workers, for a frame that waits.
pub(super) fn mesh_in_place(levels: &[Grid], chunks: &[LevelChunk]) -> Vec<ChunkMesh> {
    let meshed = rook_platform::get()
        .workers
        .map(chunks, |c| mesh(levels, std::slice::from_ref(c)));
    meshed.into_iter().flatten().collect()
}
