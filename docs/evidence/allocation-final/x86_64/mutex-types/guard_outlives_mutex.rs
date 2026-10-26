#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client<'a>() -> Guard<'a, u64> {
        let mutex = Mutex::new(17); mutex.try_lock().unwrap()
    }
