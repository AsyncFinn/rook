//! The part and Workspace properties physics watches.

use super::*;

/// The physics properties [`Physics`] watches, resolved once.
#[derive(Debug)]
pub(crate) struct Props {
    pub(crate) cframe: PropertyId,
    pub(crate) size: PropertyId,
    anchored: PropertyId,
    pub(crate) can_collide: PropertyId,
    massless: PropertyId,
    root_priority: PropertyId,
    material: PropertyId,
    custom_properties: PropertyId,
    collision_group: PropertyId,
    pub(crate) can_touch: PropertyId,
    shape: Option<PropertyId>,
    pub(crate) linear_velocity: PropertyId,
    pub(crate) angular_velocity: PropertyId,
    pub(crate) gravity: PropertyId,
    pub(crate) fallen_height: PropertyId,
    group_data: PropertyId,
    pub(crate) touches_use_groups: PropertyId,
    /// Each face's ParamA, ParamB and SurfaceInput, which drive RotateV
    /// and RotateP.
    pub(crate) surface_inputs: Vec<PropertyId>,
}

impl Props {
    pub(crate) fn resolve(dom: &Dom) -> Option<Self> {
        let db = dom.database();
        let base = db.class_by_name("BasePart")?.id;
        let part = db.class_by_name("Part")?.id;
        let ws = db.class_by_name("Workspace")?.id;
        let p = |class, name: &str| db.find_canonical_property(class, name).map(|p| p.id);
        Some(Self {
            cframe: p(base, "CFrame")?,
            size: p(base, "Size")?,
            anchored: p(base, "Anchored")?,
            can_collide: p(base, "CanCollide")?,
            massless: p(base, "Massless")?,
            root_priority: p(base, "RootPriority")?,
            material: p(base, "Material")?,
            custom_properties: p(base, "CustomPhysicalProperties")?,
            collision_group: p(base, "CollisionGroup")?,
            can_touch: p(base, "CanTouch")?,
            shape: p(part, "Shape"),
            linear_velocity: p(base, "AssemblyLinearVelocity")?,
            angular_velocity: p(base, "AssemblyAngularVelocity")?,
            gravity: p(ws, "Gravity")?,
            fallen_height: p(ws, "FallenPartsDestroyHeight")?,
            group_data: p(ws, "CollisionGroupData")?,
            touches_use_groups: p(ws, "TouchesUseCollisionGroups")?,
            surface_inputs: ["Top", "Bottom", "Left", "Right", "Front", "Back"]
                .iter()
                .flat_map(|face| {
                    ["ParamA", "ParamB", "SurfaceInput"]
                        .map(|input| p(base, &format!("{face}{input}")))
                })
                .collect::<Option<_>>()?,
        })
    }

    /// Changes that alter which bodies and colliders exist, or which
    /// colliders interact (`CanCollide` and `CollisionGroup` change contact
    /// pairs, and with them touch events, so they rebuild too).
    pub(crate) fn is_structural(&self, p: PropertyId) -> bool {
        p == self.size
            || p == self.anchored
            || p == self.can_collide
            || p == self.massless
            || p == self.root_priority
            || p == self.collision_group
            || Some(p) == self.shape
    }

    /// Changes to a collider's mass, friction, elasticity or touchability
    /// alone, which update it in place.
    pub(crate) fn is_surface(&self, p: PropertyId) -> bool {
        p == self.material || p == self.custom_properties || p == self.can_touch
    }

    /// Workspace properties physics reads.
    pub(crate) fn is_workspace(&self, p: PropertyId) -> bool {
        p == self.gravity
            || p == self.fallen_height
            || p == self.group_data
            || p == self.touches_use_groups
    }

    /// Changes that move a body.
    pub(crate) fn is_pose(&self, p: PropertyId) -> bool {
        p == self.cframe || p == self.linear_velocity || p == self.angular_velocity
    }
}
