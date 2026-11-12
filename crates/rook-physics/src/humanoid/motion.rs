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

/// The legs below standing height: an overdamped spring pushing the root
/// up to it, with no gravity (measured: landing at 14 to 108 studs/s dips
/// about 0.0064 s times that speed, what's left of the dip shrinks about
/// 43% a frame, and the root lags 0.3 below walking up a 30° slope).
const STIFFNESS: f32 = 2890.0;
const DAMPING: f32 = 119.0;

/// The legs above standing height don't hold the root up: gravity takes
/// it, and they pull it down at least at this rate per stud above, up to
/// `DESCENT_SPEED` times standing height per second (measured with gravity
/// off: a constant 8.20 for standing height 3.0075, 13.64 for 5 and 9.55
/// for 3.5, slower from within about a third of a stud; with gravity it
/// falls faster than that, unbraked, until it reaches standing height).
const DESCENT_RATE: f32 = 24.0;
const DESCENT_SPEED: f32 = 2.727;

/// AutoRotate: the turn closes at this rate per second, up to `MAX_TURN`
/// radians per second (measured: the angle left shrinks 14% a frame, at
/// most about 20 degrees a frame).
const TURN_RATE: f32 = 9.0;
const MAX_TURN: f32 = 20.0;
/// A client faces its character to the camera at once, as the frame is
/// drawn ([`Physics::face_now`]); between frames the steps hold that
/// heading, closing any turn within one 240 Hz step.
const FACE_RATE: f32 = 240.0;

/// A body held upright rights itself: the lean closes at this rate per
/// second, up to `MAX_RISE` radians per second (measured: 20 degrees to 5
/// and 40 to 12 in a 60 Hz frame, and about 2,000 degrees a second rising
/// from upside down, in the air or on a floor alike).
const RISE_RATE: f32 = 85.0;
const MAX_RISE: f32 = 35.0;
/// Radians of lean too small to right.
const STOOD: f32 = 1e-4;

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
        let gravity = self.world.gravity;
        let rb = &mut self.world.bodies[r.body];
        let mut v = rb.linvel();
        let walk = c.drive.walk;
        // The controller applies gravity itself, where the state falls
        // (the body's own gravity is off while it's driven).
        let fall = gravity * h;
        let turn = turns(c.state);
        match c.state {
            s if s.grounded() => {
                let felt = c.floor.and_then(|f| {
                    let e = root.position.y - (f.y + c.drive.standing_height);
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
                        let rel = v.y - floor.velocity.y;
                        let rel = if e > 0.0 {
                            descend(rel, fall.y, e, c.drive.standing_height, h)
                        } else {
                            rel + (-DAMPING * rel - STIFFNESS * e) * h
                        };
                        v.y = floor.velocity.y + rel;
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
        rb.set_linvel(v, true);
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

    /// One step of a held body righting itself, about its root part,
    /// which stays put. It keeps its heading.
    pub(super) fn right(&mut self, r: &Rooted, h: f32) {
        let up = self.root_pose(r).rotation * glam::Vec3::Y;
        let lean = up.angle_between(glam::Vec3::Y);
        if lean < STOOD {
            return;
        }
        let by = (lean * (1.0 - (-RISE_RATE * h).exp())).min(MAX_RISE * h);
        // Straight upside down, any way over will do.
        let axis = up.cross(glam::Vec3::Y).try_normalize();
        let spin = glam::Quat::from_axis_angle(axis.unwrap_or(glam::Vec3::X), by);
        let rb = &mut self.world.bodies[r.body];
        let pose = turned_about(rb.position(), r.offset, spin);
        rb.set_position(pose, true);
        self.body_moved(r.body);
    }

    /// Turn `humanoid`'s character to face the horizontal `way` at once,
    /// where its steering would face it there (AutoRotate, a state that
    /// turns). A client calls this once its frame's camera is final, so
    /// the character it draws faces that camera: steered toward the
    /// heading physics saw at the frame's start, it was drawn a frame
    /// behind the mouse (docs/design/architecture.md#frame-order). The
    /// steps until the next frame hold the heading.
    pub fn face_now(&mut self, dom: &mut Dom, humanoid: InstanceId, way: glam::Vec3) {
        let faces = |c: &Controller| c.drive.auto_rotate && turns(c.state);
        if !self.controllers.get(&humanoid).is_some_and(faces) {
            return;
        }
        // Script changes so far go in first (they may rebuild the body),
        // so the journal entries this write leaves are the only ones
        // skipped.
        self.sync(dom);
        let Some(c) = self.controllers.get_mut(&humanoid) else {
            return;
        };
        c.drive.face = Some(way);
        let root = c.drive.root;
        let Some(r) = self.rooted(root) else {
            return;
        };
        let root = self.root_pose(&r);
        let look = horizontal(root.rotation * glam::Vec3::NEG_Z);
        let angle = look.cross(way).y.atan2(look.dot(way));
        if angle == 0.0 {
            return;
        }
        let spin = glam::Quat::from_rotation_y(angle);
        let rb = &mut self.world.bodies[r.body];
        let pose = turned_about(rb.position(), r.offset, spin);
        rb.set_position(pose, true);
        rb.set_angvel(Vector::ZERO, true);
        // Drawn between its last two steps, both facing the new way.
        if let Some(before) = self.before_last_step.get_mut(&r.body) {
            *before = turned_about(before, r.offset, spin);
        }
        self.body_moved(r.body);
        if let Some(props) = self.props.take() {
            self.write_body(dom, &props, r.body);
            self.props = Some(props);
        }
        self.cursor = dom.journal().end();
    }
}

/// States in which AutoRotate turns the character: those held upright,
/// but not while climbing.
fn turns(state: State) -> bool {
    state.upright() && state != State::Climbing
}

/// `pose` turned by `spin` about its root part, `offset` into it, which
/// stays put.
fn turned_about(pose: &Pose, offset: CFrame, spin: glam::Quat) -> Pose {
    let about = (cframe_of(pose) * offset).position;
    Pose::from_parts(
        about + spin * (pose.translation - about),
        (spin * pose.rotation).normalize(),
    )
}

/// The air's steering: the horizontal velocity turns toward `want` by at
/// most [`AIR_ACCEL`].
fn air(v: glam::Vec3, want: glam::Vec3, h: f32) -> glam::Vec3 {
    let dv = (horizontal(want) - horizontal(v)).clamp_length_max(AIR_ACCEL * h);
    v + dv
}

/// One step of the legs `above` standing height: the vertical velocity
/// over the floor, `rel`, goes down by whichever is faster, `fall`
/// (gravity's step) or closing on the legs' pull at the ground's rate. So
/// nothing brakes it before it lands (measured: off a step its first frame
/// was as fast at gravity 20 as at 100, and after that it gains gravity's
/// pace).
fn descend(rel: f32, fall: f32, above: f32, standing_height: f32, h: f32) -> f32 {
    let pull = -(DESCENT_RATE * above).min(DESCENT_SPEED * standing_height);
    let pulled = if rel > pull {
        pull + (rel - pull) * (-GROUND_RATE * h).exp()
    } else {
        rel
    };
    pulled.min(rel + fall)
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
