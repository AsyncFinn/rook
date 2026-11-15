//! Players and their characters: spawning, teams, SpawnLocations and the
//! Humanoid controller's reports.

use rook_dom::InstanceId;

use crate::runtime::ctx::ScriptError;
use crate::{MessageType, Runtime};

impl Runtime {
    /// Spawn a player's character (`Player:LoadCharacter()`), firing the
    /// signals it causes.
    pub fn load_character(&self, player: InstanceId) -> Result<InstanceId, ScriptError> {
        self.guarded(|| {
            let r = crate::api::methods::characters::spawn_character(&self.lua, player);
            crate::runtime::threads::drain_deferred(&self.lua);
            r
        })
    }

    /// A player joined: it goes on an auto-assignable team, and its look
    /// is asked of the avatar API. Returns whether the look is on its way
    /// (its first spawn waits for it: `World::avatars_pending`).
    pub fn welcome_player(&self, player: InstanceId) -> bool {
        let start = self.world().dom.journal().end();
        {
            let mut w = self.world_mut();
            if let Err(e) =
                crate::api::methods::characters::spawning::assign_team(&mut w.dom, player)
            {
                w.push_log(MessageType::Error, format!("assigning a team: {e}"));
            }
        }
        self.dispatch_since(start);
        crate::api::methods::characters::request_avatar(&self.lua, player)
    }

    /// A part touched a SpawnLocation (team changes on touch).
    pub fn spawn_touched(&self, spawn: InstanceId, part: InstanceId) {
        let start = self.world().dom.journal().end();
        {
            let mut w = self.world_mut();
            let result =
                crate::api::methods::characters::spawning::spawn_touched(&mut w.dom, spawn, part);
            if let Err(e) = result {
                w.push_log(MessageType::Error, format!("changing team on touch: {e}"));
            }
        }
        self.dispatch_since(start);
    }

    /// The Humanoid controller stepped: its reports' property changes and
    /// state events.
    pub fn humanoids_stepped(&self, reports: Vec<rook_physics::humanoid::Report>) {
        if reports.is_empty() {
            return;
        }
        let result =
            self.guarded(|| crate::api::methods::characters::movement::stepped(&self.lua, reports));
        if let Err(e) = result {
            tracing::warn!("Humanoid events: {e}");
        }
    }
}
