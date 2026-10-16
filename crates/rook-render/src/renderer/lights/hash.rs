//! The lights as the shaders read them, and their spatial hash
//! (`renderer/lights.rs` has the whole picture).

use super::*;

/// Ranges past this light no further (Studio's limit with
/// `FFlagExtendLightRangeTo120`).
const MAX_RANGE: f32 = 120.0;
/// Bucket entries a rebuild may make before the cubes grow (a place with
/// thousands of wide lights).
const MAX_ENTRIES: usize = 1 << 21;
/// The smallest cube the hash cuts the world into, in studs.
const MIN_CELL: f32 = 16.0;
/// One light as the shaders read it (`Light` in `shaders/lights.wgsl`):
/// what Studio's voxel lighting writes into each voxel it reaches.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct GpuLight {
    /// xyz: where it is (a surface light's face centre); w: the distance
    /// its falloff reaches 0.
    pub(super) position: [f32; 4],
    /// rgb: `Brightness` x `Color` squared (Studio's light units); w: how
    /// fast it falls off, per stud.
    pub(super) color: [f32; 4],
    /// xyz: where it points; w: a spot light's tan(Angle / 2), a surface
    /// light's cosine where its cone has faded out.
    pub(super) direction: [f32; 4],
    /// x: 0 point, 1 spot, 2 surface; y: how far a spot light brightens
    /// from its origin, in studs.
    pub(super) kind: [f32; 4],
}

impl GpuLight {
    pub(crate) fn of(light: &LocalLight) -> Self {
        let range = light.range.clamp(0.0, MAX_RANGE);
        // Measured: the falloff reaches 0 at 0.8 x Range + 5.5 studs
        // (Range 8, 16, 32, 60: 11.9, 18.3, 31, 53.4), losing 1/12 a
        // stud, more slowly for short ranges (Range 8: 0.857 at the
        // light).
        let short = (16.0 - range).max(0.0) / 4.0;
        let reach = 0.8 * range + 5.5;
        let slope = 1.0 / (12.0 + short);
        let c = light.color.map(|c| c.clamp(0.0, 1.0).powi(2));
        let b = light.brightness.max(0.0);
        let half_angle = |angle: f32| (angle.clamp(0.0, 180.0) * 0.5).to_radians();
        let (direction, cone, kind, ramp) = match light.shape {
            LightShape::Point => (Vec3::NEG_Y, 0.0, 0.0, 0.0),
            // Measured: a spot light brightens over its first 0.7 x Range
            // from its origin.
            LightShape::Spot { direction, angle } => (
                direction,
                half_angle(angle).min(1.5).tan(),
                1.0,
                0.7 * range.max(1.0),
            ),
            // Fitted: a surface light shines from its face's centre, its
            // cone fading by the cosine out to 45 degrees past half its
            // Angle (at Angle 90, the cosine itself).
            LightShape::Surface { direction, angle } => (
                direction,
                (half_angle(angle) + 45f32.to_radians())
                    .min(std::f32::consts::PI)
                    .cos(),
                2.0,
                0.0,
            ),
        };
        Self {
            position: [light.position.x, light.position.y, light.position.z, reach],
            color: [c[0] * b, c[1] * b, c[2] * b, slope],
            direction: [direction.x, direction.y, direction.z, cone],
            kind: [kind, ramp, 0.0, 0.0],
        }
    }

    pub(super) fn at(&self) -> Vec3 {
        Vec3::from_slice(&self.position[..3])
    }

    /// The box every voxel it lights lies in, padded by [`SLACK`].
    fn reach(&self) -> Aabb {
        let extent = Vec3::splat(self.position[3].max(0.0) + SLACK);
        Aabb {
            min: self.at() - extent,
            max: self.at() + extent,
        }
    }
}

/// The bucket a cube hashes to (`bucket` in `shaders/light_volume.wgsl`).
fn bucket(cube: [i32; 3], mask: u32) -> usize {
    let h = (cube[0] as u32).wrapping_mul(73_856_093)
        ^ (cube[1] as u32).wrapping_mul(19_349_663)
        ^ (cube[2] as u32).wrapping_mul(83_492_791);
    (h & mask) as usize
}

/// The cubes a box covers, for cubes `cell` studs across.
fn cubes(reach: &Aabb, cell: f32) -> ([i32; 3], [i32; 3]) {
    let lo = (reach.min / cell).floor();
    let hi = (reach.max / cell).floor();
    (
        [lo.x as i32, lo.y as i32, lo.z as i32],
        [hi.x as i32, hi.y as i32, hi.z as i32],
    )
}

/// The lights' hash: cubes `cell` studs across, and each bucket's
/// (first entry, count) into `entries`, light indices.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct LightHash {
    pub(super) cell: f32,
    pub(super) buckets: Vec<[u32; 2]>,
    pub(super) entries: Vec<u32>,
}

impl LightHash {
    pub(crate) fn build(lights: &[GpuLight]) -> Self {
        profiling::scope!("LightHash::build");
        let reaches: Vec<Aabb> = lights.iter().map(GpuLight::reach).collect();
        let count = |cell: f32| -> usize {
            reaches
                .iter()
                .map(|r| {
                    let (lo, hi) = cubes(r, cell);
                    (0..3)
                        .map(|i| (hi[i] - lo[i] + 1).max(0) as usize)
                        .product::<usize>()
                })
                .sum()
        };
        let mut cell = MIN_CELL;
        while count(cell) > MAX_ENTRIES {
            cell *= 2.0;
        }
        let buckets = count(cell).next_power_of_two().clamp(64, 1 << 20);
        let mask = buckets as u32 - 1;
        // Count, then fill (a counting sort), listing a light once per
        // bucket however many of its cubes land there.
        let mut last = vec![u32::MAX; buckets];
        let mut counts = vec![0u32; buckets];
        let each = |f: &mut dyn FnMut(usize, u32)| {
            for (i, r) in reaches.iter().enumerate() {
                let (lo, hi) = cubes(r, cell);
                for x in lo[0]..=hi[0] {
                    for y in lo[1]..=hi[1] {
                        for z in lo[2]..=hi[2] {
                            f(bucket([x, y, z], mask), i as u32);
                        }
                    }
                }
            }
        };
        each(&mut |b, i| {
            if last[b] != i {
                last[b] = i;
                counts[b] += 1;
            }
        });
        let mut ranges = Vec::with_capacity(buckets);
        let mut start = 0u32;
        for &c in &counts {
            ranges.push([start, 0]);
            start += c;
        }
        let mut entries = vec![0u32; start as usize];
        last.fill(u32::MAX);
        each(&mut |b, i| {
            if last[b] != i {
                last[b] = i;
                let r = &mut ranges[b];
                entries[(r[0] + r[1]) as usize] = i;
                r[1] += 1;
            }
        });
        Self {
            cell,
            buckets: ranges,
            entries,
        }
    }

    /// The lights listed where `p` is.
    #[cfg(test)]
    fn at(&self, p: Vec3) -> &[u32] {
        let c = (p / self.cell).floor();
        let mask = self.buckets.len() as u32 - 1;
        let [start, count] = self.buckets[bucket([c.x as i32, c.y as i32, c.z as i32], mask)];
        &self.entries[start as usize..(start + count) as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(at: Vec3, range: f32) -> GpuLight {
        GpuLight::of(&LocalLight {
            position: at,
            shape: LightShape::Point,
            color: [1.0; 3],
            brightness: 1.0,
            range,
            shadows: false,
        })
    }

    #[test]
    fn each_light_is_listed_wherever_it_reaches() {
        let lights = [
            point(Vec3::ZERO, 8.0),
            point(Vec3::new(100.0, 0.0, 0.0), 60.0),
            point(Vec3::new(-500.0, 30.0, 20.0), 4.0),
        ];
        let hash = LightHash::build(&lights);
        assert!(hash.at(Vec3::new(5.0, 5.0, 0.0)).contains(&0));
        assert!(hash.at(Vec3::new(55.0, 0.0, 0.0)).contains(&1));
        assert!(hash.at(Vec3::new(-501.0, 30.0, 20.0)).contains(&2));
        // A light is listed once per bucket.
        for [start, count] in &hash.buckets {
            let list = &hash.entries[*start as usize..(start + count) as usize];
            let mut sorted = list.to_vec();
            sorted.dedup();
            assert_eq!(sorted.len(), list.len());
        }
    }
}
