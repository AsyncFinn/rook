//! The game the player shows: a play session with one local player, or
//! the server alone (Studio's Run mode).

use std::path::Path;

use anyhow::Context;
use rook_engine::{DataModel, InputEvent, Options, Server, Session};

pub struct Game {
    session: Session,
    /// The client whose view is shown; `None` shows the server (Run mode).
    view: Option<usize>,
}

impl Game {
    /// Load a place and start it. With `play`, a local player joins and the
    /// window shows that player's client, as in Studio's Play mode.
    pub fn load(path: &Path, play: bool) -> anyhow::Result<Self> {
        let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let mut server = Server::load_place(&bytes, &Options::default())?;
        server.start();
        let mut session = Session::new(server, Options::default());
        let view = if play {
            Some(session.join("Player1", 1)?)
        } else {
            None
        };
        let game = Self { session, view };
        game.print_logs();
        Ok(game)
    }

    pub fn server(&self) -> &DataModel {
        self.session.server()
    }

    /// The DataModel the window shows.
    pub fn view(&self) -> &DataModel {
        self.view
            .and_then(|i| self.session.client(i))
            .unwrap_or_else(|| self.session.server())
    }

    pub fn view_mut(&mut self) -> &mut DataModel {
        match self.view {
            Some(i) => self.session.client_mut(i).expect("client exists"),
            None => self.session.server_mut(),
        }
    }

    /// Send window input to the local player's client (Run mode has no player).
    pub fn input(&mut self, event: InputEvent) {
        if let Some(i) = self.view {
            self.session.input(i, event);
        }
    }

    pub fn set_viewport(&mut self, width: u32, height: u32) {
        self.view_mut().set_viewport(width, height);
    }

    pub fn step(&mut self) {
        self.session.step();
    }

    pub fn run_for(&mut self, seconds: f64) {
        self.session.run_for(seconds);
        self.print_logs();
    }

    /// Run extra Luau as a server script.
    pub fn run_server_script(&self, source: &str) -> anyhow::Result<()> {
        self.session
            .server()
            .runtime()
            .run_source("ServerScriptService.RookScript", source, None)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Print new output from the server and every client.
    pub fn print_logs(&self) {
        let sides = std::iter::once(("server", self.session.server()))
            .chain(self.session.clients().map(|c| ("client", c)));
        for (side, dm) in sides {
            for entry in dm.take_log() {
                rook_engine::console::print_entry(Some(side), &entry);
            }
        }
    }
}
