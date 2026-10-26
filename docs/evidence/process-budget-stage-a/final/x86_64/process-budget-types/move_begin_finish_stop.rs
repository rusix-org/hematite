#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::process_budget::{Budget, Error, Finish, Snapshot};
pub fn client() -> Result<Snapshot, Error> {
        let original = Budget::new(100, 10)?; let mut budget = original;
        let _ = budget.begin(5)?; let _ = budget.finish(8)?;
        budget.stop()?; Ok(budget.snapshot())
    }
