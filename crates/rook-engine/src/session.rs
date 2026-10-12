//! A play session: one server DataModel and the clients connected to it,
//! kept in sync by the in-process replicator (architecture §8, Play mode).

use std::collections::HashMap;

use rook_dom::{Dom, InstanceId};
use rook_lua::{NetValue, RemoteKind, RemoteMessage};
use rook_net::Replicator;
use rook_types::Value;

use crate::{DataModel, EngineError, Options};

#[derive(Debug)]
struct ClientSlot {
    client: DataModel,
    replicator: Replicator,
    /// The server's Player for this client.
    player: InstanceId,
}

#[derive(Debug)]
pub struct Session {
    server: DataModel,
    clients: Vec<ClientSlot>,
    options: Options,
    /// Invokes in flight, by session-wide id: who asked, and their own id.
    calls: HashMap<u64, (Origin, u64)>,
    next_call: u64,
}

/// Which DataModel made an invoke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    Server,
    Client(usize),
}

/// Copies of `from`'s archivable children placed under `to`.
fn copy_children(dom: &mut Dom, from: Option<InstanceId>, to: InstanceId) {
    let Some(from) = from else {
        return;
    };
    for child in dom.children(from).to_vec() {
        if let Some(copy) = dom.clone_instance(child) {
            let _ = dom.set_parent(copy, Some(to));
        }
    }
}

impl Session {
    /// Start a session around a server (its scripts already started).
    pub fn new(mut server: DataModel, options: Options) -> Self {
        server.set_auto_trim(false);
        Self {
            server,
            clients: Vec::new(),
            options,
            calls: HashMap::new(),
            next_call: 0,
        }
    }

    pub fn server(&self) -> &DataModel {
        &self.server
    }

    pub fn server_mut(&mut self) -> &mut DataModel {
        &mut self.server
    }

    pub fn clients(&self) -> impl Iterator<Item = &DataModel> {
        self.clients.iter().map(|c| &c.client)
    }

    pub fn client(&self, index: usize) -> Option<&DataModel> {
        self.clients.get(index).map(|c| &c.client)
    }

    pub fn client_mut(&mut self, index: usize) -> Option<&mut DataModel> {
        self.clients.get_mut(index).map(|c| &mut c.client)
    }

    /// A player joins: the server gets a Player (with Backpack and
    /// PlayerGui filled from StarterPack and StarterGui) and fires
    /// PlayerAdded; a new client DataModel receives the replicated game,
    /// its PlayerScripts (from StarterPlayerScripts), and starts its
    /// LocalScripts. Returns the client's index.
    ///
    /// Roblox copies StarterGui and StarterPack when the character spawns;
    /// without characters (M4) Rook copies them on join (Approximation).
    pub fn join(&mut self, name: &str, user_id: i64) -> Result<usize, EngineError> {
        let (player, start) = {
            let mut w = self.server.runtime().world_mut();
            let dom = &mut w.dom;
            let start = dom.journal().end();
            let players = dom
                .get_service("Players")
                .map_err(|e| EngineError::Lua(e.to_string()))?;
            let player = dom
                .create_by_name("Player")
                .map_err(|e| EngineError::Lua(e.to_string()))?;
            dom.set_name(player, name.to_owned()).ok();
            let _ = dom.set_property_by_name(player, "UserId", Value::Int64(user_id));
            let _ = dom.set_property_by_name(player, "DisplayName", Value::String(name.to_owned()));
            for container in ["Backpack", "PlayerGui"] {
                if let Ok(c) = dom.create_by_name(container) {
                    let _ = dom.set_parent(c, Some(player));
                }
            }
            let gui = dom.find_first_child(player, "PlayerGui", false);
            let backpack = dom.find_first_child(player, "Backpack", false);
            let starter_gui = dom.find_service("StarterGui");
            let starter_pack = dom.find_service("StarterPack");
            if let Some(gui) = gui {
                copy_children(dom, starter_gui, gui);
            }
            if let Some(backpack) = backpack {
                copy_children(dom, starter_pack, backpack);
            }
            let _ = dom.set_parent(player, Some(players));
            (player, start)
        };
        let players = self.server.runtime().world().dom.find_service("Players");
        self.server.runtime().dispatch_since(start);
        if let Some(players) = players {
            let ud = self.server.runtime().push_instance(player);
            if let Ok(ud) = ud {
                let _ = self.server.runtime().fire_event(players, "PlayerAdded", ud);
            }
            self.server.runtime().drain_deferred();
        }

        let mut client = DataModel::new_client(&self.options)?;
        let mut replicator = Replicator::new();
        replicator.set_player(Some(player));
        {
            let server_world = self.server.runtime().world();
            let mut client_world = client.runtime().world_mut();
            replicator.initial(&server_world.dom, &mut client_world.dom);
            client_world.signal_behavior = server_world.signal_behavior;
            let dom = &mut client_world.dom;
            dom.journal_mut().set_enabled(false);
            if let (Some(local), Some(players)) =
                (replicator.client_id(player), dom.find_service("Players"))
            {
                if let Some(pid) = dom
                    .get(players)
                    .and_then(|i| dom.database().find_property(i.class, "LocalPlayer"))
                    .map(|p| p.id)
                {
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
        client.start();
        self.clients.push(ClientSlot {
            client,
            replicator,
            player,
        });
        Ok(self.clients.len() - 1)
    }

    /// One frame: the server steps, its changes replicate to every client,
    /// then each client steps.
    pub fn step(&mut self) {
        profiling::scope!("Session::step");
        self.server.step();
        self.route();
        for slot in &mut self.clients {
            let start = {
                let server_world = self.server.runtime().world();
                let mut client_world = slot.client.runtime().world_mut();
                let start = client_world.dom.journal().end();
                slot.replicator
                    .sync(&server_world.dom, &mut client_world.dom);
                start
            };
            slot.client.runtime().dispatch_since(start);
            slot.client.step();
        }
        self.route();
        self.server.trim_journal();
    }

    fn new_call(&mut self, origin: Origin, call: u64) -> u64 {
        self.next_call += 1;
        self.calls.insert(self.next_call, (origin, call));
        self.next_call
    }

    /// Deliver queued remote calls in both directions. Messages arrive in
    /// the order they were sent; each crosses within the same session frame
    /// (Approximation: Roblox delivers on the next network tick).
    fn route(&mut self) {
        profiling::scope!("Session::route");
        for msg in self.server.runtime().take_outbox() {
            self.route_from_server(msg);
        }
        for index in 0..self.clients.len() {
            let outbox = self.clients[index].client.runtime().take_outbox();
            for msg in outbox {
                self.route_from_client(index, msg);
            }
        }
    }

    fn route_from_server(&mut self, msg: RemoteMessage) {
        let RemoteMessage { remote, kind, args } = msg;
        let to_client = |slot: &ClientSlot, args: &[NetValue]| -> Vec<NetValue> {
            args.iter()
                .cloned()
                .map(|mut a| {
                    a.map_instances(&|id| slot.replicator.client_id(id));
                    a
                })
                .collect()
        };
        match kind {
            RemoteKind::FireClient { player } => {
                for slot in self.clients.iter().filter(|s| s.player == player) {
                    if let Some(r) = remote.and_then(|r| slot.replicator.client_id(r)) {
                        slot.client
                            .runtime()
                            .deliver_event(r, None, &to_client(slot, &args));
                    }
                }
            }
            RemoteKind::FireAllClients => {
                for slot in &self.clients {
                    if let Some(r) = remote.and_then(|r| slot.replicator.client_id(r)) {
                        slot.client
                            .runtime()
                            .deliver_event(r, None, &to_client(slot, &args));
                    }
                }
            }
            RemoteKind::InvokeClient { player, call } => {
                let Some(index) = self.clients.iter().position(|s| s.player == player) else {
                    self.server.runtime().deliver_reply(
                        call,
                        false,
                        &[NetValue::String(b"Player is not connected".to_vec())],
                    );
                    return;
                };
                let id = self.new_call(Origin::Server, call);
                let slot = &self.clients[index];
                if let Some(r) = remote.and_then(|r| slot.replicator.client_id(r)) {
                    slot.client
                        .runtime()
                        .deliver_invoke(r, None, id, &to_client(slot, &args));
                }
            }
            RemoteKind::Reply { call, ok } => {
                if let Some((Origin::Client(index), original)) = self.calls.remove(&call)
                    && let Some(slot) = self.clients.get(index)
                {
                    slot.client
                        .runtime()
                        .deliver_reply(original, ok, &to_client(slot, &args));
                }
            }
            RemoteKind::FireServer | RemoteKind::InvokeServer { .. } => {}
        }
    }

    fn route_from_client(&mut self, index: usize, msg: RemoteMessage) {
        let RemoteMessage { remote, kind, args } = msg;
        let slot = &self.clients[index];
        let args: Vec<NetValue> = args
            .into_iter()
            .map(|mut a| {
                a.map_instances(&|id| slot.replicator.server_id(id));
                a
            })
            .collect();
        let remote = remote.and_then(|r| slot.replicator.server_id(r));
        let player = slot.player;
        match kind {
            RemoteKind::FireServer => {
                if let Some(r) = remote {
                    self.server.runtime().deliver_event(r, Some(player), &args);
                }
            }
            RemoteKind::InvokeServer { call } => {
                let id = self.new_call(Origin::Client(index), call);
                match remote {
                    Some(r) => self
                        .server
                        .runtime()
                        .deliver_invoke(r, Some(player), id, &args),
                    None => {
                        self.calls.remove(&id);
                        self.clients[index].client.runtime().deliver_reply(
                            call,
                            false,
                            &[NetValue::String(
                                b"RemoteFunction is not replicated".to_vec(),
                            )],
                        );
                    }
                }
            }
            RemoteKind::Reply { call, ok } => {
                if let Some((Origin::Server, original)) = self.calls.remove(&call) {
                    self.server.runtime().deliver_reply(original, ok, &args);
                }
            }
            RemoteKind::FireClient { .. }
            | RemoteKind::FireAllClients
            | RemoteKind::InvokeClient { .. } => {}
        }
    }

    /// Run frames until `seconds` of server time have passed.
    pub fn run_for(&mut self, seconds: f64) {
        let target = self.server.now() + seconds;
        while self.server.now() + 1e-9 < target {
            self.step();
        }
    }

    /// Feed a window input event to a client. ClickDetector interactions
    /// fire on the client with its LocalPlayer and on the server with the
    /// server's Player (creator-docs ClickDetector: the events fire on both).
    pub fn input(&mut self, index: usize, event: crate::InputEvent) {
        let Some(slot) = self.clients.get_mut(index) else {
            return;
        };
        let clicks = slot.client.input(event);
        for click in clicks {
            let event = click.kind.event();
            let local = {
                let w = slot.client.runtime().world();
                w.dom
                    .find_service("Players")
                    .and_then(|p| w.dom.property_by_name(p, "LocalPlayer"))
            };
            if let Some(Value::Ref(Some(r))) = local
                && let Ok(ud) = slot.client.runtime().push_instance(InstanceId::from_ref(r))
            {
                let _ = slot.client.runtime().fire_event(click.detector, event, ud);
                slot.client.runtime().drain_deferred();
            }
            if let Some(server_detector) = slot.replicator.server_id(click.detector)
                && let Ok(ud) = self.server.runtime().push_instance(slot.player)
            {
                let _ = self.server.runtime().fire_event(server_detector, event, ud);
                self.server.runtime().drain_deferred();
            }
        }
    }

    /// The server-side Player of a client.
    pub fn player(&self, index: usize) -> Option<InstanceId> {
        self.clients.get(index).map(|c| c.player)
    }
}
