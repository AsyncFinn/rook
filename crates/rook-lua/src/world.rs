//! Engine state shared by every Lua callback in one DataModel.

use std::collections::{BTreeMap, HashMap};

use mlua::{Function, Table, Thread, Value as LuaValue};
use rook_dom::{Dom, InstanceId};
use rook_scheduler::{Clock, Scheduler};
use rook_signal::{ConnectionId, Signals};

/// `workspace.SignalBehavior`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalBehavior {
    /// Handlers run when the signal fires (what `Default` means today).
    Immediate,
    /// Handlers run at the next resumption point.
    Deferred,
}

/// What a connection does when its signal fires.
#[derive(Debug, Clone)]
pub enum Handler {
    /// `Connect`/`Once`: call the function on a new thread.
    Call(Function),
    /// `Wait`: resume the waiting thread with the arguments.
    Resume(Thread),
}

/// How a resumed `wait` reports its result.
#[derive(Debug, Clone, Copy)]
pub enum WaitKind {
    /// `wait()`: returns elapsed time and `time()`.
    Legacy { since: f64 },
    /// `task.wait()`: returns elapsed time.
    Task { since: f64 },
    /// `delay`/`spawn`/`task.delay`: the job is a call, nothing to report.
    None,
}

/// Work queued in the scheduler.
#[derive(Debug, Clone)]
pub enum Job {
    /// Resume a yielded thread.
    Resume {
        thread: Thread,
        args: Vec<LuaValue>,
        wait: WaitKind,
    },
    /// Call a function on a new thread (`spawn`, `delay`, `task.delay` with a function).
    Call {
        func: Function,
        args: Vec<LuaValue>,
        /// `delay`/`spawn` pass elapsed time and `time()` to the function.
        legacy_since: Option<f64>,
    },
    /// Destroy an instance (`Debris:AddItem`).
    Destroy(InstanceId),
    /// A Deferred-mode signal handler.
    Fire {
        connection: ConnectionId,
        handler: Handler,
        args: Vec<LuaValue>,
    },
}

/// Output message type, as in `Enum.MessageType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageType {
    Output,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub kind: MessageType,
    pub message: String,
    /// `time()` when it was logged.
    pub time: f64,
    /// Wall-clock time when it was logged.
    pub wall: std::time::SystemTime,
    /// Where it came from, as Studio shows it: `Script:3` (the script's
    /// name and line).
    pub source: Option<String>,
}

/// `Workspace.Model.Script` and line 3 → `Script:3`.
pub(crate) fn short_source(chunk: &str, line: usize) -> String {
    let name = chunk.rsplit('.').next().unwrap_or(chunk);
    format!("{name}:{line}")
}

/// Input devices as the client sees them (UserInputService state).
#[derive(Debug, Default, Clone)]
pub struct InputState {
    /// Held keys, as `Enum.KeyCode` values.
    pub keys: std::collections::HashSet<u32>,
    /// Mouse buttons 1-3 held.
    pub buttons: [bool; 3],
    /// Cursor position in pixels from the viewport's top left.
    pub mouse: (f32, f32),
    /// Cursor movement during the last event.
    pub mouse_delta: (f32, f32),
    /// The local player's Mouse (`Player:GetMouse()`).
    pub mouse_object: Option<InstanceId>,
}

/// A visual effect's method a script called: what the renderer applies
/// besides properties.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EffectCall {
    /// `ParticleEmitter:Emit(count)`.
    Emit(u32),
    /// `ParticleEmitter:Clear()`, `Trail:Clear()`.
    Clear,
    /// `Beam:SetTextureOffset(offset)`.
    TextureOffset(f32),
}

/// What scripts did to a Sound that isn't a property (see
/// `api/methods/sound.rs`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SoundControl {
    /// Where `Play` starts: the last `TimePosition` a script set, or 0
    /// after `Stop`.
    pub start: f64,
    /// Loops since the last `Play` (`DidLoop`'s count).
    pub loops: u32,
}

/// The audio API's script state (`api/methods/audio.rs`).
#[derive(Debug, Default)]
pub struct AudioState {
    /// AudioAnalyzers' last spectra, written by the engine on clients.
    pub spectra: HashMap<InstanceId, Vec<f32>>,
    /// `AudioPlayer:Play(atTime)` and `Stop(atTime)` calls waiting for
    /// their time.
    pub scheduled: Vec<Scheduled>,
    /// The last id a scheduled call returned.
    pub next_action: i64,
}

/// A scheduled `Play` or `Stop`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scheduled {
    pub id: i64,
    pub player: InstanceId,
    pub play: bool,
    /// On the DataModel's clock (`SoundService:GetMixerTime`).
    pub at: f64,
}

/// Where 3D sounds are heard from (`SoundService:SetListener`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Listener {
    /// `Workspace.CurrentCamera`.
    #[default]
    Camera,
    CFrame(rook_types::CFrame),
    /// The part's position, facing as the camera does (Inference).
    ObjectPosition(InstanceId),
    ObjectCFrame(InstanceId),
}

/// A script's run state.
#[derive(Debug, Default)]
pub(crate) struct ModuleState {
    /// `require` results, by ModuleScript.
    pub(crate) results: HashMap<InstanceId, mlua::RegistryKey>,
    /// Modules currently loading, with the thread running each body: a
    /// require from that thread is a cycle, one from another thread waits.
    pub(crate) loading: HashMap<InstanceId, mlua::Thread>,
    /// Threads waiting for a module another thread is loading.
    pub(crate) waiting: HashMap<InstanceId, Vec<mlua::Thread>>,
}

/// Everything a DataModel's scripts can reach.
#[derive(Debug)]
pub struct World {
    pub dom: Dom,
    pub signals: Signals<Handler>,
    pub scheduler: Scheduler<Job>,
    pub signal_behavior: SignalBehavior,
    /// Whether this DataModel is a client (RunService:IsClient).
    pub is_client: bool,
    /// Keyboard and mouse state (clients only).
    pub input: InputState,
    /// Remote calls waiting for the session to deliver them.
    pub outbox: Vec<crate::net::RemoteMessage>,
    /// ContentProvider requests waiting for the engine to resolve them.
    pub content: crate::api::content::ContentRequests,
    pub assets: crate::api::assets::AssetRequests,
    /// The physics world. Scripts query it (raycasts, regions), which
    /// brings it up to date with the DOM first, so it lives here with the
    /// DOM. Servers step it; a client's only answers queries (see
    /// `rook_physics::Physics::for_queries`).
    pub physics: rook_physics::Physics,
    /// Threads waiting for an invoke's reply, by call id.
    pub(crate) pending_calls: HashMap<u64, Thread>,
    pub(crate) next_call: u64,
    pub log: Vec<LogEntry>,
    /// Calls to Roblox members Rook doesn't implement yet, by `Class.Member`.
    pub unimplemented: BTreeMap<String, u64>,
    /// Calls to members Rook accepts but ignores for now, by `Class.Member`.
    pub inert: BTreeMap<String, u64>,
    /// Threads that ended with an error.
    pub errors: u64,
    pub(crate) modules: ModuleState,
    /// Running scripts and the environment each runs in, so they don't
    /// start twice and stopping one can find its threads.
    pub(crate) started: HashMap<InstanceId, Table>,
    /// Whether any script has been stopped, so threads are only checked for
    /// a stopped owner once there can be one.
    pub(crate) any_stopped: bool,
    /// Sounds' and AudioPlayers' script state, for those a script has
    /// played or seeked.
    pub sounds: HashMap<InstanceId, SoundControl>,
    pub listener: Listener,
    /// Sounds given to `SoundService:PlayLocalSound`, for the engine to
    /// play once.
    pub local_sounds: Vec<InstanceId>,
    pub audio: AudioState,
    /// Effect methods called since the renderer last took them, in order.
    pub effect_calls: Vec<(InstanceId, EffectCall)>,
}

impl World {
    pub fn new(dom: Dom, clock: Clock) -> Self {
        Self {
            dom,
            signals: Signals::default(),
            scheduler: Scheduler::new(clock),
            signal_behavior: SignalBehavior::Immediate,
            is_client: false,
            input: InputState::default(),
            outbox: Vec::new(),
            content: Default::default(),
            assets: Default::default(),
            physics: rook_physics::Physics::new(),
            pending_calls: HashMap::new(),
            next_call: 0,
            log: Vec::new(),
            unimplemented: BTreeMap::new(),
            inert: BTreeMap::new(),
            errors: 0,
            modules: ModuleState::default(),
            started: HashMap::new(),
            any_stopped: false,
            sounds: HashMap::new(),
            listener: Listener::Camera,
            local_sounds: Vec::new(),
            audio: AudioState::default(),
            effect_calls: Vec::new(),
        }
    }

    pub fn push_log(&mut self, kind: MessageType, message: String) {
        self.push_log_from(kind, message, None);
    }

    pub fn push_log_from(&mut self, kind: MessageType, message: String, source: Option<String>) {
        let time = self.scheduler.now();
        self.log.push(LogEntry {
            kind,
            message,
            time,
            wall: std::time::SystemTime::now(),
            source,
        });
    }

    pub(crate) fn count_unimplemented(&mut self, what: String) {
        *self.unimplemented.entry(what).or_default() += 1;
    }

    /// Number of scripts started so far.
    pub fn started_count(&self) -> usize {
        self.started.len()
    }

    pub(crate) fn count_inert(&mut self, what: String) {
        *self.inert.entry(what).or_default() += 1;
    }
}
