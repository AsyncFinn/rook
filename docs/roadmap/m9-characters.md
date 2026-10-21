# M9 Characters

Humanoid 905 files, Tool 1,152, HopperBin 1,107, SpawnLocation 1,509,
Seat 642, VehicleSeat 230. Needs physics, the head mesh, terrain and GUI.

1. Spawning at SpawnLocations, R15 and R6 characters (`RigType`, and
   HumanoidDescription, 15 files), respawning. Begun: the spawn flow
   (`LoadCharacter`, CharacterAutoLoads, StarterCharacterScripts, Backpack
   and PlayerGui refills, CharacterAdded) with a bare, anchored body.
2. Humanoid: state machine, walking, jumping, climbing, swimming, health
   and death, joints breaking.
3. Default camera and control scripts: the PlayerModule modern places
   get, and what older places expect (Core-Scripts, Apache-2.0).
4. Tools and the backpack; HopperBin as [decided](unscheduled.md#undecided).
5. Seats and VehicleSeats.
6. Playing animations, enough for Rook's own Animate (Animation is in 270
   files; the rest of animation is M11).
