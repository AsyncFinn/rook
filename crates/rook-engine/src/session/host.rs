//! The two ends of a play session, which only talk through messages: a
//! [`ServerHost`] owns the server DataModel, a [`ClientHost`] one client's.
//! Each can run on its own thread (docs/design/architecture.md#engine-systems: a DataModel's Luau, DOM
//! and physics stay on one thread); [`crate::Session`] runs them together
//! in one, delivering each message as soon as it's sent.
//!
//! Remote calls cross with the sender's instance ids translated into the
//! server's: the server sends and receives its own ids, and each client
//! translates through its replication [`Sink`]. Each end numbers the
//! invokes it delivers and translates replies back to the caller's number.

use std::collections::HashMap;

use rook_dom::InstanceId;
use rook_lua::{EffectCall, NetValue, RemoteKind, RemoteMessage};
use rook_net::{Sink, Snapshot, Source, Update};
use rook_types::Value;

use crate::{DataModel, EngineError, InputEvent, Options};

/// From the server to one client.
#[derive(Debug, Clone)]
pub enum ToClient {
    /// A remote event, invoke or reply, with the server's instance ids.
    Remote(RemoteMessage),
    /// Replicated changes.
    Updates(Vec<Update>),
    /// Effect methods the server's scripts called (`ParticleEmitter:Emit`
    /// and the like), which show on every client.
    Effects(Vec<(InstanceId, EffectCall)>),
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
    /// Backpack and character (equipping or unequipping it), which the
    /// server repeats.
    MoveTool {
        tool: InstanceId,
        parent: InstanceId,
    },
    /// Where the client's simulation put its character: the root part of
    /// the assembly, which the rest of it follows (network ownership).
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
    /// The server's Player for this client.
    pub player: InstanceId,
    /// The client handles signals the way the server does.
    pub signal_behavior: rook_lua::SignalBehavior,
}

#[derive(Debug)]
struct Link {
    source: Source,
    player: InstanceId,
}

/// The server end: its DataModel and what each client should see.
#[derive(Debug)]
pub struct ServerHost {
    server: DataModel,
    clients: Vec<Link>,
    /// Invokes from clients delivered here, by our number: the client and
    /// its own number for the call.
    calls: HashMap<u64, (usize, u64)>,
    next_call: u64,
    /// Players whose first character spawns at the start of a step, once
    /// their look has arrived.
    spawning: Vec<InstanceId>,
    /// Humanoids that died, and when their players respawn (`None`
    /// without `CharacterAutoLoads`).
    dead: HashMap<InstanceId, (InstanceId, Option<f64>)>,
}

impl ServerHost {
    /// Host a server (its scripts already started).
    pub fn new(mut server: DataModel) -> Self {
        // Each client's source reads the frame's changes before they go.
        server.set_auto_trim(false);
        Self {
            server,
            clients: Vec::new(),
            calls: HashMap::new(),
            next_call: 0,
            spawning: Vec::new(),
            dead: HashMap::new(),
        }
    }

    pub fn server(&self) -> &DataModel {
        &self.server
    }

    pub fn server_mut(&mut self) -> &mut DataModel {
        &mut self.server
    }

    /// The server's Player of each client, by index.
    pub fn player(&self, client: usize) -> Option<InstanceId> {
        self.clients.get(client).map(|c| c.player)
    }

    /// A player joins: the server gets a Player (with an empty Backpack and
    /// PlayerGui) and fires PlayerAdded. Returns the client's index and
    /// what it starts with. With `Players.CharacterAutoLoads` the
    /// character spawns at the start of the next step, once the client
    /// runs, so it arrives like any later change.
    pub fn join(&mut self, name: &str, user_id: i64) -> Result<(usize, Joined), EngineError> {
        let (player, start) = {
            let mut w = self.server.runtime().world_mut();
            let dom = &mut w.dom;
            let start = dom.journal().end();
            let players = dom.get_service("Players")?;
            let player = dom.create_by_name("Player")?;
            dom.set_name(player, name.to_owned())?;
            dom.set_property_by_name(player, "UserId", Value::Int64(user_id))?;
            dom.set_property_by_name(player, "DisplayName", Value::String(name.to_owned()))?;
            for container in ["Backpack", "PlayerGui"] {
                let c = dom.create_by_name(container)?;
                dom.set_parent(c, Some(player))?;
            }
            // Observed: a Player takes these from StarterPlayer once, as it
            // joins; later changes to StarterPlayer don't reach it.
            if let Some(starter) = dom.find_service("StarterPlayer") {
                for (from, to) in STARTER_PLAYER_COPIES {
                    #[expect(
                        clippy::disallowed_methods,
                        reason = "copied as they are, whatever their types"
                    )]
                    let copied = dom
                        .property_by_name(starter, from)
                        .map(|v| dom.set_property_by_name(player, to, v));
                    if let Some(Err(e)) = copied {
                        tracing::warn!("Player.{to} not copied from StarterPlayer: {e}");
                    }
                }
            }
            dom.set_parent(player, Some(players))?;
            w.client_players.insert(player);
            (player, start)
        };
        let players = self.server.runtime().world().dom.find_service("Players");
        self.server.runtime().dispatch_since(start);
        self.server.runtime().welcome_player(player);
        if let Some(players) = players {
            let ud = self.server.runtime().push_instance(player);
            if let Ok(ud) = ud {
                self.server.runtime().fire_event(players, "PlayerAdded", ud);
            }
            self.server.runtime().drain_deferred();
        }
        let auto_loads = players.is_none_or(|p| {
            self.server
                .runtime()
                .world()
                .dom
                .bool_property(p, "CharacterAutoLoads")
                != Some(false)
        });
        if auto_loads {
            self.spawning.push(player);
        }
        let mut source = Source::new(Some(player));
        let (snapshot, signal_behavior) = {
            let w = self.server.runtime().world();
            (source.snapshot(&w.dom), w.signal_behavior)
        };
        self.clients.push(Link { source, player });
        Ok((
            self.clients.len() - 1,
            Joined {
                snapshot,
                player,
                signal_behavior,
            },
        ))
    }

    /// One frame of the server: players whose looks arrived and dead ones
    /// whose `Players.RespawnTime` passed spawn first, and characters that
    /// died during it are noticed after.
    pub fn step(&mut self) {
        let now = self.server.now();
        let mut due = Vec::new();
        let pending = self.server.runtime().world().avatars_pending.clone();
        self.spawning.retain(|&p| {
            let waiting = pending.contains(&p);
            if !waiting {
                due.push(p);
            }
            waiting
        });
        self.dead.retain(|_, (p, at)| {
            let respawn = at.is_some_and(|at| at <= now + 1e-9);
            if respawn {
                due.push(*p);
            }
            !respawn
        });
        for player in due {
            let gone = self.server.runtime().world().dom.parent(player).is_none();
            if gone {
                continue;
            }
            if let Err(e) = self.server.runtime().load_character(player) {
                let mut w = self.server.runtime().world_mut();
                w.push_log(rook_lua::MessageType::Error, format!("LoadCharacter: {e}"));
            }
        }
        self.server.step();
        self.notice_deaths();
    }

    /// Fire `Died` for players' Humanoids whose health reached 0, and set
    /// their respawn (creator-docs Players.RespawnTime; measured in Studio
    /// 0.744: the new character came 5.3 s after Died with the default 5).
    /// Approximation: the body stays whole until health and death (M9
    /// step 4) break its joints.
    fn notice_deaths(&mut self) {
        let now = self.server.now();
        let died: Vec<(InstanceId, InstanceId, Option<f64>)> = {
            let w = self.server.runtime().world();
            let dom = &w.dom;
            let players = dom.find_service("Players");
            let respawn_time = players
                .and_then(|p| dom.number_property(p, "RespawnTime"))
                .unwrap_or(5.0);
            let auto_loads =
                players.is_none_or(|p| dom.bool_property(p, "CharacterAutoLoads") != Some(false));
            self.clients
                .iter()
                .filter_map(|c| {
                    let character = dom.ref_property(c.player, "Character")?;
                    let humanoid = dom.find_first_child_of_class(character, "Humanoid")?;
                    let health = dom.number_property(humanoid, "Health_XML")?;
                    (health <= 0.0 && !self.dead.contains_key(&humanoid))
                        .then(|| (humanoid, c.player, auto_loads.then_some(now + respawn_time)))
                })
                .collect()
        };
        for (humanoid, player, respawn) in died {
            self.dead.insert(humanoid, (player, respawn));
            self.server.runtime().fire_event(humanoid, "Died", ());
            self.server.runtime().drain_deferred();
        }
    }

    /// The replicated changes since the last call, for each client in turn.
    /// Afterwards the server's journal is trimmed.
    pub fn updates(&mut self) -> Vec<Vec<Update>> {
        let updates = {
            let w = self.server.runtime().world();
            self.clients
                .iter_mut()
                .map(|c| c.source.updates(&w.dom))
                .collect()
        };
        self.server.trim_journal();
        updates
    }

    /// Remote calls the server's scripts made since the last call, by the
    /// client each goes to. An invoke to a player with no client fails at
    /// once.
    pub fn outgoing(&mut self) -> Vec<(usize, ToClient)> {
        let mut out = Vec::new();
        // Effect calls go to the clients' renderers: those each step moved
        // into the server's render changes, then any made since. Without
        // clients (Run mode) the server's own renderer takes them.
        if !self.clients.is_empty() {
            let mut calls = std::mem::take(&mut self.server.render_changes.effect_calls);
            calls.append(&mut self.server.runtime().world_mut().effect_calls);
            if !calls.is_empty() {
                for i in 0..self.clients.len() {
                    out.push((i, ToClient::Effects(calls.clone())));
                }
            }
        }
        for msg in self.server.runtime().take_outbox() {
            let RemoteMessage { remote, kind, args } = msg;
            let mut send = |index: usize, kind: RemoteKind| {
                out.push((
                    index,
                    ToClient::Remote(RemoteMessage {
                        remote,
                        kind,
                        args: args.clone(),
                    }),
                ));
            };
            match kind {
                RemoteKind::FireClient { player } => {
                    for (i, _) in self
                        .clients
                        .iter()
                        .enumerate()
                        .filter(|(_, c)| c.player == player)
                    {
                        send(i, RemoteKind::FireClient { player });
                    }
                }
                RemoteKind::FireAllClients => {
                    for i in 0..self.clients.len() {
                        send(i, RemoteKind::FireAllClients);
                    }
                }
                RemoteKind::InvokeClient { player, call } => {
                    match self.clients.iter().position(|c| c.player == player) {
                        Some(i) => send(i, RemoteKind::InvokeClient { player, call }),
                        None => self.server.runtime().deliver_reply(
                            call,
                            false,
                            &[NetValue::String(b"Player is not connected".to_vec())],
                        ),
                    }
                }
                RemoteKind::Reply { call, ok } => {
                    if let Some((index, original)) = self.calls.remove(&call) {
                        send(index, RemoteKind::Reply { call: original, ok });
                    }
                }
                RemoteKind::FireServer | RemoteKind::InvokeServer { .. } => {}
            }
        }
        out
    }

    /// Take client `index`'s simulation of its character: the assembly of
    /// `part` moves there, as a script's move would, if its player's client
    /// owns it. Other clients see the move; the sender isn't sent its own
    /// move back (its later changes by the server's scripts do reach it).
    /// Physics writes fire no Changed signals, and neither does this.
    fn apply_pose(
        &mut self,
        index: usize,
        player: InstanceId,
        part: InstanceId,
        cframe: rook_types::CFrame,
        [linear, angular]: [glam::Vec3; 2],
    ) {
        let mut w = self.server.runtime().world_mut();
        let owned = w
            .dom
            .ref_property(player, "Character")
            .is_some_and(|c| w.dom.is_ancestor_of(c, part));
        if !owned {
            return;
        }
        let start = w.dom.journal().end();
        let dom = &mut w.dom;
        let moved = rook_physics::assembly::move_part(dom, part, cframe)
            .and_then(|()| {
                rook_physics::assembly::set_velocity(dom, part, "AssemblyLinearVelocity", linear)
            })
            .and_then(|()| {
                rook_physics::assembly::set_velocity(dom, part, "AssemblyAngularVelocity", angular)
            });
        if let Err(e) = moved {
            tracing::warn!("a client's character move failed: {e}");
        }
        let source = &mut self.clients[index].source;
        for change in w.dom.journal().since(start) {
            if let rook_dom::Change::Property { id, property } = change
                && let Some(value) = w.dom.property(*id, *property)
            {
                source.echo(*id, *property, value);
            }
        }
    }

    /// Deliver a message from client `index`.
    pub fn deliver(&mut self, index: usize, msg: ToServer) {
        let Some(player) = self.player(index) else {
            return;
        };
        let rt = self.server.runtime();
        match msg {
            ToServer::Click { detector, event } => {
                if let Ok(ud) = rt.push_instance(player) {
                    rt.fire_event(detector, event, ud);
                    rt.drain_deferred();
                }
            }
            ToServer::MoveTool { tool, parent } => {
                rt.repeat_tool_move(player, tool, parent);
                rt.drain_deferred();
            }
            ToServer::Pose {
                part,
                cframe,
                linear,
                angular,
            } => self.apply_pose(index, player, part, cframe, [linear, angular]),
            ToServer::Remote(RemoteMessage { remote, kind, args }) => match kind {
                RemoteKind::FireServer => {
                    if let Some(r) = remote {
                        rt.deliver_event(r, Some(player), &args);
                    }
                }
                RemoteKind::InvokeServer { call } => {
                    self.next_call += 1;
                    self.calls.insert(self.next_call, (index, call));
                    if let Some(r) = remote {
                        self.server.runtime().deliver_invoke(
                            r,
                            Some(player),
                            self.next_call,
                            &args,
                        );
                    }
                }
                RemoteKind::Reply { call, ok } => rt.deliver_reply(call, ok, &args),
                RemoteKind::FireClient { .. }
                | RemoteKind::FireAllClients
                | RemoteKind::InvokeClient { .. } => {}
            },
        }
    }
}

/// StarterPlayer's properties a joining Player copies, as (StarterPlayer
/// name, Player name).
const STARTER_PLAYER_COPIES: [(&str, &str); 12] = [
    ("CameraMaxZoomDistance", "CameraMaxZoomDistance"),
    ("CameraMinZoomDistance", "CameraMinZoomDistance"),
    ("CameraMode", "CameraMode"),
    ("DevCameraOcclusionMode", "DevCameraOcclusionMode"),
    ("DevComputerCameraMovementMode", "DevComputerCameraMode"),
    ("DevComputerMovementMode", "DevComputerMovementMode"),
    ("DevTouchCameraMovementMode", "DevTouchCameraMode"),
    ("DevTouchMovementMode", "DevTouchMovementMode"),
    ("EnableMouseLockOption", "DevEnableMouseLock"),
    ("AutoJumpEnabled", "AutoJumpEnabled"),
    ("HealthDisplayDistance", "HealthDisplayDistance"),
    ("NameDisplayDistance", "NameDisplayDistance"),
];

/// A client end: its DataModel and how its instances map to the server's.
#[derive(Debug)]
pub struct ClientHost {
    client: DataModel,
    sink: Sink,
    /// Invokes from the server delivered here, by our number: the server's
    /// number for the call.
    calls: HashMap<u64, u64>,
    next_call: u64,
    /// Effect calls on instances that hadn't replicated yet (created and
    /// called in the same server frame), tried again after the next
    /// updates.
    early_effects: Vec<(InstanceId, EffectCall)>,
    /// The character pose last sent, so a still character sends nothing.
    last_pose: Option<ToServer>,
    /// ClickDetector hovers the frame's own mouse refresh found, to send.
    hover_out: Vec<ToServer>,
}

impl ClientHost {
    /// A client DataModel built from what the server sent on joining, with
    /// its LocalScripts started.
    pub fn new(joined: &Joined, options: &Options) -> Result<Self, EngineError> {
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
                // PlayerScripts exists only on the client.
                if let Ok(ps) = dom.create_by_name("PlayerScripts") {
                    dom.attach_unchecked(ps, local);
                    let starter = dom
                        .find_service("StarterPlayer")
                        .and_then(|sp| dom.find_first_child(sp, "StarterPlayerScripts", false));
                    if let Some(starter) = starter {
                        for child in dom.children(starter).to_vec() {
                            if let Some(copy) = dom.clone_instance(child) {
                                dom.attach_unchecked(copy, ps);
                            }
                        }
                    }
                }
            }
            dom.journal_mut().set_enabled(true);
        }
        client.reset_cursors();
        client.ensure_mouse();
        if let Some(patch) = &options.core_scripts {
            patch.install(&client);
        }
        client.start();
        Ok(Self {
            client,
            sink,
            calls: HashMap::new(),
            next_call: 0,
            early_effects: Vec::new(),
            last_pose: None,
            hover_out: Vec::new(),
        })
    }

    /// Hand effect calls to the client's renderer, by the client's ids;
    /// with `keep`, those on instances it doesn't have yet wait for the
    /// next updates.
    fn effect_calls(
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
        self.hover();
    }

    /// One frame of the client, `dt` seconds long.
    pub fn step_by(&mut self, dt: f64) {
        self.client.step_by(dt);
        self.hover();
    }

    /// The mouse sees what's under it after the frame moved the camera and
    /// the world, as Mouse.Hit follows the camera in Roblox while the
    /// mouse holds still.
    fn hover(&mut self) {
        let clicks = self.client.refresh_hover();
        let out = self.fire_clicks(clicks);
        self.hover_out.extend(out);
    }

    /// Deliver a message from the server.
    pub fn deliver(&mut self, msg: &ToClient) {
        let rt = self.client.runtime();
        match msg {
            ToClient::Updates(updates) => {
                let (start, local, added) = {
                    let mut w = rt.world_mut();
                    let start = w.dom.journal().end();
                    let local = w.dom.local_player();
                    let character =
                        |dom: &rook_dom::Dom| local.and_then(|p| dom.ref_property(p, "Character"));
                    let before = character(&w.dom);
                    self.sink.apply(updates, &mut w.dom);
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
    fn follow_character(&self, character: InstanceId) {
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
    fn character_pose(&self) -> Option<ToServer> {
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
        for (tool, parent) in self.client.runtime().take_tool_moves() {
            if let (Some(tool), Some(parent)) =
                (self.sink.server_id(tool), self.sink.server_id(parent))
            {
                out.push(ToServer::MoveTool { tool, parent });
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
    fn fire_clicks(&self, clicks: Vec<crate::bridges::input::ClickEvent>) -> Vec<ToServer> {
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
