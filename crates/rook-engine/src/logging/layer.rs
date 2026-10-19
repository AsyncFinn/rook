//! The `tracing` layer that prints events as console lines.

use std::fmt::Write;

use rook_lua::MessageType;
use tracing::field::{Field, Visit};
use tracing::{Event, Level, Metadata, Subscriber};
use tracing_subscriber::layer::{Context, Layer};

use super::{Filter, ours};
use crate::console;

pub(super) struct ConsoleLayer {
    filter: Filter,
}

impl ConsoleLayer {
    pub(super) fn new(filter: Filter) -> Self {
        Self { filter }
    }

    /// Whether the log file takes an event: Rook's own at `debug` and up,
    /// libraries' warnings.
    fn files(meta: &Metadata<'_>) -> bool {
        let level = *meta.level();
        if ours(meta.target()) {
            level <= Level::DEBUG
        } else {
            level <= Level::WARN
        }
    }
}

impl<S: Subscriber> Layer<S> for ConsoleLayer {
    fn enabled(&self, meta: &Metadata<'_>, _: Context<'_, S>) -> bool {
        Self::files(meta) || self.filter.shows(meta.target(), *meta.level())
    }

    fn on_event(&self, event: &Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        let mut message = Message::default();
        event.record(&mut message);
        let kind = match *meta.level() {
            Level::ERROR => MessageType::Error,
            Level::WARN => MessageType::Warning,
            Level::INFO => MessageType::Info,
            _ => MessageType::Output,
        };
        let terminal = self.filter.shows(meta.target(), *meta.level());
        console::print_rook(kind, &message.text, terminal);
    }
}

/// An event's message, then its other fields as `name=value`.
#[derive(Default)]
struct Message {
    text: String,
}

impl Visit for Message {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let fields = std::mem::take(&mut self.text);
            let _ = write!(self.text, "{value:?}");
            self.text.push_str(&fields);
        } else {
            let _ = write!(self.text, "  {}={value:?}", field.name());
        }
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "message" {
            self.text.insert_str(0, value);
        } else {
            let _ = write!(self.text, "  {}={value}", field.name());
        }
    }
}
