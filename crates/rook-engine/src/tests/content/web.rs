//! Web-backed members and asset delivery, answered locally with no
//! endpoint and by the configured host with one (a server on this
//! machine standing in for it).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{Receiver, channel};

use rook_platform::endpoints::Endpoints;

use crate::tests::*;

/// A web server on a free local port answering each request with
/// `answer(its base URL, path)`; each request comes out of the receiver
/// as `"METHOD path body"`.
fn serve(answer: fn(&str, &str) -> Vec<u8>) -> (String, Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let (seen, requests) = channel();
    let at = base.clone();
    let accept = move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut length = 0;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header.trim().is_empty() {
                    break;
                }
                if let Some((name, value)) = header.split_once(':')
                    && name.eq_ignore_ascii_case("content-length")
                {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let mut parts = line.split_whitespace();
            let (method, path) = (parts.next().unwrap(), parts.next().unwrap());
            let reply = answer(&at, path);
            let _ = seen.send(format!(
                "{method} {path} {}",
                String::from_utf8_lossy(&body)
            ));
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                reply.len()
            );
            stream.write_all(head.as_bytes()).unwrap();
            stream.write_all(&reply).unwrap();
        }
    };
    rook_platform::get()
        .threads
        .spawn("test-web", Box::new(accept))
        .unwrap();
    (base, requests)
}

/// A server DataModel whose Players holds Ann (user 5).
fn with_ann(endpoints: Endpoints) -> Server {
    let s = Server::new(&Options {
        endpoints: Some(Arc::new(endpoints)),
        ..Options::default()
    })
    .unwrap();
    let players = service(&s, "Players");
    let mut w = s.runtime().world_mut();
    let ann = w.dom.create_by_name("Player").unwrap();
    w.dom.set_name(ann, "Ann".into()).unwrap();
    w.dom
        .set_property_by_name(ann, "UserId", Value::Int64(5))
        .unwrap();
    w.dom.set_parent(ann, Some(players)).unwrap();
    drop(w);
    s
}

const SCRIPT: &str = r#"
    local Players = game:GetService("Players")
    print(pcall(Players.GetUserIdFromNameAsync, Players, "ann"))
    print(pcall(Players.GetNameFromUserIdAsync, Players, 6))
    local result = game:GetService("TextService"):FilterStringAsync("hello there", 5)
    print(result:GetNonChatStringForBroadcastAsync(), result:GetChatForUserAsync(1))
    print(Players.Ann:GetRankInGroup(7), Players.Ann:GetRoleInGroup(7))
    print("done")
"#;

/// With no endpoints the session answers: its players are the users,
/// filtering changes nothing, nobody is in a group.
#[test]
fn members_answer_locally_without_endpoints() {
    let mut s = with_ann(Endpoints::offline());
    s.run_extra_script(SCRIPT).unwrap();
    let out = step_until(&mut s, |out| out.iter().any(|l| l == "done"));
    assert_eq!(
        out,
        [
            "true 5",
            "false Players:GetNameFromUserIdAsync() failed: Unknown user",
            "hello there hello there",
            "0 Guest",
            "done",
        ]
    );
}

/// With a base URL every domain's requests go to `<base>/<domain>` in
/// Roblox's shape, and the members return what it answers.
#[test]
fn members_ask_the_configured_host() {
    let (base, requests) = serve(|_, path| {
        let json = match path {
            "/users/v1/usernames/users" => r#"{"data":[{"id":42,"name":"Ann"}]}"#,
            "/users/v1/users/6" => r#"{"id":6,"name":"Bo"}"#,
            "/textfilter/v1/filter-text" => r#"{"chat":"hello there","broadcast":"*****"}"#,
            "/groups/v2/users/5/groups/roles" => {
                r#"{"data":[{"group":{"id":7},"role":{"name":"Admin","rank":255}}]}"#
            }
            _ => "{}",
        };
        json.as_bytes().to_vec()
    });
    let mut s = with_ann(Endpoints::new(Some(&base), []));
    s.run_extra_script(SCRIPT).unwrap();
    let out = step_until(&mut s, |out| out.iter().any(|l| l == "done"));
    assert_eq!(
        out,
        [
            "true 42",
            "true Bo",
            "***** hello there",
            "255 Admin",
            "done"
        ]
    );
    let seen: Vec<String> = requests.try_iter().collect();
    assert_eq!(
        seen,
        [
            r#"POST /users/v1/usernames/users {"excludeBannedUsers":true,"usernames":["ann"]}"#,
            "GET /users/v1/users/6 ",
            r#"POST /textfilter/v1/filter-text {"context":"PrivateChat","text":"hello there","userId":5}"#,
            "GET /groups/v2/users/5/groups/roles ",
            "GET /groups/v2/users/5/groups/roles ",
        ]
    );
}

/// CoreScripts' `RequestInternal` stays offline for a domain with no
/// endpoint and reaches the host for one with.
#[test]
fn request_internal_goes_only_where_configured() {
    let (base, requests) = serve(|_, _| br#"{"ok":1}"#.to_vec());
    let endpoints = Endpoints::new(Some("off"), [("apis", base.as_str())]);
    let mut s = with_ann(endpoints);
    let source = r#"
        local Http = game:GetService("HttpService")
        local left = 2
        for _, url in {"https://apis.roblox.com/x?y=1", "https://users.roblox.com/v1/users/1"} do
            Http:RequestInternal({ Url = url, Method = "GET" }):Start(function(ok, r)
                print(url, ok, r.StatusCode, r.Body)
                left -= 1
                if left == 0 then print("done") end
            end)
        end
    "#;
    // Only Roblox's own scripts, which come compiled, may call it.
    let script = add_under(&s, service(&s, "ServerScriptService"), "Script", "Core", "");
    let code = mlua::chunk::Compiler::new().compile(source).unwrap();
    let compiled = rook_lua::Compiled {
        code: code.into(),
        core: true,
    };
    s.runtime().world_mut().bytecode.insert(script, compiled);
    let mut out = step_until(&mut s, |out| out.iter().any(|l| l == "done"));
    out.sort();
    assert_eq!(
        out,
        [
            "done",
            r#"https://apis.roblox.com/x?y=1 true 200 {"ok":1}"#,
            "https://users.roblox.com/v1/users/1 false 0 ",
        ]
    );
    assert_eq!(requests.try_iter().collect::<Vec<_>>(), ["GET /x?y=1 "]);
}

/// Asset delivery asks the configured host where an asset is and
/// downloads it from where that host says.
#[test]
fn asset_delivery_goes_to_the_configured_host() {
    let (base, requests) = serve(|base, path| match path {
        "/cdn/7" => b"asset bytes".to_vec(),
        _ => format!(r#"{{"locations":[{{"assetFormat":"source","location":"{base}/cdn/7"}}]}}"#)
            .into_bytes(),
    });
    let mut config = rook_content::Config::empty();
    config.order = vec![rook_content::ProviderKind::Delivery];
    config.network = true;
    config.endpoints = Endpoints::new(None, [("assetdelivery", base.as_str())]);
    let resolver = rook_content::Resolver::new(config);
    let r = rook_content::ContentRef::Asset {
        id: 7,
        version: None,
    };
    assert_eq!(&*resolver.resolve(&r).unwrap().bytes, b"asset bytes");
    assert_eq!(
        requests.try_iter().collect::<Vec<_>>(),
        ["GET /v2/assetId/7 ", "GET /cdn/7 "]
    );
}

/// A resolver whose only provider is the network, with these endpoints.
fn network_resolver(endpoints: Endpoints) -> rook_content::Resolver {
    let mut config = rook_content::Config::empty();
    config.order = vec![rook_content::ProviderKind::Delivery];
    config.network = true;
    config.endpoints = endpoints;
    rook_content::Resolver::new(config)
}

/// The thumbnails host answering one batch request with an image on its
/// own "CDN".
fn thumbnail_host(base: &str, path: &str) -> Vec<u8> {
    match path {
        "/cdn/thumb" => b"png bytes".to_vec(),
        "/users/v1/usernames/users" => br#"{"data":[{"id":156,"name":"builderman"}]}"#.to_vec(),
        _ => format!(
            r#"{{"data":[{{"requestId":"0","state":"Completed","imageUrl":"{base}/cdn/thumb"}}]}}"#
        )
        .into_bytes(),
    }
}

/// An old asset thumbnail URL asks the thumbnails host for the asset at
/// the size it snaps to, then downloads the image it names, as Studio
/// draws it.
#[test]
fn old_thumbnail_urls_load_from_the_thumbnails_host() {
    let (base, requests) = serve(thumbnail_host);
    let resolver = network_resolver(Endpoints::new(Some(&base), []));
    let url = "http://www.roblox.com/Game/Tools/ThumbnailAsset.ashx?fmt=png&wd=110&ht=110&aid=7";
    let r = rook_content::ContentRef::parse(url).unwrap();
    let fetched = resolver.resolve(&r).unwrap();
    assert_eq!(&*fetched.bytes, b"png bytes");
    assert_eq!(fetched.source.kind, rook_content::SourceKind::Thumbnails);
    assert_eq!(
        requests.try_iter().collect::<Vec<_>>(),
        [
            r#"POST /thumbnails/v1/batch [{"format":"png","isCircular":false,"requestId":"0","size":"150x150","targetId":7,"type":"Asset"}]"#,
            "GET /cdn/thumb ",
        ]
    );
}

/// `rbxthumb://` goes to the host as it is, a circular filter and all; a
/// username is looked up on the `users` domain first.
#[test]
fn rbxthumb_and_usernames_load_from_their_hosts() {
    let (base, requests) = serve(thumbnail_host);
    let resolver = network_resolver(Endpoints::new(Some(&base), []));
    for url in [
        "rbxthumb://type=AvatarHeadShot&id=156&w=48&h=48&filters=circular",
        "http://www.roblox.com/Thumbs/Avatar.ashx?x=100&y=100&username=BuilderMan",
    ] {
        let r = rook_content::ContentRef::parse(url).unwrap();
        assert_eq!(&*resolver.resolve(&r).unwrap().bytes, b"png bytes", "{url}");
    }
    assert_eq!(
        requests.try_iter().collect::<Vec<_>>(),
        [
            r#"POST /thumbnails/v1/batch [{"format":"png","isCircular":true,"requestId":"0","size":"48x48","targetId":156,"type":"AvatarHeadShot"}]"#,
            "GET /cdn/thumb ",
            r#"POST /users/v1/usernames/users {"excludeBannedUsers":false,"usernames":["BuilderMan"]}"#,
            r#"POST /thumbnails/v1/batch [{"format":"png","isCircular":false,"requestId":"0","size":"100x100","targetId":156,"type":"Avatar"}]"#,
            "GET /cdn/thumb ",
        ]
    );
}

/// A thumbnail still rendering is asked for again until it's done; one
/// the service has nothing for fails without a download.
#[test]
fn pending_thumbnails_are_waited_for_and_errors_fail() {
    use std::sync::atomic::{AtomicU32, Ordering};
    static ASKED: AtomicU32 = AtomicU32::new(0);
    let (base, requests) = serve(|base, path| match path {
        "/cdn/thumb" => b"png bytes".to_vec(),
        _ if ASKED.fetch_add(1, Ordering::SeqCst) < 2 => {
            br#"{"data":[{"requestId":"0","state":"Pending","imageUrl":null}]}"#.to_vec()
        }
        _ => thumbnail_host(base, path),
    });
    let resolver = network_resolver(Endpoints::new(Some(&base), []));
    let r = rook_content::ContentRef::parse("rbxthumb://type=Asset&id=7&w=420&h=420").unwrap();
    assert_eq!(&*resolver.resolve(&r).unwrap().bytes, b"png bytes");
    let seen: Vec<String> = requests.try_iter().collect();
    assert_eq!(seen.len(), 4, "{seen:?}");

    let (base, requests) = serve(|_, _| {
        br#"{"data":[{"requestId":"0","state":"Error","errorMessage":"","imageUrl":"x"}]}"#.to_vec()
    });
    let resolver = network_resolver(Endpoints::new(Some(&base), []));
    let r = rook_content::ContentRef::parse("rbxthumb://type=Asset&id=1&w=150&h=150").unwrap();
    assert_eq!(
        resolver.resolve(&r).unwrap_err().kind,
        rook_content::FailureKind::NotFound
    );
    assert_eq!(requests.try_iter().count(), 1);
}

const THUMBNAIL_SCRIPT: &str = r#"
    local Players = game:GetService("Players")
    print(Players:GetUserThumbnailAsync(156, Enum.ThumbnailType.HeadShot, Enum.ThumbnailSize.Size420x420))
    print(Players:GetUserThumbnailAsync(156.7, "AvatarBust", 0))
    print(Players:GetUserThumbnailAsync(-1, Enum.ThumbnailType.AvatarThumbnail, Enum.ThumbnailSize.Size352x352))
    print(pcall(Players.GetUserThumbnailAsync, Players, 156, 1, 7))
    print(pcall(Players.GetUserThumbnailAsync, Players, 156, Enum.Material.Plastic, 0))
    print(pcall(Players.GetUserThumbnailAsync, Players, 156, Enum.ThumbnailType.HeadShot))
    print("done")
"#;

/// `GetUserThumbnailAsync` gives `rbxthumb://` and asks the thumbnails
/// host whether it's ready; with no host nothing is.
#[test]
fn user_thumbnails_are_ready_when_the_host_has_them() {
    let errors = [
        "false Unable to cast double to ThumbnailSize",
        "false Unable to cast Material to ThumbnailType",
        "false Argument 3 missing or nil",
        "done",
    ];
    let (base, requests) = serve(thumbnail_host);
    let mut s = with_ann(Endpoints::new(Some(&base), []));
    s.run_extra_script(THUMBNAIL_SCRIPT).unwrap();
    let out = step_until(&mut s, |out| out.iter().any(|l| l == "done"));
    let mut want = vec![
        "rbxthumb://type=AvatarHeadShot&id=156&w=420&h=420 true",
        "rbxthumb://type=AvatarBust&id=156&w=48&h=48 true",
        "rbxthumb://type=Avatar&id=-1&w=352&h=352 false",
    ];
    want.extend(errors);
    assert_eq!(out, want);
    assert_eq!(
        requests.try_iter().collect::<Vec<_>>(),
        [
            r#"POST /thumbnails/v1/batch [{"format":"png","isCircular":false,"requestId":"0","size":"420x420","targetId":156,"type":"AvatarHeadShot"}]"#,
            r#"POST /thumbnails/v1/batch [{"format":"png","isCircular":false,"requestId":"0","size":"48x48","targetId":156,"type":"AvatarBust"}]"#,
        ]
    );

    let mut s = with_ann(Endpoints::offline());
    s.run_extra_script(THUMBNAIL_SCRIPT).unwrap();
    let out = step_until(&mut s, |out| out.iter().any(|l| l == "done"));
    let mut want = vec![
        "rbxthumb://type=AvatarHeadShot&id=156&w=420&h=420 false",
        "rbxthumb://type=AvatarBust&id=156&w=48&h=48 false",
        "rbxthumb://type=Avatar&id=-1&w=352&h=352 false",
    ];
    want.extend(errors);
    assert_eq!(out, want);
}

/// A thumbnail stored today answers without asking; one stored two days
/// ago is asked for again and replaced.
#[test]
fn stored_thumbnails_are_asked_for_again_once_stale() {
    let (base, requests) = serve(thumbnail_host);
    let dir = std::env::temp_dir().join(format!("rook-thumbnail-stale-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let r = rook_content::ContentRef::parse("rbxthumb://type=GroupIcon&id=9&w=150&h=150").unwrap();
    let key = r.cache_key().unwrap();
    let cache = rook_content::resolver::cache::Cache::open(dir.join("cache")).unwrap();
    cache.put(&key, b"old icon", "https://cdn/old").unwrap();
    let resolver = || {
        let mut config = rook_content::Config::empty();
        config.order = vec![rook_content::ProviderKind::Delivery];
        config.network = true;
        config.cache_dir = Some(dir.join("cache"));
        config.endpoints = Endpoints::new(Some(&base), []);
        rook_content::Resolver::new(config)
    };
    assert_eq!(&*resolver().resolve(&r).unwrap().bytes, b"old icon");
    assert_eq!(requests.try_iter().count(), 0);

    let refs = dir.join("cache/refs").join(&key);
    let line = std::fs::read_to_string(&refs).unwrap();
    let mut parts: Vec<String> = line.trim_end().splitn(5, ' ').map(str::to_owned).collect();
    parts[3] = (parts[3].parse::<u64>().unwrap() - 2 * 86_400).to_string();
    std::fs::write(&refs, format!("{}\n", parts.join(" "))).unwrap();
    assert_eq!(&*resolver().resolve(&r).unwrap().bytes, b"png bytes");
    assert_eq!(requests.try_iter().count(), 2);
    assert_eq!(&*resolver().resolve(&r).unwrap().bytes, b"png bytes");
    assert_eq!(requests.try_iter().count(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}
