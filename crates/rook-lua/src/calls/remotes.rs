//! RemoteEvents and RemoteFunctions: the outbox, and delivering events,
//! invokes and replies.

use rook_dom::InstanceId;

use crate::runtime::signal::SignalKey;
use crate::{NetValue, RemoteMessage, Runtime};

impl Runtime {
    /// Remote calls made since the last call, for the session to deliver.
    pub fn take_outbox(&self) -> Vec<RemoteMessage> {
        std::mem::take(&mut self.world_mut().outbox)
    }

    /// Deliver a RemoteEvent firing: `OnServerEvent(player, ...)` on a
    /// server, `OnClientEvent(...)` on a client.
    pub fn deliver_event(&self, remote: InstanceId, player: Option<InstanceId>, args: &[NetValue]) {
        let event = if self.world().is_client {
            "OnClientEvent"
        } else {
            "OnServerEvent"
        };
        let lua = &self.lua;
        let mut values = Vec::with_capacity(args.len() + 1);
        if let Some(p) = player {
            match crate::api::instance::push_instance(lua, p) {
                Ok(ud) => values.push(mlua::Value::UserData(ud)),
                Err(_) => return,
            }
        }
        for a in args {
            values.push(crate::net::from_net(lua, a).unwrap_or(mlua::Value::Nil));
        }
        // Chat messages travel as TextChatService's own firings.
        let chat = self.world().dom.class_name(remote) == "TextChatService";
        self.guarded(|| {
            if chat {
                let handler = if self.world().is_client {
                    "onClient"
                } else {
                    "onServer"
                };
                let api = lua
                    .named_registry_value::<mlua::Table>("rook.std")
                    .and_then(|std| std.get::<mlua::Table>("text_chat"))
                    .and_then(|api| api.get::<mlua::Function>(handler));
                if let Ok(f) = api {
                    // Errors are the thread's to report.
                    let _ =
                        crate::runtime::threads::spawn(lua, f, mlua::MultiValue::from_vec(values));
                }
            } else {
                crate::runtime::threads::fire(lua, &SignalKey::event(remote, event), &values);
            }
            crate::runtime::threads::drain_deferred(lua);
        });
    }

    /// Run an incoming invoke against the remote's callback; the reply goes
    /// to this DataModel's outbox.
    pub fn deliver_invoke(
        &self,
        remote: InstanceId,
        player: Option<InstanceId>,
        call: u64,
        args: &[NetValue],
    ) {
        let lua = &self.lua;
        let callback = if self.world().is_client {
            "OnClientInvoke"
        } else {
            "OnServerInvoke"
        };
        let Ok(runner) = lua.named_registry_value::<mlua::Function>("rook.remote_runner") else {
            return;
        };
        let Ok(remote_ud) = crate::api::instance::push_instance(lua, remote) else {
            return;
        };
        let mut values = vec![
            mlua::Value::UserData(remote_ud),
            mlua::Value::String(lua.create_string(callback).expect("short string")),
            mlua::Value::Number(call as f64),
        ];
        if let Some(p) = player
            && let Ok(ud) = crate::api::instance::push_instance(lua, p)
        {
            values.push(mlua::Value::UserData(ud));
        }
        for a in args {
            values.push(crate::net::from_net(lua, a).unwrap_or(mlua::Value::Nil));
        }
        self.guarded(|| {
            // The thread reports its own errors; starting it fails only
            // out of memory, which Luau has reported already.
            let _ = crate::runtime::threads::spawn(lua, runner, mlua::MultiValue::from_vec(values));
            crate::runtime::threads::drain_deferred(lua);
        });
    }

    /// Resume the thread waiting on `call` with `(ok, results)`.
    pub fn deliver_reply(&self, call: u64, ok: bool, args: &[NetValue]) {
        let lua = &self.lua;
        let Some(thread) = self.world_mut().pending_calls.remove(&call) else {
            return;
        };
        let packed = (|| -> mlua::Result<mlua::Value> {
            if !ok {
                return crate::net::from_net(lua, args.first().unwrap_or(&NetValue::Nil));
            }
            let t = lua.create_table()?;
            for (i, a) in args.iter().enumerate() {
                t.raw_set(i + 1, crate::net::from_net(lua, a)?)?;
            }
            t.raw_set("n", args.len())?;
            Ok(mlua::Value::Table(t))
        })()
        .unwrap_or(mlua::Value::Nil);
        self.guarded(|| {
            crate::runtime::threads::resume(lua, &thread, (ok, packed));
            crate::runtime::threads::drain_deferred(lua);
        });
    }
}
