# Changelog

Every release, newest first. How versions are numbered is in
[CONTRIBUTING](CONTRIBUTING.md#versioning).

## [Unreleased]

- While sounds play, a frame rereads only the sounds, SoundGroups and audio API objects that changed, and works out only how loud each is where it is now. Rook rebuilt everything it heard every frame, and twice with an AudioAnalyzer.
- Changing a Highlight or other adornment redraws only it, a change inside one world GUI redraws only that GUI, and name tags are worked out again only for characters that changed. Rook redrew every part of the adorned Model, every world GUI, and every name tag each frame.
- A client with no character of its own no longer runs physics steps for parts the server moves, and those standing still cost nothing. Rook stepped and wrote back all of them every frame.
- `typeof` names Faces, Axes and PhysicalProperties, and printing Faces or Axes lists their members, as in Roblox. Rook gave all three the same internal name. See [Faces, Axes and PhysicalProperties](docs/behavior/scripting/faces-and-axes.md).
- Reading a member a datatype doesn't have raises Roblox's error for that type. Rook returned nil for some and worded others differently. See [datatype errors](docs/behavior/scripting/datatype-errors.md#members-a-datatype-doesnt-have).
- A script stops the moment it, or an ancestor, is destroyed or moved out of where scripts run, or it's disabled, as in Roblox. Rook kept moved and destroyed scripts running, and stopped disabled ones only at the end of the frame. See [running scripts](docs/behavior/scripting/running-scripts.md).
- Objects no longer vanish when the camera turns to certain angles. Rook could stop drawing parts across a place after loading some meshes. See [file meshes](docs/behavior/meshes/file-meshes.md#file-layouts).
- Reading a member with a lowercase first letter, such as `mouse.x` or `label.textSize`, finds the member, as in Roblox, after any child of that name; writes still need the real name. Rook raised an error, and let `.name` be written. See [member lookup](docs/behavior/scripting/member-lookup.md).
- `BasePart:GetRenderCFrame`, `VRService:GetUserCFrame` and `GetUserCFrameEnabled` work, and `ContextActionService:BindActionToInputTypes` binds as `BindAction` does. Rook raised errors.
- `ReplicatedFirst:RemoveDefaultLoadingScreen` does nothing, as Rook shows no default loading screen. It raised an error.
- `RunService`, `ScriptContext` and `TeleportService` are named "Run Service", "Script Context" and "Teleport Service", as in Roblox. Rook named them after their class. See [service names](docs/behavior/property-defaults.md#service-names).
- Releases are built on GitHub's machines instead of a developer's, so the programs carry no one's local paths, and every pull request and push to main runs `scripts/check` there.
- `wait(math.huge)` and other waits too long for a float last one step, as in Roblox; Rook waited forever. See [waiting](docs/behavior/scripting/waiting.md).
- Your own character no longer blocks the mouse; clicks reach what's behind it, as in Roblox. See [the mouse](docs/behavior/physics/queries.md#the-mouse).
- Scripts a game enables while the place starts now run. Rook only started the ones after the enabling script in the tree.
- Ladders built from rungs on a solid board can be climbed, as in Roblox. Rook missed the gaps between thin rungs.
- Datatype constructors read an argument that isn't a number as 0, as Roblox does; Rook raised an error. See [datatype errors](docs/behavior/scripting/datatype-errors.md#constructors-dont-check-numbers).
- A FloorWire's images draw once each, turned about the wire to face the camera, as in today's Studio. Rook drew two crossed copies fixed in the world. See [adornments](docs/behavior/rendering/adornments.md#floorwire).
- Beam and Trail images run their rows along the effect and their columns across, as in Studio. Rook had them the other way round.
- A client can't move its character while the server has it anchored, as in Roblox. A move the client sent just before could undo the server's.
- A Decal or Texture no longer shows through a part lying just above it at a low angle. Rook pulled images towards the camera too far on sloped views.
- The player is split along what other apps need: `rook-view` is now `rook-scene`, a new `rook-client` crate holds the game a player sees and its window, `rook-render` owns drawing into a window, and a new `rook-server` runs a place's server with no window. Nothing plays or draws differently.
- `Content` works in scripts: `Content.fromUri`, `Content.fromAssetId`, `Content.none` and a value's `SourceType` and `Uri`, as in Studio. Rook had none of them. See [Content](docs/behavior/scripting/content.md).
- Places on the legacy chat get it, as in Studio. Rook moved every place to TextChatService. See [the legacy chat](docs/behavior/gui/chat.md#the-legacy-chat).
- `CollectionService:GetInstanceAddedSignal` and `GetInstanceRemovedSignal` work, firing as Studio's do. See [tag signals](docs/behavior/scripting/tags.md).
- A child named like an engine-only property, such as `Tags`, is found by indexing, as in Studio. Rook raised an error.
- Two reads of one signal compare equal, as in Studio. They didn't.
- `utf8.nfcnormalize` and `utf8.nfdnormalize` are added.
- Characters right themselves and fall over past 45 degrees, as in Roblox. Rook let tilts build up and stick.
- FallingDown, GettingUp and Ragdoll last as long as in Roblox. Rook's timings were off.
- Skies show up in places that change something every frame. Rook only sent a sky image to the renderer on a frame with no other changes.
- A Sky with an image that won't load draws Roblox's default sky, as Studio does; Rook drew a plain color. See [a sky that won't load](docs/behavior/lighting/sky.md#a-sky-that-wont-load).
- A GUI image that fails to load is reported as an image. Rook called it a sky image.
- `cargo xtask package` builds a clean folder to hand to someone. There was no such step.
- The command-line tool is `rook`, and `rook place.rbxl` or `rook play` opens the player. Commands that only work on the repository moved to `cargo xtask`.
- Any asset can be replaced with a local file in the overrides folder. Rook always used the cache or asset delivery. See [overrides](docs/design/data-and-logging.md#overrides).
- MeshParts and unions collide, are queried and weigh as the shapes Studio saved in them. Rook used their box. See [collision shapes](docs/behavior/physics/collision-shapes.md#meshparts).
- `Model:MoveTo` lifts a model out of what's in its way, as Studio does. Rook left it inside. See [queries](docs/behavior/physics/queries.md#modelmoveto).
- Joints take effect when Studio's do. Rook waited for the next physics step. See [joints](docs/behavior/physics/joints.md#when-joints-take-effect).
- Tables passed through bindables arrive as copies made the way Roblox makes them. Rook passed the same table. See [bindables](docs/behavior/scripting/bindables.md).
- `Random` gives the numbers Roblox gives for a seed. Rook's differed. See [Random](docs/behavior/scripting/random.md).
- A failing datatype method reaches `pcall` as text with Studio's wording. Rook passed an error object scripts couldn't read. See [datatype errors](docs/behavior/scripting/datatype-errors.md).
- `Mouse.Target` and `Mouse.Hit` reach as far as in Studio. Rook's reach was shorter. See [queries](docs/behavior/physics/queries.md#the-mouse).
- `DoubleSided` MeshParts draw their back faces as Studio does. Rook ignored the property.
- File meshes and the Head draw as SmoothPlastic whatever their part's material, as in Studio. Rook drew the part's material on them.
- Glass looks like Studio's at its default quality. Rook's Glass differed.
- Normal maps draw the right way out, color maps tint only where their alpha says, and detail maps and distance fading match Studio. Rook drew bumps sunk and covered mortar.
- Server-simulated parts far from anything your client simulates no longer cost physics work every step. Rook moved and checked all of them. See [replication](docs/behavior/replication.md#physics).
- Your character no longer jitters while you turn. The server sent its own slightly different copy of your pose back to you. See [who simulates a character](docs/behavior/characters/ownership.md).
- Assemblies go to sleep when Roblox's do and read zero velocity asleep. Rook used its physics library's own rule. See [sleeping](docs/behavior/physics/sleeping.md).
- NaN or infinite values in parts are handled as in Studio. Rook's server could crash on them. See [NaN and infinite values](docs/behavior/physics/nan-and-infinite-values.md).
- Many parts flying around at once no longer slow the light voxels. Rook did extra work for every moved part on one thread. See [local lights](docs/behavior/lighting/local-lights.md).
- Content arriving is spread over frames within a per-frame budget. Rook handled it all in the frame it landed, freezing that frame. See [per-frame budgets](docs/design/architecture.md#per-frame-budgets).
- The light around the camera catches up within a few frames of a big turn. It lagged seconds behind.
- `rook-player --bench` times content loading from when the scene is built. It timed from when the place started loading.
- Parts that barely move no longer mark the light voxels again every frame. Rook re-marked every moved part. See [local lights](docs/behavior/lighting/local-lights.md).
- Each frame goes to the GPU as one batch of commands. Moved parts went in batches of their own.
- SurfaceGuis and BillboardGuis out of view aren't drawn, and those among see-through parts break up the drawing only where they overlap. Rook drew them all and split more.
- SurfaceGuis and BillboardGuis share one buffer and are drawn in as few commands as possible. Each had its own.
- Frames can draw on a render thread while the next one steps, when that's faster. Rook drew and stepped one after the other. See [frame order](docs/design/architecture.md#frame-order) and [frame threads](docs/behavior/rendering/frame-threads.md).
- Particle emitters out of view don't step, as in Studio, and SurfaceGuis and server-moved parts are updated only when they move. Rook updated all of them every frame.

## [0.4.0] - 2026-11-04 - Frames that cost what changed; physics, GUI and terrain closer to Studio

- Indirect draw arguments go to the GPU straight from each pass's runs, and transparent parts reuse their sort lists. Both were rebuilt every frame.
- Each physics step writes HingeConstraints' `CurrentAngle` from a list of the hinges alone, in a fixed order. Rook looked through every constraint in hash map order.
- A font arriving lays out again only the text that was waiting for it. Rook laid out all the GUI.
- Changing a text object's text colors or transparencies shows at once and lays out only that object's text. Colors didn't show until something else laid the text out, and transparency tweens laid out all the GUI every frame.
- Changing a Model's own properties no longer redraws every part in it. Rook sent it to the renderer as a change to each part.
- Disconnecting a connection looks only at the frame's deferred queue. Rook looked through every waiting thread.
- A terrain edit gathers grass again only where it reached. Rook rebuilt every grass tile near the camera.
- When the renderer's instance buffer grows, the old contents are copied on the GPU and only changed slots go up. Rook sent every part again.
- A light turning on, coming, going or moving refills only the light voxels it lit before and after. Rook refilled all of them.
- `rook-player --bench` prints per-frame work counters and flags systems that touch far more than changed, and `scripts/event-soak.luau` benches small events. There were no such counters. See [frame times](docs/guide/getting-started.md#frame-times).
- A FloorWire's wire follows the floor under it as that floor changes. Rook found the floor only when the wire or its parts changed. See [adornments](docs/behavior/rendering/adornments.md#floorwire).
- Only parts drawn somewhere new redraw, the light voxels stay anchored to the world, and a moving part refills only the light it can reach. Rook redrew every awake part and refilled far more light. See [local lights](docs/behavior/lighting/local-lights.md#how-rook-does-it).
- A terrain edit sends and redraws only the chunks it touched, as Roblox sends edits. Rook wrote out, sent and compared the whole terrain. See [the Terrain API](docs/behavior/terrain/api.md) and [replication](docs/behavior/replication.md#terrain).
- `MakeJoints` and box and radius queries find parts through one index kept by physics, and now find turned cylinders and wedges near their box's corners. Rook built a box for every part on each call and could miss those. See [scene queries](docs/behavior/physics/queries.md).
- A terrain edit swaps in only the chunks it reached and averages far terrain again only under it. Rook rebuilt the renderer's whole scene. See [the Terrain API](docs/behavior/terrain/api.md).
- A moving part refills only the light voxel tiles its change can reach, every frame. Rook refilled all the light voxels after any move, causing hitches. See [local lights](docs/behavior/lighting/local-lights.md#how-rook-does-it).
- In shift lock and first person your character turns with the camera in the same frame. It trailed by a frame. See [frame order](docs/design/architecture.md#frame-order).
- Settings come in tiers: developer overrides are a Cargo feature on by default, `cargo dist` builds leave them out, and run-time settings come from a flag, a `ROOK_*` variable or the default. Release builds took only some settings, and there was a settings file. See [settings](docs/design/data-and-logging.md#settings) and [build profiles](CONTRIBUTING.md#build-profiles).
- Your Open Cloud key lives in the system's credential store, set with `rook-tools key set`, and is read only when Roblox's asset delivery refuses an asset. Rook read it from a variable only. See [the Open Cloud key](docs/design/data-and-logging.md#the-open-cloud-key).
- Parts whose instances changed go to the GPU on their own, and lights, emitters and sky go to the renderer only when they change. Rook sent whole chunks and sent those on any move.
- The server sends one pose per moving assembly, as Roblox does, and WeldConstraints keep their hidden `CFrame0`. Rook sent every part's CFrame. See [replication](docs/behavior/replication.md#physics).
- Parts the server moves and other players' characters play back smoothly a little behind the server, as in Roblox, and your character rides platforms the server moves. They jumped between updates and platforms slid out from under you. See [replication](docs/behavior/replication.md#physics).
- The audio graph's instances are kept in one list, unwatched property changes cost one lookup, and physics knows whether anything can move without visiting every body. Each of these did more work per frame.
- Loading a place moves values out of the decoded file instead of copying them, taking about a third less memory at its peak. Rook held the data twice.
- Your character feels the floor after every physics step, as Roblox's controller does, so it walks the same at any frame rate. It felt the floor once a frame.
- Moving parts' velocities no longer go to the change log, and its readers check classes and properties more cheaply. Every reader looked at and dropped those entries.
- Your client no longer solves joints between parts the server moves. It solved them every step for nothing. See [character ownership](docs/behavior/characters/ownership.md#in-rook).
- The cursor over a ClickDetector is Roblox's hand, drawn into the frame as Roblox draws its cursor. Rook showed the system's pointing hand. See [the cursor](docs/behavior/gui/input.md#the-cursor).
- Every frame shown at the display's rate steps the game at least one refresh. A frame after a slow one could step less, so walking stuttered. See [camera and controls](docs/behavior/characters/camera.md#in-rook).
- Thin far terrain lies where Studio draws it, grass grows on the ring that draws the ground, and far rings never skip a level. Far ground sat at the wrong height, grass floated or sank, and rings could close up passages. See [terrain level of detail](docs/behavior/terrain/detail.md).
- Grass and textured surfaces far from the world's center shade from positions relative to the camera. They crawled with static up close.
- Meshes of versions 6.00 and 7.00 load, and every mesh and union format is read by Rook's own decoder. Those meshes drew nothing. See [file layouts](docs/behavior/meshes/file-meshes.md#file-layouts).
- Scripts compile at Luau optimization level 2, as in live games. Rook used a lower level.
- Destroyed instances are freed once nothing holds them, as in Roblox. They stayed in memory for the whole session. See [collection](docs/design/architecture.md#structure).
- A focused TextBox that is destroyed, unparented or moved loses focus at once, as in Roblox. Rook kept typing into it. See [TextBox](docs/behavior/gui/input.md#textbox).
- The change log is cleared only once every reader has read it, so what scripts do after the server's step is kept. It could be lost.
- `typeof` is Luau's own builtin again, as in Roblox, and engine functions return through a cheaper path. Rook's own version was slower and differed at the edges.
- Zooming into first person while holding right-click centers the cursor at once, and dragging the window no longer stalls the game. The cursor stayed put and the game froze while dragging.
- The camera is drawn as the camera script leaves it in the same frame, so turning keeps up with the mouse. It trailed by a frame.
- Invoke callbacks and engine-side values belong to their instance. They vanished when no script held the object.
- Name reads, `GetChildren` and `ClassName` are cached for scripts. Each crossed into Rust every time. See [what a member read costs](docs/design/architecture.md#luau-bridge-rook-lua).
- Anchored parts are static colliders with no physics body, so moving one costs only itself. Moving one reread every anchored part.
- `--release` builds no longer use whole-program optimization; it moved to a new `dist` profile. Release rebuilds took minutes longer.
- Screen GUI hands each object to the renderer on its own, so only what changed is redrawn, and Hints and Messages are tracked as they're added. Rook redrew the whole screen and searched Workspace.
- Terrain water at shores, far lakes and blends between materials match Studio. Shores were stair steps, far lakes sank and material edges were sharpened. See [shorelines](docs/behavior/terrain/water.md#shorelines) and [blending](docs/behavior/terrain/blending.md).
- Old `rbxasset://` paths and built-in material textures load what Studio loads, worked out from Studio's behavior alone. Some came from elsewhere and differed. See [asset fetching](docs/behavior/content/asset-fetching.md) and [materials](docs/behavior/rendering/materials.md).
- Far terrain meets near terrain as smoothly as in Studio, and its rings are squares on the ground. Rings sat too high, with dark walls and shards. See [terrain level of detail](docs/behavior/terrain/detail.md).
- Terrain water reflects the sky and draws its underside as Studio's does. Rook's water reflected no sky. See [water](docs/behavior/terrain/water.md#what-studios-water-shaders-say).
- Water held in solid voxels at shores is drawn, as in Studio. Rook drew only Water voxels. See [shorelines](docs/behavior/terrain/water.md#shorelines).
- Terrain is colored by Studio's own rule, and grass takes the ground's color, shades and casts shadows as Studio's does. Rook tinted with fitted colors. See [terrain colors](docs/behavior/terrain/materials.md#colors) and [grass](docs/behavior/terrain/grass.md).
- MeshParts draw their mesh's lower levels of detail far from the camera, where Studio switches them. Rook drew only the full level. See [file meshes](docs/behavior/meshes/file-meshes.md#levels-of-detail).
- Physics always runs whole steps and what's drawn sits between the last two, as Roblox does. Steps followed the frame rate, so play depended on it. See [camera and controls](docs/behavior/characters/camera.md#in-rook).
- Mouse properties don't fire `Changed`, as in Roblox, and the mouse looks under the cursor only when something changed. It fired and looked every frame.
- Moving an anchored part re-reads only what's attached to it, and GUI tracks `Modal` buttons as they change. Both looked through everything.
- A ClickDetector's reach is measured from the character's root part whatever it's called. Rook needed a `HumanoidRootPart`.
- Far terrain is drawn coarser in rings, as in Studio, meshed off the main thread. Rook drew all terrain at full detail. See [terrain level of detail](docs/behavior/terrain/detail.md).
- World GUI among see-through parts switches only its own images. Rook bound every image again for each one. See [world GUI](docs/behavior/gui/world.md#drawing).
- The light voxels are kept by world position, refill only what the volume moved onto, and follow `Camera.Focus` as Studio's do. They were refilled whole as the camera turned. See [local lights](docs/behavior/lighting/local-lights.md#how-rook-does-it).
- ClickDetectors measure their reach from your character, as in Roblox. Rook measured from the camera.
- The default R15 character is the one Studio builds today, scaled as Studio scales it. Rook used an older, shorter rig. See [rigs and looks](docs/behavior/characters/rigs.md#r15).
- The client's render step runs after physics, as in Roblox. It ran before, so the camera trailed.
- In first person and shift lock your character faces where the camera looks, as in Roblox. It didn't.
- Lone anchored parts share one physics body. Each had its own.
- Mouse movement is reported only while the mouse is held or locked, as in Roblox, and locking the mouse no longer crashes. The camera turned on a free mouse.
- The client runs one frame per drawn frame, as Roblox does. It stepped at a fixed rate and repeated frames.
- You can play your own character with Roblox's PlayerModule, as in Studio (M9 step 4). There were no controls. See [camera and controls](docs/behavior/characters/camera.md).
- Characters climb over low trusses as in Studio. They flipped between climbing and running in front of them.
- Output is written on a thread of its own. Printing made the game wait for the console. See [output off the game thread](docs/design/data-and-logging.md#output-off-the-game-thread).
- Canceling a tween no longer errors. Rook misspelled `Cancelled`.
- Humanoids walk, jump, climb and swim through Roblox's states, with their events and methods (M9 step 3). A stand-in only held characters upright. See [movement and states](docs/behavior/characters/movement.md).
- With Roblox's CoreScripts, Escape opens the in-game menu, and ContextActionService works. Escape closed `rook-player`, and there was no ContextActionService.
- A property reset to its default takes its StyleSheet value again at once. It stayed until something else restyled it.
- Names and health bars show over Humanoids' heads as Studio draws them (M9 step 1). Rook drew none. See [names and health over heads](docs/behavior/characters/overhead.md).
- Text outlines are one soft layer, as in Studio. Rook drew eight overlapping copies.
- A Model with a `PrimaryPart` pivots on it, as in Studio. Rook used its stored pivot. See [pivots](docs/behavior/pivots.md).
- Characters face the way their SpawnLocation does, teams follow `TeamColor` as in Studio, and `Team:GetPlayers()` works. These differed or were missing. See [spawning and teams](docs/behavior/characters/spawning.md).
- `rook-player` release builds work from a terminal again and still open no console from Explorer. Release builds couldn't talk to a terminal. See [getting started](docs/guide/getting-started.md#opening-a-place-without-a-terminal).
- Binary files that saved `TextSize` as a whole number load, converted as Studio does. Rook rejected them.
- A transparent MeshPart or mesh is see-through where its texture is, as in Studio. Rook ignored the texture's alpha. See [mesh textures](docs/behavior/rendering/textures-and-decals.md).
- SurfaceGuis and BillboardGuis sort among see-through parts by distance. They drew over parts in front. See [world GUI](docs/behavior/gui/world.md).
- Sun shadows follow `Lighting.PrioritizeLightingQuality`, as in Studio, with soft voxel shadows when it's off. Rook always drew a shadow map.

## [0.3.0] - 2026-10-25 - M7 lights and effects, M8 GUI under way, first characters

### 2026-10-24 to 2026-10-25

- Rook's settings come in three kinds: build settings in `rook-build.toml`, the player's choices on `UserSettings().GameSettings`, and developer overrides. They were all in `settings.json`, which is gone. See [settings](docs/design/data-and-logging.md#settings).
- `rook-player` works without a terminal: drop a place on it to open it, its window shows once the first frame is drawn, and errors show in a message box. It needed a terminal and showed a blank window while loading. See [getting started](docs/guide/getting-started.md#opening-a-place-without-a-terminal).
- Release builds optimize the whole program, leave out the unused OpenGL backend and compress the embedded API data, so they're smaller. They were bigger.

Checked against Roblox 0.744.0.

- Players spawn as characters, with Roblox's rigs, spawn locations, teams and respawning, and each client simulates its own character (M9 steps 1 and 2). There were no characters. See [rigs and looks](docs/behavior/characters/rigs.md), [spawning and teams](docs/behavior/characters/spawning.md) and [who simulates a character](docs/behavior/characters/ownership.md).
- Rook tracks the live Roblox version and keeps its own API database, updated by `rook-tools gen-api`. It pinned one version and used rbx-dom's database. See [CONTRIBUTING](CONTRIBUTING.md#keeping-up-with-roblox).
- Rook downloads the client packages `rbxasset://` files come in from Roblox's CDN, as Roblox's bootstrapper does. It needed a Roblox or Studio install. See [client packages](docs/behavior/content/client-packages.md).
- The player's settings live on `UserSettings().GameSettings`, change from the in-game menu and are saved. They weren't. See [user settings](docs/behavior/user-settings.md).
- Only the nearest StyleLink styles what's under it, and styles reach objects scripts make while styling. Rook applied every link above an object, so styling changed from run to run.
- UITextSizeConstraint holds `TextScaled` text between its limits. Rook ignored it.
- With `--core-scripts`, Roblox's health bar and backpack work, with the Humanoid and Tool members behind them. They didn't.
- `webBase` and `webDomains` choose where each Roblox web domain's requests go, and web-backed members answer locally when there's no endpoint. Only asset delivery went to the web. See [web endpoints](docs/design/web-endpoints.md).
- Chat works through TextChatService's default channels, and Roblox's chat window shows it with `--core-scripts` (M8 step 10, under way). There was no chat.
- With `--core-scripts`, the in-game menu's Help and Report pages lay out as in Roblox. Frames that grow to their content had no height.
- StyleSheets style what scripts change during the frame. Those changes were dropped.
- `rook-player --screenshot` waits for GUI to settle after its images arrive. It shot too early.
- With `--core-scripts`, Roblox's logo button opens its in-game menu, and `--click` clicks before a screenshot or dump. Neither worked.
- Rounded and gradient GUI on the screen draws where it is. Rook placed it too high by the top bar's height.
- Roblox's top bar draws with `--core-scripts`, and `--dump` prints each GUI object's position, background and image. The top bar didn't draw.

### 2026-10-23: M8 steps 6 and 7, Roblox's CoreScripts, faster large places

Pinned Roblox version: 0.741.19.

- With `--core-scripts`, each client runs Roblox's modern CoreScripts from your Studio install, StyleSheets work, and game scripts gain the members those scripts need (M8 step 9, under way). Roblox's own GUI didn't run. See [the behavior page](docs/behavior/gui/core-scripts.md).
- A tweened GUI object lays out only what it moves, mesh and image uploads place only their own parts, `ClassName` is cached, and `getfenv` gives a script's own environment at every level, as in Roblox. Each of these did far more work, and `getfenv` differed.
- UICorner, UIStroke, UIGradient, flex and table layouts, UIPageLayout and CanvasGroups draw and lay out as in Studio (M8 step 7). Rook had none of them.
- A Frame's or button's `Style` draws Roblox's own textures and insets as in Studio (M8 step 6). Rook drew them as Custom.
- `rook-tools drift` no longer checks spelling. It flagged names taken from Roblox.

### 2026-10-22: M8 steps 3 to 5: world GUI, images and input

Pinned Roblox version: 0.741.19.

- The cursor is Roblox's own, from your install's images. Rook showed the system's cursor.
- Properties the reflection database has no default for read Studio's. They read false, 0 or empty. See [the behavior page](docs/behavior/property-defaults.md).
- GUI takes the mouse and keyboard before the world, with buttons, scrolling and TextBoxes as in Studio (M8 step 5). GUI took no input.
- `AbsolutePosition` leaves out the top bar and `IgnoreGuiInset` reaches clients, as in Studio. Rook counted the top bar.
- The dev profile and `scripts/check` share more of their builds, so building is faster. They rebuilt more.
- GUI a server gives a player before their first spawn stays, as in Studio. The first spawn destroyed it.
- GUI images and ScrollingFrames draw as in Studio (M8 step 4). Rook drew neither.
- Server scripts run in `safeenv` environments, as Roblox's do, and several instance operations do less work. They ran slower.
- Script calls that return strings, tables or userdata are faster, and the Luau heap limit is checked by the watchdog. Each value went through a protected call.
- Large screen text draws from signed distance fields, as in Roblox. It drew from glyph masks and blurred.
- SurfaceGuis and BillboardGuis draw in the world as in Studio (M8 step 3). Rook drew neither.
- Removing an instance fires its signals in Studio's order. Rook fired them after the instance had gone.
- Docs are organized by area and topic, one page per topic. Long pages held many topics.
- `rook-tools drift` checks the docs too. It checked only code.

### 2026-10-21: M8 steps 1 and 2: GUI layout and text

Pinned Roblox version: 0.741.19.

- GUI text draws and measures as in Studio (M8 step 2). Rook drew no text.
- GUI lays out and draws as in Studio, and the window's size and the top bar inset reach scripts as in Roblox, starting M8 GUI. Rook drew no GUI.
- Rook has an icon. It had none.
- `rook-platform` is the only way crates reach the host, and scene building moved to a new `rook-view` crate, so the server links no rendering code. Nothing draws or behaves differently.

### 2026-10-20: M7 Lights and effects

Pinned Roblox version: 0.741.19. Finishes M7 Lights and effects: local lights light the scene; Fire, Smoke, Sparkles, ParticleEmitters, Beams, Trails and Explosions draw as in Studio, through Studio's tone curve; and Highlights and the adornments draw as in Studio.

- Highlights draw as in Studio. Rook drew none.
- Line, Image and Wireframe HandleAdornments and SelectionPartLassos draw as in Studio (M7 step 6). Rook drew none.
- FloorWire textures draw as in Studio, and the wire keeps off the floor and its parts. Rook drew no textures.
- Parts reflect the sky through Studio's own blur and BRDF, and Reflectance works only where Studio's does. Rook reflected the sky's average color.
- Handles, ArcHandles and FloorWires draw as in Studio. Rook drew none.
- SelectionSpheres, SurfaceSelections and the Box, Sphere, Cylinder and Cone HandleAdornments draw as in Studio. Rook drew none.
- SelectionBoxes draw as in Studio. Rook drew none.
- Explosions hit, break joints, push parts, carve terrain and draw as in Studio. They did nothing.
- A server has a `NetworkServer` and a client a `NetworkClient`, as in a Studio playtest, and a second `require` of a module still running waits for it. Neither existed, and the `require` raised an error.
- Terrain water reflects the skybox and is lit and see-through as in Studio. It reflected a gradient and showed too much through it.
- Moved parts, lights, transparent parts, culling and particles do less work each frame, and the player uses mimalloc. Each did more, and the system allocator slowed big places. `--bench` prints more of the frame and `--turn` turns the camera.
- ParticleEmitters, Beams and Trails draw as in Studio. Rook drew none.
- The sun, its highlight and fog are drawn in Studio's light units with Studio's shaders. Sunlit faces were too bright.
- Lighting adds up in Studio's light units and goes through Studio's tone curve once, at the end of the frame. Rook blended after the curve.
- The player's window opens at once and shows a loading screen, as Roblox's does. It opened only once the place had loaded.
- Material textures are kept at full size, as Studio draws them, and stay compressed on the GPU. Rook halved them.
- Each part's material textures start at a spot of their own, as in Studio. Identical parts showed identical patterns.
- A place with duplicate `UniqueId`s prints Studio's error. It printed nothing.
- `CFrame.XVector`, `YVector` and `ZVector` are the rotation's columns, as in Studio. They were its rows.
- A model without a PrimaryPart has its bounding box turned to its pivot, as in Studio. It used the world axes.
- A Decal on a Cylinder's Bottom face shows the right way up, as in Studio. It was turned.
- Material sizes come from Studio's own OBJ export. They were measured from the screen.
- Material textures on wedge slopes repeat as in Studio. They used the slope's true length.
- Parts go into GPU batches in spatial order, so culling skips more. They went in scene order.
- Each frame visits only playing sounds, moved parts mark only their own light voxels, transparent parts are culled before sorting, and rays stop at the nearest part. Each did far more work.
- Mouse movement and the wheel pass one InputObject to every `InputChanged`, as in Studio. Rook made a new one per event.
- The command-line tool is `rook-tools`. It was `rook`.
- Rook prints a summary of what didn't load, logs every run and shows Studio's asset errors. Failed meshes, unions and images were dropped without a word.
- A data folder holds settings, logs, crash reports, the REPL history and kept content. There was none. See [data and logging](docs/design/data-and-logging.md).
- `rook-tools logs` prints the newest log's path and `rook-tools content` takes a place file. Neither did.
- Materials break up their repeats as Studio does, plainly, Organic or by Wang tiles, and terrain turns and shifts its layers. Big baseplates showed stripes. See [materials](docs/behavior/rendering/materials.md) and [terrain](docs/behavior/terrain/materials.md).
- Grassy terrain no longer shows white specks. Nearly empty voxels drew as Air.
- `require(assetId)` and `InsertService:LoadAsset` load published models, as in Studio. Both raised an error. See [content](docs/behavior/content/asset-fetching.md).
- Fire, Smoke and Sparkles draw as in Studio (M7 step 2). Rook drew none. See [effects](docs/behavior/effects/fire-smoke-and-sparkles.md).
- A playing sound keeps the device's clock, as a Roblox client does. It jumped back to the engine's when frames were slow. See [audio](docs/behavior/audio/playback.md).
- Players get a character on joining and on `Player:LoadCharacter()`, built in Roblox's order (M9 step 1 begun). They had none.
- A reference set on the server before its target reaches clients arrives once the target does. It stayed nil.
- Model moves and `CFrame:Lerp` are rigid. A slightly skewed pivot scaled the model further each move.
- Local lights with `Shadows` are blocked by parts, as in Studio's voxel lighting. They shone through everything. See [lights](docs/behavior/lighting/local-lights.md#shadows).
- A Script disabled and enabled again runs again from the top, and disabling one stops it, as in Roblox. It never ran again.
- `Tween:Pause()` during a tween's `DelayTime` does nothing, as in Studio. Rook paused it.
- rook-engine builds without its `repl` feature. It didn't.
- Mesh files and images near the camera load first. They loaded in file order.
- Roofs and walls block the sky's light, as in Studio. It lit roofed rooms like the open sky. See [materials](docs/behavior/rendering/materials.md#the-skys-diffuse-light).
- `Lighting.EnvironmentDiffuseScale` lights surfaces with the sky, as in Studio. Rook ignored it. See [materials](docs/behavior/rendering/materials.md#the-skys-diffuse-light).
- See-through Glass filters what's behind it as Studio does, once, where it's nearest. It darkened it nearly to black. See [materials](docs/behavior/rendering/materials.md#neon-glass-and-forcefield).
- Surfaces are lit by the brighter of Ambient and OutdoorAmbient, as in Studio. Rook added them. See [lights](docs/behavior/lighting/ambient.md).
- The light voxels reach as far as Studio's, placed ahead of the camera. They stopped short. See [lights](docs/behavior/lighting/local-lights.md#how-far-the-grid-reaches).
- Terrain blends where materials meet, as Studio's terrain shader does. It switched along the voxels' steps.
- PointLight, SpotLight and SurfaceLight light the scene through voxel lighting, as in Studio (M7 step 1). Rook had no local lights. See [lights](docs/behavior/lighting/local-lights.md).
- rook-render's shaders are WESL modules checked at build time. They were plain WGSL. Drawing is unchanged.
- Rook needs Rust 1.97.1 or later. It needed an older one.
- Constraints are stiff but not rigid, as in Studio, so parts saved overlapping an anchored part rest. They thrashed.
- Terrain in legacy-material places draws the legacy textures, as Studio does. It always drew the 2022 set.
- Terrain takes `Terrain.MaterialColors` as Studio does. Rook multiplied textures by the color.
- Mono sounds play 3 dB down on each side and the mix isn't clipped, as in Studio. They were louder and crackled.
- `rook-player --record-audio` mixes a place to a WAV file. There was no way to.

## [0.2.0] - 2026-10-16 - M3 to M6: physics, content, audio and terrain

### 2026-10-16: M6 steps 3 and 4: terrain collision and the Terrain API; M6 Terrain done

Pinned Roblox version: 0.741.19. Finishes M6 Terrain: archived places with terrain show it, parts rest on it and queries find it, and scripts build and dig it as Studio does; the conformance case `terrain/terrain_api` passes.

- The Terrain API sets voxels as Studio does, with the legacy cell methods and `Region3int16` (M6 step 4). Scripts couldn't edit terrain.
- `workspace.Terrain` is never nil, as in Roblox. A place saved without one had none.
- Terrain collides and queries find it, as in Studio (M6 step 3). Parts and rays went through it.
- One place decides how many background threads Rook uses, and `ROOK_THREADS` overrides it. Each system picked its own.
- Content loading splits fetching from decoding, with decoding on one shared pool at lower priority. Each loader did both on its own threads.
- `GetModelCFrame` is the bounding box's CFrame, and bounding boxes turn to the primary part, as in Studio. Rook used the primary part's CFrame and the world axes.
- `Instance.new` names instances as Studio does. Rook named them after their class.
- Clicks reach a ClickDetector through fully transparent parts that don't collide, as in Studio. Those parts took the click.
- `rook-player` shows the pointing-hand cursor over anything with a ClickDetector, as Roblox does. It didn't.
- `rook-player --bench` prints when content finished loading and the frame times until then. It didn't.

### 2026-10-16: Fixes: draw calls, Windows start-up, deprecated properties

Pinned Roblox version: 0.741.19.

#### Fix: deprecated lowercase properties read stale values

- A deprecated lowercase property reads and writes its capitalized twin, as in Roblox. Rook kept each at its default.
- `game.lighting` gives Lighting. It was nil.

#### Fix: `className` is nil

- `className` gives the class name. It was nil.

#### Fix: the player crashes on OpenGL

- The player runs on devices that can't view a texture in a second format, such as OpenGL. It stopped at once there.
- `WGPU_BACKEND` picks the graphics backend. There was no way to.

#### Fix: the player won't start without the Visual C++ runtime

- Windows builds include the C and C++ runtime, so `rook-player.exe` starts without the Visual C++ Redistributable. It failed to start.

#### Fix: drawing off-screen parts that share a batch with visible ones

- The renderer culls within batches, not just whole batches. Off-screen parts in a visible batch were drawn. Frames look the same.

#### Fix: too many draw calls on big places

- The renderer draws a whole pass in a few multi-draws from shared buffers and one image table. It made one draw call per mesh and image. Frames look the same.
- `--bench` prints draw calls, triangles and CPU and GPU times per pass. It didn't.

### 2026-10-15: Fixes: speed on big places

Pinned Roblox version: 0.741.19.

#### Fix: slow rendering on big places

- The renderer skips batches the camera or the sun can't see. It drew all of them. Frames look the same.
- `--bench` prints the draw calls per frame. It didn't.

#### Fix: `game.workspace` is nil

- `game.workspace` gives Workspace. It was nil.

#### Fix: the player rebuilds the whole scene on its first frame

- The player keeps the scene it built at load on its first frame, and `--bench` reuses it. Both rebuilt it.

#### Fix: a frame stalls when many sounds finish loading

- A sound's file is freed on the content worker that decoded it. It was freed on the engine thread, stalling it.

#### Fix: slow server steps on big places, part 2

- Physics runs Rook's friction and elasticity mixing only on parts that need it. It ran on every part, so contacts were never reused.

#### Fix: slow server steps on big places

- Physics no longer runs continuous collision detection, as Roblox doesn't. Rook ran it on every fast part.

### 2026-10-15: M6 step 2: terrain materials, water, grass and waves

Pinned Roblox version: 0.741.19. M6 Terrain, step 2, measured again.

#### M6 step 2: terrain's look checked against Studio

- Water plays the install's normal frames on Studio's phase and reflects the sky's color, as in Studio. Its ripples scrolled.
- Grass blades are single triangles of one color, swaying and turning into the wind as in Studio. They were spikes that differed.
- Scripts changing terrain water properties or `Workspace.GlobalWind` take effect at once. They were read only when the terrain was built.
- Non-default `MaterialColors` still tint terrain differently from Studio. See [terrain materials](docs/behavior/terrain/materials.md).

#### Fix: moved models balloon and vanish

- Parts' CFrames are orthonormalized whenever a script sets them, as Roblox does. Models moved over and over grew and vanished.
- Gains that aren't finite mix as silence. They could silence all audio.

#### M6 step 2: grass blades and water waves

- Grass grows blades with `Terrain.Decoration` on, as in Studio. There was no grass.
- Water surfaces rise and fall with `WaterWaveSize` and `WaterWaveSpeed`, rippled and glinting, as in Studio. Water was flat. See [grass](docs/behavior/terrain/grass.md).

#### M6 step 2: terrain materials and water

- Terrain is textured from Studio's own terrain material table. It was flat colors.
- `Terrain.MaterialColors` tints the textures through their tint masks. It only colored the surface.
- A MaterialVariant that MaterialService names for a terrain material replaces its textures. It didn't.
- Terrain's shapes come from the same table, with rough and faceted materials. They were all smooth.
- Water sits half a voxel lower and reflects some of the sky, as Studio's does. It sat higher and reflected nothing. See [terrain materials](docs/behavior/terrain/materials.md) and [stubs](docs/guide/stubs.md).
- Cobblestone quantizes as Studio's does. It was a guess.

### 2026-10-15: M6 step 1: terrain voxels and their surface

Pinned Roblox version: 0.741.19. M6 Terrain, step 1.

- Smooth terrain is decoded and meshed as Roblox does, each material shaped and colored as in Studio. Rook drew no terrain.
- Legacy terrain shows nothing, as in today's Studio.
- `SmoothGrid` replicates to clients. They had no terrain.
- `rook terrain <file>` shows what a file's Terrain holds. There was no such command. See [terrain voxels](docs/behavior/terrain/voxels.md).

### 2026-10-15: M5 step 5: the audio API; M5 Audio done

Pinned Roblox version: 0.741.19. Finishes M5 step 5 and with it M5 Audio: archived places play their sounds, positioned in 3D, and the audio conformance cases pass.

#### M5 step 5: the audio API; M5 Audio done

- AudioPlayers play through the same engine and cache as Sounds, as Studio's do. Rook had no audio API.
- Wires connect by Studio's rules. They didn't exist.
- AudioEmitters are heard by AudioListeners with Studio's falloff and panning, and a client gets Studio's default listener. They didn't exist.
- AudioFader, AudioEqualizer, AudioDistortion, AudioPitchShifter, AudioReverb, AudioChorus, AudioEcho and AudioCompressor are heard as in Studio. They weren't.
- AudioAnalyzer measures levels and spectrum on clients, as Studio's does. It didn't. See [the audio API](docs/behavior/audio/audio-api.md) and [stubs](docs/guide/stubs.md).
- The `RobloxSecurity` capability is called `RobloxEngine`, as Studio's errors name it. It had Rook's own name.
- The conformance case `content/audio_api` passes. There was none.

#### AmbientReverb as loud as Studio's

- `SoundService.AmbientReverb` is as loud as Studio's. It was far too loud. See [SoundGroups and effects](docs/behavior/audio/soundgroups-and-effects.md).

### 2026-10-15: M5 step 4: SoundGroups and sound effects

Pinned Roblox version: 0.741.19. Finishes M5 step 4; M5 Audio continues with the modern audio API.

#### M5 step 4: SoundGroups and sound effects

- A Sound is heard through its SoundGroup and the groups above it, each with its own effects and `Volume`. Rook had no SoundGroups.
- Equalizer, Reverb, PitchShift and Distortion sound effects are heard in Studio's `Priority` order. They weren't.
- `PlaybackLoudness` on clients is read after the sound's own effects, as in Studio. It was read before.
- `SoundService.AmbientReverb` and `SoundService:PlayLocalSound` work as in Studio. They didn't.
- `rook-audio` builds without its output device. It needed one.
- A new SoundService's `RespectFilteringEnabled` reads true, and setting `Sound.SoundGroup` to something else errors, as in Studio. It read false and accepted anything. See [SoundGroups and effects](docs/behavior/audio/soundgroups-and-effects.md).

#### Material textures from Roblox

- Material textures load from Roblox's TexturePack assets through asset delivery, with no Studio needed. Rook read them from Studio's local cache and drew stand-ins for the rest. See [materials](docs/behavior/rendering/materials.md).
- Reading Studio's content cache and Rook's own drawn material textures are removed. Offline with nothing cached, materials draw untextured.

#### Unions without saved hulls are boxes

- A union with no saved collision hulls collides as its box, as in Studio. Rook fetched its drawn geometry for collision. See [collision shapes](docs/behavior/physics/collision-shapes.md).
- `Options::wait_for_content` is `wait_for_sounds` again. Union geometry no longer loads within the frame.

### 2026-10-14: M15 step 1: unions collide as their shapes

Pinned Roblox version: 0.741.19. Starts M15 step 1; MeshParts' collision shapes follow.

#### M15 step 1: unions collide as their shapes

- Unions collide and are queried as their saved hulls or drawn geometry. They were boxes. See [collision shapes](docs/behavior/physics/collision-shapes.md).
- `rook content` describes a union asset's collision geometry. It didn't.
- `Options::wait_for_sounds` is `wait_for_content`, and union geometry loads within the frame. Only sounds did.

#### Properties Roblox replicates under hidden twins

- Sound's `Playing`, `TimePosition` and `PlaybackSpeed` reach clients through their hidden twins, as in Roblox. Clients never played sounds, or played them at the wrong pitch.
- Every other property Roblox sends under a hidden twin reaches clients too, and a test pins the list. They never did.

### 2026-10-14: M5 step 3: 3D sound

Pinned Roblox version: 0.741.19. Finishes M5 step 3; M5 Audio continues with SoundService, SoundGroups and effects.

- Sounds under a part or Attachment are heard from there, fading and panned as in Studio. Every sound was global. See [3D sound](docs/behavior/audio/3d-sound.md).
- `SoundService:SetListener` and `GetListener` work. They didn't.
- `MaxDistance` reads and sets `RollOffMaxDistance`, and negative roll-off distances read 0, as in Studio. They differed.
- The mixer ramps gain changes, so moving sounds don't click. They clicked.

### 2026-10-14: M5 step 2: the Sound API

Pinned Roblox version: 0.741.19. Finishes M5 step 2; M5 Audio continues with 3D sound.

- Sound's methods follow Studio in every state. Several differed.
- Sounds fire `Ended`, `DidLoop` and `Loaded` as in Studio, and `Volume` and `PlaybackSpeed` keep to Studio's ranges. They didn't.
- `PlaybackLoudness` reads the sound's level on clients, and `PlayOnRemove` works. Neither did.
- The mixer starts a sound over on every `Play`. It didn't near the start. See [the Sound API](docs/behavior/audio/sound-api.md).

### 2026-10-14: M5 step 1: audio decoding and playback

Pinned Roblox version: 0.741.19. Finishes M5 step 1; M5 Audio continues with the Sound API.

- Sounds are heard: WAV, Ogg Vorbis and MP3 decode and `rook-player` mixes them to the output device. Rook played no sound.
- Sounds load, report their length and advance as in Studio. See [playback](docs/behavior/audio/playback.md).
- `rook content` decodes the sound files it shows. It didn't.

### 2026-10-14: M4 step 6: unions

Pinned Roblox version: 0.741.19. Finishes M4 step 6 and M4 Content.

#### M4 step 6: unions

- Unions draw their own geometry, as in Studio. They drew their bounding box. See [unions](docs/behavior/meshes/unions.md).
- `Instance.new` names unions, NegateOperations and IntersectOperations as Studio does, and unions read `InitialSize` as `MeshSize`. They differed.
- `rook corpus-scan` and `rook content` count and describe unions, and a file that panics no longer ends the scan. Neither did.

#### MakeJoints: wedge slopes from either side

- `MakeJoints` tries the other part when the first makes no joint, as in Studio. Parts on a wedge's slope came loose.

#### MakeJoints: balls and cylinders

- `MakeJoints` joins balls and cylinders as their boxes, as in Studio. They were never joined.

### 2026-10-14: M4 step 5: PBR shading and materials

Pinned Roblox version: 0.741.19. Finishes M4 step 5 (materials and PBR); M4 Content continues with unions.

#### M4 step 5: MaterialVariant, MaterialService and SurfaceAppearance

- MaterialVariants draw, as in Studio. They didn't.
- MaterialService's override methods answer as Studio's do, and changing it redraws the scene. They didn't exist.
- SurfaceAppearance draws on MeshParts. It didn't.
- Places that save `Use2022Materials` on draw the 2022 set, as in Studio. Rook drew the legacy set for every place. See [materials](docs/behavior/rendering/materials.md).
- A mesh file whose normals are all zero is lit as in Studio. It drew black.
- Rook's index of Studio's content cache notices changes. It could miss textures.

#### M4 step 5: material fixes

- Wedge faces take textures in their own plane. They stretched along long slopes.
- Slate and Basalt draw at Studio's size, and Slate with its right normal map. They were half size.

#### M4 step 5: the built-in materials

- Every material draws its textures, tinted and tiled as Studio does. Parts drew flat colors. See [materials](docs/behavior/rendering/materials.md).
- Neon glows, Glass filters and ForceField glows, as in Studio. They drew plain.
- `rook-content` reads KTX2 images and Studio's content cache. It didn't.
- Every place draws the 2022 materials. Later found wrong, above.

#### M4 step 5: PBR shading

- Every material shades with its own roughness and metalness, and `Lighting.EnvironmentSpecularScale` works, as in Studio. Parts all shaded alike. See [materials](docs/behavior/rendering/materials.md).
- `Enum.Material` is defined once, in `rook-types`. It was defined in several places.
- The materials doc records where Studio keeps downloaded material textures. It didn't.

### 2026-10-14: M3: physics; M4 steps 1–4

Pinned Roblox version: 0.741.19. Finishes M3 Physics; M4 Content is in progress (built-in mesh shapes, asset fetching, file meshes, textures and decals, trusses, skyboxes).

- Source and docs are split by topic, with docs in `docs/guide/`, `docs/design/` and `docs/behavior/`. Large files held many topics. No behavior changed.
- TrussParts draw as Studio draws them, and setting a truss's Size snaps it as Studio does. They drew as boxes.
- A truss's `Style` loads from files. It read as the default.
- The README has a new pitch and credits, and the roadmap lists planned extensions. It described less.
- The README is a short overview linking to the guides. It held everything.
- `rook-physics` simulates Workspace parts with Rapier on the server. Parts didn't move.
- Joints join parts into assemblies, simulated as one body, as in Roblox (M3 step 2). Parts were simulated alone.
- `MakeJoints`, `BreakJoints` and the assembly members work, and setting `CFrame` or `Position` moves parts as in Roblox. They didn't.
- Places saved before explicit joints are joined by surfaces on load, as Roblox did then. They fell apart.
- `rook-dom` indexes reference properties both ways. It indexed them one way.
- Materials have Studio's density, friction and elasticity, mixed as in Studio, and `GetMass` uses the real shape (M3 step 3). Every part was alike.
- `Touched` and `TouchEnded` fire from contacts. They didn't.
- Collision groups work as in Roblox. They didn't.
- The physics conformance cases `physics/joints` and `physics/contacts` pass. There were none.
- Member lookups are resolved once per class and take faster paths into Rust. Scripts ran slower.
- `rook-player` has a command bar that runs Luau in the open place. It had none.
- Engine functions that return nothing return nothing. They returned one `nil`.
- BodyMovers push their assembly as in Studio (M3 step 4). They did nothing.
- RotateV and RotateP are driven by their surface inputs. They were free.
- HingeConstraint, BallSocketConstraint, RopeConstraint, RodConstraint, SpringConstraint, VectorForce and Torque work. They did nothing.
- `Velocity` and `RotVelocity` are views of the assembly's velocities, and anchored parts keep theirs and carry what rests on them. They were stored apart and did nothing.
- `MakeJoints` treats a wedge's slope as a face, as Studio does. Rook treated wedges as boxes.
- The physics conformance cases `physics/movers`, `physics/constraints` and `physics/velocity` pass, and recording picks the right Studio window. There were none.
- `ROADMAP.md` is the one place for the plan. The plan was spread across docs.
- RotateV's torque is limited as in Studio, and parts spin as fast as Roblox lets them. Rotors slammed to full speed and spins were capped.
- Tracy has zones per resumed script, per member access and around engine functions. It didn't.
- Scene queries work as in Studio (M3 step 5). Scripts couldn't query the world.
- `RaycastParams`, `OverlapParams` and `RaycastResult` exist with Studio's messages. They didn't.
- Camera's ray and point methods work, and the client's camera gets the window size. They didn't.
- Clients have a physics world for queries, and part shapes are defined once. The mouse had its own geometry.
- The physics conformance case `physics/queries` passes. There was none.
- SpecialMesh, BlockMesh and CylinderMesh draw their shape as in Studio (M4 step 1). Parts drew their own shape.
- Physics rebuilds walk each cluster once, Instance methods are cached in Luau, material changes update colliders in place, and engine maps use a fast hash. Each did more work.
- The Head MeshType draws Rook's own head mesh. It drew a box.
- Changing a mesh redraws its part. It didn't.
- Content references are parsed as Studio does and resolve through a configurable chain of providers (M4 step 2). Rook loaded no assets.
- Old `rbxasset://` paths resolve to the public assets Roblox uploaded for them, as in Studio. They failed.
- A hash-checked disk cache keeps downloads. Every run fetched again.
- `ContentProvider:PreloadAsync` and `RequestQueueSize` work as in Studio. They didn't.
- `rook content` shows how references resolve, and `rook corpus-scan` reports content properties. Neither did.
- The Head draws the install's `head.mesh` when there is one. It drew Rook's own.
- `MeshId`, `Texture`, `SoundId` and the other string properties are views of the `Content` properties, as in Studio. They read empty and file meshes drew as boxes.
- Rook needs Rust 1.94 or later. It declared an older one.
- File meshes and MeshParts draw, sized as in Studio (M4 step 3). They didn't.
- `MeshPart.MeshSize` reads `InitialSize`, and `PreloadAsync` reports a `MeshId` that isn't a mesh, as in Studio. It read 0.
- A negative mesh `Scale` mirrors only mesh files, as in Studio. It turned built-in shapes inside out.
- Permission errors name the capability as Roblox does. Rook's names differed.
- `rook content --place` decodes meshes, `rook corpus-scan` counts them, and `rook-player --fov` sets the field of view. They didn't.
- Rendering uses reverse Z with an infinite far plane. Distant surfaces z-fought.
- The server runs on its own thread, physics runs in parallel, the script timeout and property reads are cheaper, and drawing updates only what changed. Each made play slower.
- Physics keeps its bodies in deterministic maps. Runs played out differently.
- Holding the right mouse button hides the cursor and holds it in place. It wandered out of the window.
- Changes a server script makes at the end of a frame are kept. They could be trimmed before anything saw them.
- The sun casts shadows from a shadow map. Nothing cast shadows.
- The sun follows `Lighting:GetSunDirection()` as in Studio. A textbook formula placed it wrongly.
- Images load in the background, and Decals, Textures, mesh textures and Plastic's surfaces draw as in Studio (M4 step 4). Parts drew no images.
- Setting image properties to web URLs, or `MeshId` on a SpecialMesh, behaves as in Studio. It didn't.
- `rook corpus-scan` and `rook content --place` count materials, surfaces and images. They didn't.
- The download cache moved to the temp folder, is kept under a size limit, and `rook cache` manages it. It grew without limit.
- The place's Sky draws as in Studio, with the install's default sky otherwise, and reflective parts take its color. Rook drew no skybox.
- Balls and cylinders are built as Roblox builds them. Decals on them stretched.
- Decals on a CylinderMesh face the right way and draw over parts in the same plane. They were turned and fought.
- BlockMesh parts draw plain, as in Studio. They drew studs.
- Assets refused anonymously before are retried with an Open Cloud key. They weren't.

## [0.1.0] - 2026-10-11 - M0 to M2: formats, scripting, the client and the renderer

### 2026-10-11: M2: client side, renderer, input

Pinned Roblox version: 0.741.19.

- `rook-render` and `rook-player`: wgpu renderer for parts and primitive meshes, camera, basic lighting and sky; fly camera; `--screenshot`, `--bench`.
- Client/server split: separate DataModels joined by the in-process replicator (`rook-net`); `Player`, `Backpack`, `PlayerGui`, `PlayerScripts`; LocalScripts.
- RemoteEvent, UnreliableRemoteEvent and RemoteFunction.
- TweenService, Tween and the legacy GUI tweens.
- Input: UserInputService, `Mouse`, ClickDetector, mouse ray queries.
- Build profiles and Tracy zones; incremental scene updates.
- Console output in Roblox Studio's format, colored by message type.
- Every inert or partial API is listed in [stubs](docs/guide/stubs.md).

### 2026-10-11: M0 and M1: formats, DOM and scripting

Pinned Roblox version: 0.741.19.

- M0: workspace, datatypes, class database, binary and XML place formats, the legacy XML upgrade, `rook dump/roundtrip/corpus-scan`.
- M1: `rook-dom`, `rook-signal`, `rook-scheduler`, the Luau bridge (`rook-lua`), the core Instance API and the members list; headless server Scripts (`run-headless`, `corpus-run`); conformance through Studio's MCP server.
- Renamed from Raven to Rook.

[Unreleased]: https://github.com/AsyncFinn/rook/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/AsyncFinn/rook/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/AsyncFinn/rook/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/AsyncFinn/rook/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/AsyncFinn/rook/releases/tag/v0.1.0
