#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client() -> u64 {
        let mut mutex = Mutex::new(17u64);
        core::mem::forget(mutex.try_lock().unwrap());
        *mutex.get_mut() = 29; mutex.into_inner()
    }
