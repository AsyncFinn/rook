//! Web-backed members and asset delivery, answered locally with no
//! endpoint and by the configured host with one (a server on this
//! machine standing in for it).

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{Receiver, channel};

use rook_platform::endpoints::Endpoints;

use super::*;

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
    s.runtime().world_mut().bytecode.insert(script, code.into());
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
