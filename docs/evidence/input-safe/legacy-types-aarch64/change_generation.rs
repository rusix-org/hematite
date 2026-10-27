#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::native::{Issuer, Session, Request, Outcome, Host, CopyError};
pub fn client(i:&mut Issuer) { i.next = 1; }
