//! Native EL0 execution fixtures. Kernel safety does not depend on this input.
#![no_std]
#![no_main]
use core::arch::global_asm;
use core::panic::PanicInfo;

// CASE is chosen at compile time by the pinned fixture builder, not by startup.
include!("process-case.rs");
// SAFETY: Fixed entry and syscall shims obey the documented native wire and
// AAPCS64. Deliberate faults execute only in untrusted EL0 test processes.
global_asm!(include_str!("process.S"), case = const CASE);
#[repr(C)]
struct Reply {
    status: u64,
    bytes: u64,
}
// SAFETY: Assembly functions below preserve AAPCS64 and return two u64 words;
// fault never returns, while a valid exit is intercepted by the kernel.
unsafe extern "C" {
    fn call_write(version: u64, operation: u64, handle: u64, address: u64, length: u64) -> Reply;
    fn call_exit(code: u64, address: u64, length: u64) -> Reply;
    fn deliberate_fault(case: u64) -> !;
}
#[used]
// SAFETY: Explicit ELF R-only data symbol, independent of compiler layout.
#[unsafe(no_mangle)]
static message: [u8; 15] = *b"hello from EL0\n";
#[used]
// SAFETY: Linker keeps this array in the explicitly writable LOAD for NX probes.
#[unsafe(link_section = ".fixture.data")]
// SAFETY: Unique symbol identifies this retained fixture array for readelf.
#[unsafe(no_mangle)]
static data: [u8; 256] = pattern();
const fn pattern() -> [u8; 256] {
    let mut x = [0; 256];
    let mut i = 0;
    while i < 256 {
        x[i] = i as u8;
        i += 1;
    }
    x
}
#[used]
// SAFETY: Retained private zero-initialized fixture storage, never shared with
// another process; hello reads it only before any fixture write can occur.
#[unsafe(link_section = ".fixture.bss")]
static mut ZERO: [u8; 4096] = [0; 4096];
fn demand(ok: bool) {
    if !ok {
        finish(201);
    }
}
fn request(v: u64, op: u64, h: u64, p: u64, n: u64, expected: u64, written: u64) {
    // SAFETY: Scalars cross the defined wire. This shim returns only copied
    // response values and preserves AAPCS64; invalid pointers are intentional.
    let reply = unsafe { call_write(v, op, h, p, n) };
    demand(reply.status == expected && reply.bytes == written);
}
fn finish(code: u64) -> ! {
    // SAFETY: Exit has fixed reserved-zero arguments and cannot return on success.
    let _ = unsafe { call_exit(code, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}
fn word(bytes: &[u8], i: usize) -> u64 {
    u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap())
}
// SAFETY: Unique assembly entry target uses the C ABI with x0 startup and x1 case.
#[unsafe(no_mangle)]
extern "C" fn fixture_main(startup: u64, case: u64) -> ! {
    demand(startup == 0x1_0010_0000 && case == CASE);
    // SAFETY: The admitted fixture profile maps this exact 4KiB startup page
    // readable for the whole run. Wrong startup addresses were rejected above.
    let bytes = unsafe { core::slice::from_raw_parts(startup as *const u8, 4096) };
    let handle = word(bytes, 24);
    let pointer = core::ptr::addr_of!(data) as u64;
    match case {
        0 => {
            demand(&bytes[..8] == b"HEMATITE" && bytes[8..16] == [1, 0, 0, 0, 64, 0, 0, 0]);
            demand(
                word(bytes, 16) == 1
                    && handle != 0
                    && word(bytes, 32) == 256
                    && word(bytes, 40) == 4096
                    && word(bytes, 48) == 64,
            );
            demand(bytes[56..].iter().all(|b| *b == 0));
            for i in 0..256 {
                demand(core::hint::black_box(data[i]) == i as u8);
            }
            for i in 0..4096 {
                demand(
                    // SAFETY: ZERO is fixture-owned BSS; these bounded reads
                    // have no concurrent writers in the private process.
                    unsafe {
                        core::ptr::addr_of!(ZERO)
                            .cast::<u8>()
                            .add(i)
                            .read_volatile()
                    } == 0,
                );
            }
            request(1, 1, handle, core::ptr::addr_of!(message) as u64, 15, 0, 15);
        }
        1 => request(1, 1, handle, 0, 1, 4, 0),
        2 => request(1, 1, handle, u64::MAX - 3, 8, 4, 0),
        3 => request(1, 1, handle, 0x1_001f_ffff, 2, 4, 0),
        4 => request(1, 1, handle, pointer, 257, 5, 0),
        5 => request(1, 1, handle ^ 1, u64::MAX, 0, 3, 0),
        6 => request(2, 1, handle, u64::MAX, 0, 1, 0),
        7 => request(1, 255, handle, u64::MAX, 0, 2, 0),
        8..=18 | 28 | 30..=32 => {
            // SAFETY: Deliberate EL0 fault/loop/bad-SP entry is confined to this
            // input program; the kernel must contain it without trusting us.
            unsafe { deliberate_fault(case) }
        }
        19 => request(1, 1, handle, u64::MAX, 0, 0, 0),
        20 => request(1, 1, handle, pointer, 256, 0, 256),
        21 => {
            for _ in 0..16 {
                request(1, 1, handle, pointer, 256, 0, 256);
            }
            request(1, 1, handle, pointer, 1, 6, 0);
            request(1, 1, handle, u64::MAX, 0, 0, 0);
        }
        22 | 23 => {
            for _ in 0..64 {
                request(
                    if case == 22 { 1 } else { 2 },
                    1,
                    handle,
                    u64::MAX,
                    0,
                    if case == 22 { 0 } else { 1 },
                    0,
                );
            }
            // This must be unreachable: a returning 64th call is terminal.
            finish(202);
        }
        24 => {
            for _ in 0..63 {
                request(1, 1, handle, u64::MAX, 0, 0, 0);
            }
        }
        25 => {
            for (code, p, n) in [(256, 0, 0), (0, 1, 0), (0, 0, 1)] {
                // SAFETY: Invalid exit arguments exercise declared recoverable
                // wire errors; the fixed shim preserves the caller context.
                let r = unsafe { call_exit(code, p, n) };
                demand(r.status == 7 && r.bytes == 0);
            }
        }
        26 => request(
            1,
            1,
            handle.checked_sub(1 << 32).unwrap(),
            u64::MAX,
            0,
            3,
            0,
        ),
        27 => {
            demand(handle == 0 && word(bytes, 16) == 0);
            request(1, 1, 1, u64::MAX, 0, 3, 0);
        }
        _ => finish(203),
    }
    finish(0)
}
// SAFETY: Assembly supplies a live aligned 1024-byte snapshot on its valid
// private user stack; it does not mutate it while this C ABI checker runs.
#[unsafe(no_mangle)]
// SAFETY: Only the reviewed assembly caller supplies this private snapshot.
unsafe extern "C" fn check_registers(pointer: *const [u64; 128], stage: u64) -> u64 {
    // SAFETY: The fixed assembly caller meets the snapshot contract above.
    let state = unsafe { &*pointer };
    let mut okay = true;
    for (r, value) in state[..31].iter().enumerate() {
        let expected = if stage == 0 {
            if r == 0 { 0x1_0010_0000 } else { 0 }
        } else {
            match r {
                0 | 1 | 3 => 0,
                2 => u64::MAX,
                8 => 1,
                _ => 0x3000_0000_0000_0000 | (stage << 8) | r as u64,
            }
        };
        okay &= *value == expected;
    }
    okay &= state[31]
        == if stage == 0 {
            0x1_0020_0000
        } else {
            0x1_0020_0000 - 1024
        };
    okay &= state[32]
        == match stage {
            0 => 0,
            1 => 0xa000_0000,
            2 => 0x5000_0000,
            3 => 0xf000_0000,
            _ => u64::MAX,
        };
    okay &= state[33] == stage << 22;
    for r in 0..32 {
        okay &= state[34 + r * 2]
            == if stage == 0 {
                0
            } else {
                0x1000_0000_0000_0000 | (stage << 8) | r as u64
            };
        okay &= state[35 + r * 2]
            == if stage == 0 {
                0
            } else {
                0x2000_0000_0000_0000 | (stage << 8) | r as u64
            };
    }
    okay &= state[98]
        == if stage == 0 {
            0
        } else {
            0x0800_0000 | (1 << (stage - 1))
        };
    okay &= state[99]
        == if stage == 0 {
            0
        } else {
            0x1234_0000_0000_0000 | stage
        };
    okay &= state[100] == 0 && state[101] == 0 && state[102] == 0;
    u64::from(okay)
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    finish(204)
}
