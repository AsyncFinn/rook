# The data model

Roblox's datatypes, the class database and the runtime instance tree everything else reads. Part of the [architecture](../architecture.md).

## Datatypes (`rook-types`)

- Rook's own types, built on `glam`, with `From`/`TryFrom` conversions to `rbx_types::Variant` at the I/O boundary.
- `Value` is the dynamic property value: `Bool`, `Int`, `Float`, `String`, `Content`, `Vector3`, `CFrame`, `Color3`, `BrickColor`, `UDim`/`UDim2`, `Rect`, `NumberSequence`, `ColorSequence`, `NumberRange`, `PhysicalProperties`, `Font`, `Faces`/`Axes`, `EnumItem(EnumId, u32)`, `Ref(Option<InstanceId>)`, `Attributes`, `Tags`, and so on.
- Math semantics follow Luau (f64 `number`) where scripts can see the result. Engine-internal hot paths may use f32 where that's invisible. CFrame orthonormalisation, `lerp`, `ToEulerAnglesXYZ` and similar functions are verified against Studio values ([Testing and measurement](../architecture.md#testing-and-measurement)).
- `lune-roblox` (MPL-2.0) is the reference for datatype edge cases. If any file is adapted from it, that file stays MPL.

## Reflection (`rook-reflection`)

- Source: Rook's own copy of rbx-dom's reflection database (`data/database.msgpack`), started from `rbx_reflection_database` 3.0.1 (Roblox 0.741) and updated by `cargo xtask gen-api` from each new API dump, since rbx-dom's releases trail Roblox's. It includes the class tree, members, tags (`Deprecated`, `NotCreatable`, `Service`, security levels) and per-class defaults.
- **Overlay data** (Rook-owned, in-repo TOML/JSON): which members Rook has implemented, how each property is stored ([Runtime DOM](#runtime-dom-rook-dom)), behavior notes and provenance IDs, and extension classes/members marked `rook_extension = true`.
- At startup the database is compiled into dense IDs: `ClassId(u16)`, `PropertyId`, `EnumId`. Each class gets a **layout**: its full inherited property list, mapped to slots. Name lookups go through interned atoms and a per-class `(name → member)` cache.
- Updating to a new Roblox version is a deliberate change: run `gen-api`, review its list of added, removed and changed members, then run the checks and the conformance suite ([CONTRIBUTING](../../../CONTRIBUTING.md#keeping-up-with-roblox)).

## Runtime DOM (`rook-dom`)

### Why not use `rbx_dom_weak` at runtime

`WeakDom` stores each instance's properties as a string-keyed map of `Variant`s. That suits serialization, but the runtime needs fast typed access, change tracking, destroyed state, locked parents, attributes and tags, script identity, and derived properties (`Position` from `CFrame`). Rook converts at the I/O boundary only.

### Structure

- `InstanceId`: a generational arena key (`slotmap`). Stale IDs never alias new instances, and destroyed instances stay valid handles until collected.
- `InstanceData`: `class: ClassId`, `name`, `parent`, ordered `children` (`GetChildren` order), flags (`archivable`, `parent_locked`, `destroyed`), a property store, attributes, tags, and an optional behavior-state slot.
- **Property backing**, chosen per property in the overlay:
  - `Stored`: the value lives in the instance's `Properties`, as a delta from the class default, as Unreal keeps an object's differences from its class default object. Values unlike the default sit in a list sorted by property id; a property set to its default (files save every property, and most hold their default) keeps only its id, so it still counts as set and saves. Unset ones read the class default. This is how every class works before it has behavior.
  - `Native(getter, setter)`: implemented by a behavior module, for derived or hot properties (`BasePart.Position`, `Orientation`, `CFrame`, GUI `AbsolutePosition`).
- **Class behavior registry**: `trait ClassBehavior` with hooks (`on_created`, `on_property_changed`, `on_ancestry_changed`, `on_destroying`) and method implementations. Classes with no registered behavior are **data-only**: still fully creatable, scriptable and serializable. Calling an unimplemented method raises a clear `"<Class>:<Method> is not implemented in Rook"` error and increments a counter that feeds the roadmap.
- **Hierarchy rules** follow Roblox: no cycles; locked parents (`Destroy`, services); `Parent` set last when cloning; `Destroy` locks the parent, disconnects connections and recurses; `Clone` respects `Archivable`; services are found or created by `GetService`.
- **Change journal**: every mutation appends an event (`Created`, `PropertyChanged(id, prop)`, `ChildAdded`, `ChildRemoved`, `AncestryChanged`, `Destroying`). Two kinds of consumers read it:
  1. the signal dispatcher (`Changed`, `GetPropertyChangedSignal`, `ChildAdded`, `DescendantAdded`, `AncestryChanged`, ...), and
  2. engine systems (physics, GUI, render sync, audio). Each keeps its own cursor and processes dirty sets at its phase. Systems never get callbacks in the middle of a mutation.

  Entries are dropped only by `DataModel::trim_journal`, which first has every reader read what it hasn't (`read_journal`), so a change made after a step (a `Died` handler, a remote between frames) reaches a reader that acts only at the next step. Debug builds check in `Journal::since` that no reader is behind a trim, so a reader left out fails the tests. Loads turn the journal off before anything reads it; their readers' first look scans the tree.
- **Collection**: a destroyed instance leaves the arena once nothing holds it, as Roblox frees one once nothing, Lua included, references it. An instance holds its parent, children and reference targets; a destroyed one is held by a live instance holding it, a script's Lua handle (a userdata in the weak instance table), the few things the runtime keeps for later (`World::held`), or another destroyed instance held so. Live instances are never collected. Each `trim_journal` frees what the previous one announced, if still unheld, then records `Collected` for the destroyed instances nothing holds now, so every journal reader drops what it keeps of one (the renderer gets it in `RenderChanges::collected`, to drop without reading) a full frame before its slot goes; what the runtime and input keep is dropped as it's freed. A check costs what is destroyed and not yet freed, not the place's size. Debug builds fail any read of a freed id, so a holder that kept one fails the tests (`rook-dom/src/collect.rs`).
- **No global state.** Several DataModels can exist in one process (server + client for Play mode, Studio edit + play). Each has its own DOM, Lua VM, scheduler and physics world.
