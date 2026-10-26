#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::native::{Issuer, Session, Request, Outcome, Host, CopyError};
pub fn client(i:&Issuer) { let _ = <Issuer as Clone>::clone(i); }
