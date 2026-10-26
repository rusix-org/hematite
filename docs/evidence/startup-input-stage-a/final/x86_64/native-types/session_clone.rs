#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::native::{Issuer, Session, Request, Outcome, Host, CopyError};
pub fn client(s:&Session) { let _ = <Session as Clone>::clone(s); }
