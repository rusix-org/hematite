#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client(mutex: &Mutex<u64>) {
        let _ = <Mutex<u64> as Clone>::clone(mutex);
    }
