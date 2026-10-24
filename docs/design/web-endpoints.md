# Web endpoints

Every web request Rook makes goes to an endpoint chosen by one setting, so Rook can run offline, against a mirror, or against a server of your own that answers the same web API. This page lists each request, which Roblox web domain it belongs to, what Rook answers when that domain has no endpoint, and what a server needs to answer when it has one. How to set the endpoints is in [Tools](../guide/tools.md#web-endpoints).

## How a request finds its endpoint

Roblox's web API is split across domains named `<domain>.roblox.com` (`users`, `friends`, `assetdelivery`, ...). `rook_platform::endpoints::Endpoints` gives each domain a base URL, or none:

- A domain listed in `webDomains` goes to the URL given there, or nowhere for `"off"`.
- Otherwise `webBase` decides: `"off"` sends nothing anywhere; a domain such as `example.com` sends `users.roblox.com`'s requests to `https://users.example.com`; a URL such as `http://127.0.0.1:8080` sends them to `http://127.0.0.1:8080/users`, so one server can answer every domain.
- With neither set, `assetdelivery` goes to Roblox, as Rook always did, and so do `clientsettings` and `setup` (Roblox's client packages); every other domain answers locally.

A request's path and query are kept as they are: `https://users.roblox.com/v1/users/1` becomes `<users endpoint>/v1/users/1`. Scripts still see Roblox's URLs (`ContentProvider.BaseUrl` stays `https://www.roblox.com/`); Rook rewrites them as they are sent. `roblox.com` with no domain is the `www` domain. URLs that aren't on a Roblox web domain are never sent this way.

Requests go out from the engine on a thread each, and the script that asked waits for the answer. Headless runs, which must repeat exactly, wait for every answer within the frame.

## Rook's own requests

| What | Domain | Request | Without an endpoint |
|---|---|---|---|
| Asset ids (`rbxassetid://`, meshes, images, sounds, `InsertService:LoadAsset`, `require(assetId)`) | `assetdelivery` | `GET /v2/assetId/{id}` (or `/v2/assetId/{id}/version/{v}`), answering `{"locations": [{"assetFormat": "source", "location": "<url>"}]}`, then a `GET` of that location | Only the cache and local content answer |
| Roblox's live client version, for its packages | `clientsettings` | `GET /v2/client-version/WindowsStudio64`, reading `clientVersionUpload` | Packages already downloaded answer, then the install |
| Roblox's client packages (`rbxasset://` files, the CoreScripts model) | `setup` (Roblox's is `https://setup.rbxcdn.com`, not a `roblox.com` domain) | `GET /{version}-rbxPkgManifest.txt`, then `GET /{version}-{package}.zip` for each package a path needs ([client packages](../behavior/content/client-packages.md)) | Packages already downloaded answer, then the install |
| Assets refused anonymously, with an Open Cloud key | none | `GET https://apis.roblox.com/asset-delivery-api/v1/assetId/{id}` with the key in `x-api-key` | The key is a Roblox credential, so it goes only to Roblox, and only while `assetdelivery` goes to Roblox; a mirror's refusals stand |
| `rbxthumb://` thumbnails | `thumbnails` | Not fetched yet | Fail as unsupported |
| `HttpService:GetAsync`, `PostAsync`, `RequestAsync` (game scripts) | none | Refused, as with `HttpEnabled` off; these reach any site, not Roblox's web API | Refused |

What asset delivery answers and how Rook caches it is in [asset fetching](../behavior/content/asset-fetching.md).

## Scripting APIs Roblox answers from its servers

Members with an endpoint path send the request shown and return what Roblox would make of the answer; a non-2xx answer or no answer at all raises, as a failed web call does in Roblox. "Local answer" is what they return with no endpoint.

| Members | Domain | Request | Local answer |
|---|---|---|---|
| `Players:GetUserIdFromNameAsync(name)` | `users` | `POST /v1/usernames/users` `{"usernames": [name], "excludeBannedUsers": true}`, reading `data[1].id` | The session's player of that name (any case), else an error |
| `Players:GetNameFromUserIdAsync(id)` | `users` | `GET /v1/users/{id}`, reading `name` | The session's player with that id, else an error |
| `UserService:GetUserInfosByUserIdsAsync(ids)` | `users` | `POST /v1/users` `{"userIds": ids, "excludeBannedUsers": false}`, reading `data[]` (`id`, `name`, `displayName`, `hasVerifiedBadge`) | The session's players among `ids` |
| `Player:IsFriendsWith(id)` | `friends` | `GET /v1/users/{player}/friends/statuses?userIds={id}`, true when a status is `Friends` | `false` |
| `Player:GetRankInGroup`, `GetRoleInGroup`, `IsInGroup` | `groups` | `GET /v2/users/{player}/groups/roles`, reading the entry whose `group.id` matches (`role.rank`, `role.name`) | `0`, `"Guest"`, `false` |
| `BadgeService:UserHasBadgeAsync(user, badge)` | `badges` | `GET /v1/users/{user}/badges/awarded-dates?badgeIds={badge}`, true when `data` isn't empty | `false` |
| `MarketplaceService:UserOwnsGamePassAsync(user, pass)` | `inventory` | `GET /v1/users/{user}/items/GamePass/{pass}`, true when `data` isn't empty | `false` |
| `MarketplaceService:PlayerOwnsAsset(player, asset)` | `inventory` | `GET /v1/users/{player}/items/Asset/{asset}`, true when `data` isn't empty | `false` |
| `TextService:FilterStringAsync(text, user, context)`, its TextFilterResult's `GetChatForUserAsync`, `GetNonChatStringForUserAsync`, `GetNonChatStringForBroadcastAsync`; `Chat:FilterStringAsync`, `Chat:FilterStringForBroadcast` | `textfilter` | `POST /v1/filter-text` `{"text", "userId", "context"}` (`PrivateChat` or `PublicChat`), answering `{"chat": "...", "broadcast": "..."}`. Roblox filters through a service of its own with no public API, so this request is Rook's | The text unchanged: a local session has no one to filter for |
| `HttpService:RequestInternal(options)` (Roblox's CoreScripts) | the URL's | `options.Method` to `options.Url` rewritten, with its `Headers` and `Body`; the callback gets `(true, {StatusCode, Body, Headers})` | The callback gets `(false, {StatusCode = 0, HttpError = "ConnectFail"})`, which the CoreScripts treat as offline |
| `HttpRbxApiService:GetAsyncFullUrl`, `PostAsyncFullUrl`, `RequestAsync` (Roblox's CoreScripts) | the URL's | The URL rewritten, returning the body of a 2xx answer | Raise `HttpError: ConnectFail` |
| `HttpRbxApiService:GetAsync(path)`, `PostAsync(path, data)` | `api` | `/{path}` on the legacy `api` domain (`assets/{id}/versions`) | Raise `HttpError: ConnectFail` |

These answer locally only, with no endpoint path yet:

| Members | Roblox's domain | Local answer |
|---|---|---|
| `PolicyService:GetPolicyInfoForPlayerAsync` | Roblox's own policy service | No restrictions |
| DataStores (`DataStoreService`, `GlobalDataStore`, ordered stores) | Roblox's game servers | Kept in memory for the run |
| `LocalizationService:GetTranslatorForPlayerAsync` and cloud tables | `localizationtables` | The place's own LocalizationTables |
| TextChatService messages | `textfilter` | Delivered unfiltered |

These aren't implemented yet and raise "not implemented in Rook yet" (Roblox's CoreScripts get nil): `Players:GetUserThumbnailAsync` (`thumbnails`), `Players:GetFriendsAsync` (`friends`), `Players:GetCharacterAppearanceInfoAsync`, `GetHumanoidDescriptionFromUserId` and AvatarEditorService (`avatar`), GroupService (`groups`), `BadgeService:GetBadgeInfoAsync` and `AwardBadge` (`badges`), `MarketplaceService:GetProductInfo` (`economy`), AssetService (`games`, `develop`), MessagingService and MemoryStoreService (Roblox's game servers). Each gets a local answer and an endpoint path when it's implemented, listed here.

## What a server of your own needs

A server answering Rook serves the requests above for the domains it's configured for, in Roblox's JSON shapes; fields Rook doesn't read can be left out. To serve assets, it answers `assetdelivery`'s `GET /v2/assetId/{id}` with a location and serves the bytes there (any URL, its own included). Everything else is optional: a domain with no endpoint answers locally, so a server can start with `users` and `textfilter` and grow. `webBase` set to the server's URL sends every domain to `<URL>/<domain>/...`, so one process can route them by the first path segment.
