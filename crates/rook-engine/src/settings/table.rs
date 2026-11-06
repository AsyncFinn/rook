//! Every run-time setting, declared once. The table in
//! docs/design/data-and-logging.md#run-time-settings lists them in this
//! order, and a test holds the two together.

use super::{Setting, Tier, Unset};

pub const FRAME_TIMES: Setting = Setting {
    name: "frameTimes",
    tier: Tier::Any,
    unset: Unset::Is("off"),
    var: "ROOK_FRAME_TIMES",
    flag: Some("frame-times"),
};

pub const UNCAPPED: Setting = Setting {
    name: "uncapped",
    tier: Tier::Any,
    unset: Unset::Is("off"),
    var: "ROOK_UNCAPPED",
    flag: Some("uncapped"),
};

pub const FRAMERATE_CAP: Setting = Setting {
    name: "framerateCap",
    tier: Tier::Any,
    unset: Unset::Means("the player's `FramerateCap`"),
    var: "ROOK_FRAMERATE_CAP",
    flag: Some("framerate-cap"),
};

pub const AUDIO_DEVICE: Setting = Setting {
    name: "audioDevice",
    tier: Tier::Any,
    unset: Unset::Means("the player's output device"),
    var: "ROOK_AUDIO",
    flag: None,
};

pub const THREADS: Setting = Setting {
    name: "threads",
    tier: Tier::Any,
    unset: Unset::Means("the machine's logical cores"),
    var: "ROOK_THREADS",
    flag: None,
};

pub const CACHE_MAX_SIZE: Setting = Setting {
    name: "cacheMaxSize",
    tier: Tier::Any,
    unset: Unset::Is("2GB"),
    var: "ROOK_CACHE_MAX",
    flag: None,
};

pub const CACHE_FOLDER: Setting = Setting {
    name: "cacheFolder",
    tier: Tier::Any,
    unset: Unset::Means("`cache` in the data folder"),
    var: "ROOK_CACHE_DIR",
    flag: None,
};

pub const LOG: Setting = Setting {
    name: "log",
    tier: Tier::Any,
    unset: Unset::Is("info"),
    var: "ROOK_LOG",
    flag: Some("verbose"),
};

pub const OPEN_CLOUD_API_KEY: Setting = Setting {
    name: "openCloudApiKey",
    tier: Tier::Any,
    unset: Unset::Means("the key `rook-tools key set` saved"),
    var: "ROOK_OPEN_CLOUD_API_KEY",
    flag: None,
};

pub const WEB_BASE: Setting = Setting {
    name: "webBase",
    tier: Tier::Developer,
    unset: Unset::Means("the build's `webBase`"),
    var: "ROOK_WEB_BASE",
    flag: None,
};

pub const WEB_DOMAINS: Setting = Setting {
    name: "webDomains",
    tier: Tier::Developer,
    unset: Unset::Means("the build's `webDomains`"),
    var: "ROOK_WEB_DOMAINS",
    flag: None,
};

pub const CONTENT_ORDER: Setting = Setting {
    name: "contentOrder",
    tier: Tier::Developer,
    unset: Unset::Is("overrides,defaults,install,folders,delivery"),
    var: "ROOK_CONTENT_ORDER",
    flag: None,
};

pub const CONTENT_FOLDERS: Setting = Setting {
    name: "contentFolders",
    tier: Tier::Developer,
    unset: Unset::Means("none"),
    var: "ROOK_CONTENT_DIRS",
    flag: None,
};

pub const ROBLOX_INSTALL: Setting = Setting {
    name: "robloxInstall",
    tier: Tier::Developer,
    unset: Unset::Means("the newest install found"),
    var: "ROOK_ROBLOX_INSTALL",
    flag: None,
};

pub const ASSET_DELIVERY: Setting = Setting {
    name: "assetDelivery",
    tier: Tier::Developer,
    unset: Unset::Means("on in `rook-player` and `rook-tools content`, off in headless runs"),
    var: "ROOK_ASSET_DELIVERY",
    flag: None,
};

pub const DATA_FOLDER: Setting = Setting {
    name: "dataFolder",
    tier: Tier::Developer,
    unset: Unset::Means("the platform's folders"),
    var: "ROOK_DATA_DIR",
    flag: None,
};

pub const CORE_SCRIPTS: Setting = Setting {
    name: "coreScripts",
    tier: Tier::Developer,
    unset: Unset::Is("off"),
    var: "ROOK_CORE_SCRIPTS",
    flag: Some("core-scripts"),
};

/// Every setting, in the docs' order.
pub const ALL: &[&Setting] = &[
    &FRAME_TIMES,
    &UNCAPPED,
    &FRAMERATE_CAP,
    &AUDIO_DEVICE,
    &THREADS,
    &CACHE_MAX_SIZE,
    &CACHE_FOLDER,
    &LOG,
    &OPEN_CLOUD_API_KEY,
    &WEB_BASE,
    &WEB_DOMAINS,
    &CONTENT_ORDER,
    &CONTENT_FOLDERS,
    &ROBLOX_INSTALL,
    &ASSET_DELIVERY,
    &DATA_FOLDER,
    &CORE_SCRIPTS,
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The docs' table's first five columns, row by row.
    #[test]
    fn the_docs_table_lists_every_setting() {
        let doc = include_str!("../../../../docs/design/data-and-logging.md");
        let table = doc
            .split("### Run-time settings")
            .nth(1)
            .expect("the docs have a Run-time settings section");
        let rows: Vec<Vec<&str>> = table
            .lines()
            .skip_while(|l| !l.starts_with('|'))
            .take_while(|l| l.starts_with('|'))
            .skip(2)
            .map(|l| {
                l.trim_matches('|')
                    .split('|')
                    .map(str::trim)
                    .take(5)
                    .collect()
            })
            .collect();
        let code = |s: Option<&str>| s.map_or_else(String::new, |s| format!("`{s}`"));
        let expected: Vec<Vec<String>> = ALL
            .iter()
            .map(|s| {
                vec![
                    code(Some(s.name)),
                    code(Some(s.var)),
                    code(s.flag.map(|f| format!("--{f}")).as_deref()),
                    match s.tier {
                        Tier::Any => "any build".into(),
                        Tier::Developer => "developer".into(),
                    },
                    match s.unset {
                        Unset::Is(v) => format!("`{v}`"),
                        Unset::Means(m) => m.to_owned(),
                    },
                ]
            })
            .collect();
        assert_eq!(rows, expected);
    }
}
