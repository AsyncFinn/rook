use crate::tests::*;

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
    let other = crate::runtime::bytecode::compiler()
        .compile("return 'kept for it'")
        .unwrap();
    crate::runtime::bytecode::keep(kept, other);
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

/// Scripts that came compiled by a newer Luau than mlua knows about run,
/// as Roblox's own scripts are, and bytecode too new for the VM says which
/// version it is.
#[test]
fn compiled_scripts_of_every_version_the_vm_runs_load() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    let mut scripts = Vec::new();
    for version in [14, 15] {
        let code = crate::runtime::bytecode::compiler()
            .compile(format!("print('version {version} runs')"))
            .unwrap();
        let mut w = rt.world_mut();
        let script = w.dom.create_by_name("Script").unwrap();
        w.dom.set_name(script, format!("V{version}")).unwrap();
        let code = with_version(&code, version).into();
        w.bytecode.insert(script, Compiled { code, core: true });
        scripts.push(script);
    }
    for script in scripts {
        rt.run_script(script);
    }
    let out: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(out[0], "version 14 runs", "{out:?}");
    assert!(
        out[1].contains("bytecode version mismatch (expected [3..14], got 15)"),
        "{out:?}"
    );
}

/// `code`, one function compiled at Luau's default version 9, rewritten
/// as `version` (11 adds a feedback vector to each function, 12 its size
/// before it).
fn with_version(code: &[u8], version: u8) -> Vec<u8> {
    fn varint(code: &[u8], at: &mut usize) -> usize {
        let mut n = 0;
        let mut shift = 0;
        loop {
            let b = code[*at];
            *at += 1;
            n |= usize::from(b & 0x7f) << shift;
            shift += 7;
            if b & 0x80 == 0 {
                return n;
            }
        }
    }
    assert_eq!((code[0], code[1]), (9, 3), "version 9, types version 3");
    let mut at = 2;
    for _ in 0..varint(code, &mut at) {
        at += varint(code, &mut at);
    }
    // The userdata type names, to a 0.
    while code[at] != 0 {
        at += 1;
        varint(code, &mut at);
    }
    at += 1;
    assert_eq!(varint(code, &mut at), 1, "one function");
    // The function runs to the main function's index, 0, at the end.
    let mut function = code[at..code.len() - 1].to_vec();
    assert_eq!(function[4] & 1 << 3, 0, "the function isn't inlinable");
    function.push(0);
    assert!(function.len() < 0x80, "its size fits a byte");
    let mut out = vec![version, 3];
    out.extend_from_slice(&code[2..at]);
    out.push(function.len() as u8);
    out.extend(function);
    out.push(0);
    out
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

/// A ModuleScript named `name` under `parent`, running `source`.
fn module(
    rt: &Runtime,
    parent: rook_dom::InstanceId,
    name: &str,
    source: &str,
) -> rook_dom::InstanceId {
    let mut w = rt.world_mut();
    let module = w.dom.create_by_name("ModuleScript").unwrap();
    w.dom.set_name(module, name.into()).unwrap();
    let source_id = w.dom.property_id(module, "Source").unwrap();
    w.dom
        .set_property(module, source_id, rook_types::Value::String(source.into()))
        .unwrap();
    w.dom.set_parent(module, Some(parent)).unwrap();
    module
}

/// Require by string, as Luau's rules apply to instances: `@self/` from the
/// requiring script, `./` from its parent and `../` one parent further, with
/// Studio 0.748's errors.
#[test]
fn require_by_string_paths() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    let ws = rt.world_mut().dom.get_service("Workspace").unwrap();
    let lib = module(&rt, ws, "Lib", "return require('@self/Inner')");
    module(
        &rt,
        lib,
        "Inner",
        "return require('./Sibling') + require('../Top')",
    );
    module(&rt, lib, "Sibling", "return 1");
    module(&rt, ws, "Top", "return 10");
    let main = rt.world_mut().dom.create_by_name("Script").unwrap();
    rt.world_mut().dom.set_parent(main, Some(ws)).unwrap();
    rt.run_source(
        "Workspace.Script",
        r#"
        print(require("./Lib"), require("./Lib") == require(workspace.Lib))
        print(pcall(require, "./Missing"))
        print(pcall(require, "Lib"))
        print(pcall(require, "@foo/Lib"))
        print(pcall(require, "./Lib/../Top"), pcall(require, ".//Top"))
        print(pcall(require, "././Top"))
        print(pcall(require, "../../X"))
        print(pcall(require, "../X/../../X"))
        print(pcall(require, "./"))
        "#,
        Some(main),
    )
    .unwrap();
    assert_eq!(
        finish(&rt, 2),
        [
            "11 true",
            "false error requiring \"./Missing\": could not resolve child component \"Missing\"",
            "false error requiring \"Lib\": require path must start with a valid prefix: ./, ../, or @",
            "false error requiring \"@foo/Lib\": @foo is not a valid alias",
            "true true 10",
            "true 10",
            "false error requiring \"../../X\": could not get parent of component \"..\"",
            "false error requiring \"../X/../../X\": could not resolve child component \"X\"",
            "false Unable to require module from given path './'",
        ]
    );
}
