//! `rook-tools run-headless`: run a place's server Scripts without a window,
//! and with `--join` a player's client too.

use std::path::Path;
use std::time::{Duration, Instant};

use rook_engine::session::Session;
use rook_engine::{DataModel, InputEvent, Options, ScreenPoint, Server};
use rook_lua::{LogEntry, MessageType};

/// How to run (the command's flags).
#[derive(Debug)]
pub(crate) struct Run<'a> {
    pub seconds: f64,
    pub timeout: f64,
    pub quiet: bool,
    pub script: Option<&'a Path>,
    /// A player joins, with a client of their own.
    pub join: bool,
    /// The client runs Roblox's CoreScripts (`settings::core_scripts`).
    pub core_scripts: bool,
    /// Print the client's tree from this instance (`CoreGui.RobloxGui`)
    /// at the end, with what places and shows its GUI.
    pub dump: Option<&'a str>,
    /// Points the joined player clicks after `seconds`, in window pixels,
    /// half a second apart; the run goes on a second after the last.
    pub clicks: &'a [ScreenPoint],
    /// The joined player's settings load from this file and are saved to
    /// it at the end (`rook_engine::user_settings`).
    pub user_settings: Option<&'a Path>,
    /// Keys (`Enum.KeyCode` names) the player presses after the clicks,
    /// half a second apart.
    pub keys: &'a [String],
    #[cfg(feature = "repl")]
    pub repl: bool,
}

pub(crate) fn run(file: &Path, how: &Run<'_>) -> anyhow::Result<()> {
    let bytes = fs_err::read(file)?;
    let options = Options {
        script_timeout: Duration::from_secs_f64(how.timeout),
        // Headless runs download nothing unless assetDelivery says so, as
        // the place's own content resolves.
        core_scripts: rook_engine::settings::core_scripts(
            how.core_scripts.then_some(true),
            &rook_content::Resolver::new(rook_engine::settings::content_config(false)),
        ),
        user_settings: how.user_settings.map(Path::to_owned),
        ..Options::default()
    };
    anyhow::ensure!(
        !how.core_scripts || options.core_scripts.is_some(),
        "no CoreScripts to run (see the warning above)"
    );
    let started = Instant::now();
    let mut server = Server::load_place(&bytes, &options)?;
    server.start();
    if let Some(script) = how.script {
        let source = fs_err::read_to_string(script)?;
        server
            .run_extra_script(&source)
            .map_err(anyhow::Error::msg)?;
    }
    if how.join {
        let mut session = Session::new(server, options);
        let index = session.join("Player1", 1)?;
        // GUI lays out on a screen the size of rook-player's window.
        if let Some(client) = session.client_mut(index) {
            client.set_viewport(1280, 720);
        }
        session.run_for(how.seconds);
        for point in how.clicks {
            for event in point.click() {
                session.input(index, event);
                session.run_for(0.15);
            }
        }
        for code in how.keys {
            for down in [true, false] {
                let code = code.clone();
                session.input(index, InputEvent::Key { code, down });
                session.run_for(0.25);
            }
        }
        if !how.clicks.is_empty() || !how.keys.is_empty() {
            session.run_for(1.0);
        }
        if let Some(client) = session.client_mut(index) {
            client.save_user_settings();
        }
        let server_log = session.server().take_log();
        let client = session.client(0).expect("joined");
        let client_log = client.take_log();
        if !how.quiet {
            for entry in &server_log {
                rook_engine::console::print_entry("Server", entry);
            }
            for entry in &client_log {
                rook_engine::console::print_entry("Client", entry);
            }
        }
        summary("Server", session.server(), &server_log, started);
        summary("Client", client, &client_log, started);
        if let Some(path) = how.dump {
            dump(client, path);
        }
        return Ok(());
    }
    server.run_for(how.seconds);
    let log = server.take_log();
    if !how.quiet {
        for entry in &log {
            rook_engine::console::print_entry("Server", entry);
        }
    }
    #[cfg(feature = "repl")]
    let log = if how.repl {
        [log, interactive(&mut server)].concat()
    } else {
        log
    };
    summary("Server", &server, &log, started);
    Ok(())
}

/// Print the tree at `path` (names from the DataModel down, by dots):
/// each instance's class and tags, and for GUI its absolute size and
/// whether it shows.
fn dump(dm: &DataModel, path: &str) {
    let world = dm.runtime().world();
    let dom = &world.dom;
    let mut at = Some(dom.root());
    for name in path.split('.') {
        at = at.and_then(|id| dom.find_first_child(id, name, false));
    }
    let Some(top) = at else {
        println!("\nno {path} on the client");
        return;
    };
    println!("\n{path}:");
    let mut stack = vec![(top, 0usize)];
    while let Some((id, depth)) = stack.pop() {
        let mut line = format!(
            "{}{} [{}]",
            "  ".repeat(depth),
            dom.name(id),
            dom.class_name(id)
        );
        if let Some(inst) = dom.get(id)
            && !inst.tags.is_empty()
        {
            line.push_str(&format!(" .{}", inst.tags.join(" .")));
        }
        if let Some(selector) = dom.string_property(id, "Selector") {
            line.push_str(&format!(" {selector:?}"));
        }
        if dom.is_a(id, "GuiBase2d") {
            if let Some(size) = dom.vector2_property(id, "AbsoluteSize") {
                line.push_str(&format!(" {}x{}", size.0.x, size.0.y));
            }
            if dom.bool_property(id, "Visible") == Some(false)
                || dom.bool_property(id, "Enabled") == Some(false)
            {
                line.push_str(" hidden");
            }
            if let Some(t) = dom.float_property(id, "BackgroundTransparency")
                && t < 1.0
            {
                let c = dom
                    .color3_property(id, "BackgroundColor3")
                    .unwrap_or_default();
                line.push_str(&format!(" background {t} ({}, {}, {})", c.r, c.g, c.b));
            }
            if let Some(at) = dom.vector2_property(id, "AbsolutePosition") {
                line.push_str(&format!(" at {}, {}", at.0.x, at.0.y));
            }
            if let Some(text) = dom.string_property(id, "Text")
                && !text.is_empty()
            {
                line.push_str(&format!(" text {text:?}"));
            }
            if let Some(image) = dom.string_property(id, "Image")
                && !image.is_empty()
            {
                line.push_str(&format!(" image {image}"));
            }
        }
        println!("{line}");
        for &c in dom.children(id).iter().rev() {
            stack.push((c, depth + 1));
        }
    }
}

/// What ran on one side, and the members it called that Rook lacks.
fn summary(side: &str, dm: &DataModel, log: &[LogEntry], started: Instant) {
    let errors = log.iter().filter(|e| e.kind == MessageType::Error).count();
    let world = dm.runtime().world();
    println!(
        "\n{side}: {} scripts started, {} log lines, {errors} errors, {:.1}s simulated in {:.2}s",
        dm.started_scripts(),
        log.len(),
        dm.now(),
        started.elapsed().as_secs_f64()
    );
    if !world.unimplemented.is_empty() {
        println!("Not implemented in Rook yet (calls):");
        let mut list: Vec<_> = world.unimplemented.iter().collect();
        list.sort_by(|a, b| b.1.cmp(a.1));
        for (member, n) in &list {
            println!("  {n:>6}  {member}");
        }
    }
    if !world.inert.is_empty() {
        println!("Accepted but ignored for now (calls):");
        for (member, n) in &world.inert {
            println!("  {n:>6}  {member}");
        }
    }
}

/// Run in real time at 60 Hz with the command prompt until input ends,
/// printing output as it comes. Returns the output.
#[cfg(feature = "repl")]
fn interactive(server: &mut Server) -> Vec<rook_lua::LogEntry> {
    let Some(mut repl) = rook_engine::console::repl::Repl::start() else {
        rook_engine::console::print_message(
            MessageType::Warning,
            "--repl needs an interactive terminal",
        );
        return Vec::new();
    };
    let frame = Duration::from_secs_f64(1.0 / 60.0);
    let mut next = Instant::now();
    let mut log = Vec::new();
    while !repl.closed() {
        repl.poll(false, |_, line| {
            if let Err(e) = rook_engine::console::repl::run_line(server, line) {
                rook_engine::console::print_message(MessageType::Error, &e);
            }
        });
        server.step();
        for entry in server.take_log() {
            rook_engine::console::print_entry("Server", &entry);
            log.push(entry);
        }
        next += frame;
        match next.checked_duration_since(Instant::now()) {
            Some(wait) => std::thread::sleep(wait),
            None => next = Instant::now(),
        }
    }
    log
}
