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
    /// Skip parts more transparent than this.
    pub max_transparency: Option<f32>,
    /// `ExcludeInstances`: parts in or under these are skipped, whatever the
    /// other lists say (measured in Studio 0.748: exclusion wins).
    pub exclude: FxHashSet<InstanceId>,
    /// `IncludeInstances`: when set, only parts in or under these are
    /// considered, on top of the legacy list (the lists intersect; an empty
    /// set considers nothing, `None` everything).
    pub only: Option<FxHashSet<InstanceId>>,
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
            max_transparency: None,
            ..Default::default()
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
            max_transparency: None,
            ..Default::default()
        }
    }
}

/// Whether `part` is fully transparent with `CanCollide` off: what
/// [`Filter::skip_invisible_ghosts`] skips.
pub(super) fn is_invisible_ghost(dom: &Dom, props: &Props, part: InstanceId) -> bool {
    dom.property(part, props.can_collide) == Some(Value::Bool(false))
        && transparency(dom, props, part) >= 1.0
}

fn transparency(dom: &Dom, props: &Props, part: InstanceId) -> f32 {
    (dom.property(part, props.transparency))
        .and_then(|t| t.as_number())
        .unwrap_or(0.0) as f32
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
        if filter.only.as_ref().is_some_and(FxHashSet::is_empty) {
            return false;
        }
        let off = |property| dom.property(part, property) == Some(Value::Bool(false));
        let collides = !off(props.can_collide);
        if !collides && (filter.respect_can_collide || off(props.can_query)) {
            return false;
        }
        if filter.skip_invisible_ghosts && is_invisible_ghost(dom, props, part) {
            return false;
        }
        if let Some(max) = filter.max_transparency
            && transparency(dom, props, part) > max
        {
            return false;
        }
        if let Some(group) = self.group {
            let of_part = self.groups.part_id(dom, props.collision_group, part);
            if !self.groups.ids_collide(group, of_part) {
                return false;
            }
        }
        // One walk up from the part answers all three lists.
        let (mut listed, mut excluded) = (false, false);
        let mut included = filter.only.is_none();
        let lists = !filter.instances.is_empty() || !filter.exclude.is_empty() || !included;
        if lists {
            for i in std::iter::once(part).chain(dom.ancestors(part)) {
                listed |= filter.instances.contains(&i);
                excluded |= filter.exclude.contains(&i);
                included |= filter.only.as_ref().is_some_and(|only| only.contains(&i));
                if excluded {
                    return false;
                }
            }
        }
        listed == filter.include && included
    }
}
