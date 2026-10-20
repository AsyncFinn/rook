//! Collision groups, stored where Roblox saves them: Workspace's hidden
//! `CollisionGroupData` property. PhysicsService edits it and physics
//! reads it, so it is the one record of the groups.
//!
//! Format (read from saved places): a version byte (1), the group count,
//! then per group its id, the mask's byte length (4), the mask as a
//! little-endian i32 (bit n set: collides with group n) and the name as a
//! length byte and UTF-8.
//!
//! Behavior observed in Studio 0.742: `Default` is id 0 and can't be
//! changed; new groups take the lowest free id; there are at most 32;
//! unregistering a group lets every other group collide with its id again.

use rook_dom::{Dom, DomError, InstanceId};
use rook_types::{BinaryString, Value};

/// At most this many groups, `Default` included (and Rapier's group bits).
pub const MAX_GROUPS: usize = 32;

pub const DEFAULT: &str = "Default";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub id: u8,
    pub mask: i32,
    pub name: String,
}

/// A failed PhysicsService call, with Roblox's message.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GroupError {
    #[error("Can not use/change the Default collision group.")]
    Default,
    #[error("Collision group requires a valid name.")]
    EmptyName,
    #[error("Collision group name length cannot exceed 100 chars")]
    LongName,
    #[error("Cannot register more than 32 collision groups in this world.")]
    Full,
    #[error("Both collision groups must be registered.")]
    NotRegistered,
    #[error("Could not register collision group, the name already exists.")]
    Exists,
    #[error("Collision group not found.")]
    NotFound,
}

/// The registered groups, in id order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CollisionGroups(pub Vec<Group>);

impl Default for CollisionGroups {
    fn default() -> Self {
        Self(vec![Group {
            id: 0,
            mask: -1,
            name: DEFAULT.to_owned(),
        }])
    }
}

impl CollisionGroups {
    /// The groups saved on Workspace. Places that only have the older
    /// `CollisionGroups` string get the default table (Approximation; the
    /// archive's samples of it list only `Default`).
    pub fn read(dom: &Dom) -> Self {
        dom.find_service("Workspace")
            .and_then(|ws| dom.binary_property(ws, "CollisionGroupData"))
            .and_then(|b| Self::decode(&b))
            .unwrap_or_default()
    }

    pub fn write(&self, dom: &mut Dom) -> Result<(), DomError> {
        let Some(ws) = dom.find_service("Workspace") else {
            return Ok(());
        };
        let data = Value::BinaryString(BinaryString::from(self.encode()));
        dom.set_property_by_name(ws, "CollisionGroupData", data)
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let mut it = bytes.iter().copied();
        if it.next()? != 1 {
            return None;
        }
        let count = it.next()?;
        let mut groups = Vec::with_capacity(count.into());
        for _ in 0..count {
            let id = it.next()?;
            if it.next()? != 4 {
                return None;
            }
            let mask = i32::from_le_bytes([it.next()?, it.next()?, it.next()?, it.next()?]);
            let len = it.next()?;
            let name: Vec<u8> = (0..len).map(|_| it.next()).collect::<Option<_>>()?;
            groups.push(Group {
                id,
                mask,
                name: String::from_utf8_lossy(&name).into_owned(),
            });
        }
        groups.sort_by_key(|g| g.id);
        Some(Self(groups))
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![1, self.0.len() as u8];
        for g in &self.0 {
            out.push(g.id);
            out.push(4);
            out.extend(g.mask.to_le_bytes());
            out.push(g.name.len() as u8);
            out.extend(g.name.as_bytes());
        }
        out
    }

    pub fn get(&self, name: &str) -> Option<&Group> {
        self.0.iter().find(|g| g.name == name)
    }

    /// The group parts named `name` simulate in; unknown names (which a
    /// part's `CollisionGroup` may hold) act as `Default` (Inference).
    pub fn id_of(&self, name: &str) -> u8 {
        self.get(name).map_or(0, |g| g.id)
    }

    /// The mask of the group with this id.
    pub fn mask_of(&self, id: u8) -> i32 {
        self.0.iter().find(|g| g.id == id).map_or(-1, |g| g.mask)
    }

    /// `RegisterCollisionGroup`; registering an existing name does nothing.
    pub fn register(&mut self, name: &str) -> Result<(), GroupError> {
        check_name(name)?;
        if self.get(name).is_some() {
            return Ok(());
        }
        if self.0.len() >= MAX_GROUPS {
            return Err(GroupError::Full);
        }
        let id = (0..MAX_GROUPS as u8)
            .find(|i| self.0.iter().all(|g| g.id != *i))
            .ok_or(GroupError::Full)?;
        self.0.push(Group {
            id,
            mask: -1,
            name: name.to_owned(),
        });
        self.0.sort_by_key(|g| g.id);
        Ok(())
    }

    /// `UnregisterCollisionGroup`; an unknown name does nothing.
    pub fn unregister(&mut self, name: &str) -> Result<(), GroupError> {
        if name == DEFAULT {
            return Err(GroupError::Default);
        }
        let Some(pos) = self.0.iter().position(|g| g.name == name) else {
            return Ok(());
        };
        let id = self.0.remove(pos).id;
        for g in &mut self.0 {
            g.mask |= 1 << id;
        }
        Ok(())
    }

    /// `RenameCollisionGroup`; an unknown name, or a new name already in
    /// use, does nothing.
    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), GroupError> {
        if from == DEFAULT || to == DEFAULT {
            return Err(GroupError::Default);
        }
        check_name(to)?;
        if self.get(to).is_some() {
            return Ok(());
        }
        if let Some(g) = self.0.iter_mut().find(|g| g.name == from) {
            g.name = to.to_owned();
        }
        Ok(())
    }

    /// `CollisionGroupSetCollidable`, applied both ways.
    pub fn set_collidable(&mut self, a: &str, b: &str, collides: bool) -> Result<(), GroupError> {
        let (Some(ia), Some(ib)) = (self.get(a).map(|g| g.id), self.get(b).map(|g| g.id)) else {
            return Err(GroupError::NotRegistered);
        };
        for (g, other) in [(ia, ib), (ib, ia)] {
            let group = self.0.iter_mut().find(|x| x.id == g).expect("registered");
            if collides {
                group.mask |= 1 << other;
            } else {
                group.mask &= !(1 << other);
            }
        }
        Ok(())
    }

    /// Whether parts in groups `a` and `b` collide. Unknown names act as
    /// `Default`, as for parts (observed for `RaycastParams.CollisionGroup`).
    pub fn groups_collide(&self, a: &str, b: &str) -> bool {
        let (ia, ib) = (self.id_of(a), self.id_of(b));
        self.mask_of(ia) & (1 << ib) != 0 && self.mask_of(ib) & (1 << ia) != 0
    }

    /// `CollisionGroupsAreCollidable`; unknown groups collide.
    pub fn collidable(&self, a: &str, b: &str) -> bool {
        match (self.get(a), self.get(b)) {
            (Some(a), Some(b)) => a.mask & (1 << b.id) != 0 && b.mask & (1 << a.id) != 0,
            _ => true,
        }
    }
}

fn check_name(name: &str) -> Result<(), GroupError> {
    if name.is_empty() {
        Err(GroupError::EmptyName)
    } else if name.len() > 100 {
        Err(GroupError::LongName)
    } else {
        Ok(())
    }
}

/// A part's `CollisionGroup` name.
pub fn part_group(dom: &Dom, part: InstanceId) -> String {
    dom.string_property(part, "CollisionGroup")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT.to_owned())
}
