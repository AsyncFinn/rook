# Chat

Checked against Studio 0.744 on 2026-10-23.

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
- `ChatVersion` stays what the place saves. Unset in a file it reads
  LegacyChatService (Studio 0.746), so every old place is on the
  [legacy chat](#the-legacy-chat). `Chat.IsAutoMigrated` reads false
  (Inference: Roblox's servers migrate a live experience, which nothing
  does to a local one; its value at the identities Studio's tools run at
  couldn't be read).
- With `CreateDefaultTextChannels` (Confirmed: creator-docs
  TextChatService), a `TextChannels` folder holds `RBXGeneral` and
  `RBXSystem`, and each joining player gets a TextSource in both, named
  after them, with their `UserId` and `CanSend`.
- Translating needs Roblox's servers, so each client's
  `ChatTranslationEnabled` is off and the chat window's translation tip
  counts as seen.
- `CanUserChatAsync` (on TextChatService and the legacy Chat) is true:
  everyone may chat in a local server.

## The legacy chat

What a server does for a place on `ChatVersion` LegacyChatService with
`Chat.LoadDefaultChat` on, from Roblox's `ServerStarterScript`,
`ChatWindowInstaller` and `ChatServiceInstaller`, checked in Studio 0.746
on a legacy copy of a Baseplate. Code: `rook-engine`'s `chat/legacy.rs`.

- It happens after the place's first scripts start: they find no
  `ChatServiceRunner` in ServerScriptService, and one that waits for it
  gets it.
- What the Chat service saves is kept, a fork of Roblox's chat among them.
  What's missing comes from Roblox's `models/ServerCoreScripts.rbxm`, the
  same way as the [CoreScripts](core-scripts.md): only builds with
  developer overrides, with `coreScripts` on, load it.
- `ChatServiceRunner` is copied to ServerScriptService, `ChatScript` and
  `BubbleChat` to StarterPlayerScripts, each unless one of that name is
  there. A script waits for those copies, so a place's saved forks are
  copied with no Roblox file at all.
- A place with no fork needs that file: without it nothing makes a runner,
  and a script waiting for one waits forever, until Rook has its own
  ([M18 step 4](../../roadmap/m18-offline-content.md)).
- What Roblox's file made isn't saved with the place (`Archivable` off),
  which doesn't stop a player getting StarterPlayerScripts' copies.
- Clients see what the Chat service holds, though the API dump marks the
  service NotReplicated: the window waits for `ClientChatModules` there.
- `Players.ClassicChat` is true and `Players.BubbleChat` false, as Studio
  reads them for a place. The window starts only if one is on.
- The window tells the server what's said with `Players:Chat`, which fires
  `Player.Chatted`. Its messages travel by the runner's own remotes.

`Chat:Chat` (bubbles a script makes) isn't implemented yet.

## Messages

- `TextChannel:SendAsync(text, metadata)` on a client fires
  `TextChatService.SendingMessage` with a TextChatMessage whose `Status`
  is Sending and returns it; the message goes to the server only when the
  local player's TextSource in that channel `CanSend`. Before it returns,
  the message passes `OnIncomingMessage` as a delivered one does, so a
  game's styling (a colored name) is on it before the chat window shows
  it (Studio 0.747). The server's copy arrives later as a new
  TextChatMessage with the same `MessageId`; the returned one stays
  Sending.
- The server fires `Player.Chatted`, then delivers it to every client, or
  to those the channel's `ShouldDeliverCallback` doesn't refuse.
- On each client the message (Status Success, the server's `Timestamp`,
  `PrefixText` the speaker's display name and a colon) passes the
  service's `OnIncomingMessage`, then the channel's, whose returned
  TextChatMessageProperties replace its `PrefixText` and `Text` (the
  channel's sees what the service's returned, Studio 0.747), and fires
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
