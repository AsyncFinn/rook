//! What didn't load, reported once loading settles: one summary of the
//! failures grouped by cause, each failure in detail at `debug`
//! (`--verbose`), and the ones Studio's Output shows as errors (in red),
//! as Studio does.
//!
//! Studio warns, once per file, when an asset can't be fetched (missing,
//! or not allowed), not about Rook's own gaps; those (a version Rook can't
//! read yet) are in the summary and the detail only. The warning's wording follows Studio's
//! image warning, `Image "<url>" failed to load in "<path>": <reason>`
//! (Inference: recalled, not checked against Studio this time).

use std::collections::{BTreeMap, HashSet};

use rook_content::{ContentRef, Failure, FailureKind, Want};
use rook_dom::{Dom, InstanceId};

use super::loader::FileRef;

/// What failed to load, by what wanted it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum What {
    Union,
    Mesh,
    /// A built-in material's or terrain's texture.
    MaterialTexture,
    Image,
    Sky,
}

impl What {
    fn of(dom: &Dom, part: Option<InstanceId>, key: &FileRef) -> Self {
        let Some(part) = part else {
            return Self::Sky;
        };
        if matches!(
            key.0,
            ContentRef::Material(..) | ContentRef::TexturePack(..)
        ) {
            return Self::MaterialTexture;
        }
        match key.1 {
            Want::Mesh if dom.is_a(part, "PartOperation") => Self::Union,
            Want::Mesh => Self::Mesh,
            _ => Self::Image,
        }
    }

    fn count(self, n: usize) -> String {
        let (one, many) = match self {
            Self::Union => ("union", "unions"),
            Self::Mesh => ("mesh", "meshes"),
            Self::MaterialTexture => ("material texture", "material textures"),
            Self::Image => ("image", "images"),
            Self::Sky => ("sky image", "sky images"),
        };
        format!("{n} {}", if n == 1 { one } else { many })
    }
}

/// One instance whose file didn't load.
#[derive(Debug, Clone)]
pub(crate) struct Failed {
    pub what: What,
    pub path: String,
    pub reference: ContentRef,
    pub failure: Failure,
}

/// Failures waiting to be reported.
#[derive(Debug, Default)]
pub(crate) struct Report {
    /// Each instance and file reported once.
    seen: HashSet<(Option<InstanceId>, FileRef)>,
    /// Files Studio-style warnings were printed for (once each).
    warned: HashSet<FileRef>,
    pending: Vec<(Option<InstanceId>, FileRef, Failure)>,
}

impl Report {
    /// Note that `part`'s file `key` failed (`None` for the sky).
    pub(crate) fn add(&mut self, part: Option<InstanceId>, key: &FileRef, failure: &Failure) {
        if self.seen.insert((part, key.clone())) {
            self.pending.push((part, key.clone(), failure.clone()));
        }
    }

    /// Report what failed since the last flush.
    pub(crate) fn flush(&mut self, dom: &Dom) {
        if self.pending.is_empty() {
            return;
        }
        let mut failed = Vec::new();
        for (part, key, failure) in std::mem::take(&mut self.pending) {
            let f = Failed {
                what: What::of(dom, part, &key),
                path: part.map_or_else(|| "Lighting".to_owned(), |p| dom.full_name(p)),
                reference: key.0.clone(),
                failure,
            };
            // Studio prints these in red, once per file, naming the first
            // instance.
            if studio_warns(f.failure.kind) && self.warned.insert(key) {
                tracing::error!(
                    target: "content",
                    "{} \"{}\" failed to load in \"{}\": {}",
                    studio_word(f.what),
                    f.reference,
                    f.path,
                    f.failure.message
                );
            }
            tracing::debug!(
                target: "content",
                "{}: {}: {}",
                f.path,
                f.reference,
                f.failure.message
            );
            failed.push(f);
        }
        let mut lines = summarize(&failed).into_iter();
        if let Some(head) = lines.next() {
            tracing::warn!(target: "content", "{head}");
        }
        for line in lines {
            tracing::info!(target: "content", "{line}");
        }
    }
}

/// Whether Studio's Output would warn about this failure too.
fn studio_warns(kind: FailureKind) -> bool {
    matches!(kind, FailureKind::NotFound | FailureKind::Forbidden)
}

fn studio_word(what: What) -> &'static str {
    match what {
        What::Union | What::Mesh => "Mesh",
        What::MaterialTexture | What::Image | What::Sky => "Image",
    }
}

/// One cause's failures.
struct Group<'a> {
    what: What,
    cause: String,
    count: usize,
    files: HashSet<&'a ContentRef>,
    first: &'a str,
}

/// The summary: a first line counting what didn't load, then one line per
/// cause with how many instances and files it hit and the first instance.
pub(crate) fn summarize(failed: &[Failed]) -> Vec<String> {
    if failed.is_empty() {
        return Vec::new();
    }
    let mut by_what: BTreeMap<What, usize> = BTreeMap::new();
    let mut groups: Vec<Group> = Vec::new();
    for f in failed {
        *by_what.entry(f.what).or_default() += 1;
        let cause = cause(&f.failure);
        let group = match groups
            .iter_mut()
            .position(|g| g.what == f.what && g.cause == cause)
        {
            Some(i) => &mut groups[i],
            None => {
                groups.push(Group {
                    what: f.what,
                    cause,
                    count: 0,
                    files: HashSet::new(),
                    first: &f.path,
                });
                groups.last_mut().expect("pushed")
            }
        };
        group.count += 1;
        group.files.insert(&f.reference);
    }
    let counts: Vec<String> = by_what.iter().map(|(w, &n)| w.count(n)).collect();
    let counts = match counts.as_slice() {
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        [] => unreachable!("failures counted"),
    };
    let mut lines = vec![format!("{counts} didn't load (--verbose for details)")];
    groups.sort_by_key(|g| (g.what, std::cmp::Reverse(g.count)));
    for g in groups {
        let files = match g.files.len() {
            1 => "1 file".to_owned(),
            n => format!("{n} files"),
        };
        lines.push(format!(
            "    {} ({files}): {} (first: {})",
            g.what.count(g.count),
            g.cause,
            g.first
        ));
    }
    lines
}

/// What a failure groups by: its kind and message (asset delivery's
/// messages don't name the asset; a decoder's are the same for every file
/// of a version).
fn cause(failure: &Failure) -> String {
    format!(
        "{} ({})",
        failure.message.trim_end_matches('.'),
        failure.kind.name()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_by_cause_with_the_first_instance() {
        let failed = |what, path: &str, kind, message: &str, id| Failed {
            what,
            path: path.into(),
            reference: ContentRef::Asset { id, version: None },
            failure: Failure::new(kind, message),
        };
        let v3 = "union geometry CSGMDL3 is not supported yet";
        let no = "Authentication required to access Asset.";
        let lines = summarize(&[
            failed(
                What::Union,
                "Workspace.Ruins.Union",
                FailureKind::UnknownVersion,
                v3,
                1,
            ),
            failed(
                What::Mesh,
                "Workspace.Tree.Mesh",
                FailureKind::Forbidden,
                no,
                5,
            ),
            failed(
                What::Union,
                "Workspace.Wall",
                FailureKind::UnknownVersion,
                v3,
                2,
            ),
            failed(What::Union, "Workspace.Gate", FailureKind::Forbidden, no, 7),
            failed(
                What::Union,
                "Workspace.Arch",
                FailureKind::UnknownVersion,
                v3,
                2,
            ),
        ]);
        assert_eq!(
            lines,
            [
                "4 unions and 1 mesh didn't load (--verbose for details)",
                "    3 unions (2 files): union geometry CSGMDL3 is not supported yet (unknown-version) (first: Workspace.Ruins.Union)",
                "    1 union (1 file): Authentication required to access Asset (forbidden) (first: Workspace.Gate)",
                "    1 mesh (1 file): Authentication required to access Asset (forbidden) (first: Workspace.Tree.Mesh)",
            ]
        );
    }
}
