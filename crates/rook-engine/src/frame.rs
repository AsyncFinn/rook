//! One frame: scripts, content, physics.

use super::*;

type Outcome = (
    rook_content::ContentRef,
    Result<rook_content::Fetched, rook_content::Failure>,
);

/// Sound files and union assets the loader finished this frame.
#[derive(Default)]
struct Finished {
    sounds: Vec<Outcome>,
    unions: Vec<Outcome>,
}

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
        self.runtime.resume_due(Queue::Legacy);
        self.fire(rs, "Stepped", (now, dt));
        self.fire(rs, "PreSimulation", dt);
        self.simulate(dt);
        self.fire(rs, "PostSimulation", dt);
        self.fire(rs, "Heartbeat", dt);
        self.runtime.resume_due(Queue::Task);
        self.start_new_scripts();
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
    /// sounds' `Loaded`, unions' collision geometry).
    fn update_content(&mut self) {
        let start = self.runtime.world().dom.journal().end();
        let mut events = Vec::new();
        let sound_files = self.apply_sound_changes(&mut events);
        self.update_unions();
        let done = self.run_loader(sound_files);
        for (r, outcome) in done.sounds {
            self.sound_loaded(r, outcome, &mut events);
        }
        for (r, outcome) in done.unions {
            self.union_loaded(r, outcome);
        }
        self.fire_sound_events(start, events);
    }

    /// Hand scripts' new content requests, sounds' files and union assets
    /// to the loader and deliver what finished on the engine thread
    /// (architecture §8 "Content"); finished sound files and union assets
    /// are returned. Nothing happens unless something asked for content.
    fn run_loader(&mut self, sound_files: Vec<rook_content::ContentRef>) -> Finished {
        let union_assets = std::mem::take(&mut self.unions.queued);
        let requests = self.runtime.take_content_requests();
        let mut results = self.runtime.take_content_answers();
        let content = &mut self.content;
        let mut done = Finished::default();
        if requests.is_empty()
            && results.is_empty()
            && sound_files.is_empty()
            && union_assets.is_empty()
            && content.tickets.is_empty()
            && self.sounds.tickets.is_empty()
            && self.unions.tickets.is_empty()
        {
            return done;
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
        for r in union_assets {
            let ticket = loader.request_as(r.clone(), rook_content::Want::Collision);
            self.unions.tickets.insert(ticket, r);
        }
        let waited_for = !self.sounds.tickets.is_empty() || !self.unions.tickets.is_empty();
        let finished = if content.wait_for_content && waited_for {
            loader.wait_all()
        } else {
            loader.poll()
        };
        for (ticket, outcome) in finished {
            if let Some(request) = content.tickets.remove(&ticket) {
                results.push((request, outcome.is_ok()));
            } else if let Some(r) = self.sounds.tickets.remove(&ticket) {
                done.sounds.push((r, outcome));
            } else if let Some(r) = self.unions.tickets.remove(&ticket) {
                done.unions.push((r, outcome));
            }
        }
        self.runtime.deliver_content(&results);
        done
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
