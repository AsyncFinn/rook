# M18 Offline content

Preservation: a place should keep working after the assets it names
stop being downloadable.

1. Content packs: `rook-tools content` exports everything a place or a folder
   of places needs into one pack, and Rook loads from packs before the
   network.
2. A pack format that's documented and checked (hashes, versions), so
   archives can share packs.
3. Reports of what a place still can't find, offline.
4. Rook's own PlayerModule and CoreGui, matching modern Roblox's behavior
   by behavior, checked against Roblox's running in Rook, so a place
   plays with nothing of Roblox's but its assets
   ([decided](../design/research.md#open-questions) 2026-10-26).
