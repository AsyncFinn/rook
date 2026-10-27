# Spawning and teams

Checked against Studio 0.744 on 2026-10-26.

Where characters appear, which team players are on, and respawning:
`rook_lua`'s `characters::spawning` and `rook_engine`'s session host.
Measured in a Baseplate playtest by respawning one player with
`LoadCharacter` under each condition; the conformance cases
`characters/spawn_choice` and `characters/team_properties` record it.

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
- Spawns anywhere under Workspace count (one inside a Model did), and
  none outside it (one in ReplicatedStorage didn't).
- The character faces the way its spawn does (measured: a spawn turned
  90° turned the character with it). Inference: a tilted spawn's way,
  kept level.
- A spawn with a `Duration` gives the new character a ForceField for that
  many seconds (creator-docs; measured: the default 10 gave one, 0 none).

## Teams

- A joining player goes on the `AutoAssignable` team with the fewest
  players, the first such team in Teams on a tie (creator-docs for the
  fewest; the tie rule is Inference, as balancing needs several players
  in Studio, M10). With no such team, the player stays neutral.
- A player's team follows its `TeamColor`: unless `Neutral`, it's on the
  first team in Teams of that color. Measured, setting each in turn:
  - `Team` sets `TeamColor` to the team's and `Neutral` false; set to the
    second of two teams of one color, the player is on the first.
  - `TeamColor` joins the first team of that color; a color no team has
    leaves the player on no team with `Neutral` as it was (false).
  - `Neutral` true leaves the team, keeping `TeamColor`; false joins the
    first team of `TeamColor` again.
  - A team recolored loses its players (they keep their color, on no
    team, until a team has it); a team taken out of Teams hands them to
    the next of their color, and a destroyed one leaves them on none.
    Rook moves them once a frame, before scripts resume (Inference: the
    cases read it a frame later).
- `Team:GetPlayers()` lists the players on it, and errors for a team not
  in Teams. The order of the Changed signals these fire isn't followed
  (Studio fires `TeamColor` twice on joining a team).
- Touching a non-neutral SpawnLocation with `AllowTeamChangeOnTouch`
  puts the character's player on the spawn's team (creator-docs; on the
  server, where touches of a client's character are seen).

## Respawning

When a player's Humanoid's health reaches 0, `Died` fires on the server
and, with `Players.CharacterAutoLoads`, the player respawns
`Players.RespawnTime` later (measured: the new character came 5.3 s
after Died with the default 5; Rook takes exactly `RespawnTime`). The
body falling apart, regeneration and `Died` on the client are step 4's.
