//! Monotonic notification observations; payload ownership remains separate.
#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NotifyError {
    Exhausted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitError {
    Unsupported,
    Exhausted,
}

/// Snapshot before checking a synchronized predicate, then wait if it is false.
/// Notification asks all existing waiters to recheck; it is not a queued permit.
pub struct Event {
    sequence: AtomicU64,
}

impl Event {
    pub const fn new() -> Self {
        Self {
            sequence: AtomicU64::new(0),
        }
    }

    pub fn snapshot(&self) -> u64 {
        self.sequence.load(Ordering::Acquire)
    }

    /// Successful notifications publish prior writes. Exhaustion leaves the
    /// sequence unchanged and does not publish another notification.
    pub fn notify(&self) -> Result<u64, NotifyError> {
        self.sequence
            .try_update(Ordering::AcqRel, Ordering::Acquire, |value| {
                value.checked_add(1)
            })
            .map(|previous| previous.saturating_add(1))
            .map_err(|_| NotifyError::Exhausted)
    }
}

impl Default for Event {
    fn default() -> Self {
        Self::new()
    }
}

/// Boundary fixtures stay beside private storage; no sequence reset is exposed.
pub(crate) fn contract() -> bool {
    let event = Event::new();
    if event.snapshot() != 0
        || event.notify() != Ok(1)
        || event.notify() != Ok(2)
        || event.snapshot() != 2
    {
        return false;
    }
    let terminal = Event {
        sequence: AtomicU64::new(u64::MAX - 1),
    };
    terminal.notify() == Ok(u64::MAX)
        && terminal.snapshot() == u64::MAX
        && terminal.notify() == Err(NotifyError::Exhausted)
        && terminal.notify() == Err(NotifyError::Exhausted)
        && terminal.snapshot() == u64::MAX
}

pub(crate) fn terminal_waits(context: &mut dyn crate::thread_api::ThreadContext) -> bool {
    let terminal = Event {
        sequence: AtomicU64::new(u64::MAX - 1),
    };
    terminal.notify() == Ok(u64::MAX)
        && context.wait(&terminal, u64::MAX - 1) == Ok(())
        && context.wait(&terminal, u64::MAX) == Err(WaitError::Exhausted)
        && terminal.notify() == Err(NotifyError::Exhausted)
}
