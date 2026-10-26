#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::process_budget::{Budget, Error, Finish, Snapshot};
pub fn client(budget: Budget) { let moved = budget; let _ = budget.snapshot(); drop(moved); }
