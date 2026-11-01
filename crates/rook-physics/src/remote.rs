//! Assemblies the server simulates, on a client: their replicated poses
//! are played back a little behind the newest, between updates, as
//! Roblox's client shows them (docs/behavior/replication.md#physics). The
//! standard snapshot interpolation: each update is kept with the server's
//! time, and every step moves the body, as a kinematic target, to where
//! the updates around the playback time put it (cubic Hermite between
//! their positions and velocities). Contacts, a floor's velocity under a
//! Humanoid, drawing and scripts all see that motion.

use std::collections::VecDeque;

use super::*;

/// How far behind the newest update the client shows a server's assembly.
/// Measured in Studio: a part the server moved at a steady speed was shown
/// 0.127 to 0.164 s (median 0.146) behind the server's newest update the
/// client had, which arrived about 30 times a second.
const DELAY: f64 = 0.146;

/// Past the server's newest frame (its updates are late), a track goes on
/// at its velocity for at most this long, then waits (Inference: the delay
/// is there to cover late updates, so a gap longer than it is a stop).
const EXTRAPOLATE: f64 = DELAY;

/// The playback clock closes the gap to its target over about this long,
/// so updates arriving unevenly (the server falling behind and catching
/// up) don't jerk what's shown; further off than [`JUMP`], it jumps there.
const CATCH_UP: f64 = 1.0;
const JUMP: f64 = 0.5;

/// One update of an assembly: where its body was and how fast it moved.
#[derive(Debug, Clone, Copy)]
struct Snapshot {
    time: f64,
    pose: CFrame,
    /// The velocity of the body's origin (the root part's place), from
    /// the assembly's (its center of mass's) and its spin.
    linear: glam::Vec3,
    angular: glam::Vec3,
}

#[derive(Debug, Default)]
pub(crate) struct Playback {
    tracks: HashMap<RigidBodyHandle, VecDeque<Snapshot>>,
    /// The server time shown now.
    clock: Option<f64>,
    /// The newest update's server time.
    newest: f64,
    /// The server time of the updates being applied.
    receiving: Option<f64>,
}

impl Playback {
    pub(crate) fn forget(&mut self, h: RigidBodyHandle) {
        self.tracks.remove(&h);
    }

    pub(crate) fn plays(&self, h: RigidBodyHandle) -> bool {
        self.tracks.contains_key(&h)
    }

    pub(crate) fn any(&self) -> bool {
        !self.tracks.is_empty()
    }

    /// Advance the clock by a step of `h` seconds.
    fn advance(&mut self, h: f64) -> f64 {
        let target = self.newest - DELAY;
        let clock = match self.clock {
            Some(c) if (target - (c + h)).abs() <= JUMP => {
                let c = c + h;
                c + (target - c) * (h / CATCH_UP).min(1.0)
            }
            _ => target,
        };
        self.clock = Some(clock);
        clock
    }
}

/// Where a track has its body at `t`.
fn sample(track: &VecDeque<Snapshot>, t: f64) -> CFrame {
    let Some(i) = track.iter().position(|s| s.time > t) else {
        let last = track.back().expect("tracks aren't empty");
        let ahead = (t - last.time).clamp(0.0, EXTRAPOLATE) as f32;
        let spin = glam::Quat::from_scaled_axis(last.angular * ahead);
        let turn = spin * glam::Quat::from_mat3(&last.pose.rotation);
        return CFrame {
            position: last.pose.position + last.linear * ahead,
            rotation: glam::Mat3::from_quat(turn.normalize()),
        };
    };
    let Some(a) = i.checked_sub(1).map(|i| track[i]) else {
        return track[0].pose;
    };
    let b = track[i];
    let span = (b.time - a.time) as f32;
    let s = ((t - a.time) as f32 / span).clamp(0.0, 1.0);
    let (s2, s3) = (s * s, s * s * s);
    let position = a.pose.position * (2.0 * s3 - 3.0 * s2 + 1.0)
        + a.linear * span * (s3 - 2.0 * s2 + s)
        + b.pose.position * (-2.0 * s3 + 3.0 * s2)
        + b.linear * span * (s3 - s2);
    let turn = glam::Quat::from_mat3(&a.pose.rotation)
        .slerp(glam::Quat::from_mat3(&b.pose.rotation), s);
    CFrame {
        position,
        rotation: glam::Mat3::from_quat(turn.normalize()),
    }
}

impl Physics {
    /// Apply what arrived from the server in one of its frames, at server
    /// time `time`: the server's assemblies keep their new poses to play
    /// back rather than jump there, and the DOM keeps showing where they
    /// are now, so scripts never see the newest pose ahead of time.
    pub fn receive_updates(&mut self, dom: &mut Dom, time: f64) {
        if !self.built {
            return;
        }
        self.playback.newest = self.playback.newest.max(time);
        self.playback.receiving = Some(time);
        self.sync(dom);
        self.playback.receiving = None;
        // An assembly this frame didn't move held still at its last pose:
        // the server sends only changes, and a part coming to rest may
        // last have sent a velocity it no longer has.
        for track in self.playback.tracks.values_mut() {
            if let Some(&last) = track.back()
                && last.time < time
                && (last.linear != glam::Vec3::ZERO || last.angular != glam::Vec3::ZERO)
            {
                track.push_back(Snapshot {
                    time,
                    linear: glam::Vec3::ZERO,
                    angular: glam::Vec3::ZERO,
                    ..last
                });
            }
        }
        if let Some(props) = self.props.take() {
            for &h in self.playback.tracks.keys() {
                self.write_body(dom, &props, h);
            }
            self.props = Some(props);
        }
        self.cursor = dom.journal().end();
    }

    /// A replicated move of the server's assembly `h`: keep it to play
    /// back. Anything else moving it (a local script) puts it there at
    /// once, as on Roblox, until the next update.
    pub(crate) fn replicated_move(&mut self, dom: &Dom, props: &Props, h: RigidBodyHandle) -> bool {
        let Some(time) = self.playback.receiving else {
            return false;
        };
        let (Some(body), Some(rb)) = (self.bodies.get(&h), self.world.bodies.get(h)) else {
            return false;
        };
        if !body.remote {
            return false;
        }
        // The root part: the others follow it by the server's joints,
        // which needn't be this client's.
        let part = body.asm.root();
        let Some(Value::CFrame(cf)) = dom.property(part, props.cframe) else {
            return false;
        };
        let pose = cf * body.asm.offset_of(part).unwrap_or_default().inverse();
        let vector = |p| match dom.property(part, p) {
            Some(Value::Vector3(v)) => v.0,
            _ => glam::Vec3::ZERO,
        };
        let angular = vector(props.angular_velocity);
        let com = pose.rotation * to_glam(rb.local_center_of_mass());
        let linear = vector(props.linear_velocity) - angular.cross(com);
        let now = self.playback.clock.unwrap_or(time - DELAY);
        let current = Snapshot {
            time: now,
            pose: cframe_of(rb.position()),
            linear: to_glam(rb.linvel()),
            angular: to_glam(rb.angvel()),
        };
        let track = self.playback.tracks.entry(h).or_insert_with(|| {
            // Played on from where it's shown.
            VecDeque::from([current])
        });
        // Updates of one server frame arrive together; the last counts.
        if track.back().is_some_and(|s| s.time >= time) {
            track.pop_back();
        }
        track.push_back(Snapshot {
            time,
            pose,
            linear,
            angular,
        });
        true
    }

    /// Before a step of `h` seconds: move each played assembly to where
    /// its updates have it at the step's end.
    pub(crate) fn play_remote(&mut self, h: f32) {
        if !self.playback.any() {
            return;
        }
        let clock = self.playback.advance(f64::from(h));
        for (handle, track) in &mut self.playback.tracks {
            // Keep the update before the clock, for the span it's in.
            while track.len() > 1 && track[1].time <= clock {
                track.pop_front();
            }
            if let Some(rb) = self.world.bodies.get_mut(*handle) {
                rb.set_next_kinematic_position(pose_of(&sample(track, clock)));
            }
        }
    }
}
