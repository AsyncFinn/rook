use super::*;

#[test]
fn errors_are_plain_strings() {
    expect(
        r#"
        local ok, e = pcall(function() return workspace.Nope end)
        print(ok, type(e), e)
        ok, e = pcall(function() Instance.new("Part").Size = "big" end)
        print(e)
        ok, e = pcall(function() Instance.new("NotAClass") end)
        print(e)
        ok, e = pcall(function() workspace.Parent = nil end)
        print(e)
        "#,
        0,
        &[
            "false string Nope is not a valid member of Workspace \"Workspace\"",
            "Unable to assign property Size. Vector3 expected, got string",
            "Unable to create an Instance of type \"NotAClass\"",
            "The Parent property of Workspace is locked, current parent: Game, new parent NULL",
        ],
    );
}

#[test]
fn uncaught_errors_report_with_stack() {
    let out = run("local x = nil\nprint(x.y)", 0);
    assert_eq!(out.len(), 1);
    assert!(
        out[0].starts_with("ERR Workspace.Script:2: attempt to index nil with 'y'"),
        "{out:?}"
    );
    assert!(
        out[0].contains("Stack Begin\nScript 'Workspace.Script', Line 2"),
        "{out:?}"
    );
}

#[test]
fn engine_calls_pass_results_and_errors_through() {
    // No results, several results, a callback's results (the shim calls it
    // from Lua), instance arrays, and string errors from `inst.Key` both ways.
    expect(
        r##"
        local m = Instance.new("Model")
        local p = Instance.new("Part")
        p.Parent = m
        print(select("#", m:GetBoundingBox()), select("#", Instance.new("Folder"):Destroy()), select("#", warn()))
        local f = Instance.new("BindableFunction")
        f.OnInvoke = function(a, b) return b, a, nil end
        print(select("#", f:Invoke(1, 2)), f:Invoke(1, 2))
        local kids = m:GetChildren()
        print(#kids, kids[1] == p, m:GetChildren()[1] == kids[1])
        print(pcall(function() return m.Nope end))
        print(pcall(function() m.Nope = 1 end))
        print(pcall(function() return m[1] end))
        print(m["Part"] == p, rawequal(m.Destroy, p.Destroy))
        "##,
        0,
        &[
            "WARN ",
            "2 0 0",
            "3 2 1 nil",
            "1 true true",
            "false Nope is not a valid member of Model \"Model\"",
            "false Nope is not a valid member of Model \"Model\"",
            "false 1 is not a valid member of Model \"Model\"",
            "true true",
        ],
    );
}
