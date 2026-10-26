#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client(mutex: &Mutex<u64>) -> &u64 {
        let guard = mutex.try_lock().unwrap(); &*guard
    }
