#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
static LOCK: Mutex<u64> = Mutex::new(31);
        pub fn client() -> Option<u64> { LOCK.try_lock().map(|guard| *guard) }
    
