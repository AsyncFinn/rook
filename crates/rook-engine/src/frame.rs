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
        self.runtime.resume_due(Queue::Legacy);
        self.fire(rs, "Stepped", (now, dt));
        self.fire(rs, "PreSimulation", dt);
        self.simulate(dt);
        self.fire(rs, "PostSimulation", dt);
        self.fire(rs, "Heartbeat", dt);
        self.runtime.resume_due(Queue::Task);
        self.start_new_scripts();
        self.check_render_changes();
        if self.auto_trim {
            self.trim_journal();
        }
    }

    /// Hand scripts' new content requests to the loader and deliver what
    /// finished, just before `wait()` resumption: PreloadAsync callbacks
    /// run and finished calls return on the engine thread (architecture §8
    /// "Content"). Nothing happens unless scripts asked for content.
    fn update_content(&mut self) {
        let requests = self.runtime.take_content_requests();
        let mut results = self.runtime.take_content_answers();
        let content = &mut self.content;
        if requests.is_empty() && results.is_empty() && content.tickets.is_empty() {
            return;
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
        for (ticket, outcome) in loader.poll() {
            if let Some(request) = content.tickets.remove(&ticket) {
                results.push((request, outcome.is_ok()));
            }
        }
        self.runtime.deliver_content(&results);
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
