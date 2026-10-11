//! A play session: one server DataModel and the clients connected to it,
//! kept in sync by the in-process replicator (architecture §8, Play mode).

use rook_dom::{Dom, InstanceId};
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
}

fn set(dom: &mut Dom, id: InstanceId, name: &str, value: Value) {
    let pid = dom
        .get(id)
        .and_then(|i| dom.database().find_canonical_property(i.class, name))
        .map(|p| p.id);
    if let Some(pid) = pid {
        let _ = dom.set_property(id, pid, value);
    }
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
            set(dom, player, "UserId", Value::Int64(user_id));
            set(dom, player, "DisplayName", Value::String(name.to_owned()));
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
        self.server.step();
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
        self.server.trim_journal();
    }

    /// Run frames until `seconds` of server time have passed.
    pub fn run_for(&mut self, seconds: f64) {
        let target = self.server.now() + seconds;
        while self.server.now() + 1e-9 < target {
            self.step();
        }
    }

    /// The server-side Player of a client.
    pub fn player(&self, index: usize) -> Option<InstanceId> {
        self.clients.get(index).map(|c| c.player)
    }
}
