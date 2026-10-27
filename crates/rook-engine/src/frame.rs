//! One frame: scripts, content, physics.

use super::*;

impl DataModel {
    /// Run one frame.
    ///
    /// Order (Inference from the creator-docs task scheduler diagram):
    /// legacy resumption (`wait`, `spawn`, `delay`), Stepped/PreSimulation,
    /// [physics], PostSimulation, Heartbeat, `task` resumption; then, on a
    /// client, the render step and GUI layout, just before it's drawn.
    pub fn step(&mut self) {
        self.step_frame(None);
    }

    /// Run one frame `dt` seconds long, as a client drawn at any rate
    /// does: Roblox runs one frame per drawn frame, its physics in 240 Hz
    /// steps within it.
    pub fn step_by(&mut self, dt: f64) {
        self.step_frame(Some(dt));
    }

    fn step_frame(&mut self, dt: Option<f64>) {
        profiling::scope!("DataModel::step");
        let (now, dt) = {
            let mut w = self.runtime.world_mut();
            let dt = match dt {
                Some(dt) => w.scheduler.begin_frame_by(dt),
                None => w.scheduler.begin_frame(),
            };
            (w.scheduler.now(), dt)
        };
        let rs = self.run_service;
        if self.role == Role::Server {
            // Inference: before scripts resume, so one that waited a frame
            // sees its players moved off a removed or recolored team.
            self.runtime.keep_teams();
        }
        if self.role == Role::Server {
            self.lay_out_gui();
        }
        self.update_content();
        self.update_web();
        self.advance_sounds(dt);
        self.update_audio(dt);
        self.runtime.resume_due(Queue::Legacy);
        self.fire(rs, "Stepped", (now, dt));
        self.fire(rs, "PreSimulation", dt);
        self.simulate(dt);
        self.fire(rs, "PostSimulation", dt);
        self.fire(rs, "Heartbeat", dt);
        self.runtime.resume_due(Queue::Task);
        if self.role == Role::Client {
            // A client's frame ends as Roblox's begins, with what's drawn:
            // the render step, then GUI, so the camera script sees the
            // pose this frame's physics left and the drawing shows the
            // same (Roblox: RenderStepped, draw, physics, each frame).
            self.render_phase(dt);
        }
        self.start_new_scripts();
        {
            // Scripts' terrain edits, for the renderer and replication.
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            w.physics.write_terrain(&mut w.dom);
        }
        self.check_render_changes();
        self.check_sounds();
        self.check_unions();
        self.check_explosions();
        self.check_user_settings();
        if self.auto_trim {
            self.trim_journal();
        }
    }

    /// Before the frame is drawn (creator-docs RunService.PreRender;
    /// RenderStepped is its older name), after the functions bound to the
    /// render step; then GUI.
    fn render_phase(&mut self, dt: f64) {
        let rs = self.run_service;
        self.runtime.render_step(dt);
        self.fire(rs, "RenderStepped", dt);
        self.fire(rs, "PreRender", dt);
        // Inference: the engine's own camera moves come after the
        // scripts', just before drawing.
        self.runtime.step_cameras();
        self.place_drawn();
        // A script may have replaced the current camera.
        self.sync_viewport_size();
        self.ease_scrolls();
        self.lay_out_gui();
        self.refresh_gui_hover();
    }

    /// Measured: GUI is laid out after RenderStepped. Styles apply first,
    /// as they decide where and how big things are.
    fn lay_out_gui(&self) {
        self.runtime.restyle();
        self.runtime.sync_gui();
    }

    /// Content, sounds and unions, just before `wait()` resumption: sounds
    /// and unions that changed last frame are looked at, files go to the
    /// loader, and what finished is delivered (PreloadAsync callbacks,
    /// sounds' `Loaded`).
    fn update_content(&mut self) {
        self.update_unions();
        let start = self.runtime.world().dom.journal().end();
        let mut events = Vec::new();
        let sound_files = self.apply_sound_changes(&mut events);
        for (r, outcome) in self.run_loader(sound_files) {
            self.sound_loaded(r, outcome, &mut events);
        }
        self.fire_sound_events(start, events);
    }

    /// Send scripts' new web requests and deliver the answers that came
    /// back, all of them when runs must repeat exactly (as for sounds).
    fn update_web(&mut self) {
        for (id, request) in self.runtime.take_web_requests() {
            self.web.send(id, request);
        }
        for (id, answer) in self.web.finished(self.content.wait_for_sounds) {
            self.runtime.deliver_web(id, answer);
        }
    }

    /// Hand scripts' new content requests and sounds' files to the loader
    /// and deliver what finished on the engine thread (docs/design/architecture.md#engine-systems
    /// "Content"); finished sound files are returned. Nothing happens
    /// unless scripts or sounds asked for content.
    fn run_loader(
        &mut self,
        sound_files: Vec<rook_content::ContentRef>,
    ) -> Vec<(
        rook_content::ContentRef,
        Result<rook_content::Fetched, rook_content::Failure>,
    )> {
        let requests = self.runtime.take_content_requests();
        let mut results = self.runtime.take_content_answers();
        let assets = self.runtime.take_asset_requests();
        let fonts = self.runtime.world_mut().gui.take_font_requests();
        let content = &mut self.content;
        let mut sounds_done = Vec::new();
        if requests.is_empty()
            && results.is_empty()
            && assets.is_empty()
            && sound_files.is_empty()
            && fonts.is_empty()
            && content.tickets.is_empty()
            && content.assets.is_empty()
            && content.fonts.is_empty()
            && self.sounds.tickets.is_empty()
        {
            return sounds_done;
        }
        profiling::scope!("update_content");
        let loader = content.loader.get_or_insert_with(|| {
            let resolver = content.resolver.clone().unwrap_or_else(|| {
                Arc::new(rook_content::Resolver::new(
                    crate::settings::content_config(false),
                ))
            });
            rook_content::Loader::new(resolver)
        });
        for (request, reference, want) in requests {
            content
                .tickets
                .insert(loader.request_as(reference, want), request);
        }
        let mut assets_done = Vec::new();
        for (request, id) in assets {
            match u64::try_from(id).ok().filter(|&id| id > 0) {
                Some(id) => {
                    let r = rook_content::ContentRef::Asset { id, version: None };
                    content.assets.insert(loader.request(r), request);
                }
                None => assets_done.push((request, Err(rook_content::FailureKind::NotFound))),
            }
        }
        for r in sound_files {
            let ticket = loader.request_as(r.clone(), rook_content::Want::Audio);
            self.sounds.tickets.insert(ticket, r);
        }
        for url in fonts {
            match rook_content::ContentRef::parse(&url) {
                Some(r) => {
                    content.fonts.insert(loader.request(r), url);
                }
                None => self.runtime.world_mut().gui.font_arrived(&url, None),
            }
        }
        // Runs that must repeat exactly wait for fonts as for sounds.
        let waiting = !self.sounds.tickets.is_empty() || !content.fonts.is_empty();
        let finished = if content.wait_for_sounds && waiting {
            loader.wait_all()
        } else {
            loader.poll()
        };
        for (ticket, outcome) in finished {
            if let Some(request) = content.tickets.remove(&ticket) {
                results.push((request, outcome.is_ok()));
            } else if let Some(request) = content.assets.remove(&ticket) {
                assets_done.push((request, read_asset(outcome)));
            } else if let Some(r) = self.sounds.tickets.remove(&ticket) {
                sounds_done.push((r, outcome));
            } else if let Some(url) = content.fonts.remove(&ticket) {
                let bytes = outcome.ok().map(|f| f.bytes);
                self.runtime.world_mut().gui.font_arrived(&url, bytes);
            }
        }
        self.runtime.deliver_content(&results);
        for (request, fill) in assets_done {
            self.runtime.deliver_asset(request, fill);
        }
        sounds_done
    }

    /// Step physics and write the results into the DOM. Each side
    /// simulates what it owns (`rook_lua::authority`): a client its own
    /// character, the server the rest, with Explosions going off first.
    /// Parts that fell below Workspace.FallenPartsDestroyHeight are
    /// destroyed on the server.
    fn simulate(&mut self, dt: f64) {
        let server = self.role == Role::Server;
        let authority = rook_lua::authority(&self.runtime.world());
        if authority == rook_physics::Authority::Only(Vec::new()) {
            // A client with no character: its world only answers queries.
            return;
        }
        let blasts = if server {
            self.age_explosions(dt);
            self.set_off_explosions()
        } else {
            Vec::new()
        };
        let (fallen, touches, reached, humanoids) = {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            w.physics.set_authority(&w.dom, authority);
            w.physics.sync(&mut w.dom);
            let drives = rook_lua::humanoid_drives(w);
            w.physics.drive_humanoids(drives);
            w.physics.step(dt);
            let fallen = w.physics.write_back(&mut w.dom);
            let humanoids = w.physics.humanoid_reports(&w.dom);
            (
                fallen,
                w.physics.take_touches(),
                w.physics.take_reached(),
                humanoids,
            )
        };
        self.runtime.humanoids_stepped(humanoids);
        self.fire_explosion_hits(blasts);
        // Both parts fire, the lower id first (observed: the part made
        // first fired first in Studio; the real order is unknown).
        for t in touches {
            let event = if t.began { "Touched" } else { "TouchEnded" };
            for (part, other) in [(t.part, t.other), (t.other, t.part)] {
                if let Ok(other) = self.runtime.push_instance(other) {
                    self.runtime.fire_event(part, event, other);
                }
                let is_spawn = self.runtime.world().dom.is_a(part, "SpawnLocation");
                if server && t.began && is_spawn {
                    self.runtime.spawn_touched(part, other);
                }
            }
        }
        for rocket in reached {
            self.runtime.fire_event(rocket, "ReachedTarget", ());
        }
        self.runtime.drain_deferred();
        if server {
            for id in fallen {
                self.runtime.destroy(id);
            }
        }
    }
}

/// Read a fetched model asset, for scripts that load it: a function that
/// copies its instances into the Model they get.
fn read_asset(
    outcome: Result<rook_content::Fetched, rook_content::Failure>,
) -> rook_lua::AssetFill {
    let fetched = outcome.map_err(|f| f.kind)?;
    let loaded =
        rook_formats::load(&fetched.bytes).map_err(|_| rook_content::FailureKind::Invalid)?;
    Ok(Box::new(move |dom, model| {
        rook_formats::into_dom(&loaded.dom, dom, model);
    }))
}
