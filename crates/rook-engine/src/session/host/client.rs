//! A client end: its DataModel, the replication sink mapping its
//! instances to the server's, and what it sends back.

use super::*;

/// A client end: its DataModel and how its instances map to the server's.
#[derive(Debug)]
pub struct ClientHost {
    pub(super) client: DataModel,
    pub(super) sink: Sink,
    /// Invokes from the server delivered here, by our number: the server's
    /// number for the call.
    pub(super) calls: FxHashMap<u64, u64>,
    pub(super) next_call: u64,
    /// Effect calls on instances that hadn't replicated yet (created and
    /// called in the same server frame), tried again after the next
    /// updates.
    pub(super) early_effects: Vec<(InstanceId, EffectCall)>,
    /// The character pose last sent, so a still character sends nothing.
    pub(super) last_pose: Option<ToServer>,
    /// ClickDetector hovers the frame's own mouse refresh found, to send.
    pub(super) hover_out: Vec<ToServer>,
}

impl ClientHost {
    /// A client DataModel built from what the server sent on joining, with
    /// its LocalScripts started. `present` is the input devices already
    /// there, such as pads plugged in before launch: the client hears them
    /// before its scripts start, as Studio lists a pad plugged in before
    /// launch from the start (measured). Roblox's own scripts need it: the
    /// shortcut bar reads each button's picture once, when it starts.
    pub fn new(
        joined: &Joined,
        options: &Options,
        present: &[InputEvent],
    ) -> Result<Self, EngineError> {
        let mut client = DataModel::new_client(options)?;
        let mut sink = Sink::new();
        {
            let mut w = client.runtime().world_mut();
            sink.snapshot(&joined.snapshot, &mut w.dom);
            w.signal_behavior = joined.signal_behavior;
            let dom = &mut w.dom;
            dom.journal_mut().set_enabled(false);
            if let (Some(local), Some(players)) =
                (sink.client_id(joined.player), dom.find_service("Players"))
            {
                if let Some(pid) = dom.property_id(players, "LocalPlayer") {
                    dom.load_property(players, pid, Value::Ref(Some(local.to_ref())));
                }
                crate::chat::set_up_local_player(dom, local);
                // PlayerScripts exists only on the client.
                if let Ok(ps) = dom.create_by_name("PlayerScripts") {
                    dom.attach_unchecked(ps, local);
                    let starter = dom
                        .find_service("StarterPlayer")
                        .and_then(|sp| dom.find_first_child(sp, "StarterPlayerScripts", false));
                    if let Some(starter) = starter {
                        for child in dom.children(starter).to_vec() {
                            let copy = dom.copy_instance(child);
                            dom.attach_unchecked(copy, ps);
                        }
                    }
                }
            }
            dom.journal_mut().set_enabled(true);
        }
        client.reset_cursors();
        client.ensure_mouse();
        for event in present {
            client.input(event.clone());
        }
        if let Some(patch) = &options.core_scripts {
            patch.install(&client);
        }
        client.start();
        Ok(Self {
            client,
            sink,
            calls: FxHashMap::default(),
            next_call: 0,
            early_effects: Vec::new(),
            last_pose: None,
            hover_out: Vec::new(),
        })
    }

    /// Hand effect calls to the client's renderer, by the client's ids;
    /// with `keep`, those on instances it doesn't have yet wait for the
    /// next updates.
    pub(super) fn effect_calls(
        client: &DataModel,
        sink: &Sink,
        early: &mut Vec<(InstanceId, EffectCall)>,
        calls: &[(InstanceId, EffectCall)],
        keep: bool,
    ) {
        let mut w = client.runtime().world_mut();
        for &(id, call) in calls {
            match sink.client_id(id) {
                Some(local) => w.effect_calls.push((local, call)),
                None if keep => early.push((id, call)),
                None => {}
            }
        }
    }

    pub fn client(&self) -> &DataModel {
        &self.client
    }

    pub fn client_mut(&mut self) -> &mut DataModel {
        &mut self.client
    }

    /// One frame of the client.
    pub fn step(&mut self) {
        self.client.step();
        self.forget_freed();
        self.hover();
    }

    /// One frame of the client, `dt` seconds long.
    pub fn step_by(&mut self, dt: f64) {
        self.client.step_by(dt);
        self.forget_freed();
        self.hover();
    }

    /// Copies the client's scripts destroyed and its trim freed are no
    /// longer the server's instances' (their slots may be reused).
    pub(super) fn forget_freed(&mut self) {
        for id in self.client.take_freed() {
            self.sink.forget(id);
        }
    }

    /// The mouse sees what's under it after the frame moved the camera and
    /// the world, as Mouse.Hit follows the camera in Roblox while the
    /// mouse holds still.
    pub(super) fn hover(&mut self) {
        let clicks = self.client.refresh_hover();
        let out = self.fire_clicks(clicks);
        self.hover_out.extend(out);
    }

    pub fn deliver(&mut self, msg: &ToClient) {
        let rt = self.client.runtime();
        match msg {
            &ToClient::Updates { time, ref updates } => {
                let (start, local, added) = {
                    let mut w = rt.world_mut();
                    let start = w.dom.journal().end();
                    let local = w.dom.local_player();
                    let character =
                        |dom: &rook_dom::Dom| local.and_then(|p| dom.ref_property(p, "Character"));
                    let before = character(&w.dom);
                    self.sink.apply(updates, &mut w.dom);
                    let w = &mut *w;
                    w.physics.receive_updates(&mut w.dom, time);
                    let after = character(&w.dom);
                    (start, local, (after != before).then_some(after).flatten())
                };
                rt.dispatch_since(start);
                let early = std::mem::take(&mut self.early_effects);
                Self::effect_calls(
                    &self.client,
                    &self.sink,
                    &mut self.early_effects,
                    &early,
                    false,
                );
                // The local player's new character fires CharacterAdded on
                // the client too.
                if let (Some(local), Some(character)) = (local, added)
                    && let Ok(ud) = rt.push_instance(character)
                {
                    rt.fire_event(local, "CharacterAdded", ud);
                    rt.drain_deferred();
                    self.follow_character(character);
                }
            }
            &ToClient::ActivateTool(activation) => {
                if let Some(tool) = self.sink.client_id(activation.tool) {
                    rt.deliver_tool_activation(None, ToolActivation { tool, ..activation });
                }
            }
            ToClient::Effects(calls) => {
                Self::effect_calls(
                    &self.client,
                    &self.sink,
                    &mut self.early_effects,
                    calls,
                    true,
                );
            }
            ToClient::Remote(RemoteMessage { remote, kind, args }) => {
                let args: Vec<NetValue> = args
                    .iter()
                    .cloned()
                    .map(|mut a| {
                        a.map_instances(&|id| self.sink.client_id(id));
                        a
                    })
                    .collect();
                let remote = remote.and_then(|r| self.sink.client_id(r));
                match kind {
                    RemoteKind::FireClient { .. } | RemoteKind::FireAllClients => {
                        if let Some(r) = remote {
                            rt.deliver_event(r, None, &args);
                        }
                    }
                    RemoteKind::InvokeClient { call, .. } => {
                        self.next_call += 1;
                        self.calls.insert(self.next_call, *call);
                        if let Some(r) = remote {
                            self.client
                                .runtime()
                                .deliver_invoke(r, None, self.next_call, &args);
                        }
                    }
                    RemoteKind::Reply { call, ok } => rt.deliver_reply(*call, *ok, &args),
                    RemoteKind::FireServer | RemoteKind::InvokeServer { .. } => {}
                }
            }
        }
    }

    /// The camera follows a new character: after `CharacterAdded`, the
    /// current camera's CameraSubject becomes its Humanoid and its
    /// CameraType goes back to Custom (Observed, also after a script made
    /// it Scriptable; the PlayerModule sets neither).
    pub(super) fn follow_character(&self, character: InstanceId) {
        let humanoid = {
            let w = self.client.runtime().world();
            w.dom.find_first_child_of_class(character, "Humanoid")
        };
        let (Some(humanoid), Some(camera)) = (humanoid, self.client.current_camera()) else {
            return;
        };
        let custom = rook_reflection::database()
            .enum_by_name("CameraType")
            .and_then(|e| e.items.iter().find(|(name, _)| *name == "Custom"))
            .map(|&(_, value)| value);
        if let Some(custom) = custom {
            self.client
                .set_property(camera, "CameraType", Value::Enum(custom));
        }
        self.client
            .set_property(camera, "CameraSubject", Value::Ref(Some(humanoid.to_ref())));
    }

    /// Where the client's simulation has its character, by the server's
    /// ids: the root part's CFrame and its assembly's velocities.
    pub(super) fn character_pose(&self) -> Option<ToServer> {
        let w = self.client.runtime().world();
        let root = rook_lua::character_root(&w.dom, w.dom.local_player()?)?;
        if !w.physics.simulates(root) {
            return None;
        }
        let velocity = |name| w.dom.vector3_property(root, name).unwrap_or_default();
        Some(ToServer::Pose {
            part: self.sink.server_id(root)?,
            cframe: w.dom.cframe_property(root, "CFrame")?,
            linear: velocity("AssemblyLinearVelocity"),
            angular: velocity("AssemblyAngularVelocity"),
        })
    }

    /// Remote calls, Tool moves and the character's pose since the last
    /// call (a pose only when it changed). An invoke of a RemoteFunction
    /// the server doesn't share fails at once.
    pub fn outgoing(&mut self) -> Vec<ToServer> {
        let mut out = std::mem::take(&mut self.hover_out);
        if let Some(pose) = self.character_pose()
            && self.last_pose.as_ref() != Some(&pose)
        {
            self.last_pose = Some(pose.clone());
            out.push(pose);
        }
        for ToolMove { tool, parent, grip } in self.client.runtime().take_tool_moves() {
            if let (Some(tool), Some(parent)) =
                (self.sink.server_id(tool), self.sink.server_id(parent))
            {
                out.push(ToServer::MoveTool { tool, parent, grip });
            }
        }
        for activation in self.client.runtime().take_tool_activations() {
            if let Some(tool) = self.sink.server_id(activation.tool) {
                out.push(ToServer::ActivateTool(ToolActivation {
                    tool,
                    ..activation
                }));
            }
        }
        for action in self.client.runtime().take_prompt_actions() {
            if let Some(prompt) = self.sink.server_id(action.prompt) {
                out.push(ToServer::Prompt(rook_lua::PromptAction {
                    prompt,
                    ..action
                }));
            }
        }
        for msg in self.client.runtime().take_outbox() {
            let RemoteMessage { remote, kind, args } = msg;
            let args: Vec<NetValue> = args
                .into_iter()
                .map(|mut a| {
                    a.map_instances(&|id| self.sink.server_id(id));
                    a
                })
                .collect();
            let remote = remote.and_then(|r| self.sink.server_id(r));
            let kind = match kind {
                RemoteKind::InvokeServer { call } if remote.is_none() => {
                    self.client.runtime().deliver_reply(
                        call,
                        false,
                        &[NetValue::String(
                            b"RemoteFunction is not replicated".to_vec(),
                        )],
                    );
                    continue;
                }
                RemoteKind::Reply { call, ok } => match self.calls.remove(&call) {
                    Some(original) => RemoteKind::Reply { call: original, ok },
                    None => continue,
                },
                RemoteKind::FireServer if remote.is_none() => continue,
                kind @ (RemoteKind::FireServer | RemoteKind::InvokeServer { .. }) => kind,
                RemoteKind::FireClient { .. }
                | RemoteKind::FireAllClients
                | RemoteKind::InvokeClient { .. } => continue,
            };
            out.push(ToServer::Remote(RemoteMessage { remote, kind, args }));
        }
        out
    }

    /// Feed a window input event to the client. ClickDetector interactions
    /// fire here with the LocalPlayer, and come back as messages for the
    /// server (creator-docs ClickDetector: the events fire on both).
    pub fn input(&mut self, event: InputEvent) -> Vec<ToServer> {
        let clicks = self.client.input(event);
        self.fire_clicks(clicks)
    }

    /// Fire ClickDetector interactions here, and give the server's copies
    /// to send.
    pub(super) fn fire_clicks(
        &self,
        clicks: Vec<crate::bridges::input::ClickEvent>,
    ) -> Vec<ToServer> {
        let mut out = Vec::new();
        for click in clicks {
            let event = click.kind.event();
            let rt = self.client.runtime();
            let local = { rt.world().dom.local_player() };
            if let Some(local) = local
                && let Ok(ud) = rt.push_instance(local)
            {
                rt.fire_event(click.detector, event, ud);
                rt.drain_deferred();
            }
            if let Some(detector) = self.sink.server_id(click.detector) {
                out.push(ToServer::Click { detector, event });
            }
        }
        out
    }
}
