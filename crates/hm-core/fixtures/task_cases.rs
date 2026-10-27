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
const CASE: u64 = number(env!("HM_TASK_CASE"));
const CHILD_FRAMES: u64 = number(env!("HM_TASK_CHILD_FRAMES"));
// SAFETY: Fixed untrusted EL0 entry/shims obey AAPCS64 and scalar native wire.
global_asm!(include_str!("task_cases.S"), case = const CASE, child_frames = const CHILD_FRAMES);
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
    fn fault_self() -> !;
    fn call_raw(version: u64, operation: u64, a: u64, b: u64, c: u64) -> Reply;
    fn overwrite_marker(console: u64) -> Reply;
    static mut descriptor_storage: [u8; 80];
    static mut input_storage: [u8; 272];
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
fn checked(reply: Reply, status: u64, result: u64) {
    demand(reply.status == status && reply.result == result);
}
fn raw(version: u64, op: u64, a: u64, b: u64, c: u64) -> Reply {
    // SAFETY: The deliberately malformed arguments are opaque syscall scalars;
    // only the kernel validates/dereferences them, never this fixture.
    unsafe { call_raw(version, op, a, b, c) }
}
fn cancel(token: u64, result: u64) {
    // SAFETY: Scalar completion token; kernel validates ownership.
    checked(unsafe { call_cancel(token) }, 0, result);
}
fn descriptor(program: u32, input: Option<(u64, u64)>, console: bool, frames: u64,
              rights: task_wire::Rights) -> [u8; 64] {
    let mut result = [0; 64];
    task_wire::Spawn::new(program, input, console, frames, rights).unwrap().encode_into(&mut result).unwrap();
    result
}
fn ordinary(program: u32) -> [u8; 64] {
    descriptor(program, if program == 1 { Some((worker_message.as_ptr() as u64, 16)) } else { None },
               true, 128, task_wire::Rights::WaitCancel)
}
fn recovery(grant: u64) {
    let token = child(spawn(grant, &ordinary(1)));
    wait(token, task_wire::Completion::Exited(55));
}
fn zero(console: u64) { checked(raw(1, 1, console, u64::MAX, 0), 0, 0); }
fn worker(console: u64, present: bool, input: &[u8]) -> ! {
    if input == b"HMFAULT\0" {
        // SAFETY: Deliberately faults only on this process's own RX entry.
        unsafe { fault_self() }
    }
    if input.len() == 32 && &input[..8] == b"HMAUTH\0\0" {
        let launch = scalar(input, 8);
        let sibling = scalar(input, 16);
        let parent_console = scalar(input, 24);
        for (op, a, b, c) in [(3, launch, 0, 64), (4, sibling, 0, 0),
            (5, sibling, 0, 0), (1, parent_console, 0, 0), (4, console, 0, 0),
            (5, console, 0, 0), (3, 0, 0, 64), (4, 0, 0, 0)] {
            checked(raw(1, op, a, b, c), 3, 0);
        }
        finish(0)
    }
    if console != 0 {
        // SAFETY: Fixed four actual checked zero WRITEs, with callee-saved state.
        demand(unsafe { worker_yield_four(console) } == 1);
        let mut copied = [0; 256];
        // SAFETY: Input is bounded by the checked startup codec; destination is
        // separate owned user storage. Opaque copy executes after actual yields.
        unsafe { copy_job(input.as_ptr(), copied.as_mut_ptr(), input.len()) };
        output(console, &copied[..input.len()]);
    }
    finish(if !present { 10 } else if input.is_empty() { 11 } else { 55 })
}
fn root_case(console: u64, grant: task_wire::Grant) -> ! {
    use task_wire::{Completion as C, Rights as R};
    let launch = grant.handle();
    match CASE {
        0 => supervisor(console, &{
            let mut bytes = [0; 64]; grant.encode_into(&mut bytes).unwrap(); bytes
        }),
        1 => {
            for size in [None, Some(0), Some(256)] {
                let mut bytes = [0u8; 256];
                for (index, byte) in bytes.iter_mut().enumerate() { *byte = index as u8; }
                let value = size.map(|n| (if n == 0 { 0 } else { bytes.as_ptr() as u64 }, n));
                let mut desc = descriptor(1, value, true, 128, R::WaitCancel);
                let token = child(spawn(launch, &desc));
                // SAFETY: Uniquely owned complete arrays; forced user stores are
                // followed by a separately named real SVC marker.
                unsafe { overwrite_job(bytes.as_mut_ptr(), bytes.len()); overwrite_job(desc.as_mut_ptr(), desc.len()); }
                // SAFETY: Scalar console token, no payload access for zero length.
                checked(unsafe { overwrite_marker(console) }, 0, 0);
                wait(token, C::Exited(match size { None => 10, Some(0) => 11, _ => 55 }));
            }
            output(console, b"copy complete\n");
        }
        2 => {
            let token = child(spawn(launch, &ordinary(2)));
            checked(raw(1, 4, token, 1, 0), 10, 0);
            cancel(token, 0); cancel(token, 1);
            wait(token, C::Cancelled);
            checked(raw(1, 4, token, 0, 0), 3, 0);
            checked(raw(1, 5, token, 0, 0), 3, 0);
        }
        3 => { let token = child(spawn(launch, &ordinary(2))); wait(token, C::Budget); }
        4 => {
            demand(!grant.console_allowed());
            checked(spawn(launch, &ordinary(1)), 8, 0);
            let token = child(spawn(launch, &descriptor(1, None, false, 128, R::WaitCancel)));
            wait(token, C::Exited(10));
        }
        25 => {
            let token = child(spawn(launch, &descriptor(1, None, false, 128, R::Wait)));
            checked(raw(1, 5, token, 0, 0), 8, 0);
            wait(token, C::Exited(10));
        }
        5 => {
            let sibling = child(spawn(launch, &ordinary(2)));
            let mut job = [0; 32]; job[..8].copy_from_slice(b"HMAUTH\0\0");
            for (index, value) in [launch, sibling, console].iter().enumerate() {
                job[8 + index * 8..16 + index * 8].copy_from_slice(&value.to_le_bytes());
            }
            let token = child(spawn(launch, &descriptor(1, Some((job.as_ptr() as u64, 32)), true, 128, R::WaitCancel)));
            wait(token, C::Exited(0)); cancel(sibling, 0); wait(sibling, C::Cancelled);
        }
        6 => {
            let old = child(spawn(launch, &ordinary(1))); wait(old, C::Exited(55));
            let new = child(spawn(launch, &ordinary(1))); demand(old != new);
            checked(raw(1, 4, old, 0, 0), 3, 0); checked(raw(1, 5, old, 0, 0), 3, 0);
            wait(new, C::Exited(55));
        }
        7 => {
            for (version, op, a, b, c, status) in [
                (2, 3, 0, 0, 0, 1), (1, 99, 0, 0, 0, 2), (1, 3, 0, u64::MAX, 0, 3),
                (1, 3, console, 0, 64, 3), (1, 3, 0xffff_ffff_0000_0002, 0, 64, 3),
                (1, 3, launch, 0, 63, 5), (1, 3, launch, 0, 65, 5),
                (1, 4, console, 99, 99, 3), (1, 5, launch, 0, 0, 3)] {
                checked(raw(version, op, a, b, c), status, 0);
            }
            recovery(launch);
        }
        8 => {
            for index in 0..17 {
                let mut desc = ordinary(1);
                match index {
                    0 => desc[0] = 2, 1 => desc[4] = 63, 2 => desc[12] |= 4,
                    3 => desc[44] = 1, 4 => desc[48] = 1, 5 => desc[56] = 1,
                    6 => desc[40] = 0, 7 => desc[40] = 2,
                    8 => desc[32..40].copy_from_slice(&0u64.to_le_bytes()),
                    9 => desc[32..40].copy_from_slice(&129u64.to_le_bytes()),
                    10 => { desc[12] = 2; desc[24..32].fill(0); },
                    11 => desc[24..32].fill(0),
                    12 => desc[8..12].fill(0), 13 => desc[8] = 3,
                    14 => desc[24..32].copy_from_slice(&257u64.to_le_bytes()),
                    15 => desc[16..24].copy_from_slice(&u64::MAX.to_le_bytes()),
                    16 => { desc[0] = 2; desc[24..32].copy_from_slice(&257u64.to_le_bytes()); },
                    _ => finish(206),
                }
                checked(spawn(launch, &desc), if index == 14 { 5 } else if index == 15 { 4 } else { 7 }, 0);
            }
            recovery(launch);
        }
        9 => {
            for address in [0, u64::MAX - 31, 0x1001f_ffe1] {
                // SAFETY: Invalid address is deliberately passed only as a scalar.
                checked(unsafe { call_spawn(launch, address, 64) }, 4, 0);
            }
            for (address, length) in [(0u64, 1u64), (u64::MAX, 2), (0x1001f_ffff, 2)] {
                let mut desc = ordinary(1);
                desc[16..24].copy_from_slice(&address.to_le_bytes());
                desc[24..32].copy_from_slice(&length.to_le_bytes());
                checked(spawn(launch, &desc), 4, 0);
            }
            // SAFETY: Single-threaded fixture owns these distinct user BSS
            // extents for its lifetime; offset one deliberately tests unaligned copy.
            let input = unsafe { &mut *core::ptr::addr_of_mut!(input_storage) };
            for (index, byte) in input[1..257].iter_mut().enumerate() { *byte = index as u8; }
            let desc = descriptor(1, Some((input.as_ptr() as u64 + 1, 256)), true, 128, R::WaitCancel);
            // SAFETY: Disjoint named fixture storage; exclusive single-thread use.
            let storage = unsafe { &mut *core::ptr::addr_of_mut!(descriptor_storage) };
            storage[1..65].copy_from_slice(&desc);
            // SAFETY: All64 descriptor bytes are live, intentionally unaligned.
            let token = child(unsafe { call_spawn(launch, storage.as_ptr() as u64 + 1, 64) });
            wait(token, C::Exited(55));
        }
        10 => {
            let a = child(spawn(launch, &descriptor(1, Some((b"HMFAULT\0".as_ptr() as u64, 8)), true, 128, R::WaitCancel)));
            let b = child(spawn(launch, &ordinary(1))); wait(a, C::Fault); wait(b, C::Exited(55));
        }
        11 => {
            let a = child(spawn(launch, &ordinary(2))); let b = child(spawn(launch, &ordinary(2)));
            cancel(a, 0); cancel(b, 0); checked(spawn(launch, &ordinary(1)), 9, 0);
            wait(a, C::Cancelled); let new = child(spawn(launch, &ordinary(1)));
            checked(raw(1, 4, a, 0, 0), 3, 0); wait(b, C::Cancelled); wait(new, C::Exited(55));
        }
        12 => {
            let mut desc = ordinary(1); desc[32..40].copy_from_slice(&(CHILD_FRAMES - 1).to_le_bytes());
            checked(spawn(launch, &desc), 9, 0);
            desc[32..40].copy_from_slice(&CHILD_FRAMES.to_le_bytes());
            let token = child(spawn(launch, &desc)); wait(token, C::Exited(55));
        }
        13 => {
            let first = child(spawn(launch, &ordinary(1))); checked(spawn(launch, &ordinary(1)), 9, 0);
            wait(first, C::Exited(55)); recovery(launch);
        }
        14 => { for _ in 0..3 { checked(spawn(launch, &ordinary(1)), 9, 0); } recovery(launch); }
        15..=19 => {
            let _ = child(spawn(launch, &ordinary(1))); let _ = child(spawn(launch, &ordinary(2)));
            if CASE == 15 { finish(17); }
            if CASE == 16 {
                // SAFETY: Own-RX deliberate fault, no kernel address dependence.
                unsafe { fault_self() }
            }
            if CASE == 18 {
                for _ in 0..61 { zero(console); }
                let _ = spawn(launch, &ordinary(1)); finish(207);
            }
            // SAFETY: CASE17 consumes actual active budget; CASE19 driver cancels
            // at preceding finalized boundary, so this loop must never be entered.
            unsafe { run_loop() }
        }
        20 | 24 => {
            let token = child(spawn(launch, &ordinary(2)));
            // SAFETY: Driver cancels/drops at actual Waiting boundary; no reply expected.
            let _ = unsafe { call_wait(token, 0) }; finish(208);
        }
        21 => {
            for _ in 0..63 { zero(console); }
            let _ = spawn(launch, &ordinary(1)); finish(209);
        }
        22 | 23 => {
            let token = child(spawn(launch, &ordinary(2)));
            for _ in 0..62 { zero(console); }
            let _ = raw(1, if CASE == 22 { 4 } else { 5 }, token, 0, 0); finish(210);
        }
        _ => finish(211),
    }
    finish(0)
}
// SAFETY: Unique C ABI entry target; startup is checked before any user view.
#[unsafe(no_mangle)]
extern "C" fn task_case_main(startup: u64, case: u64) -> ! {
    demand(startup == 0x100100000 && case == CASE);
    // SAFETY: Fixed startup mapping present and RO throughout this process lifetime.
    let page = unsafe { core::slice::from_raw_parts(startup as *const u8, 4096) };
    let console = scalar(page, 24);
    demand(&page[..8] == b"HEMATITE" && page[8..12] == [1, 0, 0, 0] && page[12..16] == [64, 0, 0, 0]
        && scalar(page, 16) == u64::from(console != 0)
        && (console == 0 || (console >> 32 != 0 && console as u32 == 1))
        && scalar(page, 32) == 256 && scalar(page, 40) == 4096 && scalar(page, 48) == 64 && scalar(page, 56) == 0);
    let view = startup_input::decode(&page[64..]).unwrap();
    match CASE { 100 => worker(console, view.is_present(), view.bytes()),
        // SAFETY: This arm deliberately loops only inside its own RX mapping.
        101 => { demand(!view.is_present()); unsafe { run_loop() } },
        _ => {
            demand(view.is_present() && console != 0);
            let grant = task_wire::Grant::decode(view.bytes()).unwrap(); root_case(console, grant)
        }
    }
}
#[panic_handler]
fn panic(_: &PanicInfo<'_>) -> ! { finish(212) }
