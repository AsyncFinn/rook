//! The state machine, run once a frame after the steps, as Roblox updates
//! states before Heartbeat (measured transitions and timings in
//! `docs/behavior/characters/movement.md`).

use super::*;

/// A grounded state turns to Freefall after this long without a floor
/// (measured: 0.1 s off a ledge, at any gravity).
const COYOTE: f32 = 0.1;

/// Freefall lands once the root is within this of standing height
/// (Inference: Studio landed at 0.23 studs above and not at 0.88).
const LANDING: f32 = 0.5;

/// Landed lasts two frames (measured).
const LANDED_TIME: f32 = 0.03;

/// How long FallingDown and Ragdoll last before GettingUp, and GettingUp
/// before Running (Approximation: both ended within these in Studio).
const DOWN_TIME: f32 = 1.0;
const GETTING_UP_TIME: f32 = 0.5;

/// Gravity stays off this long after a jump (measured: the launch frame
/// rose as if the first half of it were weightless).
const JUMP_LIFT: f32 = 1.0 / 120.0;

impl Physics {
    pub(super) fn transition(&mut self, c: &mut Controller, r: &Rooted, dt: f32) {
        c.time += dt;
        if c.state.grounded() && c.floor.is_none() {
            c.unsupported += dt;
        } else {
            c.unsupported = 0.0;
        }
        if let Some(asked) = c.requested.take()
            && asked != State::None
            && c.enabled(asked)
        {
            self.enter(c, r, asked);
            return;
        }
        if let Some(next) = self.next_state(c, r) {
            self.enter(c, r, next);
        }
    }

    /// Where the state machine goes from here, if anywhere.
    fn next_state(&self, c: &mut Controller, r: &Rooted) -> Option<State> {
        let d = &c.drive;
        let state = c.state;
        if state == State::Dead || state == State::Physics || state == State::None {
            return None;
        }
        if d.dead {
            return Some(State::Dead);
        }
        if d.platform_stand {
            return (state != State::PlatformStanding).then_some(State::PlatformStanding);
        }
        if state == State::PlatformStanding {
            return Some(State::Running);
        }
        if state == State::Seated {
            if d.jump {
                c.clear_jump = true;
                c.clear_sit = true;
                return Some(State::Running);
            }
            return (!d.sit).then_some(State::Running);
        }
        if d.sit {
            return Some(State::Seated);
        }
        let climbable = c.climb.is_some();
        let in_water = c.water.is_some();
        let jumps = matches!(
            state,
            State::Running
                | State::RunningNoPhysics
                | State::StrafingNoPhysics
                | State::Landed
                | State::Climbing
                | State::Swimming
        );
        if jumps && d.jump {
            c.clear_jump = true;
            if c.enabled(State::Jumping) {
                return Some(State::Jumping);
            }
        }
        let landed = || {
            let root = self.root_pose(r).position.y;
            let v = to_glam(self.world.bodies[r.body].linvel());
            c.floor.is_some_and(|f| {
                root - (f.y + d.standing_height) <= LANDING && v.y - f.velocity.y <= 0.0
            })
        };
        match state {
            State::FallingDown | State::Ragdoll => {
                (c.time >= DOWN_TIME).then_some(State::GettingUp)
            }
            State::GettingUp => (c.time >= GETTING_UP_TIME).then_some(State::Running),
            State::Jumping => Some(State::Freefall),
            State::Freefall if in_water => Some(State::Swimming),
            State::Freefall if climbable && d.walk != glam::Vec3::ZERO => Some(State::Climbing),
            State::Freefall if landed() => Some(State::Landed),
            State::Landed => (c.time >= LANDED_TIME).then_some(State::Running),
            State::Running | State::RunningNoPhysics | State::StrafingNoPhysics => {
                if in_water {
                    Some(State::Swimming)
                } else if climbable && d.walk != glam::Vec3::ZERO {
                    Some(State::Climbing)
                } else {
                    (c.unsupported >= COYOTE).then_some(State::Freefall)
                }
            }
            State::Climbing if !climbable => Some(if c.floor.is_some() {
                State::Running
            } else {
                State::Freefall
            }),
            State::Swimming if !in_water => Some(if c.floor.is_some() {
                State::Running
            } else {
                State::Freefall
            }),
            _ => None,
        }
    }

    /// Move to `next`, if it's enabled: a disabled state is skipped, and
    /// the Humanoid stays as it is (Inference from SetStateEnabled's
    /// documented use).
    fn enter(&mut self, c: &mut Controller, r: &Rooted, next: State) {
        let mut next = next;
        if next == State::Landed && !c.enabled(State::Landed) {
            next = State::Running;
        }
        if next == c.state || !c.enabled(next) {
            return;
        }
        c.changes.push((c.state, next));
        c.state = next;
        c.time = 0.0;
        c.unsupported = 0.0;
        if next == State::Jumping {
            let floor = c.floor.as_ref().map_or(glam::Vec3::ZERO, |f| f.velocity);
            let rb = &mut self.world.bodies[r.body];
            let mut v = to_glam(rb.linvel());
            v.y = floor.y + c.drive.jump_speed;
            rb.set_linvel(to_rapier(v), true);
            c.lift = JUMP_LIFT;
        }
    }
}
