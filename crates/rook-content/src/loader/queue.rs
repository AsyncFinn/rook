//! A work queue that hands out its most urgent item first.
//!
//! Each item has a priority, lower first (the player's is a distance from
//! the camera), which can change while the item waits; items of equal
//! priority come out in the order they went in.

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::hash::Hash;
use std::sync::{Condvar, Mutex};

/// Where an item stands: its priority's bits (non-negative floats order
/// like their bits), then when it was queued.
type Rank = (u32, u64);

fn bits(priority: f32) -> u32 {
    if priority.is_nan() {
        f32::INFINITY.to_bits()
    } else {
        priority.max(0.0).to_bits()
    }
}

struct State<K, V> {
    order: BTreeMap<Rank, K>,
    items: HashMap<K, (Rank, V)>,
    next: u64,
    open: bool,
}

pub(super) struct Queue<K, V> {
    state: Mutex<State<K, V>>,
    ready: Condvar,
}

impl<K, V> fmt::Debug for Queue<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let waiting = self.state.lock().map_or(0, |s| s.order.len());
        f.debug_struct("Queue").field("waiting", &waiting).finish()
    }
}

impl<K: Hash + Eq + Clone, V> Queue<K, V> {
    pub(super) fn new() -> Self {
        Self {
            state: Mutex::new(State {
                order: BTreeMap::new(),
                items: HashMap::new(),
                next: 0,
                open: true,
            }),
            ready: Condvar::new(),
        }
    }

    /// Queue `value` under `key`, replacing what `key` held.
    pub(super) fn push(&self, key: K, value: V, priority: f32) {
        let mut s = self.state.lock().expect("work queue");
        s.next += 1;
        let rank = (bits(priority), s.next);
        if let Some((old, _)) = s.items.insert(key.clone(), (rank, value)) {
            s.order.remove(&old);
        }
        s.order.insert(rank, key);
        drop(s);
        self.ready.notify_one();
    }

    /// Change a waiting item's priority, keeping its place among equals;
    /// nothing once it's been handed out.
    pub(super) fn set_priority(&self, key: &K, priority: f32) {
        let mut s = self.state.lock().expect("work queue");
        let s = &mut *s;
        let Some((rank, _)) = s.items.get_mut(key) else {
            return;
        };
        let new = (bits(priority), rank.1);
        if let Some(key) = s.order.remove(rank) {
            s.order.insert(new, key);
        }
        *rank = new;
    }

    /// The most urgent item and its priority, waiting for one; `None`
    /// once the queue is closed.
    pub(super) fn pop(&self) -> Option<(K, V, f32)> {
        let mut s = self.state.lock().expect("work queue");
        loop {
            if !s.open {
                return None;
            }
            if let Some((_, key)) = s.order.pop_first() {
                let (rank, value) = s.items.remove(&key).expect("queued item");
                return Some((key, value, f32::from_bits(rank.0)));
            }
            s = self.ready.wait(s).expect("work queue");
        }
    }

    /// Stop handing out items: every `pop` returns `None` from now on.
    pub(super) fn close(&self) {
        self.state.lock().expect("work queue").open = false;
        self.ready.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lowest priority first, equals in the order they came, and a changed
    /// priority moves an item.
    #[test]
    fn most_urgent_first() {
        let q = Queue::new();
        q.push("far", (), 100.0);
        q.push("near", (), 5.0);
        q.push("also near", (), 5.0);
        q.push("moved", (), 50.0);
        q.set_priority(&"moved", 1.0);
        let order: Vec<_> = std::iter::from_fn(|| {
            let item = q.pop()?;
            if item.0 == "far" {
                q.close();
            }
            Some(item.0)
        })
        .collect();
        assert_eq!(order, ["moved", "near", "also near", "far"]);
    }
}
