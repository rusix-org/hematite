#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client(value: &u64) -> u64 {
        let mutex = Mutex::new(value); let guard = mutex.try_lock().unwrap(); **guard
    }
