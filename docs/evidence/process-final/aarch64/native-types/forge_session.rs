#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::native::{Issuer, Session, Request, Outcome, Host, CopyError};
pub fn client() { let _ = Session { console: Some(1), calls: 0, written: 0, stopped: None }; }
