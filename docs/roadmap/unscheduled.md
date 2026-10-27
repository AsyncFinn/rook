# Unscheduled

Good ideas without a place in the order yet, and decisions still open.
Each can only come after the milestones it needs.

## Ideas

- **Studio**: `rook-studio`, a place editor (Explorer and Properties from
  the reflection database; moving, scaling and rotating parts; play
  testing; saving files Roblox Studio opens unchanged).
- **Rook extensions**: opt-in features beyond what Roblox allows, behind
  the explicit profile from the [architecture](../design/architecture.md#extensions),
  off by default:
  - WebSockets available to any script (needs M10).
  - EditableImage and EditableMesh without platform gating.
  - Restoring embedded legacy textures that modern Roblox loads as empty.

## Undecided

- **HopperBin** (1,813 files): modern Roblox no longer runs it. Decide
  before [M9 Characters](m9-characters.md)' tools step whether to follow
  modern behavior or offer an extension.
