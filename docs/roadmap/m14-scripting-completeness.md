# M14 Scripting completeness

Driven by the corpus run (`reports/run/`): today 60.4% of places with
server scripts run 10 seconds without a script error. Many errors are the
scripts' own (old syntax modern Roblox rejects too, missing children);
the target counts only errors Rook causes, checked against Studio.

1. Classify the corpus run's errors: Rook's (unimplemented members,
   wrong behavior) against the scripts' own, with a sample of each
   checked in Studio.
2. The unimplemented members by places affected, starting with
   `InsertService:LoadAsset` (54 places) and `require(assetId)` (17).
3. Services that talk to Roblox's website, answered locally and
   offline: MarketplaceService, BadgeService, GroupService, Players'
   web lookups.
4. TeleportService between places in one folder (multi-place games).
5. HttpService, opt-in per place.
6. Modern Luau: Parallel Luau (Actor, `task.desynchronize`, SharedTable),
   `buffer`, and the libraries and syntax of the Luau live Roblox runs.
7. Client scripts in the corpus run too, once characters and GUI exist.
