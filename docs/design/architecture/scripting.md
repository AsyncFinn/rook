# Scripting

Signals, the scheduler and the Luau bridge. Part of the [architecture](../architecture.md).

## Signals and scheduler (`rook-lua`'s `signal` and `scheduler`)

### Signals

- `RBXScriptSignal`: `Connect`, `Once`, `Wait`, `ConnectParallel` (stub until Actors exist), plus legacy `connect`/`wait` aliases. `RBXScriptConnection`: `Disconnect`, `Connected`.
- **Immediate** mode (what `SignalBehavior.Default` maps to today, and what old places use): handlers run synchronously at fire time on a new thread, with a re-entrancy depth limit of 10. This is **Confirmed** from creator-docs.
- **Deferred** mode: handlers are queued and run at resumption points. `Disconnect()` drops pending invocations; `Destroy()` disconnects but still runs pending ones. This is **Confirmed** from creator-docs.
- **Removal** (`rook-lua`'s `removal`): a script's `Destroy` or `Parent` write is split into steps (lock and `Destroying`; `DescendantRemoving` per descendant; the move; each child it had at the start), each step's journal entries dispatched before the next, so Immediate handlers see the tree mid-removal as in Studio. Host code doing the same through `Dom` records the same entries in the same order and dispatches after. **Confirmed** in Studio 0.743 (conformance case `signals/destroy_order`).

### Scheduler

- One thread per DataModel. Its Luau, DOM and physics run on it. This matches Roblox's cooperative model, and it is where OpenRBLX went wrong. A server and its client are separate DataModels, so they may run on separate threads ([threads and frames](threads-and-frames.md#threading-and-data-flow)).
- Frame pipeline. The order is an **Inference** from the creator-docs diagram, to be confirmed in Studio before it's locked in:

  ```
  input processing      → UserInputService events
  PreRender             → BindToRenderStep, camera  (client)
  [snapshot to renderer]
  replication receive
  legacy resumption     → wait(), spawn(), delay()
  PreAnimation
  animation + Humanoid step
  PreSimulation (Stepped)
  physics               → whole 240 Hz steps, the rest carried (Measured)
  PostSimulation
  Heartbeat; task.wait / task.delay resumption
  replication send
  ```

  Deferred-event drains happen at each documented resumption point.
- Queues: a min-heap of waiting threads by resume time; legacy `wait` (with its own minimum and throttling, behavior to be measured); `task.defer`/`task.spawn`; the deferred-event queue.
- **Time source abstraction**: real time (player) or fixed-step simulated time (headless runs and tests), so conformance runs are deterministic.

## Luau bridge (`rook-lua`)

- One Luau VM per DataModel (`mlua` with `luau`, `vendored`; `luau-jit` later). Each `Script`/`LocalScript`/`ModuleScript` runs on its own thread with its own environment table, which falls back to the shared globals. Environments are marked `safeenv`, as Roblox's sandbox marks them, so Luau runs builtins natively; `getfenv`/`setfenv` work as in Luau and clear the mark, except that `getfenv` gives the script's environment for level 0, C functions and Rook's own Luau, as a Roblox script's thread does (anti-tamper scripts compare every level's). Scripts compile at optimization level 2, the default in live games (Studio testing uses 1; creator-docs Luau comments), and a script's `--!optimize` comment picks its own. `typeof` is Luau's builtin, as in Roblox: it reads `__type` from datatypes' metatables, and from the vector type's (`Vector3`).
- **Instance userdata** wraps an `InstanceId`. A weak-valued registry keeps **one userdata per instance**, so `==`, `rawequal` and table keys behave like Roblox. A userdata no script holds is collected and made again on the next use, so state that belongs to the instance (callbacks such as `OnInvoke`, engine Luau's runtime-only values) is keyed by the instance's key, never by its userdata. `__index` checks properties, then methods, then children by name; each class's members are resolved once into a table by name, so a lookup is one hash probe. `__index` and `__newindex` skip the general engine-function shim (they run more than any other engine code). Destroyed instances still read.
- **What a member read costs.** Roblox's own bindings are C functions: Luau gives every interned string an atom (`lua_Callbacks::useratom`), so `__index` switches on an integer and pushes the value, and `inst.Name` costs one string push. Godot's GDScript does the same with interned `StringName` keys and `ptrcall` method binds; LuaJIT's FFI goes further, compiling a field read to a load. Rook's `__index` is a Luau shim that passes a member id to Rust, and the crossing (mlua's callback trampoline, borrows, the push) is what costs: measured on a 24,000-part walk, about 0.18 us a read for a value the shim keeps, 0.37 for one read through Rust, 0.49 for `Name`, whose string is made and interned each time. So, as CPython's attribute cache and V8's inline caches do, the shim keeps the results scripts read most and drops them when their source changes. An instance's `ClassName` never changes. Its `Name` is kept per instance userdata (weak keys) after the first fast read, and checked before anything else in the shim, so a walk reading thousands of names never leaves Luau. A kept value must never be read stale, so it isn't dropped later from the journal (read at chosen points, and off during bulk loads), but inside the change: a name only changes through `Dom::set_name`, which tells the observer the Luau bridge installs (`Dom::set_observer`), and that drops the instance's kept name before `set_name` returns, whoever renamed it and with the journal on or off. Debug builds also check every kept name against the DOM on each read, so tests fail if a name ever changes some other way. Making the crossing itself cheaper with raw C functions that skip mlua's trampoline would speed every read, but needs unsafe stack code and a second value conversion beside `to_lua`; the cache gets reads of the hot values below the cost of any crossing. `GetChildren` filled a fresh array per call, about 0.07 us a child; each parent's filled array is now kept and copied (`table.clone`, 0.003 a child), since scripts may change what they get. The same observer hears of every child added or removed and drops the parent's kept array then, so a copy is never stale and a kept array never holds a destroyed child from collection (it holds the children's handles); copying one needs no call into Rust. A children list changes only through two methods that count a version, and debug builds check each kept array's version against the DOM's on every call.
- Datatypes are userdata with Roblox's operators, constructors and immutability.
- **Yielding**: a yielding engine function (`wait`, `task.wait`, `WaitForChild`, `Event:Wait`, `InvokeServer`, async HTTP) registers the current thread with the scheduler, then yields. That way a yield inside a `coroutine.wrap` behaves as on Roblox, and the observable behavior is pinned by conformance cases.
- **Calling convention** (`rook-lua/src/runtime/ctx.rs`). mlua turns errors from Rust callbacks into userdata, and can only yield from Rust through async functions, but Roblox scripts expect error values to be plain strings and engine functions like `wait` to yield the calling coroutine. So every engine function is a raw Rust function wrapped in a small Luau shim. The Rust side returns a value and a code: no code for a single result (the shim returns it after one nil test), or a `Code`:
  - no results, or several results pending;
  - yield: the shim calls `coroutine.yield()`, the Rust side having already registered the current thread with whatever resumes it;
  - call: the value is a function whose arguments are pending, and the shim calls it from Lua, so it may yield (`require` and `Invoke` use it);
  - error: the value is the message, and the shim calls `error(msg, 0)`.

  Pending values are fetched with a second call; the shim takes two results, never varargs, which cost more than most engine functions.
- **Security identities**: game scripts run at the normal game identity. Members tagged with a higher security in reflection raise the Roblox-style permission error.
- **Errors and output**: chunk names are the instance's full name, so messages read `Workspace.Script:3: ...`. Output goes through `LogService` with Roblox-style message types.
- `require(ModuleScript)`: caching per DataModel, cyclic-require error, and the "Requested module experienced an error while loading" message.
