//! Frame-time readouts for the window title and the terminal.

use crate::stats::ms;
use crate::*;

/// Frame times as players feel them, over each second of play: in the
/// window title, and with `--frame-times` in the terminal too.
pub(crate) struct Readout {
    pub(crate) print: bool,
    pub(crate) frames: stats::Times,
    pub(crate) run: stats::Times,
    pub(crate) since: Instant,
    /// Step samples already counted in earlier seconds.
    pub(crate) server_seen: usize,
    pub(crate) client_seen: usize,
}

impl Readout {
    pub(crate) fn new(print: bool) -> Self {
        Self {
            print,
            frames: stats::Times::default(),
            run: stats::Times::default(),
            since: Instant::now(),
            server_seen: 0,
            client_seen: 0,
        }
    }

    /// Count a frame; once a second, the line to show.
    pub(crate) fn frame(&mut self, took: Duration, game: &Game) -> Option<String> {
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
        let server = game.server_steps.since(self.server_seen).summary();
        let client = game.client_steps.since(self.client_seen).summary();
        self.server_seen = game.server_steps.len();
        self.client_seen = game.client_steps.len();
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

    /// The whole run's frame and step times.
    pub(crate) fn print_run(&self, game: &Game) {
        let frames = self.run.summary();
        console::print_message(
            MessageType::Info,
            &format!("frames: {:.0} FPS, {frames}", frames.fps()),
        );
        console::print_message(
            MessageType::Info,
            &format!("server steps: {}", game.server_steps.summary()),
        );
        if !game.client_steps.is_empty() {
            console::print_message(
                MessageType::Info,
                &format!("client steps: {}", game.client_steps.summary()),
            );
        }
    }
}
