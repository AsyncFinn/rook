//! Images as Roblox's client fetches them for textures: not the upload
//! (`source`) but a KTX2 transcode of it in BC1 or BC3 blocks, split in
//! mip packs, each a file of its own on the CDN
//! (`docs/behavior/content/asset-fetching.md#textures`). Pack 0 holds the
//! smallest levels and names every pack's levels; larger packs are
//! fetched as a texture grows on screen, several in one batch request, and
//! the resolver joins the ones it has into one file (`ktx2::join`,
//! `Resolver::download_texture`). Images refused
//! anonymously come pack by pack from Open Cloud's asset delivery with the
//! user's key, which takes the same representation list.

use std::sync::atomic::Ordering;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rook_platform::network::Request;
use serde_json::{Value, json};

use super::web::{self, failure_kind};
use super::{Asset, Delivery, MAX_BYTES, OPEN_CLOUD};
use crate::{Failure, FailureKind};

/// The representation Studio 0.748 asks for (seen in its requests):
/// KTX2 from transcoder major version 8.
const FORMAT: &str = "ktx2";
const MAJOR_VERSION: &str = "8";
/// The start of the `fidelity` a desktop client sends: a mask over the
/// transcoder's schema (texture semantics, block formats the GPU samples,
/// and which mips) that asks for BC1 and BC3. The byte after it is the
/// pack, plus one. Asset delivery refuses masks it doesn't know.
const FIDELITY: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFC];
/// The transcoder's schema has eight packs (`pack0` to `pack7`).
const MAX_PACKS: usize = 8;
/// Asset delivery's `customErrorCode` for a representation not made yet.
const BEING_GENERATED: u64 = 21;
/// The message of the failure for a texture not made yet
/// ([`being_generated`]).
const BEING_GENERATED_MESSAGE: &str = "the texture is being generated";

/// Whether `failure` says the texture isn't made yet (asset delivery's
/// 406 with `customErrorCode` 21): Roblox starts making it on the first
/// request, so asking again shortly may find it.
pub(crate) fn being_generated(failure: &Failure) -> bool {
    failure.kind == FailureKind::Network && failure.message == BEING_GENERATED_MESSAGE
}

/// The `contentRepresentationPriorityList` asking for one pack: base64
/// of a JSON list of one representation.
fn priority_list(pack: u8) -> String {
    let mut fidelity = FIDELITY.to_vec();
    fidelity.push(pack + 1);
    let list = json!([{
        "format": FORMAT,
        "majorVersion": MAJOR_VERSION,
        "fidelity": STANDARD.encode(fidelity),
    }]);
    STANDARD.encode(list.to_string())
}

impl Delivery {
    /// These mip packs of an image as Roblox transcodes it for textures,
    /// in that order: anonymously through the batch endpoint, or, where
    /// that's refused and the user saved an Open Cloud key, one by one from
    /// Open Cloud. Fails when there is no such representation (not an
    /// image, refused) or it isn't made yet ([`FailureKind::Network`]:
    /// Roblox makes it on the first request, and it's there seconds later);
    /// the caller falls back to the upload.
    pub fn texture_packs(
        &self,
        id: u64,
        version: Option<u32>,
        packs: &[u8],
    ) -> Result<Vec<Asset>, Failure> {
        let packs: Vec<u8> = packs
            .iter()
            .copied()
            .filter(|&p| usize::from(p) < MAX_PACKS)
            .collect();
        let packs = packs.as_slice();
        match self.batch_packs(id, version, packs) {
            Err(f) if f.kind == FailureKind::Forbidden => match self.key() {
                Some(key) => packs
                    .iter()
                    .map(|&p| self.open_cloud_pack(id, version, p, key))
                    .collect(),
                None => Err(f),
            },
            result => result,
        }
    }

    /// One pack from Open Cloud's asset delivery, with the user's key: the
    /// list goes in the query, and the answer is a batch item's.
    fn open_cloud_pack(
        &self,
        id: u64,
        version: Option<u32>,
        pack: u8,
        key: &str,
    ) -> Result<Asset, Failure> {
        let path = match version {
            Some(v) => format!("{id}/version/{v}"),
            None => id.to_string(),
        };
        let list = priority_list(pack)
            .replace('+', "%2B")
            .replace('/', "%2F")
            .replace('=', "%3D");
        let url = format!(
            "{OPEN_CLOUD}/{path}?contentRepresentationPriorityList={list}&doNotFallbackToBaselineRepresentation=true"
        );
        let response = web::send(&Request {
            method: "GET",
            url: &url,
            headers: &[("x-api-key", key)],
            body: &[],
            max_bytes: MAX_BYTES,
        })?;
        let answer: Value = serde_json::from_slice(&response.body).map_err(|_| {
            Failure::new(
                FailureKind::Network,
                format!(
                    "HTTP {}: unexpected answer from {OPEN_CLOUD}",
                    response.status
                ),
            )
        })?;
        pack_from(&answer, response.status)
    }

    /// Their locations in one batch request, then each from the CDN.
    fn batch_packs(
        &self,
        id: u64,
        version: Option<u32>,
        packs: &[u8],
    ) -> Result<Vec<Asset>, Failure> {
        if !self.batch_works.load(Ordering::Relaxed) {
            return Err(Failure::new(
                FailureKind::Network,
                "asset delivery has no batch endpoint",
            ));
        }
        let items: Vec<Value> = packs
            .iter()
            .map(|&p| {
                let mut item = json!({
                    "assetId": id,
                    "requestId": p.to_string(),
                    "contentRepresentationPriorityList": priority_list(p),
                    "doNotFallbackToBaselineRepresentation": true,
                });
                if let Some(v) = version {
                    item["version"] = json!(v);
                }
                item
            })
            .collect();
        let body = Value::Array(items).to_string();
        let response = web::send(&Request {
            method: "POST",
            url: &self.batch,
            headers: &[("Content-Type", "application/json")],
            body: body.as_bytes(),
            max_bytes: MAX_BYTES,
        })?;
        let unexpected = || {
            Failure::new(
                FailureKind::Network,
                format!(
                    "HTTP {}: unexpected answer from {}",
                    response.status, self.batch
                ),
            )
        };
        if matches!(response.status, 404 | 405) {
            self.batch_works.store(false, Ordering::Relaxed);
        }
        let answers: Vec<Value> =
            serde_json::from_slice(&response.body).map_err(|_| unexpected())?;
        packs
            .iter()
            .map(|p| {
                let answer = answers
                    .iter()
                    .find(|a| a["requestId"].as_str().and_then(|r| r.parse().ok()) == Some(*p))
                    .ok_or_else(unexpected)?;
                pack_from(answer, response.status)
            })
            .collect()
    }
}

/// A pack from asset delivery's answer for it: its bytes from the CDN
/// where it names the representation it matched, else why not.
fn pack_from(answer: &Value, status: u16) -> Result<Asset, Failure> {
    let location = answer["location"]
        .as_str()
        .filter(|_| answer["contentRepresentationSpecifier"].is_object());
    match location {
        Some(location) => Ok(Asset {
            bytes: web::download(location, MAX_BYTES)?,
            url: location.split('?').next().unwrap_or("").to_owned(),
            version: None,
        }),
        None => Err(answer_failure(answer, status)),
    }
}

/// Why an answer has no location.
fn answer_failure(answer: &Value, status: u16) -> Failure {
    let error = &answer["errors"][0];
    let message = error["message"].as_str().unwrap_or("no texture").to_owned();
    if error["customErrorCode"].as_u64() == Some(BEING_GENERATED) {
        return Failure::new(FailureKind::Network, BEING_GENERATED_MESSAGE);
    }
    let code = error["code"].as_u64().unwrap_or(u64::from(status));
    Failure::new(failure_kind(code, status), message)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What Studio 0.748 sends (from its log: pack 0 is `/////AE=`, the
    /// last byte counting packs from 1).
    #[test]
    fn asks_as_studio_does() {
        let decoded = |p| -> Value {
            serde_json::from_slice(&STANDARD.decode(priority_list(p)).unwrap()).unwrap()
        };
        assert_eq!(
            decoded(0),
            json!([{"format": "ktx2", "majorVersion": "8", "fidelity": "/////AE="}])
        );
        assert_eq!(decoded(1)[0]["fidelity"], "/////AI=");
        assert_eq!(decoded(5)[0]["fidelity"], "/////AY=");
    }

    #[test]
    fn tells_a_texture_being_made_from_a_refusal() {
        let answer = |code: u64, custom: u64| json!({"errors": [{"code": code, "message": "m", "customErrorCode": custom}]});
        assert_eq!(
            answer_failure(&answer(406, 21), 200).kind,
            FailureKind::Network
        );
        assert_eq!(
            answer_failure(&answer(404, 15), 200).kind,
            FailureKind::NotFound
        );
        assert_eq!(
            answer_failure(&answer(401, 11), 200).kind,
            FailureKind::Forbidden
        );
        assert!(being_generated(&answer_failure(&answer(406, 21), 200)));
        assert!(!being_generated(&answer_failure(&answer(404, 15), 200)));
        assert!(!being_generated(&Failure::new(
            FailureKind::Network,
            "timed out"
        )));
    }
}
