//! HTTP requests (asset delivery, scripts' web APIs) to the URLs
//! [`crate::endpoints`] gives.

use std::fmt::Debug;

/// An HTTP request.
#[derive(Debug, Clone, Copy)]
pub struct Request<'a> {
    /// `GET`, `POST`, ...
    pub method: &'a str,
    pub url: &'a str,
    pub headers: &'a [(&'a str, &'a str)],
    /// Sent when not empty.
    pub body: &'a [u8],
    /// The answer's body is cut off past this.
    pub max_bytes: u64,
}

/// An answer, whatever its status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    /// Decompressed.
    pub body: Vec<u8>,
}

impl Response {
    /// A header's value; names are compared ignoring case, as HTTP's are.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }
}

pub trait Network: Send + Sync + Debug {
    /// Blocks until the answer has arrived. `Err` is a failure to get one
    /// at all (no connection, a timeout), described.
    fn send(&self, request: &Request<'_>) -> Result<Response, String>;
}

/// HTTPS through ureq (rustls, gzip), a minute per request at most.
#[cfg(feature = "native")]
#[derive(Debug)]
pub struct Ureq {
    agent: ureq::Agent,
}

#[cfg(feature = "native")]
impl Ureq {
    pub fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(std::time::Duration::from_secs(60)))
            .http_status_as_error(false)
            .user_agent(concat!(
                "Rook/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/AsyncFinn/rook)"
            ))
            .build()
            .into();
        Self { agent }
    }
}

#[cfg(feature = "native")]
impl Default for Ureq {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "native")]
impl Network for Ureq {
    fn send(&self, request: &Request<'_>) -> Result<Response, String> {
        use std::io::Read;
        let mut call = ureq::http::Request::builder()
            .method(request.method)
            .uri(request.url);
        for (name, value) in request.headers {
            call = call.header(*name, *value);
        }
        let call = call
            .body(request.body.to_vec())
            .map_err(|e| e.to_string())?;
        let mut response = self.agent.run(call).map_err(|e| e.to_string())?;
        let headers = response
            .headers()
            .iter()
            .filter_map(|(n, v)| Some((n.as_str().to_owned(), v.to_str().ok()?.to_owned())))
            .collect();
        let mut body = Vec::new();
        response
            .body_mut()
            .as_reader()
            .take(request.max_bytes)
            .read_to_end(&mut body)
            .map_err(|e| e.to_string())?;
        Ok(Response {
            status: response.status().as_u16(),
            headers,
            body,
        })
    }
}
