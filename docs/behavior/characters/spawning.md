# Spawning and teams

Checked against Studio 0.744 on 2026-10-24.

Where characters appear, which team players are on, and respawning:
`rook_lua`'s `characters::spawning` and `rook_engine`'s session host.
Measured in a Baseplate playtest by respawning one player with
`LoadCharacter` under each condition.

## The spawn

- A player may spawn on enabled SpawnLocations in Workspace that are
  `Neutral`, and, when on a team, on those of its team's `TeamColor`
  (creator-docs). A neutral player never uses a team's spawn (measured:
  with only a team spawn, a neutral player spawned as with none).
  Rook picks one of them at random (creator-docs; its own generator, so
  runs repeat).
- On a 12-stud spawn, players appeared anywhere up to about 1 stud from
  its edges; on a 6-stud one, exactly at its center. Rook places them at
  random 1 stud in from the edges on spawns wider than 6 studs, at the
  center otherwise (where Roblox switches is Inference).
- The root part appears 0.01 studs above standing height: the body's
  lowest point 0.01 above the spawn's top (measured: 4.0175 over a top at
  1 for an avatar whose root stands 3.0075 above its feet, then settling
  at 4.0075). One playtest spawned a player on a 6-stud team spawn half a
  stud lower, in the spawn, and it was pushed out; Rook doesn't.
- With no spawn the player may use (none, only disabled ones, or only
  other teams'), feet appear 100 studs above the origin (measured:
  root at (0, 103.0175, 0)), and the character falls.
- A part sitting on the spawn: Studio put the character inside it and
  physics pushed it out on top. Rook raises it clear of colliding parts
  in its box at once (the outcome Studio settled to).
- A spawn with a `Duration` gives the new character a ForceField for that
  many seconds (creator-docs; measured: the default 10 gave one, 0 none).

## Teams

- A joining player goes on the `AutoAssignable` team with the fewest
  players, the first such team in Teams on a tie (creator-docs for the
  fewest; the tie rule is Inference, as balancing needs several players
  in Studio). With no such team, the player stays neutral.
- Setting a player's `Team` sets its `TeamColor` to the team's and
  `Neutral` to whether it has none; setting `TeamColor` joins the first
  team of that color (or none); setting `Neutral` leaves its team
  (creator-docs Player).
- Touching a non-neutral SpawnLocation with `AllowTeamChangeOnTouch`
  puts the character's player on the spawn's team (creator-docs; on the
  server, where touches of a client's character are seen).

## Respawning

When a player's Humanoid's health reaches 0, `Died` fires on the server
and, with `Players.CharacterAutoLoads`, the player respawns
`Players.RespawnTime` later (measured: the new character came 5.3 s
after Died with the default 5; Rook takes exactly `RespawnTime`). The
body falling apart, regeneration and `Died` on the client are step 4's.
