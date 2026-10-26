use core::arch::{asm, global_asm};
use hm_kernel::{BootChecks, Platform};

// SAFETY: The PVH-only entry owns its linker storage and establishes the
// mappings, stack, CPU state and ABI required by boot_main before calling it.
global_asm!(include_str!("boot.S"), rust_entry = sym super::boot_main);

// SAFETY: Linker/boot.S define these exact-sized, aligned storage objects.
// Page tables are mutable because hardware can update Accessed/Dirty bits.
// No Rust references to that mutable hardware storage are ever constructed.
// Address-only linker markers are zero-sized, not immutable byte objects.
unsafe extern "C" {
    static mut pml4: [u64; 512];
    static mut pdpt: [u64; 512];
    static mut page_directory: [u64; 512];
    static mut page_tables: [u64; 4096];
    static __text_start: [u8; 0];
    static __text_end: [u8; 0];
    static __rodata_start: [u8; 0];
    static __rodata_end: [u8; 0];
    static __stack_guard: [u8; 0];
}

// Only boot_main constructs this capability. No port numbers, pointers or
// mapping operations are exposed to hm-kernel. Bootstrap is single-CPU with
// interrupts disabled; safe policy cannot enable interrupts or spawn tasks.
pub(super) struct BootPlatform;

impl Platform for BootPlatform {
    fn write(&mut self, text: &str) {
        for byte in text.bytes() {
            if byte == b'\n' {
                serial_byte(b'\r');
            }
            serial_byte(byte);
        }
    }

    fn read_byte(&mut self) -> Option<u8> {
        // SAFETY: COM1 status/data are non-DMA UART ports in the pinned guest.
        unsafe {
            if port_read8(0x3fd) & 1 != 0 {
                Some(port_read8(0x3f8))
            } else {
                None
            }
        }
    }

    fn checks(&self) -> BootChecks {
        let cr0: u64;
        let cr3: u64;
        let cr4: u64;
        let efer_low: u32;
        let efer_high: u32;
        // SAFETY: CPL0 bootstrap can read these registers. No memory or CPU
        // control state is changed; EFER is an architectural readable MSR.
        unsafe {
            asm!("mov {}, cr0", out(reg) cr0, options(nomem, nostack, preserves_flags));
            asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
            asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
            asm!("rdmsr", in("ecx") 0xc0000080u32,
                 out("eax") efer_low, out("edx") efer_high,
                 options(nomem, nostack, preserves_flags));
        }
        let _ = efer_high;
        let text_start = core::ptr::addr_of!(__text_start) as usize;
        let text_end = core::ptr::addr_of!(__text_end) as usize;
        let ro_start = core::ptr::addr_of!(__rodata_start) as usize;
        let ro_end = core::ptr::addr_of!(__rodata_end) as usize;
        let guard = core::ptr::addr_of!(__stack_guard) as usize;
        let root = core::ptr::addr_of!(pml4) as usize;
        let pdpt_addr = core::ptr::addr_of!(pdpt) as usize;
        let pd_addr = core::ptr::addr_of!(page_directory) as usize;
        let pt_addr = core::ptr::addr_of!(page_tables) as usize;
        let mut wx = cr3 == root as u64;
        // SAFETY: Each linker-reserved table is 4096-aligned and mapped. The
        // shim initializes it before Rust, no code can mutate it thereafter.
        // Volatile reads avoid assumptions about hardware Accessed/Dirty bits.
        unsafe {
            wx &= table_link(
                core::ptr::addr_of!(pml4).cast::<u64>().read_volatile(),
                pdpt_addr,
            );
            wx &= table_link(
                core::ptr::addr_of!(pdpt).cast::<u64>().read_volatile(),
                pd_addr,
            );
            for index in 1..512usize {
                wx &= core::ptr::addr_of!(pml4)
                    .cast::<u64>()
                    .add(index)
                    .read_volatile()
                    == 0;
                wx &= core::ptr::addr_of!(pdpt)
                    .cast::<u64>()
                    .add(index)
                    .read_volatile()
                    == 0;
            }
            for index in 8..512usize {
                wx &= core::ptr::addr_of!(page_directory)
                    .cast::<u64>()
                    .add(index)
                    .read_volatile()
                    == 0;
            }
            for index in 0..8usize {
                wx &= table_link(
                    core::ptr::addr_of!(page_directory)
                        .cast::<u64>()
                        .add(index)
                        .read_volatile(),
                    pt_addr + index * 4096,
                );
            }
        }
        for index in 0..4096usize {
            let address = index * 4096;
            let entry = leaf(index);
            if address == 0 || address == guard {
                wx &= entry == 0;
                continue;
            }
            let executable = (text_start..text_end).contains(&address);
            let writable = !executable && !(ro_start..ro_end).contains(&address);
            let expected = address as u64
                | 1
                | if writable { 2 } else { 0 }
                | if executable { 0 } else { 1u64 << 63 };
            // Hardware may set A/D; every other bit must match, including U/S.
            wx &= entry & !0x60 == expected;
        }
        BootChecks {
            write_protect: cr0 & (1 << 16) != 0,
            nx: efer_low & (1 << 11) != 0,
            user_execute_protect: cr4 & (1 << 20) != 0,
            user_access_protect: cr4 & (1 << 21) != 0,
            wx,
            stack_guard: cr3 == root as u64 && leaf(guard / 4096) == 0,
            null_guard: cr3 == root as u64 && leaf(0) == 0,
        }
    }
}

fn table_link(entry: u64, address: usize) -> bool {
    entry & !0x60 == address as u64 | 3
}

fn leaf(index: usize) -> u64 {
    if index >= 4096 {
        return u64::MAX;
    }
    // SAFETY: Bounds check selects one of the 4096 initialized leaf entries
    // in linker-owned page_tables. No references alias this hardware storage.
    unsafe {
        core::ptr::addr_of!(page_tables)
            .cast::<u64>()
            .add(index)
            .read_volatile()
    }
}

// SAFETY contract: port must be a byte-wide register of the fixed bootstrap
// UART, and the operation must not program DMA or change memory mappings.
unsafe fn port_write8(port: u16, value: u8) {
    // SAFETY: Caller establishes port authority; instruction only writes it.
    unsafe {
        asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags))
    }
}

// SAFETY contract: port must be a readable byte-wide bootstrap UART register.
unsafe fn port_read8(port: u16) -> u8 {
    let value;
    // SAFETY: Caller establishes readable port authority; no memory access.
    unsafe {
        asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags))
    }
    value
}

pub(super) fn init() -> BootPlatform {
    // SAFETY: These constants configure only COM1 (115200 baud, 8N1, FIFO).
    // Bootstrap owns the UART; interrupts remain disabled and no DMA exists.
    unsafe {
        port_write8(0x3f9, 0);
        port_write8(0x3fb, 0x80);
        port_write8(0x3f8, 1);
        port_write8(0x3f9, 0);
        port_write8(0x3fb, 3);
        port_write8(0x3fa, 0xc7);
        port_write8(0x3fc, 3);
    }
    BootPlatform
}

fn serial_byte(byte: u8) {
    for _ in 0..100_000 {
        // SAFETY: Fixed COM1 line-status and transmit registers; never DMA.
        unsafe {
            if port_read8(0x3fd) & 0x20 != 0 {
                port_write8(0x3f8, byte);
                return;
            }
        }
        core::hint::spin_loop();
    }
    // Missing output is detected by the host harness; never report a false
    // success by hanging forever waiting for a missing UART.
    exit(false);
}

pub(super) fn exit(success: bool) -> ! {
    let value: u32 = if success { 0x10 } else { 0x11 };
    // SAFETY: Port 0xf4 is the declared QEMU debug-exit device; the value
    // terminates only this VM. No arbitrary port capability is exposed.
    unsafe { asm!("out dx, eax", in("dx") 0xf4u16, in("eax") value, options(nomem, nostack)) }
    loop {
        // SAFETY: No scheduling exists; interrupts are intentionally disabled.
        unsafe { asm!("cli", "hlt", options(nomem, nostack)) }
    }
}

pub(super) const BANNER: &str = "hematite: x86_64 Rust kernel entered via PVH\n";
