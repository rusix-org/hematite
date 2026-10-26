#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::process_budget::{Budget, Error, Finish, Snapshot};
pub fn client() -> bool { hm_kernel::process_budget::check_turn_counter_overflow() }
