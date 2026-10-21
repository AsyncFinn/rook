//! Spreading short work over the host's cores: culling, particles,
//! terrain meshing. A host without threads runs it all on the calling
//! thread ([`Serial`]); the results are the same either way.

use std::cmp::Ordering;
use std::fmt::Debug;
use std::sync::{Mutex, MutexGuard, PoisonError};

/// Below this many items a sort isn't split further.
const SORT_ALONE: usize = 4096;

/// The host's workers. Implement the two primitives; callers use the
/// typed helpers on `dyn Workers` ([`join`](Self::join), [`map`](Self::map),
/// [`for_each_chunk`](Self::for_each_chunk), [`sort_by`](Self::sort_by)).
pub trait Workers: Send + Sync + Debug {
    /// The host's logical cores, at least 1.
    fn cores(&self) -> usize;
    /// Run `a` and `b`, perhaps at once; returns when both have.
    fn run_both(&self, a: &mut (dyn FnMut() + Send), b: &mut (dyn FnMut() + Send));
    /// Run `task(i)` for each `i` in `0..count`, perhaps at once; returns
    /// when all have.
    fn run_each(&self, count: usize, task: &(dyn Fn(usize) + Sync));
}

impl dyn Workers {
    /// Both results, as `rayon::join` gives them.
    pub fn join<A, B, RA, RB>(&self, a: A, b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA + Send,
        B: FnOnce() -> RB + Send,
        RA: Send,
        RB: Send,
    {
        let (mut a, mut b) = (Some(a), Some(b));
        let (mut ra, mut rb) = (None, None);
        self.run_both(&mut || ra = a.take().map(|f| f()), &mut || {
            rb = b.take().map(|f| f())
        });
        (ra.expect("a ran"), rb.expect("b ran"))
    }

    /// `f` of each item, in order.
    pub fn map<T: Sync, R: Send>(&self, items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
        let out: Vec<Mutex<Option<R>>> = items.iter().map(|_| Mutex::new(None)).collect();
        self.run_each(items.len(), &|i| *lock(&out[i]) = Some(f(&items[i])));
        out.into_iter()
            .map(|r| {
                r.into_inner()
                    .unwrap_or_else(PoisonError::into_inner)
                    .expect("every item mapped")
            })
            .collect()
    }

    /// `f(k, chunk)` for each `size`-long chunk of `items` (the last may
    /// be shorter), `k` counting chunks.
    pub fn for_each_chunk<T: Send>(
        &self,
        items: &mut [T],
        size: usize,
        f: impl Fn(usize, &mut [T]) + Sync,
    ) {
        let chunks: Vec<Mutex<&mut [T]>> = items.chunks_mut(size).map(Mutex::new).collect();
        self.run_each(chunks.len(), &|k| f(k, &mut lock(&chunks[k])));
    }

    /// Sort as `sort_unstable_by` does: halves sorted at once, then
    /// merged.
    pub fn sort_by<T, F>(&self, items: &mut [T], compare: F)
    where
        T: Copy + Send + Sync,
        F: Fn(&T, &T) -> Ordering + Sync,
    {
        self.sort_part(items, &compare);
    }

    fn sort_part<T, F>(&self, items: &mut [T], compare: &F)
    where
        T: Copy + Send + Sync,
        F: Fn(&T, &T) -> Ordering + Sync,
    {
        if items.len() <= SORT_ALONE {
            items.sort_unstable_by(compare);
            return;
        }
        let mid = items.len() / 2;
        let (left, right) = items.split_at_mut(mid);
        self.join(
            || self.sort_part(left, compare),
            || self.sort_part(right, compare),
        );
        merge(items, mid, compare);
    }
}

/// Merge the sorted `items[..mid]` and `items[mid..]` in place, taking
/// from the left run on ties. Only the left run is copied out: the next
/// slot written is never past the right run's next item.
fn merge<T: Copy>(items: &mut [T], mid: usize, compare: impl Fn(&T, &T) -> Ordering) {
    let left = items[..mid].to_vec();
    let (mut i, mut j) = (0, mid);
    for k in 0..items.len() {
        if i == left.len() {
            break;
        }
        if j < items.len() && compare(&items[j], &left[i]) == Ordering::Less {
            items[k] = items[j];
            j += 1;
        } else {
            items[k] = left[i];
            i += 1;
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Everything on the calling thread, one task after another: a host
/// without threads.
#[derive(Debug, Default)]
pub struct Serial;

impl Workers for Serial {
    fn cores(&self) -> usize {
        1
    }

    fn run_both(&self, a: &mut (dyn FnMut() + Send), b: &mut (dyn FnMut() + Send)) {
        a();
        b();
    }

    fn run_each(&self, count: usize, task: &(dyn Fn(usize) + Sync)) {
        (0..count).for_each(task);
    }
}

/// Rayon's global pool (`rook-physics` sizes it).
#[cfg(feature = "native")]
#[derive(Debug)]
pub struct Rayon;

#[cfg(feature = "native")]
impl Workers for Rayon {
    fn cores(&self) -> usize {
        std::thread::available_parallelism().map_or(2, usize::from)
    }

    fn run_both(&self, a: &mut (dyn FnMut() + Send), b: &mut (dyn FnMut() + Send)) {
        rayon::join(a, b);
    }

    fn run_each(&self, count: usize, task: &(dyn Fn(usize) + Sync)) {
        use rayon::prelude::*;
        (0..count).into_par_iter().for_each(task);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workers() -> &'static dyn Workers {
        &Serial
    }

    #[test]
    fn map_and_chunks_keep_order() {
        let w = workers();
        assert_eq!(w.map(&[1, 2, 3], |x| x * 10), [10, 20, 30]);
        let mut items = [0; 7];
        w.for_each_chunk(&mut items, 3, |k, chunk| chunk.fill(k));
        assert_eq!(items, [0, 0, 0, 1, 1, 1, 2]);
        assert_eq!(w.join(|| 1, || "b"), (1, "b"));
    }

    #[test]
    fn sorting_in_halves_sorts() {
        // A scrambled run long enough to be split three times.
        let mut items: Vec<(u32, u32)> = (0..SORT_ALONE as u32 * 8)
            .map(|i| (i.wrapping_mul(2_654_435_761) % 1000, i))
            .collect();
        let mut expected = items.clone();
        expected.sort_unstable();
        workers().sort_by(&mut items, Ord::cmp);
        assert_eq!(items, expected);
    }
}
