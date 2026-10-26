#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client(mut guard: Guard<'_, u64>) {
        let first = &mut *guard; let second = &mut *guard; *first = *second;
    }
