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
| `BasePart:MakeJoints` | Joins the part to touching parts with surface joints (Weld, Snap, Glue, Motor, Hinge) based on surface types | Physics, M3: assemblies and legacy surface joints |
| `BasePart:BreakJoints` | Destroys the part's joints (welds, Motor6Ds, surface joints) | Physics, M3. Also used to kill characters (breaking the Neck), M4 |
| `Model:MakeJoints` | `MakeJoints` on every part in the model | Physics, M3 |
| `Model:BreakJoints` | `BreakJoints` on every part in the model | Physics, M3 |
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
| Wedge and corner-wedge orientation, sun direction, lighting model | Best guesses | Checks against Studio | Renderer, with Studio screenshots |
