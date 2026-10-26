#![no_std]
#![forbid(unsafe_code)]
use hm_mutex::{Mutex, Guard};
pub fn client(mutex: &mut Mutex<u64>) {
        let guard = mutex.try_lock().unwrap(); *mutex.get_mut() = *guard;
    }
