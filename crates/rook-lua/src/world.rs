//! Engine state shared by every Lua callback in one DataModel.

use std::collections::{BTreeMap, HashMap};

use mlua::{Function, Thread, Value as LuaValue};
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

/// A script's run state.
#[derive(Debug, Default)]
pub(crate) struct ModuleState {
    /// `require` results, by ModuleScript.
    pub(crate) results: HashMap<InstanceId, mlua::RegistryKey>,
    /// Modules currently loading, to detect cycles.
    pub(crate) loading: Vec<InstanceId>,
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
    /// Scripts that have been started, so they don't start twice.
    pub(crate) started: HashMap<InstanceId, Thread>,
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
            pending_calls: HashMap::new(),
            next_call: 0,
            log: Vec::new(),
            unimplemented: BTreeMap::new(),
            inert: BTreeMap::new(),
            errors: 0,
            modules: ModuleState::default(),
            started: HashMap::new(),
        }
    }

    pub fn push_log(&mut self, kind: MessageType, message: String) {
        let time = self.scheduler.now();
        self.log.push(LogEntry {
            kind,
            message,
            time,
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
