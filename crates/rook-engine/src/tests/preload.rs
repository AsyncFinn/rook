use super::*;

/// PreloadAsync on a client, as observed in Studio: instances stand for
/// their (and their descendants') content, strings load as images,
/// asset id strings fail, empty strings are skipped, an image property
/// holding a sound fails, and the call returns after the last callback.
#[test]
fn preload_async_on_a_client() {
    let mut client = DataModel::new_client(&content_options("preload")).unwrap();
    client
        .run_extra_script(
            r#"
            local CP = game:GetService("ContentProvider")
            local model = Instance.new("Model")
            Instance.new("Decal", model).Texture = "rbxasset://textures/face.png"
            local sound = Instance.new("Decal")
            sound.Texture = "rbxasset://sounds/hit.wav"
            local results = {}
            task.spawn(function()
                CP:PreloadAsync({"rbxasset://textures/face.png", "rbxasset://sounds/hit.wav",
                    "", "rbxassetid://5", "abc", model, sound}, function(id, status)
                    table.insert(results, id .. " " .. status.Name)
                end)
                table.sort(results)
                print(table.concat(results, ", "))
                print("returned", CP.RequestQueueSize)
            end)
            print("waiting", CP.RequestQueueSize)
            "#,
        )
        .unwrap();
    let out = step_until(&mut client, |out| {
        out.iter().any(|l| l.starts_with("returned"))
    });
    assert_eq!(
        out,
        [
            "waiting 6",
            "abc Failure, rbxasset://sounds/hit.wav Failure, \
             rbxasset://sounds/hit.wav Failure, \
             rbxasset://textures/face.png Success, rbxasset://textures/face.png Success, \
             rbxassetid://5 Failure",
            "returned 0",
        ]
    );
}

/// On a server PreloadAsync never returns once there is anything to
/// fetch, and returns at once when there isn't (observed).
#[test]
fn preload_async_on_a_server() {
    let mut s = Server::new(&content_options("preload-server")).unwrap();
    s.run_extra_script(
        r#"
        local CP = game:GetService("ContentProvider")
        task.spawn(function() CP:PreloadAsync({"abc"}) print("never") end)
        CP:PreloadAsync({"", Instance.new("Folder")})
        print("nothing to fetch")
        for _, bad in {"abc", 5} do
            print(pcall(CP.PreloadAsync, CP, bad))
        end
        print(pcall(CP.PreloadAsync, CP, {true}))
        print(pcall(CP.PreloadAsync, CP, {}, 5))
        "#,
    )
    .unwrap();
    s.run_for(0.5);
    assert_eq!(
        lines(&s),
        [
            "nothing to fetch",
            "false Unable to cast string to Array",
            "false Unable to cast double to Array",
            "false Unable to cast bool to ContentId",
            "false Unable to cast double to Function",
        ]
    );
}
