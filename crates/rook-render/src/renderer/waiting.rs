//! Entries waiting for their mesh or images to reach the GPU, found by
//! the files they wait on, so an upload places only the entries it was
//! for (a place with 300,000 parts waiting on 60,000 mesh files would
//! otherwise look at all of them each frame of the upload).

use super::*;

/// A file an entry can wait on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum File {
    Mesh(DrawShape),
    Image(ImageHash),
}

#[derive(Debug, Default)]
pub(crate) struct Waiting {
    entries: HashMap<usize, PartInstance>,
    /// The entries waiting on each file. An entry that stopped waiting may
    /// still be listed; it's skipped.
    on: HashMap<File, Vec<usize>>,
}

impl Waiting {
    pub(crate) fn clear(&mut self) {
        self.entries.clear();
        self.on.clear();
    }

    /// Entry `i`, drawn as `part`, waits on `files`.
    pub(crate) fn insert(&mut self, i: usize, part: PartInstance, files: &[File]) {
        self.entries.insert(i, part);
        for file in files {
            self.on.entry(*file).or_default().push(i);
        }
    }

    pub(crate) fn remove(&mut self, i: usize) {
        self.entries.remove(&i);
    }

    pub(crate) fn get_mut(&mut self, i: usize) -> Option<&mut PartInstance> {
        self.entries.get_mut(&i)
    }

    /// Take the entries waiting on any of `uploaded`, near ones first so
    /// they take near slots.
    pub(crate) fn take(
        &mut self,
        uploaded: impl IntoIterator<Item = File>,
    ) -> Vec<(usize, PartInstance)> {
        let mut taken: Vec<(usize, PartInstance)> = Vec::new();
        for file in uploaded {
            for i in self.on.remove(&file).unwrap_or_default() {
                if let Some(part) = self.entries.remove(&i) {
                    taken.push((i, part));
                }
            }
        }
        taken.sort_by_key(|(i, p)| (crate::batch::cull::spatial_key(p.position), *i));
        taken
    }
}
