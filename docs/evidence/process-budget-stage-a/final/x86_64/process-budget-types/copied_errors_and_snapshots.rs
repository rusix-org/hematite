#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::process_budget::{Budget, Error, Finish, Snapshot};
pub fn client(snapshot: Snapshot, outcome: Finish, error: Error) -> bool {
        let copied = (snapshot, outcome, error);
        let _ = [Finish::Ready, Finish::Exhausted]; let _ = [Error::Configuration, Error::State, Error::Clock];
        copied == (snapshot, outcome, error)
    }
