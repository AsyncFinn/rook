//! Where Rook's web requests go: a base URL for each Roblox web domain
//! (`assetdelivery`, `users`, ...), so Rook can run offline, against a
//! mirror, or against a server of its own that answers Roblox's web API.
//! The engine builds one from the settings (`docs/guide/tools.md`); what
//! each API asks of its domain is in `docs/design/web-endpoints.md`.

use std::collections::HashMap;

/// The domains Rook sends to Roblox's own hosts when nothing is set: asset
/// delivery, which it called before endpoints were configurable, and the
/// two Roblox's client packages come from (the live version, then the
/// packages). Every other domain has no endpoint by default, and its APIs
/// answer locally.
const ROBLOX_BY_DEFAULT: &[&str] = &["assetdelivery", "clientsettings", "setup"];

/// Roblox's deployment CDN, the `setup` domain: the one Roblox host Rook
/// calls that isn't a `<name>.roblox.com`.
const SETUP: &str = "https://setup.rbxcdn.com";

/// Roblox's web domains, each a `<name>.roblox.com` host.
const ROBLOX: &str = "roblox.com";

/// A base URL per web domain, or none (the domain's APIs answer locally).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Endpoints {
    base: Base,
    /// Per-domain choices, which win over `base`; `None` is off.
    domains: HashMap<String, Option<String>>,
}

/// Where domains without a choice of their own go.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Base {
    /// Roblox's own hosts for [`ROBLOX_BY_DEFAULT`], nowhere for the rest.
    Default,
    /// Nowhere.
    Off,
    /// `example.com`: `users.roblox.com` becomes `https://users.example.com`.
    Domain(String),
    /// `http://127.0.0.1:8080`: `users.roblox.com` becomes
    /// `http://127.0.0.1:8080/users`, for one server answering every domain.
    Url(String),
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            base: Base::Default,
            domains: HashMap::new(),
        }
    }
}

impl Endpoints {
    /// `base` is `None` (Rook's defaults), `"off"`, a domain such as
    /// `example.com`, or a URL with its scheme. Each of `domains` maps a
    /// domain name (`users`) to a base URL, or to `"off"`.
    pub fn new<'a>(
        base: Option<&str>,
        domains: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Self {
        let base = match base.map(str::trim) {
            None | Some("") => Base::Default,
            Some(b) if b.eq_ignore_ascii_case("off") => Base::Off,
            Some(b) if b.contains("://") => Base::Url(b.trim_end_matches('/').to_owned()),
            Some(b) => Base::Domain(b.trim_matches(['.', '/']).to_ascii_lowercase()),
        };
        let domains = domains
            .into_iter()
            .map(|(name, url)| {
                let url = url.trim();
                let url = (!url.eq_ignore_ascii_case("off") && !url.is_empty())
                    .then(|| url.trim_end_matches('/').to_owned());
                (name.trim().to_ascii_lowercase(), url)
            })
            .collect();
        Self { base, domains }
    }

    /// No endpoints at all: every API answers locally.
    pub fn offline() -> Self {
        Self::new(Some("off"), [])
    }

    /// The base URL of `domain` (`users`), without a trailing slash, or
    /// `None` when its APIs answer locally.
    pub fn base(&self, domain: &str) -> Option<String> {
        if let Some(chosen) = self.domains.get(domain) {
            return chosen.clone();
        }
        match &self.base {
            Base::Default => ROBLOX_BY_DEFAULT
                .contains(&domain)
                .then(|| roblox_host(domain)),
            Base::Off => None,
            Base::Domain(d) => Some(format!("https://{domain}.{d}")),
            Base::Url(u) => Some(format!("{u}/{domain}")),
        }
    }

    /// `path` (starting with `/`) on `domain`'s endpoint.
    pub fn url(&self, domain: &str, path: &str) -> Option<String> {
        self.base(domain).map(|b| format!("{b}{path}"))
    }

    /// Whether `domain` goes to Roblox's own host, the only place a
    /// Roblox credential may be sent.
    pub fn is_roblox(&self, domain: &str) -> bool {
        self.base(domain).as_deref() == Some(roblox_host(domain).as_str())
    }

    /// A Roblox web URL (`https://users.roblox.com/v1/users/1`) on its
    /// domain's endpoint, or `None` when that domain has none or the URL
    /// isn't on a Roblox web domain. `roblox.com` itself is the `www`
    /// domain.
    pub fn rewrite(&self, url: &str) -> Option<String> {
        let (_, rest) = url.split_once("://")?;
        let (host, path) = rest
            .find(['/', '?'])
            .map_or((rest, ""), |at| rest.split_at(at));
        let host = host.to_ascii_lowercase();
        let domain = if host == ROBLOX {
            "www"
        } else {
            host.strip_suffix(ROBLOX)?.strip_suffix('.')?
        };
        let path = if path.starts_with('?') {
            format!("/{path}")
        } else {
            path.to_owned()
        };
        self.url(domain, &path)
    }
}

fn roblox_host(domain: &str) -> String {
    match domain {
        "setup" => SETUP.to_owned(),
        _ => format!("https://{domain}.{ROBLOX}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_reach_only_what_rook_always_called() {
        let e = Endpoints::default();
        assert_eq!(
            e.url("assetdelivery", "/v2/assetId/1").as_deref(),
            Some("https://assetdelivery.roblox.com/v2/assetId/1")
        );
        assert!(e.is_roblox("assetdelivery"));
        assert_eq!(
            e.url("setup", "/version-1-rbxPkgManifest.txt").as_deref(),
            Some("https://setup.rbxcdn.com/version-1-rbxPkgManifest.txt")
        );
        assert_eq!(e.base("apis"), None);
        assert_eq!(e.base("users"), None);
        assert_eq!(e.rewrite("https://users.roblox.com/v1/users/1"), None);
    }

    #[test]
    fn a_base_domain_maps_each_subdomain() {
        let e = Endpoints::new(Some("Example.com"), []);
        assert_eq!(
            e.rewrite("https://users.roblox.com/v1/users/1?x=2")
                .as_deref(),
            Some("https://users.example.com/v1/users/1?x=2")
        );
        assert_eq!(
            e.rewrite("https://www.roblox.com/").as_deref(),
            Some("https://www.example.com/")
        );
        assert_eq!(
            e.rewrite("https://roblox.com?a=1").as_deref(),
            Some("https://www.example.com/?a=1")
        );
        assert!(!e.is_roblox("assetdelivery"));
    }

    #[test]
    fn a_base_url_puts_domains_in_its_path() {
        let e = Endpoints::new(Some("http://127.0.0.1:8080/"), []);
        assert_eq!(
            e.url("users", "/v1/usernames/users").as_deref(),
            Some("http://127.0.0.1:8080/users/v1/usernames/users")
        );
    }

    #[test]
    fn domains_win_over_the_base() {
        let e = Endpoints::new(
            Some("off"),
            [("Users", "http://mirror.test/u/"), ("apis", "off")],
        );
        assert_eq!(
            e.url("users", "/v1/users/1").as_deref(),
            Some("http://mirror.test/u/v1/users/1")
        );
        assert_eq!(e.base("assetdelivery"), None);
        assert_eq!(
            Endpoints::new(None, [("assetdelivery", "off")]).base("assetdelivery"),
            None
        );
        assert_eq!(Endpoints::offline().base("assetdelivery"), None);
    }

    #[test]
    fn only_roblox_web_domains_rewrite() {
        let e = Endpoints::new(Some("example.com"), []);
        for url in [
            "https://example.com/x",
            "https://notroblox.com/x",
            "https://c0.rbxcdn.com/x",
            "users.roblox.com/v1",
        ] {
            assert_eq!(e.rewrite(url), None, "{url}");
        }
    }
}
