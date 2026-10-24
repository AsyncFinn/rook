//! Characters standing, until the Humanoid controller (M9 step 3): a
//! world holds a character it simulates upright, its root part at
//! standing height over what's below it. Approximation: Roblox's Humanoid
//! floats the root part at HipHeight with forces and its own state
//! machine.

use super::*;

impl Physics {
    /// Hold `model` standing: its root part `root` upright, `drop` above
    /// the floor under it when it's that low or lower, falling otherwise.
    /// Only after a step, and only for an assembly this world simulates.
    pub fn stand(&mut self, dom: &mut Dom, model: InstanceId, root: InstanceId, drop: f32) {
        let Some(&h) = self.parts.get(&root) else {
            return;
        };
        if self
            .bodies
            .get(&h)
            .is_none_or(|b| b.remote || b.asm.anchored)
        {
            return;
        }
        let origin = to_glam(self.world.bodies[h].translation());
        let filter = Filter {
            instances: vec![model],
            respect_can_collide: true,
            ..Filter::default()
        };
        let floor = self.raycast(dom, origin, glam::Vec3::NEG_Y * (drop + 0.5), &filter);
        let rb = &mut self.world.bodies[h];
        rb.lock_rotations(true, true);
        rb.set_angvel(Vector::ZERO, true);
        let Some(floor) = floor else {
            return;
        };
        let height = floor.position.y + drop;
        if origin.y <= height {
            let mut at = *rb.position();
            at.translation.y = height;
            rb.set_position(at, true);
            let mut v = rb.linvel();
            v.y = v.y.max(0.0);
            rb.set_linvel(v, true);
        }
    }
}
