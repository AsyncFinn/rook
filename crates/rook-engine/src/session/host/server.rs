//! The server end: the server DataModel, a link per joined client, and
//! what each client is sent and delivers.

use super::*;

#[derive(Debug)]
pub(super) struct Link {
    pub(super) source: Source,
    pub(super) player: InstanceId,
}

/// The server end: its DataModel and what each client should see.
#[derive(Debug)]
pub struct ServerHost {
    pub(super) server: DataModel,
    pub(super) clients: Vec<Link>,
    /// Invokes from clients delivered here, by our number: the client and
    /// its own number for the call.
    pub(super) calls: FxHashMap<u64, (usize, u64)>,
    pub(super) next_call: u64,
    /// Players whose first character spawns at the start of a step, once
    /// their look has arrived.
    pub(super) spawning: Vec<InstanceId>,
    /// Humanoids that died, and when their players respawn (`None`
    /// without `CharacterAutoLoads`).
    pub(super) dead: FxHashMap<InstanceId, (InstanceId, Option<f64>)>,
}

impl ServerHost {
    /// Host a server (its scripts already started).
    pub fn new(mut server: DataModel) -> Self {
        // Each client's source reads the frame's changes before they go.
        server.set_auto_trim(false);
        Self {
            server,
            clients: Vec::new(),
            calls: FxHashMap::default(),
            next_call: 0,
            spawning: Vec::new(),
            dead: FxHashMap::default(),
        }
    }

    pub fn server(&self) -> &DataModel {
        &self.server
    }

    pub fn server_mut(&mut self) -> &mut DataModel {
        &mut self.server
    }

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
    pub(super) fn notice_deaths(&mut self) {
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
    pub fn updates(&mut self) -> Vec<ToClient> {
        let updates = {
            let w = self.server.runtime().world();
            let time = w.scheduler.now();
            self.clients
                .iter_mut()
                .map(|c| ToClient::Updates {
                    time,
                    updates: c.source.updates(&w.dom),
                })
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
                // Copied for each client but the last, which takes them.
                let last = self.clients.len() - 1;
                for i in 0..last {
                    out.push((i, ToClient::Effects(calls.clone())));
                }
                out.push((last, ToClient::Effects(calls)));
            }
        }
        // A server script's activation reaches the client whose character
        // holds the Tool.
        for activation in self.server.runtime().take_tool_activations() {
            let w = self.server.runtime().world();
            let holder = w.dom.parent(activation.tool);
            let client = self.clients.iter().position(|c| {
                holder.is_some() && w.dom.ref_property(c.player, "Character") == holder
            });
            if let Some(i) = client {
                out.push((i, ToClient::ActivateTool(activation)));
            }
        }
        let mut sends: Vec<(usize, RemoteKind)> = Vec::new();
        for msg in self.server.runtime().take_outbox() {
            let RemoteMessage { remote, kind, args } = msg;
            let mut send = |index: usize, kind: RemoteKind| sends.push((index, kind));
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
            // The arguments are copied for each client but the last, which
            // takes them.
            let last = sends.pop();
            for (index, kind) in sends.drain(..) {
                let message = RemoteMessage {
                    remote,
                    kind,
                    args: args.clone(),
                };
                out.push((index, ToClient::Remote(message)));
            }
            if let Some((index, kind)) = last {
                let message = RemoteMessage { remote, kind, args };
                out.push((index, ToClient::Remote(message)));
            }
        }
        out
    }

    /// Take client `index`'s simulation of its character: the assembly of
    /// `part` moves there, as a script's move would, if its player's client
    /// owns it. One the server anchored is the server's: a pose the client
    /// sent before it heard so would undo the server's moves. Other clients see the move; the sender isn't sent its own
    /// move back (its later changes by the server's scripts do reach it).
    /// Physics writes fire no Changed signals, and neither does this.
    pub(super) fn apply_pose(
        &mut self,
        index: usize,
        player: InstanceId,
        part: InstanceId,
        cframe: rook_types::CFrame,
        [linear, angular]: [glam::Vec3; 2],
    ) {
        let mut w = self.server.runtime().world_mut();
        if rook_lua::network_owner(&w, part) != Some(player) {
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
            ToServer::MoveTool { tool, parent, grip } => {
                let made = rt.repeat_tool_move(player, tool, parent);
                rt.drain_deferred();
                if let (Some(server), Some(client)) = (made, grip) {
                    self.clients[index].source.adopt(server, client);
                }
            }
            ToServer::ActivateTool(activation) => {
                rt.deliver_tool_activation(Some(player), activation);
            }
            ToServer::Prompt(action) => rt.deliver_prompt_action(player, action),
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
pub const STARTER_PLAYER_COPIES: [(&str, &str); 12] = [
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
