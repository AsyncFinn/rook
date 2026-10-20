//! Which parts a query considers.

use super::*;

/// Which parts a query considers.
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Parts in or under these instances are skipped (or, with
    /// `include`, the only ones considered).
    pub instances: Vec<InstanceId>,
    pub include: bool,
    /// The collision group the query acts as: parts in groups that don't
    /// collide with it are skipped. `None` ignores groups (Region3).
    pub group: Option<String>,
    /// Skip parts with `CanCollide` off.
    pub respect_can_collide: bool,
    /// Skip terrain water (rays and shape casts).
    pub ignore_water: bool,
    /// Skip parts that are fully transparent with `CanCollide` off, as
    /// ClickDetector picking does (observed: such a part in front of a
    /// detector's part lets the click through, though `Mouse.Target`
    /// reports it).
    pub skip_invisible_ghosts: bool,
}

impl Filter {
    /// Legacy rays: they act as the `Default` group (observed: a part in
    /// a group that doesn't collide with `Default` is invisible to them).
    pub fn legacy_ray(ignore: Vec<InstanceId>, include: bool) -> Self {
        Self {
            instances: ignore,
            include,
            group: Some(groups::DEFAULT.to_owned()),
            respect_can_collide: false,
            ignore_water: false,
            skip_invisible_ghosts: false,
        }
    }

    /// Legacy regions ignore collision groups (observed).
    pub fn legacy_region(ignore: Vec<InstanceId>, include: bool) -> Self {
        Self {
            instances: ignore,
            include,
            group: None,
            respect_can_collide: false,
            ignore_water: false,
            skip_invisible_ghosts: false,
        }
    }

    pub(super) fn accepts(
        &self,
        dom: &Dom,
        groups: &groups::CollisionGroups,
        part: InstanceId,
    ) -> bool {
        let collides = dom.bool_property(part, "CanCollide").unwrap_or(true);
        if !collides
            && (self.respect_can_collide || !dom.bool_property(part, "CanQuery").unwrap_or(true))
        {
            return false;
        }
        if !collides
            && self.skip_invisible_ghosts
            && dom.float_property(part, "Transparency").unwrap_or(0.0) >= 1.0
        {
            return false;
        }
        if let Some(g) = &self.group
            && !groups.groups_collide(g, &groups::part_group(dom, part))
        {
            return false;
        }
        let listed = self
            .instances
            .iter()
            .any(|&i| i == part || dom.is_ancestor_of(i, part));
        listed == self.include
    }
}
