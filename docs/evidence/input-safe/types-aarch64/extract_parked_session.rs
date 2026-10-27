#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::native::{Issuer, Session, InputAdmission, ReadAdmission, ParkedRead, ReadError, Request, Outcome, Host, CopyError};
use hm_kernel::interactive_wire::{self, Grant};
struct Empty;
impl Host for Empty {
    fn copy_from_user(&mut self, _: u64, _: &mut [u8]) -> Result<(), CopyError> { Err(CopyError) }
    fn write_console(&mut self, _: &[u8]) {}
}
fn request() -> Request { Request { version: 1, operation: 6, argument: 0x100000005, address: 0, length: 1 } }
pub fn client(value: ParkedRead) { let _ = value.session; }
