# Stubs and partial implementations

Every member Rook accepts without doing the real work is listed here, with
what should eventually handle it. Scripts keep running instead of failing,
but nothing here is finished. Run reports (`rook run-headless`, `rook
corpus-run`) count calls to the inert members under "Accepted but ignored
for now".

A test (`rook-lua`, `inert_members_are_documented`) fails if a member is
made inert without an entry in the first table.

## Inert members (do nothing, return nil)

| Member | What it does on Roblox | Taken over by |
|---|---|---|
| `BasePart:SetNetworkOwner` | Chooses which machine simulates the part's assembly | Networking, M7 (single machine until then) |
| `BasePart:SetNetworkOwnershipAuto` | Returns network ownership to the engine | Networking, M7 |
| `ContentProvider:Preload` | Queues an asset for download (deprecated) | Content, M6 |
| `ContentProvider:PreloadAsync` | Downloads assets and yields until they are loaded | Content, M6. It should yield then; it returns at once now |
| `InsertService:ApproveAssetId` | Allowed a place to `LoadAsset` a specific asset (legacy) | Content, M6, together with `InsertService:LoadAsset` |
| `InsertService:ApproveAssetVersionId` | Same, for one asset version | Content, M6 |
| `FunctionalTest:Passed`, `Failed`, `Error`, `Warn` | Report results to Roblox's internal test harness | Possibly the conformance tooling; otherwise stays inert |

## Partial implementations

| Member or behavior | What Rook does now | What is missing | Taken over by |
|---|---|---|---|
| `Sound:Play`, `Stop`, `Pause`, `Resume` | Sets `Playing`/`TimePosition`, fires `Played`/`Stopped`/`Paused` | Audio output, `Ended`, `Loaded`, time advancing | Audio, M6 |
| `BasePart:GetMass` | Volume × 0.7 (Plastic's density) for every material | Per-material density, `CustomPhysicalProperties` | Physics, M3 |
| `Model:MoveTo` | Moves the model to the target | Moving up out of obstructions | Physics queries, M3 |
| `Model:GetBoundingBox`, `GetExtentsSize`, `GetModelCFrame` | World-axis-aligned box of the parts | Box aligned to the primary part; exact legacy `GetModelCFrame` | Physics/geometry, M3 |
| `PhysicalProperties.new(material)` | Returns default properties | The per-material table | Physics, M3 |
| `DataStoreService` | In-memory stores for one run; `GetSortedAsync` returns a plain table, not a `DataStorePages` | Persistence (local files), versions, `ListKeysAsync`, budgets | Player/server hosting, M2/M7 |
| `HttpService:GetAsync`, `PostAsync`, `RequestAsync` | Raise "Http requests are not enabled" | Opt-in HTTP for places that enable it | Opt-in, later |
| `require(assetId)` | Raises an error | Loading published modules | Content, M6 |
| `BindableFunction:Invoke` with no callback | Checks for a callback once per frame | Exact resumption timing | Conformance |
| Bindable arguments | Passed by reference | Roblox copies tables passed through bindables | Conformance |
| `RunService:IsClient`/`IsServer`/`IsStudio`/`IsRunMode` | Fixed server answers | Correct answers per DataModel | Client/server split, M2 |
| `ConnectParallel`, `task.synchronize`/`desynchronize` | Serial | Actors and parallel Luau | Later |
| `Workspace.SignalBehavior.AncestryDeferred` | Treated as Deferred | Its own rules | Conformance |
| `Random` | PCG32, own seeding | Roblox's exact sequence for a seed | Unknown; may stay an Approximation |
| Error messages from datatype methods | Reach `pcall` as mlua error objects | Plain strings like Instance members | Lua bridge |
| Terrain, MeshParts, unions, SpecialMesh, decals in the renderer | Not drawn, or drawn as their bounding box | Real geometry and textures | Content M6, terrain M8 |
| Wedge and corner-wedge orientation, sun direction, lighting model | Corner wedges checked by eye against archived places; the rest are best guesses | Checks against Studio | Renderer, with Studio screenshots |
| Physics: hinge motors | Rotate, RotateP and RotateV turn freely | RotateV and RotateP driving their rotation (surface `ParamA`/`ParamB`, `SurfaceInput`) | Physics, M3 step 4 |
| Physics: when joints take effect | A new or changed joint (and Motor6D `C0`/`C1`) moves parts at the next physics step | Roblox snaps parts as soon as the joint becomes active or `C0`/`C1` changes; `Transform` already waits for the step there | Conformance |
| Physics: unanchored parts joined to several anchored parts | They join the assembly of the anchored part with the highest joint id | Roblox picks the newest joint; ids usually follow creation order | Conformance |
| Physics: equal-sized root candidates | The part with the highest internal id (usually the newer one) | Roblox picks the newer part; its exact rule is unknown | Conformance |
| Legacy automatic joints | Places that don't save `Workspace.ExplicitAutoJoints` (saved before it existed) have every Workspace part joined by its surfaces when they load | Whether legacy Roblox also joined parts as scripts parented them into Workspace or moved them; checking it needs such a place open in Studio | Conformance |
| `MakeJoints` on balls and cylinders | No surfaces; other parts are treated as boxes, wedges included | Their real surfaces | Physics, later |
| Motor6D `Transform` and `DesiredAngle` together | Both apply: `C0 · Transform · Rz(CurrentAngle) · C1⁻¹` | Studio resets `CurrentAngle` when `Transform` is set; the interaction is unverified | Animation, M8 |
| `WeldConstraint` state in files | `Enabled` defaults to true; the saved `State` isn't read | A disabled WeldConstraint saved in a file loads enabled | Conformance |
| Physics: materials | Every part is Plastic (density 0.7, friction 0.3, elasticity 0.5) | Per-material table, `CustomPhysicalProperties` | Physics, M3 step 3 |
| Physics: timestep | Always 240 Hz | Adaptive 240/120/60 Hz per assembly | Physics, later |
| Physics: who simulates | The server simulates every part; clients only see replicated results | Network ownership | Characters, M4 |
| Physics: MeshParts, unions, trusses | Box colliders | Their real shapes and `CollisionFidelity` | Content, M6 |
