//! Untrusted EL0 probes for owned startup input; no kernel absolute symbols.
#![no_std]
#![no_main]
use core::arch::global_asm;
use core::panic::PanicInfo;
#[allow(dead_code)]
#[path = "../../hm-kernel/src/startup_input.rs"]
mod startup_input;

#[cfg(probe = "absent")]
const PROBE: u64 = 0;
#[cfg(probe = "empty")]
const PROBE: u64 = 1;
#[cfg(probe = "one")]
const PROBE: u64 = 2;
#[cfg(probe = "bytes255")]
const PROBE: u64 = 3;
#[cfg(probe = "binary256")]
const PROBE: u64 = 4;
#[cfg(probe = "payload_write")]
const PROBE: u64 = 5;
#[cfg(probe = "payload_execute")]
const PROBE: u64 = 6;
#[cfg(probe = "input_loop")]
const PROBE: u64 = 7;
// SAFETY: Fixed entry and AAPCS64 scalar shims; faults occur only in untrusted EL0.
global_asm!(include_str!("startup-input.S"), probe = const PROBE);
#[repr(C)]
struct Reply {
    status: u64,
    bytes: u64,
}
// SAFETY: Assembly definitions preserve AAPCS64 and return two scalar words.
unsafe extern "C" {
    fn call_write(handle: u64, address: u64, length: u64) -> Reply;
    fn call_newline(handle: u64) -> Reply;
    fn call_exit(status: u64) -> Reply;
    fn probe_end(probe: u64) -> !;
}
#[used]
// SAFETY: Unique immutable symbol independently verified in a read-only LOAD.
#[unsafe(no_mangle)]
static newline: [u8; 1] = [10];
#[used]
// SAFETY: Explicit data segment retained for the native ELF shape.
#[unsafe(link_section = ".fixture.data")]
static DATA: [u8; 1] = [0x5a];
fn finish(status: u64) -> ! {
    // SAFETY: Fixed scalar exit call; success never returns.
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
// SAFETY: Fixed assembly passes the entry startup pointer and compiled probe ID.
#[unsafe(no_mangle)]
extern "C" fn probe_main(startup: u64, probe: u64) -> ! {
    demand(startup == 0x1_0010_0000 && probe == PROBE);
    // SAFETY: The checked fixed address names this process's complete immutable
    // startup page. No mutable alias or reference outside that page is formed.
    let page = unsafe { core::slice::from_raw_parts(startup as *const u8, 4096) };
    let handle = word(page, 24);
    demand(
        &page[..8] == b"HEMATITE"
            && page[8..16] == [1, 0, 0, 0, 64, 0, 0, 0]
            && word(page, 16) == 1
            && handle >> 32 != 0
            && handle as u32 == 1
            && word(page, 32) == 256
            && word(page, 40) == 4096
            && word(page, 48) == 64
            && word(page, 56) == 0,
    );
    let view = match startup_input::decode(&page[64..]) {
        Ok(view) => view,
        Err(_) => finish(202),
    };
    let bytes = view.bytes();
    let expected_length = match probe {
        0 | 1 => 0,
        2 | 5 | 7 => 1,
        3 => 255,
        4 => 256,
        6 => 4,
        _ => finish(203),
    };
    demand(view.is_present() == (probe != 0) && bytes.len() == expected_length);
    for (i, byte) in bytes.iter().enumerate() {
        let expected = match probe {
            2 => 0x41,
            3 | 4 => i as u8,
            5 => 0x57,
            6 => [0xc0, 0x03, 0x5f, 0xd6][i],
            7 => 0x4c,
            _ => finish(204),
        };
        demand(*byte == expected);
    }
    if probe >= 5 {
        // SAFETY: Deliberate untrusted RO-write/NX/loop operations; the kernel
        // must contain the fault/deadline and never resume these probes.
        unsafe { probe_end(probe) }
    }
    // SAFETY: Fixed address lies in the retained RO startup page and the decoder
    // established the exact bounded payload. Zero writes also use this address.
    let reply = unsafe { call_write(handle, 0x1_0010_0058, bytes.len() as u64) };
    demand(reply.status == 0 && reply.bytes == bytes.len() as u64);
    // SAFETY: Fixed shim supplies its own immutable one-byte newline symbol.
    let reply = unsafe { call_newline(handle) };
    demand(reply.status == 0 && reply.bytes == 1);
    finish(match probe {
        0 => 10,
        1 => 11,
        _ => 12,
    })
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    finish(205)
}
