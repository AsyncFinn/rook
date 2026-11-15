//! The thumbnails service, on the `thumbnails` domain's endpoint
//! (`rook_platform::endpoints`): where a [`Thumbnail`]'s image is. Its
//! batch request takes the kinds by `rbxthumb://`'s names, as Studio's own
//! thumbnail fetcher asks it; a username is looked up first on the `users`
//! domain. Requests are anonymous. What the service answers is in
//! `docs/behavior/content/thumbnails.md`.

use std::time::{Duration, Instant};

use rook_platform::endpoints::Endpoints;
use rook_platform::network::Request;
use serde_json::{Value, json};

use super::web::{self, failure_kind};
use crate::{Failure, FailureKind, Target, Thumbnail};

/// The service's JSON answers are small; images come from the CDN.
const MAX_ANSWER: u64 = 1 << 20;
/// How long a thumbnail the service is still rendering is waited for, as
/// Studio waits (its `DFIntThumbnailContentProviderTimeoutSecs`).
const RENDER_WAIT: Duration = Duration::from_secs(10);
/// The first wait before asking again, doubled each time up to [`MAX_POLL`].
const FIRST_POLL: Duration = Duration::from_millis(250);
const MAX_POLL: Duration = Duration::from_secs(2);

/// The batch request's path on the `thumbnails` domain.
pub const BATCH_PATH: &str = "/v1/batch";

/// The batch request's JSON body asking for `t` of user or item `id`.
pub fn batch_body(t: &Thumbnail, id: u64) -> Value {
    json!([{
        "requestId": "0",
        "targetId": id,
        "type": t.kind.name(),
        "size": format!("{}x{}", t.width, t.height),
        "format": "png",
        "isCircular": t.circular,
    }])
}

/// The state of a batch answer's one thumbnail (`Completed`, `Pending`,
/// `Error`, `Blocked`), with its entry.
pub fn batch_state(answer: &Value) -> (Option<&str>, &Value) {
    let entry = &answer["data"][0];
    (entry["state"].as_str(), entry)
}

#[derive(Debug)]
pub struct Thumbnails {
    batch: String,
    /// The `users` domain's name lookup, if it has an endpoint.
    usernames: Option<String>,
}

impl Thumbnails {
    /// `None` when the `thumbnails` domain has no endpoint.
    pub fn new(endpoints: &Endpoints) -> Option<Self> {
        Some(Self {
            batch: endpoints.url("thumbnails", BATCH_PATH)?,
            usernames: endpoints.url("users", "/v1/usernames/users"),
        })
    }

    /// Where the thumbnail's image is, once the service has it: it may
    /// still be rendering (`Pending`), so it is asked again until it is,
    /// for [`RENDER_WAIT`] at most.
    pub fn locate(&self, t: &Thumbnail) -> Result<String, Failure> {
        let id = match &t.target {
            Target::Id(id) => *id,
            Target::Username(name) => self.user_id(name)?,
        };
        let body = batch_body(t, id).to_string();
        let start = Instant::now();
        let mut poll = FIRST_POLL;
        loop {
            let answer = post(&self.batch, &body)?;
            let (state, entry) = batch_state(&answer);
            match state {
                Some("Completed") => {
                    return entry["imageUrl"]
                        .as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| {
                            Failure::new(FailureKind::Network, "no imageUrl in a completed answer")
                        });
                }
                Some("Pending" | "InReview") if start.elapsed() + poll < RENDER_WAIT => {
                    std::thread::sleep(poll);
                    poll = (poll * 2).min(MAX_POLL);
                }
                Some("Pending" | "InReview") => {
                    return Err(Failure::new(
                        FailureKind::Network,
                        "the thumbnail was still being rendered",
                    ));
                }
                Some("Blocked") => {
                    return Err(Failure::new(
                        FailureKind::Forbidden,
                        "the thumbnail is blocked",
                    ));
                }
                // "Error" for a target that doesn't exist (an `imageUrl`
                // comes with it, of a broken image Studio doesn't draw).
                Some("Error") => {
                    let message = entry["errorMessage"]
                        .as_str()
                        .filter(|m| !m.is_empty())
                        .unwrap_or("no such thumbnail");
                    return Err(Failure::new(FailureKind::NotFound, message));
                }
                state => {
                    return Err(Failure::new(
                        FailureKind::Network,
                        format!("unexpected thumbnail state {state:?}"),
                    ));
                }
            }
        }
    }

    /// A username's user id, from the `users` domain.
    fn user_id(&self, name: &str) -> Result<u64, Failure> {
        let Some(url) = &self.usernames else {
            return Err(Failure::new(
                FailureKind::Offline,
                "a thumbnail by username needs the users domain, which has no endpoint",
            ));
        };
        let body = json!({ "usernames": [name], "excludeBannedUsers": false }).to_string();
        post(url, &body)?["data"][0]["id"]
            .as_u64()
            .ok_or_else(|| Failure::new(FailureKind::NotFound, format!("no user named {name}")))
    }
}

/// POST a JSON body and read the JSON answer of a 2xx.
fn post(url: &str, body: &str) -> Result<Value, Failure> {
    let response = web::send(&Request {
        method: "POST",
        url,
        headers: &[("Content-Type", "application/json")],
        body: body.as_bytes(),
        max_bytes: MAX_ANSWER,
    })?;
    let status = response.status;
    if !(200..300).contains(&status) {
        return Err(Failure::new(
            failure_kind(u64::from(status), status),
            format!("HTTP {status} from {url}"),
        ));
    }
    serde_json::from_slice(&response.body).map_err(|_| {
        Failure::new(
            FailureKind::Network,
            format!("HTTP {status}: unexpected answer from {url}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn thumbnails_go_to_their_domain_and_usernames_to_users() {
        let roblox = Thumbnails::new(&Endpoints::default()).unwrap();
        assert_eq!(roblox.batch, "https://thumbnails.roblox.com/v1/batch");
        assert_eq!(roblox.usernames, None);
        let mirror = Thumbnails::new(&Endpoints::new(Some("example.com"), [])).unwrap();
        assert_eq!(mirror.batch, "https://thumbnails.example.com/v1/batch");
        assert_eq!(
            mirror.usernames.as_deref(),
            Some("https://users.example.com/v1/usernames/users")
        );
        assert!(Thumbnails::new(&Endpoints::offline()).is_none());
    }
}
