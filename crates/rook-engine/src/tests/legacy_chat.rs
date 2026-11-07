//! The legacy chat's install with no Roblox file to fill it in: what a place
//! saves in the Chat service goes where it runs.

use super::*;

/// A place with its own runner and chat script saved in Chat, whose first
/// server script waits on the runner as games do.
fn forked() -> Server {
    let server = Server::new(&Options::default()).unwrap();
    let chat = service(&server, "Chat");
    let runner = add_under(
        &server,
        chat,
        "Script",
        "ChatServiceRunner",
        "print('runner ran')",
    );
    add_under(
        &server,
        runner,
        "ModuleScript",
        "ChatService",
        "return 'service'",
    );
    add_under(
        &server,
        chat,
        "LocalScript",
        "ChatScript",
        "print('window ran')",
    );
    let modules = add_under(&server, chat, "Folder", "ClientChatModules", "");
    add_under(
        &server,
        modules,
        "ModuleScript",
        "ChatSettings",
        "return {}",
    );
    add_script(
        &server,
        "ServerScriptService",
        "Waiter",
        r#"
        local sss = game:GetService("ServerScriptService")
        print("at start", sss:FindFirstChild("ChatServiceRunner"))
        print(require(sss:WaitForChild("ChatServiceRunner"):WaitForChild("ChatService")))
        "#,
    );
    server
}

#[test]
fn a_saved_runner_is_copied_to_what_waits_for_it() {
    let mut server = forked();
    server.start();
    server.step();
    assert_eq!(lines(&server), ["at start nil", "service", "runner ran"],);
}

#[test]
fn clients_get_the_chat_service_and_the_saved_chat_script() {
    let mut server = forked();
    let sp = service(&server, "StarterPlayer");
    let scripts = add_under(
        &server,
        sp,
        "StarterPlayerScripts",
        "StarterPlayerScripts",
        "",
    );
    add_under(
        &server,
        scripts,
        "LocalScript",
        "Look",
        r#"print(game:GetService("Chat"):WaitForChild("ClientChatModules").ChatSettings.ClassName)"#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Player1", 1).unwrap();
    session.run_for(1.0);
    let mut log = lines(session.client(index).unwrap());
    log.sort();
    assert_eq!(log, ["ModuleScript", "window ran"]);
}

/// The two things Roblox's server checks before installing it.
#[test]
fn nothing_is_installed_unless_the_place_wants_it() {
    for setting in ["Chat", "TextChatService"] {
        let mut server = forked();
        {
            let mut world = server.runtime.world_mut();
            let dom = &mut world.dom;
            let id = dom.get_service(setting).unwrap();
            if setting == "Chat" {
                dom.set_property_by_name(id, "LoadDefaultChat", Value::Bool(false))
                    .unwrap();
            } else {
                let version = dom.database().enum_by_name("ChatVersion").unwrap();
                let item = version.items.iter().find(|i| i.0 == "TextChatService");
                let value = Value::Enum(item.unwrap().1);
                dom.set_property_by_name(id, "ChatVersion", value).unwrap();
            }
        }
        server.start();
        server.step();
        let sss = service(&server, "ServerScriptService");
        let world = server.runtime.world();
        let copied = world.dom.find_first_child(sss, "ChatServiceRunner", false);
        assert!(copied.is_none(), "{setting}");
    }
}
