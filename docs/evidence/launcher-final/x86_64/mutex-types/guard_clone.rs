#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client(guard: &Guard<'_, u64>) {
        let _ = <Guard<'_, u64> as Clone>::clone(guard);
    }
