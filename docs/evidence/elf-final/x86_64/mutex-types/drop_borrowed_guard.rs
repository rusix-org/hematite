#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client(guard: Guard<'_, u64>) -> u64 {
        let value = &*guard; drop(guard); *value
    }
