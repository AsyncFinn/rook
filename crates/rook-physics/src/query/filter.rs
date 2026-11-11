use rook_types::Value;

use super::*;
use crate::Props;

#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// Parts in or under these instances are skipped (or, with
    /// `include`, the only ones considered). A set, so a part is looked
    /// up by walking its own ancestors, however long the list.
    pub instances: FxHashSet<InstanceId>,
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
    pub fn legacy_ray(ignore: impl IntoIterator<Item = InstanceId>, include: bool) -> Self {
        Self {
            instances: ignore.into_iter().collect(),
            include,
            group: Some(groups::DEFAULT.to_owned()),
            respect_can_collide: false,
            ignore_water: false,
            skip_invisible_ghosts: false,
        }
    }

    /// Legacy regions ignore collision groups (observed).
    pub fn legacy_region(ignore: impl IntoIterator<Item = InstanceId>, include: bool) -> Self {
        Self {
            instances: ignore.into_iter().collect(),
            include,
            group: None,
            respect_can_collide: false,
            ignore_water: false,
            skip_invisible_ghosts: false,
        }
    }
}

/// A [`Filter`] for one query. A query asks it of every part it passes, so
/// what doesn't depend on the part (the properties' ids, the group the
/// query acts as) is found once here.
pub(super) struct Accepting<'a> {
    filter: &'a Filter,
    dom: &'a Dom,
    props: &'a Props,
    groups: &'a groups::CollisionGroups,
    /// The id of the group the query acts as.
    group: Option<u8>,
}

impl<'a> Accepting<'a> {
    pub(super) fn new(
        filter: &'a Filter,
        dom: &'a Dom,
        props: &'a Props,
        groups: &'a groups::CollisionGroups,
    ) -> Self {
        Self {
            filter,
            dom,
            props,
            groups,
            group: filter.group.as_deref().map(|g| groups.id_of(g)),
        }
    }

    pub(super) fn accepts(&self, part: InstanceId) -> bool {
        let (filter, dom, props) = (self.filter, self.dom, self.props);
        let off = |property| dom.property(part, property) == Some(Value::Bool(false));
        let collides = !off(props.can_collide);
        if !collides && (filter.respect_can_collide || off(props.can_query)) {
            return false;
        }
        if !collides
            && filter.skip_invisible_ghosts
            && (dom.property(part, props.transparency))
                .and_then(|t| t.as_number())
                .unwrap_or(0.0)
                >= 1.0
        {
            return false;
        }
        if let Some(group) = self.group {
            let of_part = self.groups.part_id(dom, props.collision_group, part);
            if !self.groups.ids_collide(group, of_part) {
                return false;
            }
        }
        let listed = !filter.instances.is_empty()
            && (std::iter::once(part).chain(dom.ancestors(part)))
                .any(|i| filter.instances.contains(&i));
        listed == filter.include
    }
}
