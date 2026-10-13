//! A play session in one thread: a server and the clients connected to it,
//! each message delivered as soon as it's sent (architecture §8, Play mode).
//! The ends are the same [`ServerHost`] and [`ClientHost`] a threaded host
//! runs apart.

use rook_dom::InstanceId;

use crate::host::{ClientHost, ServerHost, ToClient};
use crate::{DataModel, EngineError, Options};

#[derive(Debug)]
pub struct Session {
    server: ServerHost,
    clients: Vec<ClientHost>,
    options: Options,
}

impl Session {
    /// Start a session around a server (its scripts already started).
    pub fn new(server: DataModel, options: Options) -> Self {
        Self {
            server: ServerHost::new(server),
            clients: Vec::new(),
            options,
        }
    }

    pub fn server(&self) -> &DataModel {
        self.server.server()
    }

    pub fn server_mut(&mut self) -> &mut DataModel {
        self.server.server_mut()
    }

    pub fn clients(&self) -> impl Iterator<Item = &DataModel> {
        self.clients.iter().map(ClientHost::client)
    }

    pub fn client(&self, index: usize) -> Option<&DataModel> {
        self.clients.get(index).map(ClientHost::client)
    }

    pub fn client_mut(&mut self, index: usize) -> Option<&mut DataModel> {
        self.clients.get_mut(index).map(ClientHost::client_mut)
    }

    /// A player joins (see [`ServerHost::join`]); a new client DataModel
    /// receives the replicated game, its PlayerScripts (from
    /// StarterPlayerScripts), and starts its LocalScripts. Returns the
    /// client's index.
    pub fn join(&mut self, name: &str, user_id: i64) -> Result<usize, EngineError> {
        let (index, joined) = self.server.join(name, user_id)?;
        self.clients.push(ClientHost::new(&joined, &self.options)?);
        Ok(index)
    }

    /// One frame: the server steps, its changes replicate to every client,
    /// then each client steps.
    pub fn step(&mut self) {
        profiling::scope!("Session::step");
        self.server.step();
        self.route();
        for (client, updates) in self.clients.iter_mut().zip(self.server.updates()) {
            client.deliver(&ToClient::Updates(updates));
            client.step();
        }
        self.route();
    }

    /// Deliver queued remote calls in both directions. Messages arrive in
    /// the order they were sent; each crosses within the same session frame
    /// (Approximation: Roblox delivers on the next network tick).
    fn route(&mut self) {
        profiling::scope!("Session::route");
        for (index, msg) in self.server.outgoing() {
            self.clients[index].deliver(&msg);
        }
        for index in 0..self.clients.len() {
            for msg in self.clients[index].outgoing() {
                self.server.deliver(index, msg);
            }
        }
    }

    /// Run frames until `seconds` of server time have passed.
    pub fn run_for(&mut self, seconds: f64) {
        let target = self.server().now() + seconds;
        while self.server().now() + 1e-9 < target {
            self.step();
        }
    }

    /// Feed a window input event to a client.
    pub fn input(&mut self, index: usize, event: crate::InputEvent) {
        let Some(client) = self.clients.get_mut(index) else {
            return;
        };
        for msg in client.input(event) {
            self.server.deliver(index, msg);
        }
    }

    /// The server-side Player of a client.
    pub fn player(&self, index: usize) -> Option<InstanceId> {
        self.server.player(index)
    }
}
