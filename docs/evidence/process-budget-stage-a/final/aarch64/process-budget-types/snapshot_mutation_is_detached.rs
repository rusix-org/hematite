#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::process_budget::{Budget, Error, Finish, Snapshot};
pub fn client(budget: &Budget) -> Snapshot {
        let before = budget.snapshot(); let mut detached = before;
        detached.remaining = 0; detached.charged = u64::MAX; detached.turns = u64::MAX;
        assert_eq!(budget.snapshot(), before); let _ = detached; budget.snapshot()
    }
