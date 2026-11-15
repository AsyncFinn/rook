//! `cargo xtask conformance`: record Roblox's behavior in Studio, then check Rook
//! against it.
//!
//! Cases are Luau files under `conformance/cases/<group>/<name>.luau`. A case
//! calls `log(...)` to record lines and may yield. `record` runs every case in
//! a Studio playtest server through Studio's MCP server and writes the output
//! to `conformance/expected/<group>/<name>.txt`. `run` runs the same cases in
//! Rook and compares. Each expected file starts with the Studio version and
//! the signal mode the place used, and Rook runs the case in that mode.
//!
//! A case whose source has a `--! client input ...` line runs on the
//! playtest client instead, with the mouse and keyboard steps it lists
//! played while it runs (input only reaches clients; see [`client`]).
//! A server case with a `--! player` line needs the player a Studio
//! playtest has; Rook joins one for it.

mod client;
mod record;
mod run;

pub(crate) use record::record;
pub(crate) use run::run;

use std::path::{Path, PathBuf};

/// Wraps a case. Both sides run exactly this code; Rook's copy reports the
/// result through `print` with a marker instead of returning it.
fn wrap(case: &str) -> String {
    format!(
        r##"local __out = {{}}
local function log(...)
    local n = select("#", ...)
    local parts = table.create(n)
    for i = 1, n do
        parts[i] = tostring((select(i, ...)))
    end
    table.insert(__out, table.concat(parts, " "))
end
local __probe = Instance.new("BindableEvent")
local __immediate = false
__probe.Event:Connect(function() __immediate = true end)
__probe:Fire()
local __mode = if __immediate then "immediate" else "deferred"
local __ok, __err = pcall(function()
{case}
end)
if not __ok then
    log("ERROR", __err)
end
return "-- studio " .. version() .. "\n-- signals: " .. __mode .. "\n" .. table.concat(__out, "\n")
"##
    )
}

const MARKER: &str = "\u{1}ROOK-CONFORMANCE\u{1}";

struct Case {
    /// `group/name`.
    id: String,
    source: String,
    /// For client cases, the input played while the case runs.
    input: Option<Vec<client::Step>>,
    /// It needs a player on the server, as a Studio playtest has.
    player: bool,
}

fn cases(root: &Path, filter: Option<&str>) -> anyhow::Result<Vec<Case>> {
    let dir = root.join("cases");
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(&dir).sort_by_file_name() {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("luau") {
            continue;
        }
        let id = path
            .strip_prefix(&dir)?
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");
        if filter.is_some_and(|f| !id.contains(f)) {
            continue;
        }
        let source = fs_err::read_to_string(path)?;
        let input = client::steps(&source)?;
        let player = source.lines().any(|l| l.trim() == "--! player");
        out.push(Case {
            id,
            source,
            input,
            player,
        });
    }
    anyhow::ensure!(!out.is_empty(), "no cases found under {}", dir.display());
    Ok(out)
}

fn expected_path(root: &Path, id: &str) -> PathBuf {
    root.join("expected").join(format!("{id}.txt"))
}
