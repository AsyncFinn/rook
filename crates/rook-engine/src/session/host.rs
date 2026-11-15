//! The two ends of a play session, which only talk through messages: a
//! [`ServerHost`] owns the server DataModel, a [`ClientHost`] one client's.
//! Each can run on its own thread (docs/design/architecture/threads-and-frames.md#threading-and-data-flow: a DataModel's Luau, DOM
//! and physics stay on one thread); [`crate::Session`] runs them together
//! in one, delivering each message as soon as it's sent.
//!
//! Remote calls cross with the sender's instance ids translated into the
//! server's: the server sends and receives its own ids, and each client
//! translates through its replication [`Sink`]. Each end numbers the
//! invokes it delivers and translates replies back to the caller's number.

mod client;
mod server;

pub use client::ClientHost;
pub use server::{STARTER_PLAYER_COPIES, ServerHost};

use rustc_hash::FxHashMap;

use rook_dom::InstanceId;
use rook_lua::{EffectCall, NetValue, RemoteKind, RemoteMessage, ToolActivation, ToolMove};
use rook_net::{Sink, Snapshot, Source, Update};
use rook_types::Value;

use crate::{DataModel, EngineError, InputEvent, Options};

#[derive(Debug, Clone)]
pub enum ToClient {
    /// A remote event, invoke or reply, with the server's instance ids.
    Remote(RemoteMessage),
    /// Replicated changes from the server frame that ended at server time
    /// `time`.
    Updates { time: f64, updates: Vec<Update> },
    /// Effect methods the server's scripts called (`ParticleEmitter:Emit`
    /// and the like), which show on every client.
    Effects(Vec<(InstanceId, EffectCall)>),
    /// A server script activated a Tool this client's character holds:
    /// it fires here too.
    ActivateTool(ToolActivation),
    /// What scripts made this client the owner of now (network
    /// ownership), sent when it changes.
    Owned(rook_lua::OwnedHere),
}

/// From a client to the server, with the server's instance ids.
#[derive(Debug, Clone, PartialEq)]
pub enum ToServer {
    Remote(RemoteMessage),
    /// The player clicked a ClickDetector: its events fire on the server
    /// too, with the server's Player (creator-docs ClickDetector).
    Click {
        detector: InstanceId,
        event: &'static str,
    },
    /// The client's scripts moved one of its player's Tools between their
    /// Backpack and character (equipping or unequipping it), or dropped
    /// it, which the server repeats.
    MoveTool {
        tool: InstanceId,
        parent: InstanceId,
        /// The grip the client made, by the client's own id: the server's
        /// grip replicates as this one.
        grip: Option<InstanceId>,
    },
    /// A Tool the player's character holds was activated or deactivated
    /// here: it fires on the server too.
    ActivateTool(ToolActivation),
    /// A ProximityPrompt's hold or trigger event fired here: it fires on
    /// the server too, with the server's Player.
    Prompt(rook_lua::PromptAction),
    /// Where the client's simulation put an assembly it owns (its
    /// character, or what a script made it the owner of): the root part,
    /// which the rest of the assembly follows.
    Pose {
        part: InstanceId,
        cframe: rook_types::CFrame,
        linear: glam::Vec3,
        angular: glam::Vec3,
    },
}

/// What a client needs to start: the replicated game and who it is.
#[derive(Debug, Clone)]
pub struct Joined {
    pub snapshot: Snapshot,
    pub player: InstanceId,
    /// The client handles signals the way the server does.
    pub signal_behavior: rook_lua::SignalBehavior,
}
