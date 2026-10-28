//! Keeping the rings of detail with the camera: each frame the rings
//! follow it (`rook_terrain::lod::Rings::follow`), and when one moved, the
//! chunks whose level changed are meshed in the background and swapped in
//! all at once, the old ones drawn until then, so the terrain never shows
//! a hole or a chunk drawn twice. Grass's blades need the 4-stud surface
//! out to their reach wherever the finest ring ends, so the level-0
//! chunks near the camera are meshed for their tops too.

use glam::IVec2;
use rook_terrain::lod::LEVELS;
use rook_terrain::mesh::can_have_surface;

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
        let near = self.grass_column(camera);
        if self.terrain.shown == Some(target) && self.terrain.near == near {
            return changed;
        }
        profiling::scope!("SceneCache::update_terrain");
        let job = self.plan(target, camera);
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

    /// What should be drawn with the rings at `rings` and the camera at
    /// `camera`.
    pub(super) fn plan(&self, rings: Rings, camera: Vec3) -> Job {
        let levels = &self.terrain.levels;
        let mut draw = HashSet::new();
        for level in 0..LEVELS {
            for c in rings.chunks(level, &levels[usize::from(level)], self.terrain.rows) {
                draw.insert((level, c));
            }
        }
        let near = self.grass_column(camera);
        let mut tops = HashSet::new();
        if let Some(column) = near {
            let (lo, hi) = self.terrain.rows;
            let r = grass_columns();
            for x in column.x - r..=column.x + r {
                for z in column.y - r..=column.y + r {
                    for y in lo..=hi {
                        let c = IVec3::new(x, y, z);
                        if can_have_surface(&levels[0], c) {
                            tops.insert(c);
                        }
                    }
                }
            }
        }
        Job {
            epoch: self.terrain.epoch,
            rings,
            draw,
            tops,
            near,
        }
    }

    /// The chunks `job` needs meshed: those to draw that aren't, and the
    /// level-0 ones whose Grass tops it needs and doesn't have.
    pub(super) fn todo(&self, job: &Job) -> Vec<LevelChunk> {
        let t = &self.terrain;
        let mut todo: Vec<LevelChunk> = job
            .draw
            .iter()
            .filter(|k| !t.chunks.contains_key(k))
            .copied()
            .collect();
        todo.extend(
            job.tops
                .iter()
                .filter(|c| !t.tops.contains_key(c) && !t.chunks.contains_key(&(0, **c)))
                .filter(|c| !job.draw.contains(&(0, **c)))
                .map(|c| (0, *c)),
        );
        todo.sort_unstable_by_key(|(l, c)| (*l, c.x, c.y, c.z));
        todo
    }

    /// Draw what `job` asked for with `meshes`, its chunks newly meshed:
    /// add those, drop the chunks it doesn't draw, and keep the Grass tops
    /// it needs. Places the new chunks' entries in the scene when `place`
    /// (a rebuilt scene places every chunk itself). Returns the entries
    /// that changed.
    pub(super) fn install(&mut self, job: &Job, meshes: Vec<ChunkMesh>, place: bool) -> Vec<usize> {
        let mut changed = Vec::new();
        let mut tiles: HashSet<IVec2> = HashSet::new();
        let mut added = Vec::new();
        for m in meshes {
            let key = (m.level, m.chunk);
            if m.level == 0 {
                let mut tops = GrassTops::default();
                add_grass_tops(&m, &mut tops);
                tiles.extend(tops.keys());
                self.terrain.tops.insert(m.chunk, tops);
            }
            if job.draw.contains(&key) && !self.terrain.chunks.contains_key(&key) {
                let meshed = self.add_chunk_meshes(m);
                self.terrain.chunks.insert(key, meshed);
                added.push(key);
            }
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
        let unneeded: Vec<IVec3> = self
            .terrain
            .tops
            .keys()
            .filter(|c| !job.tops.contains(c) && !job.draw.contains(&(0, **c)))
            .copied()
            .collect();
        for c in unneeded {
            tiles.extend(self.terrain.tops.remove(&c).expect("kept").keys());
        }
        if place {
            added.sort_unstable_by_key(|(l, c)| (*l, c.x, c.y, c.z));
            changed.extend(self.place_chunks(&added));
        }
        if !tiles.is_empty() {
            changed.extend(self.grass_tops_changed(&tiles));
        }
        self.terrain.shown = Some(job.rings);
        self.terrain.near = job.near;
        changed.sort_unstable();
        changed.dedup();
        changed
    }

    /// The camera's level-0 column, around which chunks are meshed for
    /// their Grass tops, when Grass grows blades.
    fn grass_column(&self, camera: Vec3) -> Option<IVec2> {
        self.terrain.grass.on.then(|| {
            let c = (camera / chunk_studs()).floor().as_ivec3();
            IVec2::new(c.x, c.z)
        })
    }
}

/// A level-0 chunk's edge, in studs.
fn chunk_studs() -> f32 {
    rook_terrain::lod::chunk_studs(0)
}

/// How many columns of level-0 chunks each side of the camera's hold the
/// blades it draws.
fn grass_columns() -> i32 {
    (GRASS_REACH / chunk_studs()).ceil() as i32
}

/// Mesh `chunks` on the frame's own workers, for a frame that waits.
pub(super) fn mesh_in_place(levels: &[Grid], chunks: &[LevelChunk]) -> Vec<ChunkMesh> {
    let meshed = rook_platform::get()
        .workers
        .map(chunks, |c| mesh(levels, std::slice::from_ref(c)));
    meshed.into_iter().flatten().collect()
}
