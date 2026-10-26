#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::native::{Issuer, Session, Request, Outcome, Host, CopyError};
pub fn client() -> Option<[u8;64]> {
        let mut issuer = Issuer::new(); let session = issuer.issue(true).ok()?;
        let _ = (session.calls(), session.written(), session.may_resume(), session.termination());
        Some(session.startup())
    }
