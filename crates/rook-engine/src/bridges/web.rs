//! Scripts' web requests (`rook_lua::WebRequest`), each sent on a thread
//! of its own through the host's network, their answers delivered on the
//! engine thread. Members that use them are rare and wait on a person or
//! a server, so no pool is kept.

use std::sync::mpsc::{Receiver, Sender, channel};

use rook_lua::{WebAnswer, WebRequest};
use rook_platform::network::Request;

/// Answers are cut off past this (Roblox's web APIs answer in kilobytes).
const MAX_BYTES: u64 = 16 << 20;

#[derive(Debug)]
pub(crate) struct Web {
    sender: Sender<(u64, WebAnswer)>,
    answers: Receiver<(u64, WebAnswer)>,
    in_flight: usize,
}

impl Default for Web {
    fn default() -> Self {
        let (sender, answers) = channel();
        Self {
            sender,
            answers,
            in_flight: 0,
        }
    }
}

impl Web {
    pub(crate) fn send(&mut self, id: u64, request: WebRequest) {
        let sender = self.sender.clone();
        let body = Box::new(move || {
            let headers: Vec<(&str, &str)> = request
                .headers
                .iter()
                .map(|(n, v)| (n.as_str(), v.as_str()))
                .collect();
            let answer = rook_platform::get().network.send(&Request {
                method: &request.method,
                url: &request.url,
                headers: &headers,
                body: &request.body,
                max_bytes: MAX_BYTES,
            });
            if let Err(e) = &answer {
                tracing::warn!(target: "web", "{} {}: {e}", request.method, request.url);
            }
            // The DataModel may be gone; its answer is then for no one.
            let _ = sender.send((id, answer));
        });
        if let Err(e) = rook_platform::get().threads.spawn("rook-web", body) {
            let _ = self
                .sender
                .send((id, Err(format!("couldn't start a thread: {e}"))));
        }
        self.in_flight += 1;
    }

    /// The answers that arrived; with `wait`, every request in flight's,
    /// so runs that must repeat exactly see them on the same frame.
    pub(crate) fn finished(&mut self, wait: bool) -> Vec<(u64, WebAnswer)> {
        let mut done = Vec::new();
        while self.in_flight > 0 {
            let next = if wait {
                self.answers.recv().ok()
            } else {
                self.answers.try_recv().ok()
            };
            let Some(answer) = next else {
                break;
            };
            self.in_flight -= 1;
            done.push(answer);
        }
        done
    }
}
