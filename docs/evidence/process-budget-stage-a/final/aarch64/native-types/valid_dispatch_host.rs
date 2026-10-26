#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::native::{Issuer, Session, Request, Outcome, Host, CopyError};
struct Sink;
    impl Host for Sink {
        fn copy_from_user(&mut self, _:u64, out:&mut[u8]) -> Result<(), CopyError> { out.fill(7); Ok(()) }
        fn write_console(&mut self, _: &[u8]) {}
    }
    pub fn client(session: &mut Session, request: Request) -> Outcome {
        session.dispatch(request, &mut Sink)
    }
