//! Frame-time readouts for the window title and the terminal.

use rook_engine::console;
use rook_lua::MessageType;

use super::*;
use crate::stats::{self, ms};

/// Frame times as players feel them, over each second of play: in the
/// window title, and with `--frame-times` in the terminal too.
pub(crate) struct Readout {
    pub(crate) print: bool,
    pub(crate) frames: stats::Times,
    pub(crate) run: stats::Times,
    pub(crate) since: Instant,
    /// The whole run's step times, kept only for the exit summary.
    server_run: stats::Times,
    client_run: stats::Times,
}

impl Readout {
    pub(crate) fn new(print: bool) -> Self {
        Self {
            print,
            frames: stats::Times::default(),
            run: stats::Times::default(),
            since: Instant::now(),
            server_run: stats::Times::default(),
            client_run: stats::Times::default(),
        }
    }

    /// Count a frame; once a second, the line to show.
    pub(crate) fn frame(&mut self, took: Duration, game: &mut Game) -> Option<String> {
        self.frames.push(took);
        // Only the exit summary needs every frame of the run.
        if self.print {
            self.run.push(took);
        }
        if self.since.elapsed() < Duration::from_secs(1) {
            return None;
        }
        self.since = Instant::now();
        let f = self.frames.summary();
        self.frames.clear();
        let (server, client) = self.take_steps(game);
        let (server, client) = (server.summary(), client.summary());
        let line = format!(
            "{:.0} FPS, frame median {:.2} ms, slowest 1% {:.2} ms, longest {:.2} ms, {} over 16.7 ms  |  \
             steps: server median {:.2} ms (longest {:.2}), client median {:.2} ms (longest {:.2})",
            f.fps(),
            ms(f.median),
            ms(f.slowest_1pct),
            ms(f.longest),
            f.over_budget,
            ms(server.median),
            ms(server.longest),
            ms(client.median),
            ms(client.longest),
        );
        if self.print {
            console::print_message(MessageType::Info, &line);
        }
        Some(line)
    }

    /// The game's step times since the last take, kept for the exit
    /// summary when it's printed.
    fn take_steps(&mut self, game: &mut Game) -> (stats::Times, stats::Times) {
        let server = std::mem::take(&mut game.server_steps);
        let client = std::mem::take(&mut game.client_steps);
        if self.print {
            self.server_run.append(&server);
            self.client_run.append(&client);
        }
        (server, client)
    }

    /// The whole run's frame and step times.
    pub(crate) fn print_run(&mut self, game: &mut Game) {
        self.take_steps(game);
        let frames = self.run.summary();
        console::print_message(
            MessageType::Info,
            &format!("frames: {:.0} FPS, {frames}", frames.fps()),
        );
        console::print_message(
            MessageType::Info,
            &format!("server steps: {}", self.server_run.summary()),
        );
        if !self.client_run.is_empty() {
            console::print_message(
                MessageType::Info,
                &format!("client steps: {}", self.client_run.summary()),
            );
        }
    }
}
