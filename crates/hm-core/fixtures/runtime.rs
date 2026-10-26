//! Independently linked untrusted native fixtures for real resumable groups.
#![no_std]
#![no_main]
use core::arch::global_asm;
use core::panic::PanicInfo;
#[path = "../../hm-kernel/src/startup_input.rs"]
#[allow(dead_code)]
mod startup_input;

#[cfg(probe = "context_svc_a")] const KIND: u64 = 0;
#[cfg(probe = "context_svc_b")] const KIND: u64 = 1;
#[cfg(probe = "context_irq_a")] const KIND: u64 = 2;
#[cfg(probe = "context_irq_b")] const KIND: u64 = 3;
#[cfg(probe = "loop")] const KIND: u64 = 4;
#[cfg(probe = "hello")] const KIND: u64 = 5;
#[cfg(probe = "byte_budget")] const KIND: u64 = 6;
#[cfg(probe = "call_limit")] const KIND: u64 = 7;
#[cfg(probe = "fault")] const KIND: u64 = 8;
#[cfg(probe = "yield")] const KIND: u64 = 9;
#[cfg(probe = "grant")] const KIND: u64 = 10;

// SAFETY: Entry, fixed native-wire shims and deliberate faults execute only in
// untrusted EL0. Kernel soundness never relies on these input bytes.
global_asm!(include_str!("runtime.S"), kind = const KIND);
#[repr(C)]
struct Reply { status: u64, bytes: u64 }
// SAFETY: These assembly definitions obey AAPCS64; static storage belongs only
// to this isolated process. No pointer here addresses kernel storage.
unsafe extern "C" {
    fn call_write(handle: u64, address: u64, length: u64) -> Reply;
    fn call_exit(code: u64) -> Reply;
    fn own_fault() -> !;
    fn run_loop() -> !;
    static mut private_data: u64;
}
#[used]
// SAFETY: Fixed distinct ELF symbols in read-only input storage.
#[unsafe(no_mangle)]
static message: [u8; 15] = *b"hello from EL0\n";
#[used]
// SAFETY: Fixed distinct read-only symbol for exact native grant-probe output.
#[unsafe(no_mangle)]
static grant_message: [u8; 12] = *b"grant owned\n";
#[used]
// SAFETY: Fixed symbol names one retained immutable 256-byte output pattern.
#[unsafe(no_mangle)]
static pattern: [u8; 256] = make_pattern();
const fn make_pattern() -> [u8; 256] {
    let mut result = [0; 256]; let mut i = 0;
    while i < result.len() { result[i] = i as u8; i += 1; }
    result
}
fn finish(code: u64) -> ! {
    // SAFETY: Scalar native EXIT wire; a successful exit cannot return.
    let _ = unsafe { call_exit(code) };
    loop { core::hint::spin_loop(); }
}
fn demand(value: bool) { if !value { finish(201); } }
fn request(handle: u64, address: u64, length: usize, status: u64, written: usize) {
    // SAFETY: Fixed scalar wire; kernel validates even deliberately bad inputs.
    let reply = unsafe { call_write(handle, address, length as u64) };
    demand(reply.status == status && reply.bytes == written as u64);
}
fn output(handle: u64, bytes: &[u8]) {
    request(handle, bytes.as_ptr() as u64, bytes.len(), 0, bytes.len());
}
fn word(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}
fn startup() -> (u64, startup_input::View<'static>) {
    // SAFETY: This fixture requires the existing fixed immutable startup page;
    // all 4096 bytes remain mapped while this untrusted process is alive.
    let bytes = unsafe { core::slice::from_raw_parts(0x100100000 as *const u8, 4096) };
    demand(&bytes[..8] == b"HEMATITE" && bytes[8..16] == [1, 0, 0, 0, 64, 0, 0, 0]);
    let handle = word(bytes, 24);
    demand(word(bytes, 16) == 1 && handle >> 32 != 0 && handle as u32 == 1
        && word(bytes, 32) == 256 && word(bytes, 40) == 4096 && word(bytes, 48) == 64
        && word(bytes, 56) == 0);
    let view = match startup_input::decode(&bytes[64..]) { Ok(view) => view, Err(_) => finish(201) };
    (handle, view)
}
// SAFETY: Unique assembly-called symbol; pointer is the fixture's fixed private
// stack capture area, already populated by all explicit assembly stores.
#[unsafe(no_mangle)]
extern "C" fn context_initial(pointer: *const u64) -> u64 {
    demand(pointer as u64 == 0x1001ffc00);
    // SAFETY: Fixed 104 initialized, aligned words lie within the owned stack.
    let values = unsafe { core::slice::from_raw_parts(pointer, 104) };
    demand(values[0] == 0x100100000 && values[1..31].iter().all(|&x| x == 0));
    demand(values[31] == 0x100200000 && values[32..].iter().all(|&x| x == 0));
    let (_, input) = startup(); demand(!input.is_present());
    1
}
// SAFETY: Assembly has populated the same explicit immutable capture wire and
// branches here without a Rust caller. This function exits instead of returning.
#[unsafe(no_mangle)]
extern "C" fn context_after(pointer: *const u64) -> ! {
    demand(pointer as u64 == 0x1001ffc00);
    // SAFETY: Assembly initialized all104 aligned words within the private stack.
    let words = unsafe { core::slice::from_raw_parts(pointer, 104) };
    let seed = (KIND & 1) + 1;
    for (i, &actual) in words[..31].iter().enumerate() {
        let mut expected = 0x1111000000000000 | (seed << 8) | i as u64;
        if KIND < 2 {
            expected = match i { 0 | 1 | 3 => 0, 2 => u64::MAX, 8 => 1, _ => expected };
        } else {
            let scratch = if KIND == 2 {17} else {18};
            let address = if KIND == 2 {18} else {17};
            if i == scratch { expected = 1; }
            if i == address { expected = 0x100002000; }
        }
        demand(actual == expected);
    }
    demand(words[31] == 0x1001ffc00 && words[32] == if seed == 1 {0xa0000000} else {0x50000000});
    for i in 0..32 {
        demand(words[33 + 2 * i] == 0x2222000000000000 | (seed << 8) | i as u64);
        demand(words[34 + 2 * i] == 0x3333000000000000 | (seed << 8) | i as u64);
    }
    demand(words[97] == seed << 22 && words[98] == 0x08000000 | seed);
    demand(words[99] == 0x4444000000000000 | seed && words[100..].iter().all(|&x| x == 0));
    // SAFETY: This fixture owns the dedicated writable data mapping. Volatile
    // read observes real private storage after the peer's actual execution.
    demand(unsafe { core::ptr::read_volatile(core::ptr::addr_of!(private_data)) } == seed);
    let (handle, _) = startup();
    // SAFETY: The same initialized capture is emitted byte-for-byte; no padding
    // or compiler struct representation is exposed, only explicit wire stores.
    let bytes = unsafe { core::slice::from_raw_parts(pointer.cast::<u8>(), 832) };
    for chunk in bytes.chunks(256) { output(handle, chunk); }
    finish(0)
}
// SAFETY: Unique fixed ELF entry branches here with startup address in x0.
#[unsafe(no_mangle)]
extern "C" fn runtime_main(address: u64) -> ! {
    demand(address == 0x100100000);
    let (handle, input) = startup();
    if KIND != 10 { demand(!input.is_present()); }
    match KIND {
        // SAFETY: Deliberate infinite loop runs only in this untrusted process.
        4 => unsafe { run_loop() },
        5 => { output(handle, &message); finish(0) }
        6 => {
            for _ in 0..16 { output(handle, &pattern); }
            request(handle, pattern.as_ptr() as u64, 1, 6, 0);
            request(handle, u64::MAX, 0, 0, 0);
            finish(0)
        }
        7 => {
            for _ in 0..64 { request(handle, u64::MAX, 0, 0, 0); }
            finish(202) // A correct64th-call retirement never returns here.
        }
        // SAFETY: Deliberate write targets this process's own immutable entry.
        8 => unsafe { own_fault() },
        9 => { request(handle, u64::MAX, 0, 0, 0); finish(203) }
        10 => {
            demand(input.is_present() && input.bytes().len() == 8);
            let foreign = word(input.bytes(), 0); demand(foreign != handle);
            request(foreign, u64::MAX, 0, 3, 0);
            output(handle, &grant_message); finish(0)
        }
        _ => finish(204),
    }
}
#[panic_handler]
fn panic(_: &PanicInfo) -> ! { finish(201) }
