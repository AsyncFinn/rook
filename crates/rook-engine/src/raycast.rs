//! Ray queries against parts, by their exact primitive shapes.
//!
//! A brute-force test over Workspace's parts, used by the mouse
//! (`Mouse.Hit`, `Mouse.Target`), ClickDetectors and (later) WorldRoot
//! queries. M3 step 5 replaces it with Rapier queries.

use glam::{Mat3, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_types::Value;

pub use rook_physics::{PartShape, is_part, part_shape};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RayHit {
    pub part: InstanceId,
    pub position: Vec3,
    pub normal: Vec3,
    pub distance: f32,
}

/// Where a local-space ray (origin `o`, direction `d`, `t` in 0..=1) first
/// enters a convex shape given by planes `n·x <= k`.
fn convex(o: Vec3, d: Vec3, planes: &[(Vec3, f32)]) -> Option<(f32, Vec3)> {
    let (mut t_in, mut t_out) = (0.0f32, 1.0f32);
    let mut normal = None;
    for &(n, k) in planes {
        let denom = n.dot(d);
        let dist = k - n.dot(o);
        if denom.abs() < 1e-12 {
            if dist < 0.0 {
                return None;
            }
            continue;
        }
        let t = dist / denom;
        if denom < 0.0 {
            if t > t_in {
                t_in = t;
                normal = Some(n);
            }
        } else if t < t_out {
            t_out = t;
        }
        if t_in > t_out {
            return None;
        }
    }
    // A ray that starts inside the shape doesn't hit it (Inference).
    normal.map(|n| (t_in, n))
}

/// A flat-sided shape's faces as planes `n·x <= k`, at `size`.
fn shape_planes(shape: PartShape, size: Vec3) -> Vec<(Vec3, f32)> {
    let faces = shape.faces().unwrap_or_default();
    faces
        .iter()
        .map(|face| {
            let [a, b, c] = [face[0], face[1], face[2]].map(|p| p * size);
            let n = (b - a).cross(c - a).normalize();
            (n, n.dot(a))
        })
        .collect()
}

fn sphere(o: Vec3, d: Vec3, r: f32) -> Option<(f32, Vec3)> {
    let a = d.dot(d);
    let b = 2.0 * o.dot(d);
    let c = o.dot(o) - r * r;
    if c <= 0.0 {
        return None;
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return None;
    }
    let t = (-b - disc.sqrt()) / (2.0 * a);
    (0.0..=1.0)
        .contains(&t)
        .then(|| (t, (o + d * t).normalize()))
}

/// Cylinder along X: radius `r`, half length `hx`.
fn cylinder(o: Vec3, d: Vec3, r: f32, hx: f32) -> Option<(f32, Vec3)> {
    let inside = |p: Vec3| p.x.abs() <= hx && p.y * p.y + p.z * p.z <= r * r;
    if inside(o) {
        return None;
    }
    let mut best: Option<(f32, Vec3)> = None;
    let mut consider = |t: f32, n: Vec3| {
        if (0.0..=1.0).contains(&t) && best.is_none_or(|(bt, _)| t < bt) {
            best = Some((t, n));
        }
    };
    // Curved side.
    let (a, b, c) = (
        d.y * d.y + d.z * d.z,
        2.0 * (o.y * d.y + o.z * d.z),
        o.y * o.y + o.z * o.z - r * r,
    );
    if a > 1e-12 {
        let disc = b * b - 4.0 * a * c;
        if disc >= 0.0 {
            let t = (-b - disc.sqrt()) / (2.0 * a);
            let p = o + d * t;
            if p.x.abs() <= hx {
                consider(t, Vec3::new(0.0, p.y, p.z).normalize());
            }
        }
    }
    // Caps.
    if d.x.abs() > 1e-12 {
        for side in [-1.0f32, 1.0] {
            let t = (side * hx - o.x) / d.x;
            let p = o + d * t;
            if p.y * p.y + p.z * p.z <= r * r {
                consider(t, Vec3::X * side);
            }
        }
    }
    best
}

/// What a ray test needs to know about one part.
#[derive(Debug, Clone, Copy)]
struct PartGeometry {
    id: InstanceId,
    position: Vec3,
    rotation: Mat3,
    size: Vec3,
    shape: PartShape,
    can_query: bool,
    /// Bounding sphere radius, for a cheap first test.
    radius: f32,
}

impl PartGeometry {
    fn read(dom: &Dom, id: InstanceId) -> Option<Self> {
        let Some(Value::CFrame(cf)) = dom.property_by_name(id, "CFrame") else {
            return None;
        };
        let Some(Value::Vector3(size)) = dom.property_by_name(id, "Size") else {
            return None;
        };
        Some(Self {
            id,
            position: cf.position,
            rotation: cf.rotation,
            size: size.0,
            shape: part_shape(dom, id),
            can_query: dom.property_by_name(id, "CanQuery") != Some(Value::Bool(false)),
            radius: size.0.length() * 0.5,
        })
    }

    /// Whether the ray segment passes within the bounding sphere.
    fn near(&self, origin: Vec3, dir: Vec3) -> bool {
        let len2 = dir.length_squared();
        let t = if len2 > 0.0 {
            ((self.position - origin).dot(dir) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        (origin + dir * t).distance_squared(self.position) <= self.radius * self.radius
    }

    fn hit(&self, origin: Vec3, dir: Vec3) -> Option<RayHit> {
        geometry_hit(
            self.id,
            self.position,
            self.rotation,
            self.size,
            self.shape,
            origin,
            dir,
        )
    }
}

/// Test one part. `dir`'s length is the maximum distance.
pub fn ray_part(dom: &Dom, id: InstanceId, origin: Vec3, dir: Vec3) -> Option<RayHit> {
    PartGeometry::read(dom, id)?.hit(origin, dir)
}

fn geometry_hit(
    id: InstanceId,
    position: Vec3,
    rot: Mat3,
    size: Vec3,
    shape: PartShape,
    origin: Vec3,
    dir: Vec3,
) -> Option<RayHit> {
    let inv = rot.transpose();
    let o = inv * (origin - position);
    let d = inv * dir;
    let h = shape.extent(size) * 0.5;
    let hit = match shape {
        PartShape::Ball => sphere(o, d, h.x),
        PartShape::Cylinder => cylinder(o, d, h.y, h.x),
        _ => convex(o, d, &shape_planes(shape, size)),
    }?;
    let (t, n) = hit;
    Some(RayHit {
        part: id,
        position: origin + dir * t,
        normal: (rot * n).normalize(),
        distance: dir.length() * t,
    })
}

/// Workspace parts' geometry, kept up to date from the journal so repeated
/// queries (the mouse, every frame) don't walk the instance tree.
#[derive(Debug, Default)]
pub struct PartCache {
    parts: Vec<PartGeometry>,
    index: std::collections::HashMap<InstanceId, usize>,
    stale: bool,
    built: bool,
}

impl PartCache {
    /// Parts were added, removed or moved: rebuild before the next query.
    pub fn invalidate(&mut self) {
        self.stale = true;
    }

    /// A part's properties changed.
    pub fn part_changed(&mut self, dom: &Dom, id: InstanceId) {
        if self.stale || !self.built {
            return;
        }
        if let Some(&i) = self.index.get(&id) {
            match PartGeometry::read(dom, id) {
                Some(g) => self.parts[i] = g,
                None => self.stale = true,
            }
        }
    }

    fn ensure(&mut self, dom: &Dom) {
        if self.built && !self.stale {
            return;
        }
        profiling::scope!("PartCache::rebuild");
        self.parts.clear();
        self.index.clear();
        if let Some(ws) = dom.find_service("Workspace") {
            for id in dom.descendants(ws) {
                if is_part(dom, id)
                    && let Some(g) = PartGeometry::read(dom, id)
                {
                    self.index.insert(id, self.parts.len());
                    self.parts.push(g);
                }
            }
        }
        self.built = true;
        self.stale = false;
    }

    /// Like [`raycast`], using the cache.
    pub fn raycast(
        &mut self,
        dom: &Dom,
        origin: Vec3,
        dir: Vec3,
        include: &dyn Fn(InstanceId) -> bool,
    ) -> Option<RayHit> {
        profiling::scope!("PartCache::raycast");
        self.ensure(dom);
        let mut best: Option<RayHit> = None;
        for g in &self.parts {
            if !g.can_query || !g.near(origin, dir) || !include(g.id) {
                continue;
            }
            if let Some(hit) = g.hit(origin, dir)
                && best.is_none_or(|b| hit.distance < b.distance)
            {
                best = Some(hit);
            }
        }
        best
    }
}

/// The first part along a ray from `origin` covering `dir` (its length is
/// the maximum distance), among Workspace parts with `CanQuery` on that
/// `include` accepts.
pub fn raycast(
    dom: &Dom,
    origin: Vec3,
    dir: Vec3,
    include: &dyn Fn(InstanceId) -> bool,
) -> Option<RayHit> {
    profiling::scope!("raycast");
    let ws = dom.find_service("Workspace")?;
    let mut best: Option<RayHit> = None;
    for id in dom.descendants(ws) {
        if !is_part(dom, id) {
            continue;
        }
        if dom.property_by_name(id, "CanQuery") == Some(Value::Bool(false)) || !include(id) {
            continue;
        }
        if let Some(hit) = ray_part(dom, id, origin, dir)
            && best.is_none_or(|b| hit.distance < b.distance)
        {
            best = Some(hit);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use rook_types::{CFrame, Vector3};

    fn part(dom: &mut Dom, class: &str, pos: Vec3, size: Vec3) -> InstanceId {
        let ws = dom.get_service("Workspace").unwrap();
        let p = dom.create_by_name(class).unwrap();
        dom.set_property_by_name(p, "CFrame", Value::CFrame(CFrame::from_position(pos)))
            .unwrap();
        dom.set_property_by_name(p, "Size", Value::Vector3(Vector3(size)))
            .unwrap();
        dom.set_parent(p, Some(ws)).unwrap();
        p
    }

    #[test]
    fn hits_the_nearest_face() {
        let mut dom = Dom::new();
        let near = part(
            &mut dom,
            "Part",
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::splat(2.0),
        );
        part(
            &mut dom,
            "Part",
            Vec3::new(0.0, 0.0, -20.0),
            Vec3::splat(2.0),
        );
        let hit = raycast(&dom, Vec3::ZERO, Vec3::new(0.0, 0.0, -100.0), &|_| true).unwrap();
        assert_eq!(hit.part, near);
        assert!((hit.distance - 9.0).abs() < 1e-4);
        assert!((hit.normal - Vec3::Z).length() < 1e-5);
        // Too short to reach.
        assert!(raycast(&dom, Vec3::ZERO, Vec3::new(0.0, 0.0, -5.0), &|_| true).is_none());
        // Filtered out.
        let far = raycast(&dom, Vec3::ZERO, Vec3::new(0.0, 0.0, -100.0), &|id| {
            id != near
        })
        .unwrap();
        assert!((far.distance - 19.0).abs() < 1e-4);
    }

    /// A corner wedge is tall above its +X -Z corner and flat at -X +Z.
    #[test]
    fn corner_wedge_apex_is_at_plus_x_minus_z() {
        let mut dom = Dom::new();
        part(&mut dom, "CornerWedgePart", Vec3::ZERO, Vec3::splat(2.0));
        let down = |x: f32, z: f32| {
            raycast(
                &dom,
                Vec3::new(x, 10.0, z),
                Vec3::new(0.0, -20.0, 0.0),
                &|_| true,
            )
            .unwrap()
            .position
            .y
        };
        assert!(down(0.9, -0.9) > 0.7);
        assert!(down(-0.9, 0.9) < -0.7);
    }

    #[test]
    fn balls_and_cylinders_use_their_real_shape() {
        let mut dom = Dom::new();
        let ball = part(
            &mut dom,
            "Part",
            Vec3::new(0.0, 0.0, -10.0),
            Vec3::splat(4.0),
        );
        dom.set_property_by_name(ball, "Shape", Value::Enum(0))
            .unwrap();
        // Grazing the box's corner misses the sphere.
        assert!(
            raycast(
                &dom,
                Vec3::new(1.9, 1.9, 0.0),
                Vec3::new(0.0, 0.0, -100.0),
                &|_| true
            )
            .is_none()
        );
        let hit = raycast(&dom, Vec3::ZERO, Vec3::new(0.0, 0.0, -100.0), &|_| true).unwrap();
        assert!((hit.distance - 8.0).abs() < 1e-4);
    }

    #[test]
    fn wedges_slope_up_towards_the_back() {
        let mut dom = Dom::new();
        part(&mut dom, "WedgePart", Vec3::ZERO, Vec3::splat(2.0));
        // From above near the front edge, the slope is low; near the back it's high.
        let front = raycast(
            &dom,
            Vec3::new(0.0, 10.0, -0.9),
            Vec3::new(0.0, -20.0, 0.0),
            &|_| true,
        )
        .unwrap();
        let back = raycast(
            &dom,
            Vec3::new(0.0, 10.0, 0.9),
            Vec3::new(0.0, -20.0, 0.0),
            &|_| true,
        )
        .unwrap();
        assert!(front.position.y < back.position.y);
        assert!((back.position.y - 0.9).abs() < 1e-4);
    }
}
