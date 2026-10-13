# Stubs and partial implementations

Every member Rook accepts without doing the real work is listed here, with
what should eventually handle it. Scripts keep running instead of failing,
but nothing here is finished. Run reports (`rook run-headless`, `rook
corpus-run`) count calls to the inert members under "Accepted but ignored
for now". "Taken over by" names the milestone in
[ROADMAP.md](../../ROADMAP.md) or the work that should finish it.

A test (`rook-lua`, `inert_members_are_documented`) fails if a member is
made inert without an entry in the first table.

## Inert members (do nothing, return nil)

| Member | What it does on Roblox | Taken over by |
|---|---|---|
| `BasePart:SetNetworkOwner` | Chooses which machine simulates the part's assembly | Networking (single machine until then) |
| `BasePart:SetNetworkOwnershipAuto` | Returns network ownership to the engine | Networking |
| `ContentProvider:Preload` | Queues an asset for download (deprecated; not checked in Studio) | Content |
| `InsertService:ApproveAssetId` | Allowed a place to `LoadAsset` a specific asset (legacy) | Content, together with `InsertService:LoadAsset` |
| `InsertService:ApproveAssetVersionId` | Same, for one asset version | Content |
| `FunctionalTest:Passed`, `Failed`, `Error`, `Warn` | Report results to Roblox's internal test harness | Possibly the conformance tooling; otherwise stays inert |

## Partial implementations

| Member or behavior | What Rook does now | What is missing | Taken over by |
|---|---|---|---|
| `Sound:Play`, `Stop`, `Pause`, `Resume` | Sets `Playing`/`TimePosition`, fires `Played`/`Stopped`/`Paused` | Audio output, `Ended`, `Loaded`, time advancing | Audio |
| `Model:MoveTo` | Moves the model to the target | Moving up out of obstructions | Physics, later (the queries it needs exist) |
| `Model:GetBoundingBox`, `GetExtentsSize`, `GetModelCFrame` | World-axis-aligned box of the parts | Box aligned to the primary part; exact legacy `GetModelCFrame` | Physics/geometry |
| `DataStoreService` | In-memory stores for one run; `GetSortedAsync` returns a plain table, not a `DataStorePages` | Persistence (local files), versions, `ListKeysAsync`, budgets | Server hosting, Networking |
| `HttpService:GetAsync`, `PostAsync`, `RequestAsync` | Raise "Http requests are not enabled" | Opt-in HTTP for places that enable it | Opt-in, later |
| `require(assetId)` | Raises an error | Loading published modules | Content, later: asset fetching can fetch the model, but loading it as a module isn't done |
| `ContentProvider:PreloadAsync` | On a client: resolves entries through the provider chain and calls back with `Enum.AssetFetchStatus` Success or Failure as each finishes, then returns; instances stand for their and their descendants' content properties; strings load only `rbxasset://` images (as in Studio). On a server: never returns once there is anything to fetch (as in Studio) | Which statuses Studio gives for an asset of the wrong type (a sound id in a Decal); `Loading`/`TimedOut`; exact frame timing | Conformance |
| `ContentProvider.RequestQueueSize` | Requests a DataModel is waiting on | Studio's own count, which didn't follow one call's entries exactly | Conformance |
| Other ContentProvider members (`GetAssetFetchStatus`, `AssetFetchFailed`, `ListEncryptedAssets`, ...) | Not implemented | Each, if places use it | Content |
| `MeshId`, `TextureId`, `Texture`, `SoundId`, `Image` and the other legacy content strings | Views of the `Content` properties files store (`MeshContent`, `TextureContent`, ...); setting one turns `\` into `/` in `rbxasset://` paths, as Studio does | `Changed` and `GetPropertyChangedSignal` fire for the `Content` name only; Studio rewrites lowercase `http://www.roblox.com/asset/?id=` URLs set on a Decal's or Texture's `Texture` (and `ColorMap`) to `rbxassetid://` (dropping `version`), which Rook doesn't | Conformance |
| Asset delivery | Anonymous requests; an Open Cloud API key (`ROOK_OPEN_CLOUD_API_KEY`) for assets refused anonymously | Assets that now need an account fail anonymously (most of a modern core place's); the key path is untested; `rbxthumb://` thumbnails and asset version ids don't resolve; old `rbxasset://` paths outside Rook's table (Studio resolves more of them) | Content |
| `BindableFunction:Invoke` with no callback | Checks for a callback once per frame | Exact resumption timing | Conformance |
| Bindable arguments | Passed by reference | Roblox copies tables passed through bindables | Conformance |
| `RunService:IsClient`/`IsServer`/`IsStudio`/`IsRunMode` | Fixed server answers | Correct answers per DataModel | Client/server split |
| `ConnectParallel`, `task.synchronize`/`desynchronize` | Serial | Actors and parallel Luau | Later |
| `Workspace.SignalBehavior.AncestryDeferred` | Treated as Deferred | Its own rules | Conformance |
| `Random` | PCG32, own seeding | Roblox's exact sequence for a seed | Unknown; may stay an Approximation |
| Error messages from datatype methods | Reach `pcall` as mlua error objects (except RaycastParams, OverlapParams and RaycastResult, whose members go through the engine-function shim) | Plain strings like Instance members | Lua bridge |
| Terrain, MeshParts, unions, decals in the renderer | Not drawn, or drawn as their bounding box | Real geometry and textures | Content; terrain later |
| File meshes in the renderer (`FileMesh`, SpecialMesh `MeshType` FileMesh with a `MeshId`) | Drawn as the part's own shape, as if there were no mesh (one with an empty `MeshId` draws Roblox's 1-stud box times `Scale`) | Decoding the fetched file and drawing it at `Scale` (a FileMesh ignores the part's Size); a `MeshId` that can't be loaded (not found, forbidden, a missing file, junk) draws nothing in Studio (observed) | M4 step 3 (file meshes) |
| Head MeshType | The `head.mesh` of the user's Roblox install when the player finds one; otherwise Rook's own, matching the measured silhouette and vertex layout, caps triangulated as fans | Rook's own head has no UVs (for face decals and textures) | M4 step 4 (textures) |
| Mesh `VertexColor` | Ignored (documented to tint the mesh's texture; not measured) | Its effect | M4 step 4 (textures) |
| Surface studs and materials on meshes | Meshes take the part's colour and transparency only. Studio drew the part's studs on Sphere and Cylinder meshes, and an empty-`MeshId` box without its Neon glow | Surfaces, studs and materials on meshes | M4 step 4 (textures) |
| Meshes under MeshParts, unions and trusses | A mesh child changes how these look like any other part (Inference; unchecked) | Checking them in Studio | M4 step 3 |
| Wedge and corner-wedge orientation, sun direction, lighting model | Corner wedges checked by eye against archived places; the rest are best guesses | Checks against Studio | Renderer, with Studio screenshots |
| RotateV and RotateP | Rapier motors: RotateV at 31 × input rad/s, its torque limited to 5e5 × input × the smaller part's moment of inertia about the axis at unit density; RotateP held by a stiff position motor whose angle steps 0.01 rad per step. The input comes from Part0's face that C0's Z axis points out of; Sin time restarts when the joint is rebuilt | RotateV's real torque rule: it depends on the smaller part's size alone but grows in steps (cubes from 0.5 to 8 studs measured 0.25 to 3.5 times Rook's); RotateP moves kinematically in Roblox (its part reports no angular velocity); which surface Roblox reads for hand-made joints | Conformance |
| BodyPosition and BodyGyro damping | Damper of max(D, D²/384), fitted to Studio for D from 70 to 5000 | The real rule; at low D Roblox damps a little more (P 100, D 10 overshoots to 11.3 there, 11.6 in Rook) | Conformance |
| BodyGyro and RocketPropulsion `MaxTorque` axes | World axes for BodyGyro, the rocket's own axes for RocketPropulsion (Inference from the defaults) | Checking rotated parts in Studio | Conformance |
| RocketPropulsion | Flies at MaxSpeed straight at Target + TargetOffset (TargetOffset alone without a Target), holds still inside TargetRadius, fires ReachedTarget each time it arrives | `ThrustP`/`ThrustD` (no effect seen); Studio settled at 28.8–29.4 studs/s under gravity, not 30; whether ReachedTarget refires | Conformance |
| HingeConstraint actuators | Motor: velocity motor limited by MotorMaxTorque. Servo: speed min(AngularSpeed, 20 × error) | `MotorMaxAcceleration`; Roblox's servo easing; `Restitution`, `ActuatorType` changes mid-motion | Conformance |
| BallSocket, Rope, Rod, Spring constraints | Joint or force between attachments; parts keep colliding | BallSocket limits (`LimitsEnabled`, `TwistLimitsEnabled`), Rope `WinchEnabled` and `Restitution`, Spring `LimitsEnabled` (Min/MaxLength), `CurrentDistance`/`CurrentLength`, `Thickness` | Physics, later |
| Constraints not simulated | PrismaticConstraint (11 files in the archive), AlignPosition and AlignOrientation (6 each), LinearVelocity (1), AngularVelocity, CylindricalConstraint, PlaneConstraint, UniversalConstraint, TorsionSpringConstraint, LineForce, RigidConstraint and NoCollisionConstraint (none) load but do nothing | Each, if usage justifies it | Physics, later |
| Deprecated `BodyPosition:GetLastForce`, `BodyVelocity:GetLastForce` | Not implemented (BodyVelocity's and BodyAngularVelocity's `P` is ignored, as in Studio) | The force applied last step | Physics, later |
| Mover forces | Recomputed every 240 Hz step from the body's velocity, ignoring contacts that step | Roblox solves movers with its constraints | Conformance |
| Physics: when joints take effect | A new or changed joint (and Motor6D `C0`/`C1`) moves parts at the next physics step | Roblox snaps parts as soon as the joint becomes active or `C0`/`C1` changes; `Transform` already waits for the step there | Conformance |
| Physics: unanchored parts joined to several anchored parts | They join the assembly of the anchored part with the highest joint id | Roblox picks the newest joint; ids usually follow creation order | Conformance |
| Physics: equal-sized root candidates | The part with the highest internal id (usually the newer one) | Roblox picks the newer part; its exact rule is unknown | Conformance |
| Legacy automatic joints | Places that don't save `Workspace.ExplicitAutoJoints` (saved before it existed) have every Workspace part joined by its surfaces when they load | Whether legacy Roblox also joined parts as scripts parented them into Workspace or moved them; checking it needs such a place open in Studio | Conformance |
| `MakeJoints` on balls and cylinders | No surfaces; other parts are treated as boxes, wedges included | Their real surfaces | Physics, later |
| Motor6D `Transform` and `DesiredAngle` together | Both apply: `C0 · Transform · Rz(CurrentAngle) · C1⁻¹` | Studio resets `CurrentAngle` when `Transform` is set; the interaction is unverified | Animation |
| `WeldConstraint` state in files | `Enabled` defaults to true; the saved `State` isn't read | A disabled WeldConstraint saved in a file loads enabled | Conformance |
| Physics: mass of MeshParts, unions and trusses | Their Size as a box | Their real volumes | Content |
| Physics: when touches start and end | Rapier's contacts, which start just before parts meet and end just after | Roblox ends a touch when parts stop overlapping, so a part resting exactly on another gets TouchEnded at once | Conformance |
| Physics: order of a touch's two events | The part with the lower internal id fires first (in Studio the part made first fired first) | Roblox's rule | Conformance |
| Collision groups in older files | Places with only the old `Workspace.CollisionGroups` string get just `Default` | Reading that string's groups; the archive's samples list only `Default` | Conformance |
| `Workspace.TouchEventsUseCollisionGroups` | Not read; `TouchesUseCollisionGroups` decides whether groups that don't collide also don't touch | The newer enum's meaning | Conformance |
| Physics: timestep | Always 240 Hz | Adaptive 240/120/60 Hz per assembly | Physics, later |
| Physics: who simulates | The server simulates every part; clients only see replicated results, and their physics world only answers queries | Network ownership | Characters |
| Physics: MeshParts, unions, trusses | Box colliders, in queries too | Their real shapes and `CollisionFidelity` | Content |
| Physics queries: result order | Regions and overlaps list parts in internal id order (usually creation order), so `maxParts`/`MaxParts` keeps the oldest | Roblox's own order (parts made in a row came back in that order, but three parts once came back as 1st, 3rd, 2nd) | Conformance |
| `FindPartsInRegion3` and parts made this frame | Found at once, like every other query | Studio sometimes missed parts made in the same frame until the next step (other times found them); rays and `GetPartBoundsInBox` found them at once | Conformance |
| Queries and terrain | No terrain: `terrainCellsAreCubes`, `ignoreWater` and `RaycastParams.IgnoreWater` change nothing | Hitting terrain, water | Terrain |
| `OverlapParams.Tolerance`, `BruteForceAllSlow` | Stored and printed (Tolerance checks Studio's 0-0.05 range), not used | Their effect | Conformance |
| Overlap tests | Shapes count as overlapping (`GetPartsInPart`, `GetTouchingParts`) past 0.001 studs of penetration (Inference: Studio counted 0.01, not faces that only meet). Bounds queries find candidates by collider bounds, so a cylinder whose `Size` box is much wider than its round shape can be missed near that box's corners | The exact rule | Conformance |
| `GetTouchingParts` on a non-colliding part | Reports its overlaps while something is connected to its `Touched` | Roblox checks for a TouchInterest child, which Rook doesn't create | Conformance |
| `Mouse.Hit`/`Target` | Legacy ray rules (`Default` group, `CanQuery` with `CanCollide` off), 1,000 studs from the camera, minus `TargetFilter` (Inference) | Checking the mouse's own rules in a client | Conformance |
| `ScreenPointToRay`, `WorldToScreenPoint` | Same as their Viewport versions: Rook has no top bar | Roblox clients exclude the 58-pixel top bar (`GuiService:GetGuiInset`) | GUI |
| `WorldRoot:Shapecast`, `ArePartsTouchingOthers` | Not implemented (no archive files use them) | Casting a part; the touching test | Physics, later |
