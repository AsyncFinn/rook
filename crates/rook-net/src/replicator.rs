use super::*;

/// Both halves joined directly, for a server and client in one thread.
#[derive(Debug, Default)]
pub struct Replicator {
    source: Source,
    sink: Sink,
}

impl Replicator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_player(&mut self, server_player: Option<InstanceId>) {
        self.source.player = server_player;
    }

    pub fn client_id(&self, server: InstanceId) -> Option<InstanceId> {
        self.sink.client_id(server)
    }

    pub fn server_id(&self, client: InstanceId) -> Option<InstanceId> {
        self.sink.server_id(client)
    }

    /// Copy the replicated part of the server tree into a fresh client
    /// DataModel. The client's journal is not touched (like loading a file).
    pub fn initial(&mut self, server: &Dom, client: &mut Dom) {
        let snapshot = self.source.snapshot(server);
        self.sink.snapshot(&snapshot, client);
    }

    /// Apply every server change since the last call. The caller fires
    /// client signals for the client journal entries this produces.
    pub fn sync(&mut self, server: &Dom, client: &mut Dom) {
        let updates = self.source.updates(server);
        self.sink.apply(&updates, client);
    }
}
