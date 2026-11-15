//! ProximityPrompts: which show, keys and holds, and their events passed to
//! the server.

use rook_dom::InstanceId;

use crate::{PromptAction, Runtime};

impl Runtime {
    /// A client's frame for ProximityPrompts: which show, and which holds
    /// trigger.
    pub fn update_prompts(&self) {
        self.guarded(|| {
            crate::api::methods::prompts::update(&self.lua);
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }

    /// A key (`Enum.KeyCode` value) went down or up on a client, past a
    /// focused TextBox: the shown prompts on it trigger or are let go.
    /// Returns whether prompts took it.
    pub fn prompt_key(&self, key: u32, down: bool) -> bool {
        self.guarded(|| {
            let taken = crate::api::methods::prompts::key(&self.lua, key, down);
            crate::runtime::threads::drain_deferred(&self.lua);
            taken
        })
    }

    /// Prompt events that fired here since the last call, for the server.
    pub fn take_prompt_actions(&self) -> Vec<PromptAction> {
        self.guarded(|| crate::api::methods::prompts::take_actions(&self.lua))
    }

    /// Fire a prompt event `player`'s client sent, on the server.
    pub fn deliver_prompt_action(&self, player: InstanceId, action: PromptAction) {
        self.guarded(|| {
            crate::api::methods::prompts::deliver(&self.lua, player, action);
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }
}
