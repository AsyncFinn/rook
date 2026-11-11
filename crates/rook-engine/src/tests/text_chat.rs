//! TextChatService messages as a client sees them.

use super::*;

/// A game colors names in `OnIncomingMessage`. The sender's own copy is
/// colored before `SendAsync` returns, the chat window's first look at it,
/// with the service's callback before the channel's, as in Studio 0.747.
#[test]
fn the_senders_own_message_is_restyled_before_it_shows() {
    let server = Server::new(&Options::default()).unwrap();
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
        "Colors",
        r#"
        local chat = game:GetService("TextChatService")
        local general = chat:WaitForChild("TextChannels"):WaitForChild("RBXGeneral")
        while not general:FindFirstChildOfClass("TextSource") do task.wait() end
        chat.OnIncomingMessage = function(m)
            print("service", m.Status.Name, m.PrefixText)
            local p = Instance.new("TextChatMessageProperties")
            p.PrefixText = "<font color='#ff0000'>" .. m.PrefixText .. "</font>"
            return p
        end
        general.OnIncomingMessage = function(m)
            print("channel", m.Status.Name, m.PrefixText)
        end
        print("returned", general:SendAsync("hi").PrefixText)
        "#,
    );
    let mut server = server;
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Player1", 1).unwrap();
    session.run_for(1.0);
    let colored = "<font color='#ff0000'>Player1:</font>";
    assert_eq!(
        lines(session.client(index).unwrap()),
        [
            "service Sending Player1:".to_owned(),
            format!("channel Sending {colored}"),
            format!("returned {colored}"),
            "service Success Player1:".to_owned(),
            format!("channel Success {colored}"),
        ]
    );
}
