#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
fn both<T: Send + Sync>() {}
        pub fn client() { both::<Mutex<core::cell::Cell<u64>>>(); }
    
