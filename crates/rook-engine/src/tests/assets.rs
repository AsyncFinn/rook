//! Published models scripts load: `require(assetId)` and
//! `InsertService:LoadAsset`, fetched through the content loader.

use super::*;

/// A model file holding the given `(class, name, source)` instances.
fn model_file(items: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut dom = rook_dom::Dom::new();
    let roots: Vec<InstanceId> = items
        .iter()
        .map(|&(class, name, source)| {
            let id = dom.create_by_name(class).unwrap();
            dom.set_name(id, name.into()).unwrap();
            if !source.is_empty() {
                dom.set_property_by_name(id, "Source", Value::String(source.into()))
                    .unwrap();
            }
            id
        })
        .collect();
    let weak = rook_formats::from_dom(&dom, &roots);
    rook_formats::save(&weak, rook_formats::Format::Binary).unwrap()
}

/// Options whose asset cache holds asset 100 (a MainModule and a Part)
/// and asset 101 (a Part only), with asset delivery off.
fn asset_options(name: &str) -> Options {
    let dir = std::env::temp_dir()
        .join("rook-engine-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cache = rook_content::resolver::cache::Cache::open(&dir).unwrap();
    let module = r#"print("loading", script:GetFullName(), script.Parent.Parent) task.wait() return {name = "mod"}"#;
    let assets = [
        (
            100,
            model_file(&[
                ("ModuleScript", "MainModule", module),
                ("Part", "Extra", ""),
            ]),
        ),
        (101, model_file(&[("Part", "Brick", "")])),
    ];
    for (id, bytes) in assets {
        cache
            .put(&format!("asset-{id}"), &bytes, &format!("test://{id}"))
            .unwrap();
    }
    let mut config = rook_content::Config::empty();
    config.order = vec![rook_content::ProviderKind::Delivery];
    config.cache_dir = Some(dir);
    Options {
        content: Some(Arc::new(rook_content::Resolver::new(config))),
        ..Options::default()
    }
}

/// As observed in Studio: require loads an asset once and requires its
/// MainModule (from a parentless `required_asset_<id>` Model, waiting
/// for the download; a second require while the module's body yields
/// waits for it), remembers failures, and rounds fractional ids;
/// LoadAsset returns a fresh parentless Model each call; clients may
/// do neither.
#[test]
fn require_and_load_asset() {
    let mut s = Server::new(&asset_options("assets")).unwrap();
    s.run_extra_script(
        r#"
        task.spawn(function() print("together", require(100).name) end)
        local a = require(100)
        print("same", a == require(99.6))
        print(pcall(require, 101))
        print(pcall(require, 102))
        local IS = game:GetService("InsertService")
        local m = IS:LoadAsset("100")
        print(m.ClassName, m.Name, m.Parent, #m:GetChildren(), m.MainModule.ClassName)
        print("fresh", m ~= IS:LoadAsset(100))
        print(pcall(IS.LoadAsset, IS, 102))
        print("done")
        "#,
    )
    .unwrap();
    let out = step_until(&mut s, |out| out.iter().any(|l| l == "done"));
    assert_eq!(
        out,
        [
            "loading required_asset_100.MainModule nil",
            "together mod",
            "same true",
            "false Unable to find module for asset id 101.  Does the asset have a ModuleScript named \"MainModule\"?",
            "false Downloading asset failed for asset id 102.  Is the asset id correct and is the asset type \"Model\"?",
            "Model Model nil 2 ModuleScript",
            "fresh true",
            "false Request asset was not found",
            "done",
        ]
    );

    let mut client = DataModel::new_client(&Options::default()).unwrap();
    client
        .run_extra_script(
            r#"
            print(pcall(require, 100))
            print(pcall(game:GetService("InsertService").LoadAsset, game:GetService("InsertService"), 100))
            "#,
        )
        .unwrap();
    client.step();
    assert_eq!(
        lines(&client),
        [
            "false require(assetId) cannot be called from a client.  assetId = 100",
            "false InsertService cannot be used to load assets from the client",
        ]
    );
}

/// Scripts tell a server from a client by `NetworkServer` and
/// `NetworkClient` under the DataModel; each side has only its own.
#[test]
fn network_server_and_client() {
    let mut server = Server::new(&Options::default()).unwrap();
    let print =
        r#"print(game:FindFirstChild("NetworkServer"), game:FindFirstChild("NetworkClient"))"#;
    add_script(&server, "ServerScriptService", "Server", print);
    let sp = service(&server, "StarterPlayer");
    let sps = add_under(
        &server,
        sp,
        "StarterPlayerScripts",
        "StarterPlayerScripts",
        "",
    );
    add_under(&server, sps, "LocalScript", "Client", print);
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Player1", 1).unwrap();
    session.run_for(0.2);
    assert_eq!(lines(session.server()), ["NetworkServer nil"]);
    assert_eq!(lines(session.client(index).unwrap()), ["nil NetworkClient"]);
}
