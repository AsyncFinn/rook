//! Images as Roblox's client fetches them for textures: not the upload
//! (`source`) but a KTX2 transcode of it in BC1 or BC3 blocks, split in
//! mip packs, each a file of its own on the CDN
//! (`docs/behavior/content/asset-fetching.md#textures`). Pack 0 holds the
//! smallest levels and names every pack's levels; Rook fetches it, then
//! the rest in one batch request, and joins them into one file
//! ([`ktx2::join`]). Roblox's client fetches the larger packs only as a
//! texture grows on screen; Rook fetches them all.

use std::sync::atomic::Ordering;

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use rook_platform::network::Request;
use serde_json::{Value, json};

use super::web::{self, failure_kind};
use super::{Asset, Delivery, MAX_BYTES};
use crate::decode::image::ktx2;
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
    /// An image as Roblox transcodes it for textures, every mip pack
    /// joined into one KTX2 file. Fails when there is no such
    /// representation (not an image, refused anonymously) or it isn't
    /// made yet ([`FailureKind::Network`]: Roblox makes it on the first
    /// request, and it's there seconds later); the caller falls back to
    /// the upload.
    pub fn fetch_texture(&self, id: u64, version: Option<u32>) -> Result<Asset, Failure> {
        let invalid = |e: &dyn std::fmt::Display| Failure::new(FailureKind::Invalid, e.to_string());
        let mut packs = self.packs(id, version, &[0])?;
        let layout = ktx2::File::read(&packs[0].bytes)
            .map_err(|e| invalid(&e))?
            .packs();
        let rest: Vec<u8> = (1..layout.len().min(MAX_PACKS)).map(|p| p as u8).collect();
        if !rest.is_empty() {
            packs.extend(self.packs(id, version, &rest)?);
        }
        let files: Vec<&[u8]> = packs.iter().map(|p| p.bytes.as_slice()).collect();
        let bytes = ktx2::join(&files).map_err(|e| invalid(&e))?;
        let first = packs.swap_remove(0);
        Ok(Asset {
            bytes,
            url: first.url,
            version: None,
        })
    }

    /// These packs of an image, in that order: one batch request for
    /// their locations, then each from the CDN.
    fn packs(&self, id: u64, version: Option<u32>, packs: &[u8]) -> Result<Vec<Asset>, Failure> {
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
                let location = answer["location"]
                    .as_str()
                    .filter(|_| answer["contentRepresentationSpecifier"].is_object());
                match location {
                    Some(location) => Ok(Asset {
                        bytes: web::download(location, MAX_BYTES)?,
                        url: location.split('?').next().unwrap_or("").to_owned(),
                        version: None,
                    }),
                    None => Err(answer_failure(answer, response.status)),
                }
            })
            .collect()
    }
}

/// Why a batch answer has no location.
fn answer_failure(answer: &Value, status: u16) -> Failure {
    let error = &answer["errors"][0];
    let message = error["message"].as_str().unwrap_or("no texture").to_owned();
    if error["customErrorCode"].as_u64() == Some(BEING_GENERATED) {
        return Failure::new(FailureKind::Network, message);
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
    }
}
