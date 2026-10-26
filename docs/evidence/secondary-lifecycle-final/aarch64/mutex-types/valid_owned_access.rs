#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client() -> u64 {
        let mut mutex = Mutex::new(7u64);
        { let mut guard = mutex.try_lock().unwrap(); *guard += 1; }
        *mutex.get_mut() += 1; mutex.into_inner()
    }
