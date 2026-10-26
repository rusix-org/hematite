#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client<'a>() -> Mutex<&'a u64> {
        let value = 17; Mutex::new(&value)
    }
