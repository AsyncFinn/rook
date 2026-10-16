//! One frame: scripts, content, physics.

use super::*;

impl DataModel {
    /// Run one frame.
    ///
    /// Order (Inference from the creator-docs task scheduler diagram):
    /// legacy resumption (`wait`, `spawn`, `delay`), Stepped/PreSimulation,
    /// [physics], PostSimulation, Heartbeat, `task` resumption.
    pub fn step(&mut self) {
        profiling::scope!("DataModel::step");
        let (now, dt) = {
            let mut w = self.runtime.world_mut();
            let dt = w.scheduler.begin_frame();
            (w.scheduler.now(), dt)
        };
        let rs = self.run_service;
        if self.role == Role::Client {
            // Before the frame is drawn (creator-docs RunService.PreRender;
            // RenderStepped is its older name).
            self.fire(rs, "RenderStepped", dt);
            self.fire(rs, "PreRender", dt);
        }
        self.update_content();
        self.advance_sounds(dt);
        self.update_audio(dt);
        self.runtime.resume_due(Queue::Legacy);
        self.fire(rs, "Stepped", (now, dt));
        self.fire(rs, "PreSimulation", dt);
        self.simulate(dt);
        self.fire(rs, "PostSimulation", dt);
        self.fire(rs, "Heartbeat", dt);
        self.runtime.resume_due(Queue::Task);
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
        if self.auto_trim {
            self.trim_journal();
        }
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

    /// Hand scripts' new content requests and sounds' files to the loader
    /// and deliver what finished on the engine thread (architecture §8
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
        let content = &mut self.content;
        let mut sounds_done = Vec::new();
        if requests.is_empty()
            && results.is_empty()
            && sound_files.is_empty()
            && content.tickets.is_empty()
            && self.sounds.tickets.is_empty()
        {
            return sounds_done;
        }
        profiling::scope!("update_content");
        let loader = content.loader.get_or_insert_with(|| {
            let resolver = content.resolver.clone().unwrap_or_else(|| {
                Arc::new(rook_content::Resolver::new(rook_content::Config::from_env(
                    false,
                )))
            });
            rook_content::Loader::new(resolver)
        });
        for (request, reference, want) in requests {
            content
                .tickets
                .insert(loader.request_as(reference, want), request);
        }
        for r in sound_files {
            let ticket = loader.request_as(r.clone(), rook_content::Want::Audio);
            self.sounds.tickets.insert(ticket, r);
        }
        let finished = if content.wait_for_sounds && !self.sounds.tickets.is_empty() {
            loader.wait_all()
        } else {
            loader.poll()
        };
        for (ticket, outcome) in finished {
            if let Some(request) = content.tickets.remove(&ticket) {
                results.push((request, outcome.is_ok()));
            } else if let Some(r) = self.sounds.tickets.remove(&ticket) {
                sounds_done.push((r, outcome));
            }
        }
        self.runtime.deliver_content(&results);
        sounds_done
    }

    /// Step physics and write the results into the DOM. Parts that fell
    /// below Workspace.FallenPartsDestroyHeight are destroyed.
    fn simulate(&mut self, dt: f64) {
        if self.role == Role::Client {
            return;
        }
        let (fallen, touches, reached) = {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            w.physics.sync(&mut w.dom);
            w.physics.step(dt);
            let fallen = w.physics.write_back(&mut w.dom);
            (fallen, w.physics.take_touches(), w.physics.take_reached())
        };
        // Both parts fire, the lower id first (observed: the part made
        // first fired first in Studio; the real order is unknown).
        for t in touches {
            let event = if t.began { "Touched" } else { "TouchEnded" };
            for (part, other) in [(t.part, t.other), (t.other, t.part)] {
                if let Ok(other) = self.runtime.push_instance(other) {
                    let _ = self.runtime.fire_event(part, event, other);
                }
            }
        }
        for rocket in reached {
            let _ = self.runtime.fire_event(rocket, "ReachedTarget", ());
        }
        self.runtime.drain_deferred();
        for id in fallen {
            self.runtime.destroy(id);
        }
    }
}
