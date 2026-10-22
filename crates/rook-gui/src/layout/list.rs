//! UIListLayout, with its flex and wrapping and UIFlexItems, and the sort
//! order every UI layout uses (docs/behavior/gui/layout.md#uilistlayout,
//! docs/behavior/gui/modern.md#flex).

use glam::Vec2;
use rook_dom::{Dom, InstanceId};

use super::{Arranged, Arrangement, Kid, alignment, constraints};

/// The order `layout` places `kids` in, as indices. Measured: ties keep
/// child order, and Name compares bytes (`Z` before `a`).
pub(super) fn sorted(dom: &Dom, layout: InstanceId, kids: &[Kid]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..kids.len()).collect();
    match dom.enum_item_property(layout, "SortOrder") {
        Some("LayoutOrder") => {
            order.sort_by_key(|&i| {
                dom.number_property(kids[i].id, "LayoutOrder")
                    .unwrap_or(0.0) as i64
            });
        }
        Some("Name") => order.sort_by(|&a, &b| dom.name(kids[a].id).cmp(dom.name(kids[b].id))),
        // Custom, deprecated, is child order.
        _ => {}
    }
    order
}

/// A `UIFlexAlignment`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flex {
    None,
    Fill,
    SpaceAround,
    SpaceBetween,
    SpaceEvenly,
}

fn flex(dom: &Dom, layout: InstanceId, name: &str) -> Flex {
    match dom.enum_item_property(layout, name) {
        Some("Fill") => Flex::Fill,
        Some("SpaceAround") => Flex::SpaceAround,
        Some("SpaceBetween") => Flex::SpaceBetween,
        Some("SpaceEvenly") => Flex::SpaceEvenly,
        _ => Flex::None,
    }
}

/// Where a run of `count` things `room` pixels short of filling their
/// space starts, and the gap after each, `padding` apart unless `flex`
/// spreads them. Measured: SpaceAround puts half a share at each end,
/// SpaceEvenly a share, SpaceBetween none (one thing stays at the start);
/// with no room to spare, or without spreading, the run is aligned by
/// `share` of what's left.
fn spread(flex: Flex, count: usize, room: f32, padding: f32, share: f32) -> (f32, f32) {
    let n = count as f32;
    match flex {
        _ if room <= 0.0 || count == 0 => (room * share, padding),
        Flex::SpaceAround => (room / n * 0.5, padding + room / n),
        Flex::SpaceEvenly => (room / (n + 1.0), padding + room / (n + 1.0)),
        Flex::SpaceBetween if count > 1 => (0.0, padding + room / (n - 1.0)),
        Flex::SpaceBetween => (0.0, padding),
        Flex::None | Flex::Fill => (room * share, padding),
    }
}

/// A kid's grow and shrink ratios. Measured: a UIFlexItem's `FlexMode`
/// Grow is (1, 0), Shrink (0, 1), Fill (1, 1), Custom its `GrowRatio` and
/// `ShrinkRatio`, None (0, 0) even when the list fills; a kid without one
/// is (1, 1) when the list fills along its direction, (0, 0) otherwise.
fn ratios(dom: &Dom, item: Option<InstanceId>, fills: bool) -> (f32, f32) {
    let Some(item) = item else {
        return if fills { (1.0, 1.0) } else { (0.0, 0.0) };
    };
    match dom.enum_item_property(item, "FlexMode") {
        Some("Grow") => (1.0, 0.0),
        Some("Shrink") => (0.0, 1.0),
        Some("Fill") => (1.0, 1.0),
        Some("Custom") => (
            dom.float_property(item, "GrowRatio")
                .unwrap_or(0.0)
                .max(0.0),
            dom.float_property(item, "ShrinkRatio")
                .unwrap_or(0.0)
                .max(0.0),
        ),
        _ => (0.0, 0.0),
    }
}

/// How a kid sits across its line: a share of the room left, or
/// stretched to the line.
#[derive(Debug, Clone, Copy)]
enum Across {
    Share(f32),
    Stretch,
}

/// Measured: a UIFlexItem's `ItemLineAlignment` wins over the list's
/// unless Automatic; Automatic stretches when the list fills across, and
/// otherwise aligns by the list's alignment across (`share`).
fn across(
    dom: &Dom,
    layout: InstanceId,
    item: Option<InstanceId>,
    fill: bool,
    share: f32,
) -> Across {
    let own = item.and_then(|i| dom.enum_item_property(i, "ItemLineAlignment"));
    let line = match own {
        Some(a) if a != "Automatic" => Some(a),
        _ => dom.enum_item_property(layout, "ItemLineAlignment"),
    };
    match line {
        Some("Start") => Across::Share(0.0),
        Some("Center") => Across::Share(0.5),
        Some("End") => Across::Share(1.0),
        Some("Stretch") => Across::Stretch,
        _ if fill => Across::Stretch,
        _ => Across::Share(share),
    }
}

/// The lengths along a line of kids `base` long, flexed by their `ratios`
/// to fill `length`, `padding` apart, each held by its size constraints
/// (`clamp`). Measured: room to spare is shared by grow ratio; room
/// lacking is taken by shrink ratio times length; a kid its constraints
/// stop keeps that length and the rest is shared again among the others.
fn flexed(
    base: &[f32],
    ratios: &[(f32, f32)],
    length: f32,
    padding: f32,
    clamp: impl Fn(usize, f32) -> f32,
) -> Vec<f32> {
    let n = base.len();
    let mut sizes = base.to_vec();
    let mut frozen = vec![false; n];
    let gaps = padding * n.saturating_sub(1) as f32;
    for _ in 0..=n {
        let used: f32 = (0..n)
            .map(|i| if frozen[i] { sizes[i] } else { base[i] })
            .sum();
        let room = length - gaps - used;
        let weight = |i: usize| {
            if room > 0.0 {
                ratios[i].0
            } else {
                ratios[i].1 * base[i]
            }
        };
        let total: f32 = (0..n).filter(|&i| !frozen[i]).map(weight).sum();
        if total <= 0.0 || room == 0.0 {
            break;
        }
        let mut stopped = false;
        for i in 0..n {
            if frozen[i] {
                continue;
            }
            let want = (base[i] + room * weight(i) / total).max(0.0);
            let held = clamp(i, want);
            sizes[i] = held;
            if held != want {
                frozen[i] = true;
                stopped = true;
            }
        }
        if !stopped {
            break;
        }
    }
    sizes
}

/// Measured: items follow each other along `FillDirection`, `Padding`
/// apart (its scale against the area's length that way), in lines when
/// `Wraps` (an item starts a new line when it wouldn't fit, lines
/// `Padding` apart too); each line is flexed and aligned along on its own,
/// as wide as its widest item across, and the lines are placed across by
/// the cross flex or the alignment that way. Children's `Position` and
/// `AnchorPoint` don't count. `AbsoluteContentSize` is the longest line
/// and the lines across, without the room spreading adds.
pub(super) fn arrange(dom: &Dom, layout: InstanceId, kids: &[Kid], inner: Vec2) -> Arrangement {
    let vertical = dom.enum_item_property(layout, "FillDirection") != Some("Horizontal");
    let pick = |v: Vec2| if vertical { (v.y, v.x) } else { (v.x, v.y) };
    let unpick = |along: f32, across: f32| {
        if vertical {
            Vec2::new(across, along)
        } else {
            Vec2::new(along, across)
        }
    };
    let (length, width) = pick(inner);
    let (along_share, across_share) = pick(alignment(dom, layout));
    let (main, cross) = if vertical {
        (
            flex(dom, layout, "VerticalFlex"),
            flex(dom, layout, "HorizontalFlex"),
        )
    } else {
        (
            flex(dom, layout, "HorizontalFlex"),
            flex(dom, layout, "VerticalFlex"),
        )
    };
    let padding = dom
        .udim_property(layout, "Padding")
        .map_or(0.0, |u| u.resolve(length));
    let items: Vec<Option<InstanceId>> = (kids.iter())
        .map(|k| dom.find_first_child_of_class(k.id, "UIFlexItem"))
        .collect();
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut run = 0.0;
    let wraps = dom.bool_property(layout, "Wraps") == Some(true);
    for (i, k) in kids.iter().enumerate() {
        let along = pick(k.size).0;
        let line = lines.last_mut().expect("one at least");
        if wraps && !line.is_empty() && run + padding + along > length {
            lines.push(vec![i]);
            run = along;
        } else {
            run += if line.is_empty() {
                along
            } else {
                padding + along
            };
            line.push(i);
        }
    }
    let mut sizes: Vec<Vec2> = kids.iter().map(|k| k.size).collect();
    let mut along_at = vec![0.0; kids.len()];
    let mut longest: f32 = 0.0;
    let mut thickness = Vec::with_capacity(lines.len());
    for line in &lines {
        let base: Vec<f32> = line.iter().map(|&i| pick(kids[i].size).0).collect();
        let r: Vec<(f32, f32)> = (line.iter())
            .map(|&i| ratios(dom, items[i], main == Flex::Fill))
            .collect();
        let clamp = |n: usize, along: f32| {
            let i = line[n];
            let (_, across) = pick(kids[i].size);
            pick(constraints::apply(
                dom,
                kids[i].id,
                unpick(along, across),
                inner,
            ))
            .0
        };
        let lengths = flexed(&base, &r, length, padding, clamp);
        let used = lengths.iter().sum::<f32>() + padding * line.len().saturating_sub(1) as f32;
        let (mut at, gap) = spread(main, line.len(), length - used, padding, along_share);
        for (&i, &l) in line.iter().zip(&lengths) {
            sizes[i] = unpick(l, pick(sizes[i]).1);
            along_at[i] = at;
            at += l + gap;
        }
        longest = longest.max(used);
        thickness.push(line.iter().map(|&i| pick(sizes[i]).1).fold(0.0, f32::max));
    }
    let count = lines.len();
    let spare = width - thickness.iter().sum::<f32>() - padding * count.saturating_sub(1) as f32;
    if cross == Flex::Fill && spare > 0.0 {
        for t in &mut thickness {
            *t += spare / count as f32;
        }
    }
    let room = if cross == Flex::Fill { 0.0 } else { spare };
    let (mut line_at, line_gap) = spread(cross, count, room, padding, across_share);
    let mut slots = vec![Vec2::ZERO; kids.len()];
    for (line, &thick) in lines.iter().zip(&thickness) {
        for &i in line {
            let fill = cross == Flex::Fill;
            let (along, mut wide) = pick(sizes[i]);
            let offset = match across(dom, layout, items[i], fill, across_share) {
                Across::Share(s) => (thick - wide) * s,
                Across::Stretch => {
                    wide = thick;
                    0.0
                }
            };
            sizes[i] = unpick(along, wide);
            slots[i] = unpick(along_at[i], line_at + offset);
        }
        line_at += thick + line_gap;
    }
    let across_total = thickness.iter().sum::<f32>() + padding * count.saturating_sub(1) as f32;
    Arrangement {
        slots,
        sizes,
        arranged: Arranged {
            content: unpick(longest, across_total),
            cells: None,
        },
    }
}
