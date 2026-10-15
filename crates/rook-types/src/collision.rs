//! A union's collision geometry: decoded by `rook-content` from its
//! `PhysicsData` (or its drawn geometry, when none was saved), and turned
//! into colliders by `rook-physics`.

/// A triangle mesh in a union's own space: the space of its `MeshData`,
/// at its `InitialSize`, centred on its origin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hull {
    pub positions: Vec<[f32; 3]>,
    /// Three vertex indices per triangle.
    pub indices: Vec<u32>,
}

/// What a union collides and is queried as.
#[derive(Debug, Clone, PartialEq)]
pub enum UnionCollision {
    /// Its box (`PhysicsData` `CSGPHS` `BLOCK`).
    Block,
    /// Convex pieces, as Studio saved them.
    Hulls(Vec<Hull>),
    /// No collision geometry was saved: the drawn geometry, from which
    /// physics makes its own.
    Mesh(Hull),
}
