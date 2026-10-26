#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
fn send<T: Send>() {}
        pub fn client() { send::<Mutex<*mut u8>>(); }
    
