//! Each step's steering: velocities the root body is given before Rapier
//! steps it. Constants are fitted to Studio recordings
//! (`docs/behavior/characters/movement.md`).

use super::*;

/// On a floor, the velocity closes on its target at this rate per second
/// (measured: 98% of the way in one 60 Hz frame, starting or stopping).
const GROUND_RATE: f32 = 235.0;

/// In the air (and water), the most the velocity changes per second
/// (measured: 2.38 studs/s per frame turning a jump around).
const AIR_ACCEL: f32 = 143.0;

/// The legs: a spring holding the root at standing height (fitted to a
/// landing's dip and recovery, and the lag up a slope).
const STIFFNESS: f32 = 2450.0;
const DAMPING: f32 = 80.0;

/// AutoRotate: the turn closes at this rate per second, up to `MAX_TURN`
/// radians per second (measured: the angle left shrinks 14% a frame, at
/// most about 20 degrees a frame).
const TURN_RATE: f32 = 9.0;
const MAX_TURN: f32 = 20.0;
/// Facing the camera closes the turn within a 60 Hz frame (Observed: 90
/// degrees in one frame, with a 4-degree overshoot Rook leaves out).
const FACE_RATE: f32 = 240.0;

/// Climbing goes at this share of WalkSpeed (measured: 11.2 at 16), with
/// the root drawn in to this far from the face (measured: 1.5) but never
/// pushed out: climbing a 2-stud truss from beside it, Studio climbed on
/// at 0.09 studs from the face and so got over the top.
const CLIMB_SHARE: f32 = 0.7;
const CLIMB_HOLD: f32 = 1.5;
const CLIMB_SNAP: f32 = 20.0;

/// Swimming goes at this share of WalkSpeed (measured: about 11.8 at 16),
/// floating with the root this far under the surface (measured: 0.45).
const SWIM_SHARE: f32 = 0.74;
const FLOAT_DEPTH: f32 = 0.45;
const FLOAT_RATE: f32 = 6.0;

impl Physics {
    pub(super) fn steer(&mut self, c: &mut Controller, r: &Rooted, h: f32) {
        let root = self.root_pose(r);
        let gravity = to_glam(self.world.gravity);
        c.since_sensed += h;
        let rb = &mut self.world.bodies[r.body];
        let mut v = to_glam(rb.linvel());
        let walk = c.drive.walk;
        // The controller applies gravity itself, where the state falls
        // (the body's own gravity is off while it's driven).
        let fall = gravity * h;
        let mut turn = c.state.upright();
        match c.state {
            s if s.grounded() => {
                let felt = c.floor.and_then(|f| {
                    let floor_y = f.height_at(root.position, c.since_sensed);
                    let e = root.position.y - (floor_y + c.drive.standing_height);
                    // A floor the root was moved away from holds nothing.
                    (e <= floor::REACH).then_some((f, e))
                });
                match felt {
                    Some((floor, e)) => {
                        let steep = floor.normal.y < c.drive.max_slope.cos();
                        let want =
                            horizontal(floor.velocity) + along_floor(walk, floor.normal, steep);
                        if steep {
                            // Too steep to stand on: it slides down it as
                            // gravity takes it (measured: g·sinθ·cosθ along the
                            // ground) and can't walk up.
                            let down = horizontal(floor.normal).normalize_or_zero();
                            let slide =
                                -gravity.y * floor.normal.y * horizontal(floor.normal).length();
                            v += down * slide * h;
                            v = air(v, want, h);
                        } else {
                            let keep = (-GROUND_RATE * h).exp();
                            let hv = want + (horizontal(v) - want) * keep;
                            v = glam::Vec3::new(hv.x, v.y, hv.z);
                        }
                        let a = -DAMPING * (v.y - floor.velocity.y) - STIFFNESS * e;
                        v.y += a * h;
                    }
                    None => v = air(v, walk, h) + fall,
                }
            }
            State::Jumping | State::Freefall => {
                v = air(v, walk, h);
                if c.lift > 0.0 {
                    c.lift -= h;
                } else {
                    v += fall;
                }
            }
            State::Flying => v = air(v, walk, h),
            State::Climbing => {
                turn = false;
                if let Some(climb) = c.climb {
                    let toward = -climb.normal;
                    let input = walk.dot(toward) / c.drive.walk_speed.max(f32::EPSILON);
                    let up = c.drive.walk_speed * CLIMB_SHARE * input.clamp(-1.0, 1.0);
                    let off = (root.position - climb.point).dot(climb.normal);
                    let closer = (CLIMB_HOLD - off).min(0.0);
                    v = climb.normal * closer * CLIMB_SNAP + glam::Vec3::Y * up;
                }
            }
            State::Swimming => {
                let mut want = walk * SWIM_SHARE;
                if let Some(surface) = c.water {
                    let below = surface - FLOAT_DEPTH - root.position.y;
                    want.y = below * FLOAT_RATE;
                }
                let dv = (want - v).clamp_length_max(AIR_ACCEL * h);
                v += dv;
            }
            _ => return,
        }
        rb.set_linvel(to_rapier(v), true);
        if turn {
            let look = horizontal(root.rotation * glam::Vec3::NEG_Z);
            let angle_to = |way: glam::Vec3| look.cross(way).y.atan2(look.dot(way));
            let spin = match c.drive.face {
                Some(face) if c.drive.auto_rotate => angle_to(face) * FACE_RATE,
                _ if c.drive.auto_rotate && walk != glam::Vec3::ZERO => {
                    (angle_to(walk) * TURN_RATE).clamp(-MAX_TURN, MAX_TURN)
                }
                _ => 0.0,
            };
            rb.set_angvel(Vector::new(0.0, spin, 0.0), true);
        }
    }
}

/// The air's steering: the horizontal velocity turns toward `want` by at
/// most [`AIR_ACCEL`].
fn air(v: glam::Vec3, want: glam::Vec3, h: f32) -> glam::Vec3 {
    let dv = (horizontal(want) - horizontal(v)).clamp_length_max(AIR_ACCEL * h);
    v + dv
}

/// Walking along the floor: the horizontal part of `walk` laid along the
/// slope at the same speed (measured: 16 along a 30° slope is 13.9
/// across). On a floor too steep to stand on, nothing goes uphill.
fn along_floor(walk: glam::Vec3, normal: glam::Vec3, steep: bool) -> glam::Vec3 {
    let speed = walk.length();
    if speed == 0.0 {
        return glam::Vec3::ZERO;
    }
    let mut along = horizontal((walk - normal * walk.dot(normal)).normalize_or_zero() * speed);
    if steep {
        let down = horizontal(normal).normalize_or_zero();
        let uphill = along.dot(down).min(0.0);
        along -= down * uphill;
    }
    along
}
