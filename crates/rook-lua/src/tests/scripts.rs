use super::*;

#[test]
fn require_modules() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    {
        let mut w = rt.world_mut();
        let ws = w.dom.get_service("Workspace").unwrap();
        let module = w.dom.create_by_name("ModuleScript").unwrap();
        w.dom.set_name(module, "Lib".into()).unwrap();
        let source = w.dom.property_id(module, "Source").unwrap();
        w.dom
            .set_property(
                module,
                source,
                rook_types::Value::String(
                    "print('loading', script.Name) task.wait() return { answer = 42 }".into(),
                ),
            )
            .unwrap();
        w.dom.set_parent(module, Some(ws)).unwrap();
        let bad = w.dom.create_by_name("ModuleScript").unwrap();
        w.dom.set_name(bad, "Bad".into()).unwrap();
        w.dom
            .set_property(bad, source, rook_types::Value::String("return 1, 2".into()))
            .unwrap();
        w.dom.set_parent(bad, Some(ws)).unwrap();
    }
    rt.run_source(
        "Workspace.Script",
        r#"
        local a = require(workspace.Lib)
        local b = require(workspace.Lib)
        print(a.answer, a == b)
        print(pcall(require, workspace.Bad))
        "#,
        None,
    )
    .unwrap();
    for _ in 0..5 {
        step(&rt);
    }
    let out: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(
        out,
        [
            "loading Lib",
            "42 true",
            "Module code did not return exactly one value",
            "false Module code did not return exactly one value",
        ]
    );
}

/// A script runs the bytecode kept for its source rather than compiling
/// it again, and one that doesn't compile still reports why under its
/// name.
#[test]
fn scripts_run_the_bytecode_kept_for_their_source() {
    let kept = "return 'compiled when it ran'";
    let other = crate::bytecode::compiler()
        .compile("return 'kept for it'")
        .unwrap();
    crate::bytecode::keep(kept, other);
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    {
        let mut w = rt.world_mut();
        let ws = w.dom.get_service("Workspace").unwrap();
        for (name, code) in [("Kept", kept), ("Broken", "return (")] {
            let module = w.dom.create_by_name("ModuleScript").unwrap();
            w.dom.set_name(module, name.into()).unwrap();
            let source = w.dom.property_id(module, "Source").unwrap();
            w.dom
                .set_property(module, source, rook_types::Value::String(code.into()))
                .unwrap();
            w.dom.set_parent(module, Some(ws)).unwrap();
        }
    }
    rt.run_source(
        "Workspace.Script",
        "print(require(workspace.Kept)) print(pcall(require, workspace.Broken))",
        None,
    )
    .unwrap();
    step(&rt);
    let out: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(out[0], "kept for it");
    assert!(out[1].starts_with("Workspace.Broken:1:"), "{out:?}");
    assert_eq!(
        out[2],
        "false Requested module experienced an error while loading"
    );
}

#[test]
fn runaway_scripts_time_out() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.set_script_timeout(Duration::from_millis(100));
    rt.run_source("Workspace.Script", "while true do end", None)
        .unwrap();
    let log = rt.take_log();
    assert_eq!(log.len(), 1);
    assert!(
        log[0]
            .message
            .contains("Script timeout: exhausted allowed execution time"),
        "{log:?}"
    );
}

#[test]
fn scripts_stop_at_the_heap_limit() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.set_heap_limit(64 << 20);
    rt.run_source(
        "Workspace.Script",
        r#"
        local ok, e = pcall(function()
            local t = {}
            while true do table.insert(t, {}) end
        end)
        print(ok, e)
        t = nil
        -- One allocation larger than any machine has.
        print(pcall(table.create, 2^31, 0))
        "#,
        None,
    )
    .unwrap();
    let log: Vec<_> = rt.take_log().into_iter().map(|l| l.message).collect();
    assert_eq!(log.len(), 2, "{log:?}");
    assert_eq!(log[0], "false not enough memory");
    assert!(log[1].starts_with("false "), "{log:?}");
}

/// Script environments are `safeenv`, so Luau runs builtins natively; what
/// scripts see must not change: `typeof` (Luau's builtin, which reads
/// datatypes' `__type`, the vector type's included, and ignores a proxy's),
/// globals a script assigns, and environments changed through `getfenv`.
#[test]
fn safe_environment_keeps_script_semantics() {
    expect(
        r#"
        print(typeof(Vector3.new()), typeof(game), typeof(Enum.Material.Plastic))
        local proxy = newproxy(true)
        getmetatable(proxy).__type = "Fake"
        print(typeof(proxy), typeof(CFrame.new()), typeof(nil), pcall(typeof))
        tostring = function() return "mine" end
        print(tostring(1))
        getfenv().type = function() return "custom" end
        print(type(1))
        "#,
        0,
        &[
            "Vector3 Instance EnumItem",
            "userdata CFrame nil false missing argument #1",
            "mine",
            "custom",
        ],
    );
}

/// Scripts compile at optimization level 2, as live games do, so small
/// local functions are inlined into their caller (here the main chunk,
/// whose name is empty) and `debug.info` sees it; `--!optimize 1` (Studio
/// testing's level) keeps the calls.
#[test]
fn scripts_compile_as_live_games_do() {
    let source = |header: &str| {
        format!(
            "{header}
local function inner() local n = debug.info(1, \"n\") return n end
             local function outer() local n = inner() return n end
local n = outer() print(n)"
        )
    };
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.run_source("Workspace.Live", &source(""), None).unwrap();
    rt.run_source("Workspace.Studio", &source("--!optimize 1"), None)
        .unwrap();
    let log: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(log, ["", "inner"]);
}

/// Every level of a script's stack has the script's environment, as in
/// Roblox: C functions, Rook's own Luau and level 0 included. Anti-tamper
/// scripts hang on purpose when one doesn't.
#[test]
fn getfenv_gives_the_script_environment_at_every_level() {
    expect(
        r#"
        local env = getfenv()
        xpcall(function() return game.missing end, function()
            local same = true
            for level = 0, 6 do
                same = same and getfenv(level) == env
            end
            print(same, getfenv(print) == env)
        end)
        print(pcall(getfenv, 50))
        print(pcall(getfenv, -1))
        "#,
        0,
        &[
            "true true",
            "false invalid argument #1 to 'getfenv' (invalid level)",
            "false invalid argument #1 to 'getfenv' (level must be non-negative)",
        ],
    );
}
