#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client(mutex: &Mutex<u64>) {
        let _ = Guard { mutex, _not_send_sync: core::marker::PhantomData };
    }
