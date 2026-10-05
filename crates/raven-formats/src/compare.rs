//! Structural comparison of two trees, used to verify round trips.
//!
//! Instances are matched by position (same parent, same child index), so a
//! reordering shows up as a difference. Referent properties are compared by
//! which instance they point to, not by their raw IDs.

use std::collections::HashMap;

use rbx_dom_weak::WeakDom;
use rbx_types::{Ref, Variant};

#[derive(Debug, Clone, PartialEq)]
pub enum Difference {
    ClassChanged {
        path: String,
        from: String,
        to: String,
    },
    NameChanged {
        path: String,
        from: String,
        to: String,
    },
    ChildCountChanged {
        path: String,
        from: usize,
        to: usize,
    },
    PropertyMissing {
        path: String,
        property: String,
    },
    PropertyAdded {
        path: String,
        property: String,
    },
    PropertyChanged {
        path: String,
        property: String,
        from: String,
        to: String,
    },
}

/// Options for [`compare_with`].
#[derive(Debug, Clone, Copy, Default)]
pub struct CompareOptions {
    /// Relative tolerance for float components. `None` means exact.
    ///
    /// Roblox's XML writer stores fewer significant digits than the binary
    /// format, so binary-versus-XML comparisons need a small tolerance.
    pub float_tolerance: Option<f32>,
}

/// Compare the trees under each DOM's root exactly. An empty result means they match.
pub fn compare(a: &WeakDom, b: &WeakDom) -> Vec<Difference> {
    compare_with(a, b, CompareOptions::default())
}

/// Compare the trees under each DOM's root with the given options.
pub fn compare_with(a: &WeakDom, b: &WeakDom, options: CompareOptions) -> Vec<Difference> {
    // First pass: pair instances by position.
    let mut pairs = Vec::new();
    let mut diffs = Vec::new();
    pair_children(
        a,
        b,
        a.root_ref(),
        b.root_ref(),
        String::new(),
        &mut pairs,
        &mut diffs,
    );
    let ref_map: HashMap<Ref, Ref> = pairs.iter().map(|(ra, rb, _)| (*ra, *rb)).collect();

    // Second pass: compare properties of paired instances.
    for (ra, rb, path) in &pairs {
        let ia = a.get_by_ref(*ra).expect("paired ref exists");
        let ib = b.get_by_ref(*rb).expect("paired ref exists");
        let mut keys: Vec<_> = ia.properties.keys().chain(ib.properties.keys()).collect();
        keys.sort_unstable_by(|x, y| x.as_str().cmp(y.as_str()));
        keys.dedup();
        for key in keys {
            match (ia.properties.get(key), ib.properties.get(key)) {
                (Some(_), None) => diffs.push(Difference::PropertyMissing {
                    path: path.clone(),
                    property: key.to_string(),
                }),
                (None, Some(_)) => diffs.push(Difference::PropertyAdded {
                    path: path.clone(),
                    property: key.to_string(),
                }),
                (Some(va), Some(vb)) => {
                    if !values_match(va, vb, &ref_map, options) {
                        diffs.push(Difference::PropertyChanged {
                            path: path.clone(),
                            property: key.to_string(),
                            from: format!("{va:?}"),
                            to: format!("{vb:?}"),
                        });
                    }
                }
                (None, None) => unreachable!("key came from one of the maps"),
            }
        }
    }
    diffs
}

fn pair_children(
    a: &WeakDom,
    b: &WeakDom,
    pa: Ref,
    pb: Ref,
    path: String,
    pairs: &mut Vec<(Ref, Ref, String)>,
    diffs: &mut Vec<Difference>,
) {
    let ca = a.get_by_ref(pa).expect("parent exists").children();
    let cb = b.get_by_ref(pb).expect("parent exists").children();
    if ca.len() != cb.len() {
        diffs.push(Difference::ChildCountChanged {
            path: if path.is_empty() {
                "<root>".into()
            } else {
                path.clone()
            },
            from: ca.len(),
            to: cb.len(),
        });
    }
    for (index, (&ra, &rb)) in ca.iter().zip(cb).enumerate() {
        let ia = a.get_by_ref(ra).expect("child exists");
        let ib = b.get_by_ref(rb).expect("child exists");
        let child_path = if path.is_empty() {
            format!("{}[{index}]", ia.name)
        } else {
            format!("{path}.{}[{index}]", ia.name)
        };
        if ia.class != ib.class {
            diffs.push(Difference::ClassChanged {
                path: child_path.clone(),
                from: ia.class.to_string(),
                to: ib.class.to_string(),
            });
        }
        if ia.name != ib.name {
            diffs.push(Difference::NameChanged {
                path: child_path.clone(),
                from: ia.name.clone(),
                to: ib.name.clone(),
            });
        }
        pairs.push((ra, rb, child_path.clone()));
        pair_children(a, b, ra, rb, child_path, pairs, diffs);
    }
}

fn values_match(
    a: &Variant,
    b: &Variant,
    ref_map: &HashMap<Ref, Ref>,
    options: CompareOptions,
) -> bool {
    if let Some(tolerance) = options.float_tolerance
        && let (Some(fa), Some(fb)) = (float_components(a), float_components(b))
    {
        return a.ty() == b.ty()
            && fa.len() == fb.len()
            && fa
                .iter()
                .zip(&fb)
                .all(|(x, y)| floats_close(*x, *y, tolerance));
    }
    match (a, b) {
        (Variant::Ref(ra), Variant::Ref(rb)) => {
            if ra.is_none() || rb.is_none() {
                ra.is_none() == rb.is_none()
            } else {
                // A ref to something outside the saved tree can't be mapped;
                // treat both-unmapped as equal.
                ref_map.get(ra).is_none_or(|mapped| mapped == rb)
            }
        }
        // Derived float equality treats NaN != NaN, but a NaN that survives a
        // round trip unchanged is a match. Comparing debug output handles NaN
        // in any nested position (CFrames, sequences, and so on).
        _ => a == b || (a.ty() == b.ty() && format!("{a:?}") == format!("{b:?}")),
    }
}

fn floats_close(x: f32, y: f32, tolerance: f32) -> bool {
    x == y
        || (x.is_nan() && y.is_nan())
        || (x - y).abs() <= tolerance * x.abs().max(y.abs()).max(1.0)
}

/// The float components of value types made entirely of floats.
fn float_components(v: &Variant) -> Option<Vec<f32>> {
    Some(match v {
        Variant::Float32(f) => vec![*f],
        #[allow(clippy::cast_possible_truncation)]
        Variant::Float64(f) => vec![*f as f32],
        Variant::Vector2(v) => vec![v.x, v.y],
        Variant::Vector3(v) => vec![v.x, v.y, v.z],
        Variant::Color3(c) => vec![c.r, c.g, c.b],
        Variant::NumberRange(r) => vec![r.min, r.max],
        Variant::Rect(r) => vec![r.min.x, r.min.y, r.max.x, r.max.y],
        Variant::CFrame(cf) => {
            let (p, m) = (cf.position, cf.orientation);
            vec![
                p.x, p.y, p.z, m.x.x, m.x.y, m.x.z, m.y.x, m.y.y, m.y.z, m.z.x, m.z.y, m.z.z,
            ]
        }
        Variant::NumberSequence(s) => s
            .keypoints
            .iter()
            .flat_map(|k| [k.time, k.value, k.envelope])
            .collect(),
        Variant::ColorSequence(s) => s
            .keypoints
            .iter()
            .flat_map(|k| [k.time, k.color.r, k.color.g, k.color.b])
            .collect(),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbx_dom_weak::InstanceBuilder;

    #[test]
    fn detects_changes() {
        let a = WeakDom::new(
            InstanceBuilder::new("DataModel").with_child(
                InstanceBuilder::new("Part")
                    .with_name("P")
                    .with_property("Anchored", true),
            ),
        );
        let b = WeakDom::new(
            InstanceBuilder::new("DataModel").with_child(
                InstanceBuilder::new("Part")
                    .with_name("Q")
                    .with_property("Anchored", false),
            ),
        );
        let diffs = compare(&a, &b);
        assert!(
            diffs
                .iter()
                .any(|d| matches!(d, Difference::NameChanged { .. }))
        );
        assert!(
            diffs
                .iter()
                .any(|d| matches!(d, Difference::PropertyChanged { .. }))
        );
    }

    #[test]
    fn nan_matches_nan() {
        let nan = Variant::Float32(f32::NAN);
        assert!(values_match(
            &nan,
            &nan,
            &HashMap::new(),
            CompareOptions::default()
        ));
    }

    #[test]
    fn tolerance_applies_only_when_requested() {
        let a = Variant::Float32(0.080_367_394);
        let b = Variant::Float32(0.080_367_4);
        let exact = CompareOptions::default();
        let loose = CompareOptions {
            float_tolerance: Some(1e-5),
        };
        assert!(!values_match(&a, &b, &HashMap::new(), exact));
        assert!(values_match(&a, &b, &HashMap::new(), loose));
        assert!(!values_match(
            &a,
            &Variant::Float32(0.09),
            &HashMap::new(),
            loose
        ));
    }
}
