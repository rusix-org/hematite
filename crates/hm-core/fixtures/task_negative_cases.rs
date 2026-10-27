//! Ordinary untrusted native task supervisor and finite worker.
#![no_std]
#![no_main]
use core::{arch::global_asm, panic::PanicInfo};
#[allow(dead_code)]
#[path = "../../hm-kernel/src/startup_input.rs"]
mod startup_input;
#[allow(dead_code)]
#[path = "../../hm-kernel/src/task_wire.rs"]
mod task_wire;
const fn number(text: &str) -> u64 {
    let bytes = text.as_bytes();
    let mut result = 0;
    let mut i = 0;
    while i < bytes.len() { result = result * 10 + (bytes[i] - b'0') as u64; i += 1; }
    result
}
const CASE: u64 = number(env!("HM_TASK_NEGATIVE_CASE"));
// SAFETY: Fixed untrusted EL0 shims obey AAPCS64 and the versioned scalar wire.
global_asm!(include_str!("task_negative_cases.S"), case = const CASE);
#[repr(C)]
struct Reply {
    status: u64,
    result: u64,
}
// SAFETY: These local assembly functions preserve callee-saved registers and
// synchronously access only the bounded user buffers supplied below.
unsafe extern "C" {
    fn call_write(handle: u64, address: u64, length: u64) -> Reply;
    fn call_spawn(handle: u64, address: u64, length: u64) -> Reply;
    fn call_wait(handle: u64, mode: u64) -> Reply;
    fn call_cancel(handle: u64) -> Reply;
    fn call_exit(code: u64) -> Reply;
    fn worker_yield_four(handle: u64) -> u64;
    fn copy_job(source: *const u8, destination: *mut u8, length: usize);
    fn overwrite_job(destination: *mut u8, length: usize);
    fn run_loop() -> !;
}
#[used]
// SAFETY: Unique immutable symbol inspected independently in its read-only LOAD.
#[unsafe(no_mangle)]
static worker_message: [u8; 16] = *b"worker complete\n";
#[used]
// SAFETY: Unique immutable success marker inspected in the actual ELF.
#[unsafe(no_mangle)]
static supervisor_message: [u8; 20] = *b"supervisor complete\n";
#[used]
// SAFETY: Retain a nonempty independent writable LOAD; no shared kernel memory.
#[unsafe(link_section = ".fixture.data")]
static DATA: [u8; 1] = [0x5a];
fn finish(code: u64) -> ! {
    // SAFETY: Scalar native exit; a valid exit never resumes this user code.
    let _ = unsafe { call_exit(code) };
    loop {
        core::hint::spin_loop();
    }
}
fn demand(condition: bool) {
    if !condition {
        finish(201);
    }
}
fn scalar(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn output(handle: u64, bytes: &[u8]) {
    // SAFETY: Buffer remains live and immutable during the synchronous copied WRITE.
    let reply = unsafe { call_write(handle, bytes.as_ptr() as u64, bytes.len() as u64) };
    demand(reply.status == 0 && reply.result == bytes.len() as u64);
}
fn spawn(grant: u64, descriptor: &[u8; 64]) -> Reply {
    // SAFETY: Complete bounded descriptor remains live during synchronous SPAWN.
    unsafe { call_spawn(grant, descriptor.as_ptr() as u64, 64) }
}
fn child(reply: Reply) -> u64 {
    demand(reply.status == 0 && reply.result >> 32 != 0 && matches!(reply.result as u32, 3 | 4));
    reply.result
}
fn wait(handle: u64, expected: task_wire::Completion) {
    // SAFETY: Only scalar identity/mode are passed; no caller memory is retained.
    let reply = unsafe { call_wait(handle, 0) };
    demand(reply.status == 0 && task_wire::Completion::decode(reply.result) == Ok(expected));
}
fn ordinary(program: u32) -> [u8; 64] {
    let input = if program == 1 { Some((worker_message.as_ptr() as u64, 16)) } else { None };
    let mut descriptor = [0; 64];
    task_wire::Spawn::new(program, input, true, 128, task_wire::Rights::WaitCancel)
        .unwrap().encode_into(&mut descriptor).unwrap();
    descriptor
}
fn root_case(console: u64, input: &[u8]) -> ! {
    let grant = task_wire::Grant::decode(input).unwrap();
    demand(grant.console_allowed());
    match CASE {
        0 | 2 => {
            // The reviewed after-COMMIT hook must prevent this first reply.
            let _ = spawn(grant.handle(), &ordinary(1));
        }
        1 | 3 | 5 | 9 => {
            let token = child(spawn(grant.handle(), &ordinary(if CASE == 5 { 1 } else { 2 })));
            // SAFETY: Scalar owned completion; the selected hook must prevent
            // this blocking wait from ever returning to the root.
            let _ = unsafe { call_wait(token, 0) };
        }
        4 => {
            let _ = child(spawn(grant.handle(), &ordinary(2)));
            let _ = spawn(grant.handle(), &ordinary(1));
        }
        6 => {
            demand(grant.handle() == 0xffff_ffff_0000_0002 && console == 0x1_0000_0001);
            let rejected = spawn(grant.handle(), &ordinary(1));
            demand(rejected.status == 9 && rejected.result == 0);
            finish(0)
        }
        7 => {
            demand(grant.handle() == 0x1_0000_0002 && console == 0x1_0000_0001);
            let mut previous = 0;
            for generation in 2..=4 {
                let token = child(spawn(grant.handle(), &ordinary(1)));
                demand(token > previous && token >> 32 == generation && token as u32 == 3);
                previous = token;
                wait(token, task_wire::Completion::Exited(55));
            }
            let rejected = spawn(grant.handle(), &ordinary(1));
            demand(rejected.status == 9 && rejected.result == 0);
            finish(0)
        }
        8 => {
            let _ = child(spawn(grant.handle(), &ordinary(2)));
            for _ in 0..32 {
                // SAFETY: Invalid address is an opaque scalar; zero-byte WRITE
                // must validate the live console without touching user memory.
                let result = unsafe { call_write(console, u64::MAX, 0) };
                demand(result.status == 0 && result.result == 0);
            }
        }
        _ => finish(221),
    }
    finish(220)
}
fn worker(console: u64, input: &[u8]) -> ! {
    demand(input.len() == worker_message.len());
    // SAFETY: Fixed scalar console token; four actual zero WRITEs check both
    // reply words and preserve AAPCS64. They are not a timing guarantee.
    demand(unsafe { worker_yield_four(console) } == 1);
    let mut copied = [0; 16];
    // SAFETY: Validated startup input contains exactly16 bytes and destination
    // is a distinct owned array of that size. The opaque assembly copy happens
    // after the fourth actual SVC, before comparison/output, with no retained pointer.
    unsafe { copy_job(input.as_ptr(), copied.as_mut_ptr(), copied.len()) };
    demand(copied == worker_message);
    output(console, &copied);
    finish(55)
}
// SAFETY: Unique C ABI target of the fixed native entry assembly.
#[unsafe(no_mangle)]
extern "C" fn task_negative_main(startup: u64, case: u64) -> ! {
    demand(startup == 0x100100000 && case == CASE);
    // SAFETY: The fixed native startup page is present and RO throughout this
    // process lifetime; exact address is checked before the bounded view.
    let page = unsafe { core::slice::from_raw_parts(startup as *const u8, 4096) };
    let console = scalar(page, 24);
    demand(
        &page[..8] == b"HEMATITE"
            && page[8..12] == [1, 0, 0, 0]
            && page[12..16] == [64, 0, 0, 0]
            && scalar(page, 16) == 1
            && console >> 32 != 0
            && console as u32 == 1
            && scalar(page, 32) == 256
            && scalar(page, 40) == 4096
            && scalar(page, 48) == 64
            && scalar(page, 56) == 0,
    );
    let view = match startup_input::decode(&page[64..]) {
        Ok(value) => value,
        Err(_) => finish(203),
    };
    match case {
        100 => {
            demand(view.is_present());
            worker(console, view.bytes())
        }
        101 => {
            demand(!view.is_present());
            // SAFETY: The untrusted process deliberately loops in its own RX code;
            // only actual kernel timer/cancellation can regain control.
            unsafe { run_loop() }
        }
        0..=9 => { demand(view.is_present()); root_case(console, view.bytes()) },
        _ => finish(204),
    }
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    finish(205)
}
