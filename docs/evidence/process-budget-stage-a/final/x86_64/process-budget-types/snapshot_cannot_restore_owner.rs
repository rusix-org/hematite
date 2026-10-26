#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::process_budget::{Budget, Error, Finish, Snapshot};
pub fn client(snapshot: Snapshot) -> Budget { snapshot }
