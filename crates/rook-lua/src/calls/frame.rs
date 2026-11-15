//! A client's per-frame work scripts see: render-step bindings, cameras,
//! StyleSheets, GUI layout, the names over heads and teams.

use crate::Runtime;

impl Runtime {
    /// Move interpolating cameras and keep a Scriptable camera's Focus
    /// (clients, each frame after RenderStepped).
    pub fn step_cameras(&self) {
        self.guarded(|| {
            crate::api::methods::camera::moves::step(&self.lua);
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }

    /// Run what scripts bound with `RunService:BindToRenderStep`, by
    /// priority (clients, each frame before RenderStepped).
    pub fn render_step(&self, dt: f64) {
        let Ok(step) = self
            .lua
            .named_registry_value::<mlua::Function>("rook.render_step")
        else {
            return;
        };
        self.guarded(|| {
            // Each binding runs on its own thread and reports its own errors.
            let _ = step.call::<()>(dt);
            crate::runtime::threads::drain_deferred(&self.lua);
        });
    }

    /// Apply StyleSheets to what changed since the last call.
    pub fn restyle(&self) {
        let start = self.world().dom.journal().end();
        let styled = self.guarded(|| crate::styles::restyle(&self.lua));
        if let Err(e) = styled {
            tracing::warn!("styling failed: {e}");
        }
        self.dispatch_since(start);
    }

    /// Lay GUI out again if it changed, as Roblox does each frame after
    /// RenderStepped, firing Changed for what moved.
    pub fn sync_gui(&self) {
        let start = {
            let mut w = self.world_mut();
            let w = &mut *w;
            let start = w.dom.journal().end();
            w.gui.sync(&mut w.dom);
            start
        };
        self.dispatch_since(start);
    }

    /// Work out the names and health bars over heads for this frame's
    /// drawing, once a frame (they look through the world, so they cost
    /// a ray each, all through one filter).
    pub fn sync_overheads(&self) {
        let mut w = self.world_mut();
        let w = &mut *w;
        let physics = &mut w.physics;
        w.gui.sync_overheads(&mut w.dom, |dom, lines, ignore| {
            let filter = rook_physics::Filter::legacy_ray(ignore.iter().copied(), false);
            (lines.iter())
                .map(|&(from, to)| physics.raycast(dom, from, to - from, &filter).is_some())
                .collect()
        });
    }

    /// Move players whose team left Teams or changed color, firing what
    /// that fires (once a frame, before scripts resume).
    pub fn keep_teams(&self) {
        let start = {
            let mut w = self.world_mut();
            let start = w.dom.journal().end();
            if let Err(e) = crate::api::methods::characters::spawning::keep_teams(&mut w.dom) {
                tracing::warn!("keeping teams failed: {e}");
            }
            start
        };
        self.dispatch_since(start);
    }
}
