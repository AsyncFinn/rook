//! A minimal client for Roblox Studio's built-in MCP server.
//!
//! Studio ships a launcher at `%LOCALAPPDATA%\Roblox\mcp.bat` that speaks
//! MCP (JSON-RPC over stdio) once "Studio as MCP server" is enabled in the
//! Assistant settings. Rook only uses its documented tools: `execute_luau`,
//! `start_stop_play`, `get_studio_state`, `list_roblox_studios` and
//! `user_mouse_input`.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, channel};
use std::time::{Duration, Instant};

use anyhow::{Context, bail};
use serde_json::{Value, json};

pub struct Studio {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
    next_id: u64,
    studio_id: String,
}

impl Studio {
    /// Start the MCP launcher and wait for a Studio window to attach.
    pub fn connect() -> anyhow::Result<Self> {
        let local = std::env::var("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
        let launcher = std::path::Path::new(&local).join("Roblox").join("mcp.bat");
        anyhow::ensure!(
            launcher.is_file(),
            "{} not found; enable Studio's MCP server in Assistant > Manage MCP Servers",
            launcher.display()
        );
        let mut child = Command::new("cmd.exe")
            .arg("/c")
            .arg(&launcher)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("starting Studio's MCP launcher")?;
        let stdin = child.stdin.take().expect("piped");
        let stdout = child.stdout.take().expect("piped");
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut studio = Self {
            child,
            stdin,
            lines,
            next_id: 1,
            studio_id: String::new(),
        };
        studio.request(
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "rook-conformance", "version": env!("CARGO_PKG_VERSION") },
            }),
            Duration::from_secs(30),
        )?;
        studio.notify("notifications/initialized")?;

        // Studio attaches to a fresh server a moment after it starts.
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let text =
                studio.tool_raw("list_roblox_studios", json!({}), Duration::from_secs(30))?;
            let list: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            // With several Studio windows open, ROOK_STUDIO picks the one
            // whose name (its place) contains that text.
            let wanted = std::env::var("ROOK_STUDIO").unwrap_or_default();
            let chosen = list["studios"].as_array().and_then(|all| {
                all.iter().find(|s| {
                    s["name"]
                        .as_str()
                        .is_some_and(|name| name.contains(wanted.as_str()))
                })
            });
            if let Some(id) = chosen.and_then(|s| s["id"].as_str()) {
                studio.studio_id = id.to_owned();
                return Ok(studio);
            }
            if Instant::now() > deadline {
                bail!(
                    "no Studio window attached; open a place in Studio with the MCP server enabled"
                );
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    }

    fn send(&mut self, msg: &Value) -> anyhow::Result<()> {
        writeln!(self.stdin, "{msg}")?;
        self.stdin.flush()?;
        Ok(())
    }

    fn notify(&mut self, method: &str) -> anyhow::Result<()> {
        self.send(&json!({ "jsonrpc": "2.0", "method": method }))
    }

    fn request(&mut self, method: &str, params: Value, timeout: Duration) -> anyhow::Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = match self.lines.recv_timeout(left) {
                Ok(l) => l,
                Err(RecvTimeoutError::Timeout) => bail!("Studio did not answer {method} in time"),
                Err(RecvTimeoutError::Disconnected) => bail!("Studio's MCP server exited"),
            };
            let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if msg["id"].as_u64() != Some(id) {
                continue;
            }
            if let Some(err) = msg.get("error") {
                bail!("{method} failed: {err}");
            }
            return Ok(msg["result"].clone());
        }
    }

    fn tool_raw(&mut self, name: &str, args: Value, timeout: Duration) -> anyhow::Result<String> {
        let result = self.request(
            "tools/call",
            json!({ "name": name, "arguments": args }),
            timeout,
        )?;
        let text: String = result["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if result["isError"].as_bool() == Some(true) {
            bail!("{name}: {text}");
        }
        Ok(text)
    }

    fn tool(&mut self, name: &str, mut args: Value, timeout: Duration) -> anyhow::Result<String> {
        args["studio_id"] = json!(self.studio_id);
        self.tool_raw(name, args, timeout)
    }

    /// "Edit", "Play", ... from `get_studio_state`.
    pub fn mode(&mut self) -> anyhow::Result<String> {
        let state = self.tool("get_studio_state", json!({}), Duration::from_secs(30))?;
        Ok(state
            .lines()
            .find_map(|l| l.trim().strip_prefix("- Current Studio Mode:"))
            .unwrap_or("")
            .trim()
            .to_owned())
    }

    pub fn set_playing(&mut self, play: bool) -> anyhow::Result<()> {
        self.tool(
            "start_stop_play",
            json!({ "is_start": play }),
            Duration::from_secs(120),
        )?;
        Ok(())
    }

    /// Run Luau in the playtest server and return its result.
    pub fn run_on_server(&mut self, code: &str) -> anyhow::Result<String> {
        self.tool(
            "execute_luau",
            json!({ "code": code, "datamodel_type": "Server" }),
            Duration::from_secs(120),
        )
    }

    /// Run Luau in the playtest client while the mouse moves through
    /// `moves` (screen pixels), and return its result. The code runs on its
    /// own thread so the moves arrive while it waits for them.
    pub fn run_on_client(&mut self, code: &str, moves: &[(f32, f32)]) -> anyhow::Result<String> {
        const PENDING: &str = "\u{1}pending";
        let client = |code: &str| json!({ "code": code, "datamodel_type": "Client" });
        let start = format!(
            "_G.__rook_conformance = nil\ntask.spawn(function()\n\
             _G.__rook_conformance = (function()\n{code}\nend)()\nend)"
        );
        self.tool("execute_luau", client(&start), Duration::from_secs(120))?;
        let actions: Vec<Value> = moves
            .iter()
            .flat_map(|&(x, y)| {
                [
                    json!({ "action": "moveTo", "x": x, "y": y }),
                    json!({ "action": "wait", "wait_time_ms": 200 }),
                ]
            })
            .collect();
        self.tool(
            "user_mouse_input",
            json!({ "actions": actions, "datamodel_type": "Client" }),
            Duration::from_secs(60),
        )?;
        let poll = format!(
            "local r = _G.__rook_conformance\n_G.__rook_conformance = nil\nreturn r or \"{PENDING}\""
        );
        let deadline = Instant::now() + Duration::from_secs(60);
        while Instant::now() < deadline {
            let r = self.tool("execute_luau", client(&poll), Duration::from_secs(30))?;
            if r != PENDING {
                return Ok(r);
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        bail!("client case did not finish within 60 s")
    }
}

impl Drop for Studio {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}
