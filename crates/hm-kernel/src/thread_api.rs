//! Bounded cooperative kernel execution; this is not a userspace wire ABI.
#![forbid(unsafe_code)]

pub const MAX_THREADS: usize = 4;
pub const TRACE_CAPACITY: usize = 64;

/// A borrowed execution capability. Implementations retain their own stacks.
/// Yield preserves this callback's locals and resumes it before the run returns.
pub trait ThreadContext {
    fn id(&self) -> usize;
    fn yield_now(&mut self);
}

pub type ThreadEntry = for<'a> fn(&'a mut (dyn ThreadContext + 'a), u64) -> u64;

#[derive(Clone, Copy)]
pub struct ThreadJob {
    pub entry: ThreadEntry,
    pub input: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadError {
    Unsupported,
    TooMany,
    Busy,
}

/// Values are indexed by admitted job ID. The trace records each dispatch,
/// including initial entry and resumption after a callback's final yield.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RunReport {
    pub results: [u64; MAX_THREADS],
    pub yields: [u64; MAX_THREADS],
    pub completed: usize,
    pub dispatches: u64,
    pub trace: [usize; TRACE_CAPACITY],
    pub trace_len: usize,
    pub truncated: bool,
}

impl RunReport {
    pub const fn empty() -> Self {
        Self {
            results: [0; MAX_THREADS],
            yields: [0; MAX_THREADS],
            completed: 0,
            dispatches: 0,
            trace: [0; TRACE_CAPACITY],
            trace_len: 0,
            truncated: false,
        }
    }
}
