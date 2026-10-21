//! Roblox asset delivery: the only network access in Rook. Requests are
//! anonymous; for assets refused anonymously, an Open Cloud key the user set
//! is sent to the Open Cloud endpoint (in `x-api-key`) and nowhere else.
//! Rook never reads browser cookies or any other credential. What the
//! endpoints answer is in `docs/behavior/content.md`. Requests go
//! through the host's network (`rook_platform::network`).

use std::time::Duration;

use rook_platform::network::Request;

use crate::{Failure, FailureKind};

const ANONYMOUS: &str = "https://assetdelivery.roblox.com/v2/assetId";
const OPEN_CLOUD: &str = "https://apis.roblox.com/asset-delivery-api/v1/assetId";
/// Larger answers are refused (Roblox's own limits are far below this).
const MAX_BYTES: u64 = 256 << 20;
/// Tries per request when Roblox says to slow down (HTTP 429).
const TRIES: u32 = 4;

/// Asset delivery's requests, with the user's Open Cloud key if any.
#[derive(Debug)]
pub struct Delivery {
    api_key: Option<String>,
}

impl Delivery {
    pub fn new(api_key: Option<String>) -> Self {
        Self { api_key }
    }

    pub fn has_api_key(&self) -> bool {
        self.api_key.is_some()
    }

    /// Download an asset (its latest version, or `version`).
    pub fn fetch(&self, id: u64, version: Option<u32>) -> Result<Asset, Failure> {
        let path = match version {
            Some(v) => format!("{id}/version/{v}"),
            None => id.to_string(),
        };
        let anonymous = self.locate(&format!("{ANONYMOUS}/{path}"), None);
        let (location, version) = match (anonymous, &self.api_key) {
            (Err(f), Some(key)) if f.kind == FailureKind::Forbidden => {
                self.locate(&format!("{OPEN_CLOUD}/{path}"), Some(key))?
            }
            (result, _) => result?,
        };
        let bytes = self.download(&location)?;
        let url = location.split('?').next().unwrap_or("").to_owned();
        Ok(Asset {
            bytes,
            url,
            version,
        })
    }

    /// An asset's latest version, without downloading it.
    pub fn latest_version(&self, id: u64) -> Result<u32, Failure> {
        let (_, version) = self.locate(&format!("{ANONYMOUS}/{id}"), None)?;
        version.ok_or_else(|| Failure::new(FailureKind::Network, "no version in the answer"))
    }

    /// Ask where an asset's bytes are, and which version they are (the
    /// `roblox-assetversionnumber` header, when sent).
    fn locate(&self, url: &str, key: Option<&str>) -> Result<(String, Option<u32>), Failure> {
        let (status, body, version) = self.get(url, key)?;
        let json: serde_json::Value = serde_json::from_slice(&body).map_err(|_| {
            Failure::new(
                FailureKind::Network,
                format!("HTTP {status}: unexpected answer from {url}"),
            )
        })?;
        // The legacy API answers `locations: [{assetFormat, location}]`,
        // Open Cloud `location` (creator-docs).
        let location = json["locations"]
            .as_array()
            .and_then(|all| {
                all.iter()
                    .find(|l| l["assetFormat"] == "source")
                    .or_else(|| all.first())
            })
            .and_then(|l| l["location"].as_str())
            .or_else(|| json["location"].as_str());
        if let Some(location) = location {
            return Ok((location.to_owned(), version));
        }
        let error = &json["errors"][0];
        let code = error["code"].as_u64().unwrap_or(u64::from(status));
        let message = error["message"]
            .as_str()
            .unwrap_or("no location")
            .to_owned();
        Err(Failure::new(failure_kind(code, status), message))
    }

    fn download(&self, url: &str) -> Result<Vec<u8>, Failure> {
        let (status, body, _) = self.get(url, None)?;
        if status == 200 {
            Ok(body)
        } else {
            Err(Failure::new(
                failure_kind(u64::from(status), status),
                format!("HTTP {status} from the CDN"),
            ))
        }
    }

    /// GET with retries on HTTP 429; the body is decompressed. Returns the
    /// status, the body and the asset version header.
    fn get(&self, url: &str, key: Option<&str>) -> Result<(u16, Vec<u8>, Option<u32>), Failure> {
        let key_header = key.map(|key| [("x-api-key", key)]);
        let headers: &[(&str, &str)] = match &key_header {
            Some(h) => h,
            None => &[],
        };
        let request = Request {
            url,
            headers,
            max_bytes: MAX_BYTES,
        };
        for attempt in 1..=TRIES {
            let response = rook_platform::get()
                .network
                .get(&request)
                .map_err(|e| Failure::new(FailureKind::Network, e))?;
            let status = response.status;
            if status == 429 && attempt < TRIES {
                let wait = response
                    .header("retry-after")
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1 << attempt)
                    .min(30);
                std::thread::sleep(Duration::from_secs(wait));
                continue;
            }
            let version = response
                .header("roblox-assetversionnumber")
                .and_then(|v| v.parse().ok());
            return Ok((status, response.body, version));
        }
        Err(Failure::new(
            FailureKind::Network,
            "rate limited (HTTP 429)",
        ))
    }
}

/// A downloaded asset.
#[derive(Debug)]
pub struct Asset {
    pub bytes: Vec<u8>,
    /// The URL the bytes came from, without its signature.
    pub url: String,
    /// Which version asset delivery says it is.
    pub version: Option<u32>,
}

/// An asset delivery error code (or HTTP status) as a failure kind.
fn failure_kind(code: u64, status: u16) -> FailureKind {
    match code {
        404 | 410 => FailureKind::NotFound,
        401 | 403 | 409 => FailureKind::Forbidden,
        // "Get Assets by AssetVersionId is deprecated" comes as code 0 with
        // HTTP 400.
        _ if status == 400 => FailureKind::Unsupported,
        _ => FailureKind::Network,
    }
}
