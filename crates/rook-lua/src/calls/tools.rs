//! Tools moving, activating, dropping and being picked up, and their moves
//! and activations passed between the two sides.

use rook_dom::InstanceId;

use crate::{Runtime, ToolActivation, ToolMove};

impl Runtime {
    /// Tools the client's scripts moved or dropped since the last call.
    pub fn take_tool_moves(&self) -> Vec<ToolMove> {
        std::mem::take(&mut self.world_mut().tool_moves)
    }

    /// On the server, repeat a Tool move `player`'s client made, if the
    /// Tool is that player's. Returns the grip weld the server made for
    /// it.
    pub fn repeat_tool_move(
        &self,
        player: InstanceId,
        tool: InstanceId,
        parent: InstanceId,
    ) -> Option<InstanceId> {
        self.guarded(|| crate::api::methods::tools::repeat_move(&self.lua, player, tool, parent))
    }

    /// Tool activations that fired here since the last call, for the other
    /// side.
    pub fn take_tool_activations(&self) -> Vec<ToolActivation> {
        std::mem::take(&mut self.world_mut().tool_activations)
    }

    /// Fire an activation from the other side: on the server, from
    /// `player`'s client, only of a Tool its character holds.
    pub fn deliver_tool_activation(&self, player: Option<InstanceId>, activation: ToolActivation) {
        self.guarded(|| {
            crate::api::methods::tools::deliver_activation(&self.lua, player, activation);
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }

    /// The left button went down or up over the world, on a client: the
    /// Tools the local character holds activate.
    pub fn activate_tools(&self, down: bool) {
        self.guarded(|| {
            crate::api::methods::tools::activate_held(&self.lua, down);
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }

    /// Backspace on a client: the local character drops what it holds.
    pub fn drop_tools(&self) {
        self.guarded(|| {
            crate::api::methods::tools::drop_held(&self.lua);
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }

    /// On the server, `part` began touching `other`: a Tool lying in the
    /// world may be picked up.
    pub fn tool_touched(&self, part: InstanceId, other: InstanceId) {
        self.guarded(|| crate::api::methods::tools::touched(&self.lua, part, other));
    }
}
