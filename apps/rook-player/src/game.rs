//! The game the player shows: a play session with one local player, whose
//! server runs on its own thread, or the server alone (Studio's Run mode),
//! which the window draws itself.

use std::path::Path;
use std::sync::Arc;

use rook_engine::console::repl::Side;
use rook_engine::session::host::{ClientHost, ServerHost};
use rook_engine::{DataModel, InputEvent, Options, console};
use rook_lua::MessageType;
use rook_render::Camera;

use crate::server::{ServerThread, Started};
use crate::stats::Times;

enum Mode {
    /// Play: the server on its thread, the local player's client here.
    Play {
        server: ServerThread,
        client: Box<ClientHost>,
    },
    /// Run: the server alone, here, since the window draws it.
    Run(Box<ServerHost>),
}

pub struct Game {
    mode: Mode,
    /// The camera the place was saved with.
    pub saved_camera: Camera,
    /// Server and client step times, for `--bench` and the readout.
    pub server_steps: Times,
    pub client_steps: Times,
    /// Time spent applying server frames since the last client step.
    delivering: std::time::Duration,
    /// When loading the place started (`--bench` times content from it).
    pub load_start: std::time::Instant,
}

/// A place on its way to being a [`Game`], for a window that keeps
/// drawing while it loads: [`Loading::poll`] it once a frame.
pub struct Loading {
    stage: Stage,
    finish: Finish,
}

enum Stage {
    /// Play: the server is loading the place on its thread.
    Server(crate::server::Starting),
    /// Run: the place loads here on the next poll.
    Run(Vec<u8>),
}

/// What's needed once the server is there.
struct Finish {
    options: Options,
    script: Option<String>,
    load_start: std::time::Instant,
}

impl Loading {
    /// Start loading a place (see [`Game::load`]).
    pub fn begin(
        path: &Path,
        play: bool,
        content: Arc<rook_content::Resolver>,
        script: Option<String>,
        wait_for_sounds: bool,
    ) -> anyhow::Result<Self> {
        let load_start = std::time::Instant::now();
        let bytes = fs_err::read(path)?;
        let options = Options {
            content: Some(content),
            wait_for_sounds,
            ..Options::default()
        };
        let (stage, script) = if play {
            let starting = ServerThread::spawn(bytes, options.clone(), script, ("Player1", 1))?;
            (Stage::Server(starting), None)
        } else {
            (Stage::Run(bytes), script)
        };
        Ok(Self {
            stage,
            finish: Finish {
                options,
                script,
                load_start,
            },
        })
    }

    /// The game, once the place has loaded. Joining the client (Play) or
    /// loading the place (Run) happens in the poll that returns it.
    pub fn poll(&mut self) -> Option<anyhow::Result<Game>> {
        match &mut self.stage {
            Stage::Server(starting) => {
                let started = starting.poll()?;
                Some(started.and_then(|s| self.finish.joined(s)))
            }
            Stage::Run(bytes) => Some(self.finish.run(bytes)),
        }
    }
}

impl Finish {
    fn joined(&self, (server, started): (ServerThread, Started)) -> anyhow::Result<Game> {
        let client = ClientHost::new(&started.joined, &self.options)?;
        Ok(self.game(
            Mode::Play {
                server,
                client: Box::new(client),
            },
            started.camera,
        ))
    }

    fn run(&self, bytes: &[u8]) -> anyhow::Result<Game> {
        let mut server = DataModel::load_place(bytes, &self.options)?;
        server.start();
        let host = ServerHost::new(server);
        if let Some(source) = &self.script {
            host.server()
                .run_extra_script(source)
                .map_err(anyhow::Error::msg)?;
        }
        let camera = crate::scene::initial_camera(host.server());
        Ok(self.game(Mode::Run(Box::new(host)), camera))
    }

    fn game(&self, mode: Mode, saved_camera: Camera) -> Game {
        let game = Game {
            mode,
            saved_camera,
            server_steps: Times::default(),
            client_steps: Times::default(),
            delivering: std::time::Duration::ZERO,
            load_start: self.load_start,
        };
        game.print_logs();
        game
    }
}

impl Game {
    /// Load a place and start it, waiting until it has. With `play`, a
    /// local player joins and the window shows that player's client, as
    /// in Studio's Play mode. Content requests resolve through `content`;
    /// `script` (the `--script` file) runs as a server script once the
    /// place starts. Sounds load in the background while the game runs, as
    /// in Roblox, unless `wait_for_sounds` holds each frame until they
    /// have.
    pub fn load(
        path: &Path,
        play: bool,
        content: Arc<rook_content::Resolver>,
        script: Option<String>,
        wait_for_sounds: bool,
    ) -> anyhow::Result<Self> {
        let loading = Loading::begin(path, play, content, script, wait_for_sounds)?;
        match loading.stage {
            Stage::Server(starting) => loading.finish.joined(starting.wait()?),
            Stage::Run(bytes) => loading.finish.run(&bytes),
        }
    }
    /// The DataModel the window shows.
    pub fn view(&self) -> &DataModel {
        match &self.mode {
            Mode::Play { client, .. } => client.client(),
            Mode::Run(host) => host.server(),
        }
    }

    pub fn view_mut(&mut self) -> &mut DataModel {
        match &mut self.mode {
            Mode::Play { client, .. } => client.client_mut(),
            Mode::Run(host) => host.server_mut(),
        }
    }

    /// What the window's DataModel is playing, for the mixer: its sounds,
    /// their SoundGroups, `SoundService.AmbientReverb` and the audio API's
    /// graph.
    pub fn sounds(&self) -> rook_audio::Mix<rook_dom::InstanceId> {
        use rook_audio::spatial::{RollOff, attenuation, pan};
        let view = self.view();
        let listener = view.sound_listener();
        let (at, facing) = (listener.cframe.position, listener.cframe.rotation);
        let playing = view.playing_sounds();
        let groups = view.sound_groups(&playing);
        let sounds = playing
            .into_iter()
            .map(|s| {
                // A global sound is heard at its Volume on both sides; a 3D
                // one by distance and direction from the listener.
                let gains = match s.emitter {
                    None => [s.volume; 2],
                    Some(e) => {
                        let offset = e.position - at;
                        let fade = attenuation(
                            RollOff::from_value(e.roll_off),
                            offset.length(),
                            e.min,
                            e.max,
                            listener.rolloff_scale,
                        );
                        pan(facing.transpose() * offset).map(|g| g * fade * s.volume)
                    }
                };
                rook_audio::Play {
                    key: s.id,
                    audio: s.audio,
                    position: s.position,
                    gains,
                    speed: s.speed,
                    looped: s.looped,
                    generation: s.generation,
                    effects: s.effects,
                    group: s.group,
                }
            })
            .collect();
        rook_audio::Mix {
            sounds,
            groups,
            ambient: listener.ambient_reverb,
            graph: view.audio_graph(),
        }
    }

    /// Send window input to the local player's client (Run mode has no player).
    pub fn input(&mut self, event: InputEvent) {
        if let Mode::Play { server, client } = &mut self.mode {
            for msg in client.input(event) {
                server.deliver(msg);
            }
        }
    }

    pub fn set_viewport(&mut self, width: u32, height: u32) {
        self.view_mut().set_viewport(width, height);
    }

    /// One frame of the whole game in lockstep: the server steps, its
    /// changes reach the client, then the client steps. Results are the
    /// same every run.
    pub fn step(&mut self) {
        match &mut self.mode {
            Mode::Play { server, client } => {
                let frame = server.step();
                self.server_steps.push(frame.took);
                let start = std::time::Instant::now();
                for msg in &frame.messages {
                    client.deliver(msg);
                }
                client.step();
                for msg in client.outgoing() {
                    server.deliver(msg);
                }
                self.client_steps.push(start.elapsed());
            }
            Mode::Run(host) => {
                let start = std::time::Instant::now();
                host.step();
                host.updates();
                self.server_steps.push(start.elapsed());
            }
        }
        self.print_logs();
    }

    /// Let the server keep its own 60 Hz pace (Play mode), for live play.
    pub fn free_run(&mut self) {
        if let Mode::Play { server, .. } = &self.mode {
            server.run(true);
        }
    }

    /// Live play: apply what the running server sent, then step the client
    /// `steps` times. In Run mode the server steps here.
    pub fn advance(&mut self, steps: u32) {
        match &mut self.mode {
            Mode::Play { server, client } => {
                let start = std::time::Instant::now();
                for frame in server.frames() {
                    self.server_steps.push(frame.took);
                    for msg in &frame.messages {
                        client.deliver(msg);
                    }
                }
                // Applying the server's changes counts towards the client's
                // next step, as it does in lockstep.
                self.delivering += start.elapsed();
                for _ in 0..steps {
                    let start = std::time::Instant::now();
                    client.step();
                    for msg in client.outgoing() {
                        server.deliver(msg);
                    }
                    let took = start.elapsed() + std::mem::take(&mut self.delivering);
                    self.client_steps.push(took);
                }
                self.print_logs();
            }
            Mode::Run(_) => {
                for _ in 0..steps {
                    self.step();
                }
            }
        }
    }

    pub fn run_for(&mut self, seconds: f64) {
        let target = self.view().now() + seconds;
        while self.view().now() + 1e-9 < target {
            self.step();
        }
    }

    /// Run a command-bar entry on the server or the client.
    pub fn repl(&self, side: Side, line: &str) {
        let dm = match (&self.mode, side) {
            (Mode::Play { server, .. }, Side::Server) => {
                server.repl(line);
                return;
            }
            (Mode::Play { client, .. }, Side::Client) => client.client(),
            (Mode::Run(host), _) => host.server(),
        };
        if let Err(e) = rook_engine::console::repl::run_line(dm, line) {
            console::print_message(MessageType::Error, &e);
        }
    }

    /// Whether the cursor is over a ClickDetector the player can click.
    pub fn hovering_click_detector(&self) -> bool {
        match &self.mode {
            Mode::Play { client, .. } => client.client().hovered_click_detector().is_some(),
            Mode::Run(_) => false,
        }
    }

    pub fn has_client(&self) -> bool {
        matches!(self.mode, Mode::Play { .. })
    }

    /// Print new output from this thread's DataModel (the server thread
    /// prints its own).
    pub fn print_logs(&self) {
        let side = if self.has_client() {
            "Client"
        } else {
            "Server"
        };
        for entry in self.view().take_log() {
            console::print_entry(side, &entry);
        }
    }

    /// What lockstep steps took, server and client together.
    pub fn total_steps(&self) -> Times {
        self.server_steps.sum_with(&self.client_steps)
    }
}
