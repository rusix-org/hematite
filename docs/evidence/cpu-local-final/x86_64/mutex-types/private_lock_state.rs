#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client(mutex: &Mutex<u64>) { let _ = &mutex.locked; }
    
