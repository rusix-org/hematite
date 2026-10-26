//! Normal native applications; their input/output follows the versioned wire.
#![no_std]
#![no_main]
use core::arch::global_asm;
use core::fmt::{self, Write};
use core::panic::PanicInfo;

#[cfg(app = "hello")]
const APP: u64 = 0;
#[cfg(app = "info")]
const APP: u64 = 1;
#[cfg(app = "fault")]
const APP: u64 = 2;
#[cfg(app = "loop")]
const APP: u64 = 3;

// SAFETY: Fixed entry/shims obey the native wire and AAPCS64; deliberately
// invalid operations are confined to untrusted EL0 demo applications.
global_asm!(include_str!("apps.S"), app = const APP);
#[repr(C)]
struct Reply {
    status: u64,
    bytes: u64,
}
// SAFETY: Fixed assembly definitions preserve AAPCS64 and return two u64 words.
unsafe extern "C" {
    fn call_write(handle: u64, address: u64, length: u64) -> Reply;
    fn call_exit(status: u64) -> Reply;
    fn deliberate_fault(app: u64) -> !;
}
#[used]
// SAFETY: Unique immutable ELF symbol for independent payload inspection.
#[unsafe(no_mangle)]
static message: [u8; 15] = *b"hello from EL0\n";
#[used]
// SAFETY: Retained ordinary data makes the explicit RW LOAD nonempty.
#[unsafe(link_section = ".fixture.data")]
static DATA: [u8; 1] = [0x5a];
fn finish(status: u64) -> ! {
    // SAFETY: Fixed scalar exit ABI. A valid exit never returns to this program.
    let _ = unsafe { call_exit(status) };
    loop {
        core::hint::spin_loop();
    }
}
fn demand(ok: bool) {
    if !ok {
        finish(201);
    }
}
fn word(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn output(handle: u64, bytes: &[u8]) {
    // SAFETY: The immutable user buffer remains live throughout the synchronous
    // wire call; the kernel copies it rather than retaining its address.
    let reply = unsafe { call_write(handle, bytes.as_ptr() as u64, bytes.len() as u64) };
    demand(reply.status == 0 && reply.bytes == bytes.len() as u64);
}
struct Text {
    bytes: [u8; 256],
    used: usize,
}
impl Write for Text {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let end = self.used.checked_add(text.len()).ok_or(fmt::Error)?;
        self.bytes
            .get_mut(self.used..end)
            .ok_or(fmt::Error)?
            .copy_from_slice(text.as_bytes());
        self.used = end;
        Ok(())
    }
}
// SAFETY: Unique fixed assembly entry calls this C ABI function with startup
// address in x0 and the compile-time application selection in x1.
#[unsafe(no_mangle)]
extern "C" fn app_main(startup: u64, app: u64) -> ! {
    demand(startup == 0x1_0010_0000 && app == APP);
    // SAFETY: Native startup maps this fixed address read-only for the entire
    // process; the exact address is checked before forming this bounded view.
    let bytes = unsafe { core::slice::from_raw_parts(startup as *const u8, 64) };
    let version = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    let handle = word(bytes, 24);
    let max_write = word(bytes, 32);
    let budget = word(bytes, 40);
    let calls = word(bytes, 48);
    demand(
        &bytes[..8] == b"HEMATITE"
            && version == 1
            && bytes[12..16] == [64, 0, 0, 0]
            && word(bytes, 16) == 1
            && handle >> 32 != 0
            && handle as u32 == 1
            && max_write == 256
            && budget == 4096
            && calls == 64
            && word(bytes, 56) == 0,
    );
    match app {
        0 => output(handle, &message),
        1 => {
            let mut text = Text {
                bytes: [0; 256],
                used: 0,
            };
            demand(write!(&mut text, "native ABI {version}\nconsole: granted\nwrite limit: {max_write} bytes\nconsole budget: {budget} bytes\ncall limit: {calls}\n").is_ok());
            output(handle, &text.bytes[..text.used]);
        }
        2 | 3 => {
            // SAFETY: Deliberate RX-write/loop demonstrations run only at EL0;
            // they cannot return and rely on the documented isolation/deadline.
            unsafe { deliberate_fault(app) }
        }
        _ => finish(202),
    }
    finish(0)
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    finish(203)
}
