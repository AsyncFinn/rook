//! Sending content's web requests (asset delivery, thumbnails) through the
//! host's network (`rook_platform::network`), waiting and trying again
//! while Roblox says to slow down (HTTP 429).

use std::time::Duration;

use rook_platform::network::{Request, Response};

use crate::{Failure, FailureKind};

/// Tries per request when Roblox says to slow down.
const TRIES: u32 = 4;

/// The answer, whatever its status; `Err` when none came, or Roblox still
/// said to slow down after [`TRIES`].
pub(super) fn send(request: &Request<'_>) -> Result<Response, Failure> {
    for attempt in 1..=TRIES {
        let response = rook_platform::get()
            .network
            .send(request)
            .map_err(|e| Failure::new(FailureKind::Network, e))?;
        if response.status != 429 || attempt == TRIES {
            return Ok(response);
        }
        let wait = response
            .header("retry-after")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(1 << attempt)
            .min(30);
        std::thread::sleep(Duration::from_secs(wait));
    }
    unreachable!("the last try returns")
}

/// A failure's kind from an HTTP status (or an error code an API sends in
/// its body).
pub(super) fn failure_kind(code: u64, status: u16) -> FailureKind {
    match code {
        404 | 410 => FailureKind::NotFound,
        401 | 403 | 409 => FailureKind::Forbidden,
        // "Get Assets by AssetVersionId is deprecated" comes as code 0 with
        // HTTP 400.
        _ if status == 400 => FailureKind::Unsupported,
        _ => FailureKind::Network,
    }
}

/// GET `url` and return its body, which must come with HTTP 200 (a CDN's
/// bytes).
pub(super) fn download(url: &str, max_bytes: u64) -> Result<Vec<u8>, Failure> {
    let response = send(&Request {
        method: "GET",
        url,
        headers: &[],
        body: &[],
        max_bytes,
    })?;
    if response.status == 200 {
        Ok(response.body)
    } else {
        Err(Failure::new(
            failure_kind(u64::from(response.status), response.status),
            format!("HTTP {} from the CDN", response.status),
        ))
    }
}
