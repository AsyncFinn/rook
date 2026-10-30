//! Telling a reader that keeps copies of names or children about a change
//! inside the change itself. The journal is read later and can be off, and
//! a kept copy must never be read stale.

use crate::{Dom, InstanceId};

/// A change an observer hears of as it happens, with the journal on or off.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Observed {
    /// The instance's name changed.
    Renamed(InstanceId),
    /// A child was added to the instance or removed from it.
    ChildrenChanged(InstanceId),
}

pub(crate) struct Observer(Box<dyn FnMut(Observed)>);

impl std::fmt::Debug for Observer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Observer")
    }
}

impl Dom {
    /// Call `observer` inside every rename and children change.
    pub fn set_observer(&mut self, observer: impl FnMut(Observed) + 'static) {
        self.observer = Some(Observer(Box::new(observer)));
    }

    pub(crate) fn notify(&mut self, change: Observed) {
        if let Some(observer) = &mut self.observer {
            (observer.0)(change);
        }
    }
}
