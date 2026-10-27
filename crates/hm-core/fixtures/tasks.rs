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
#[cfg(task = "supervisor")]
const TASK: u64 = 0;
#[cfg(task = "worker")]
const TASK: u64 = 1;
#[cfg(task = "loop")]
const TASK: u64 = 2;
// SAFETY: Fixed untrusted EL0 entry/shims obey AAPCS64 and the versioned scalar wire.
global_asm!(include_str!("tasks.S"), task = const TASK);
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
fn supervisor(console: u64, input: &[u8]) -> ! {
    let grant = match task_wire::Grant::decode(input) {
        Ok(value) => value,
        Err(_) => finish(202),
    };
    demand(grant.console_allowed());
    let mut job = worker_message;
    let mut descriptor = [0; 64];
    let first = task_wire::Spawn::new(
        1,
        Some((job.as_ptr() as u64, job.len() as u64)),
        true,
        128,
        task_wire::Rights::WaitCancel,
    )
    .unwrap();
    first.encode_into(&mut descriptor).unwrap();
    let worker = child(spawn(grant.handle(), &descriptor));
    // SAFETY: Both arrays are uniquely owned user storage with these exact
    // extents. Explicit assembly stores cannot disappear as dead Rust stores.
    unsafe {
        overwrite_job(job.as_mut_ptr(), job.len());
        overwrite_job(descriptor.as_mut_ptr(), descriptor.len());
    }
    task_wire::Spawn::new(2, None, true, 128, task_wire::Rights::WaitCancel)
        .unwrap()
        .encode_into(&mut descriptor)
        .unwrap();
    let busy = child(spawn(grant.handle(), &descriptor));
    demand(worker != busy);
    let rejected = spawn(grant.handle(), &descriptor);
    demand(rejected.status == 9 && rejected.result == 0);
    // Immediate and genuinely parked completion have the same defined reply.
    wait(worker, task_wire::Completion::Exited(55));
    // SAFETY: Valid owned child token and zero reserved scalar arguments.
    let cancelled = unsafe { call_cancel(busy) };
    demand(cancelled.status == 0 && cancelled.result == 0);
    // Round robin services the child's cancellation before the parent's next entry.
    // SAFETY: Repeating cancellation observes the still-owned held completion.
    let held = unsafe { call_cancel(busy) };
    demand(held.status == 0 && held.result == 1);
    wait(busy, task_wire::Completion::Cancelled);
    output(console, &supervisor_message);
    finish(0)
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
extern "C" fn task_main(startup: u64, task: u64) -> ! {
    demand(startup == 0x100100000 && task == TASK);
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
    match task {
        0 => {
            demand(view.is_present());
            supervisor(console, view.bytes())
        }
        1 => {
            demand(view.is_present());
            worker(console, view.bytes())
        }
        2 => {
            demand(!view.is_present());
            // SAFETY: The untrusted process deliberately loops in its own RX code;
            // only actual kernel timer/cancellation can regain control.
            unsafe { run_loop() }
        }
        _ => finish(204),
    }
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! {
    finish(205)
}
