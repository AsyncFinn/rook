# Chat

[M8](../../roadmap/m8-gui.md) step 10: TextChatService, which Roblox's
chat window and bubbles and games' chat code run on. Checked by running
Roblox's own chat CoreScripts ([Roblox's core scripts](core-scripts.md))
on Studio 0.744's install on 2026-10-23 with a place that sends a message
and a system message; looks from `rook-player --core-scripts --screenshot`.

Code: setting up in `rook-engine`'s `chat.rs`, messages in `rook-lua`'s
`api/luau/text_chat.luau` and `api/methods/text_chat.rs`.

## Setting up

- A server's TextChatService holds `ChatWindowConfiguration`,
  `ChatInputBarConfiguration`, `BubbleChatConfiguration` and
  `ChannelTabsConfiguration` (Inference: new places save them, and
  Roblox's chat window reads them in places saved before they existed).
- `ChatVersion` is TextChatService (Inference: Roblox moved every
  experience to it in 2025; the chat CoreScripts check
  `Chat.IsAutoMigrated`, which reads true for them).
- With `CreateDefaultTextChannels` (Confirmed: creator-docs
  TextChatService), a `TextChannels` folder holds `RBXGeneral` and
  `RBXSystem`, and each joining player gets a TextSource in both, named
  after them, with their `UserId` and `CanSend`.
- Translating needs Roblox's servers, so each client's
  `ChatTranslationEnabled` is off and the chat window's translation tip
  counts as seen.
- `CanUserChatAsync` (on TextChatService and the legacy Chat) is true:
  everyone may chat in a local server.

## Messages

- `TextChannel:SendAsync(text, metadata)` on a client fires
  `TextChatService.SendingMessage` with a TextChatMessage whose `Status`
  is Sending and returns it; the message goes to the server only when the
  local player's TextSource in that channel `CanSend`.
- The server fires `Player.Chatted`, then delivers it to every client, or
  to those the channel's `ShouldDeliverCallback` doesn't refuse.
- On each client the message (Status Success, the server's `Timestamp`,
  `PrefixText` the speaker's display name and a colon) passes the
  channel's `OnIncomingMessage`, then the service's, whose returned
  TextChatMessageProperties replace its `PrefixText` and `Text`, and fires
  `MessageReceived` on the channel and the service.
- `DisplaySystemMessage` makes a message on that client alone, without a
  TextSource.
- Messages aren't filtered: a local server has no one to filter for.
- The speaker's name reaches Roblox's chat window only through
  `PrefixText` (Inference: the window draws it before the text, colored).

## Details

- TextSource's `UserId` is NotReplicated in the API dump but reaches
  clients ([replication](../replication.md)): Roblox's chat window finds
  the local player's TextSource by it.
- `tostring` of a DateTime gives its Unix milliseconds (Inference: Roblox's
  bubble chat orders bubbles by `tonumber(tostring(timestamp):sub(-9))`).
