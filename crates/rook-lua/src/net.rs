//! Values crossing between DataModels (RemoteEvent/RemoteFunction arguments).
//!
//! Following creator-docs "Remote events and callbacks" (argument limits):
//! tables are copied (no metatables; functions and other non-transferable
//! values become nil), instances arrive as the receiver's copy or nil if the
//! receiver can't see them, and datatypes are copied by value. Mixed tables
//! keep their entries but Roblox's exact key conversion rules are
//! Inference.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_reflection::EnumId;
use rook_types::Value;

use crate::api::instance::{LuaInstance, push_instance};
use crate::convert::{datatype_value, to_lua};
use crate::types::enums::{LEnumItem, push_item};

#[derive(Debug, Clone, PartialEq)]
pub enum NetValue {
    Nil,
    Bool(bool),
    Number(f64),
    String(Vec<u8>),
    Vector3([f32; 3]),
    /// An instance, by its id in the DataModel that currently holds the value.
    Instance(InstanceId),
    EnumItem(EnumId, u32),
    Data(Value),
    Table(Vec<(NetValue, NetValue)>),
}

impl NetValue {
    /// Translate instance ids into the receiving DataModel.
    pub fn map_instances(&mut self, f: &dyn Fn(InstanceId) -> Option<InstanceId>) {
        match self {
            NetValue::Instance(id) => {
                *self = match f(*id) {
                    Some(mapped) => NetValue::Instance(mapped),
                    None => NetValue::Nil,
                };
            }
            NetValue::Table(entries) => {
                for (k, v) in entries.iter_mut() {
                    k.map_instances(f);
                    v.map_instances(f);
                }
                entries.retain(|(k, _)| *k != NetValue::Nil);
            }
            _ => {}
        }
    }
}

pub(crate) fn to_net(v: &LuaValue, depth: u32) -> NetValue {
    match v {
        LuaValue::Nil => NetValue::Nil,
        LuaValue::Boolean(b) => NetValue::Bool(*b),
        LuaValue::Integer(i) => NetValue::Number(*i as f64),
        LuaValue::Number(n) => NetValue::Number(*n),
        LuaValue::String(s) => NetValue::String(s.as_bytes().to_vec()),
        LuaValue::Vector(v) => NetValue::Vector3([v.x(), v.y(), v.z()]),
        LuaValue::Table(t) if depth < 64 => NetValue::Table(
            t.clone()
                .pairs::<LuaValue, LuaValue>()
                .filter_map(Result::ok)
                .map(|(k, v)| (to_net(&k, depth + 1), to_net(&v, depth + 1)))
                .filter(|(k, v)| *k != NetValue::Nil && *v != NetValue::Nil)
                .collect(),
        ),
        LuaValue::UserData(u) => {
            if let Ok(i) = u.borrow::<LuaInstance>() {
                return NetValue::Instance(i.0);
            }
            if let Ok(e) = u.borrow::<LEnumItem>() {
                return NetValue::EnumItem(e.enum_id, e.value);
            }
            datatype_value(u).map_or(NetValue::Nil, NetValue::Data)
        }
        _ => NetValue::Nil,
    }
}

pub(crate) fn from_net(lua: &Lua, v: &NetValue) -> mlua::Result<LuaValue> {
    Ok(match v {
        NetValue::Nil => LuaValue::Nil,
        NetValue::Bool(b) => LuaValue::Boolean(*b),
        NetValue::Number(n) => LuaValue::Number(*n),
        NetValue::String(s) => LuaValue::String(lua.create_string(s)?),
        NetValue::Vector3([x, y, z]) => LuaValue::Vector(mlua::Vector::new(*x, *y, *z)),
        NetValue::Instance(id) => LuaValue::UserData(push_instance(lua, *id)?),
        NetValue::EnumItem(e, value) => push_item(lua, *e, *value)?,
        NetValue::Data(value) => to_lua(lua, value, None)?,
        NetValue::Table(entries) => {
            let t = lua.create_table()?;
            for (k, v) in entries {
                t.raw_set(from_net(lua, k)?, from_net(lua, v)?)?;
            }
            LuaValue::Table(t)
        }
    })
}

/// What a remote call asks the session to deliver.
#[derive(Debug, Clone, PartialEq)]
pub enum RemoteKind {
    /// Client → server `OnServerEvent`.
    FireServer,
    /// Server → one client's `OnClientEvent`.
    FireClient { player: InstanceId },
    /// Server → every client's `OnClientEvent`.
    FireAllClients,
    /// Client → server `OnServerInvoke`; the reply resumes `call`.
    InvokeServer { call: u64 },
    /// Server → client `OnClientInvoke`.
    InvokeClient { player: InstanceId, call: u64 },
    /// The answer to an invoke from the other side. If `ok` is false the
    /// first argument is the error message.
    Reply { call: u64, ok: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteMessage {
    /// The remote, in the sender's DataModel (unused for replies).
    pub remote: Option<InstanceId>,
    pub kind: RemoteKind,
    pub args: Vec<NetValue>,
}
