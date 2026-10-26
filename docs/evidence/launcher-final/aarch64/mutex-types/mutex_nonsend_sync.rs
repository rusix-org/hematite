#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
fn sync<T: Sync>() {}
        pub fn client() { sync::<Mutex<*mut u8>>(); }
    
