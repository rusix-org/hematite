use core::arch::{asm, global_asm};
use hm_kernel::{BootChecks, Platform};

// SAFETY: Direct-ELF entry checks EL1/reset state, owns all linker storage,
// establishes the MMU and stack, then calls the shared Rust entry at its ABI.
global_asm!(include_str!("boot-aarch64.S"), rust_entry = sym super::boot_main);

// SAFETY: These objects have exact sizes/alignment supplied by the linker and
// assembly. Tables are mutable hardware storage; no Rust references escape.
// Address-only linker markers are zero-sized, not immutable byte objects.
unsafe extern "C" {
    static mut arm_root: [u64; 512];
    static mut arm_device_l2: [u64; 512];
    static mut arm_device_l3: [u64; 512];
    static mut arm_ram_l2: [u64; 512];
    static mut arm_ram_l3: [u64; 4096];
    static __text_start: [u8; 0];
    static __text_end: [u8; 0];
    static __rodata_start: [u8; 0];
    static __rodata_end: [u8; 0];
    static __stack_guard: [u8; 0];
    static __kernel_end: [u8; 0];
}

const UART: usize = 0x0900_0000;
const RAM: usize = 0x4000_0000;
const PAGE_RW_NX: u64 = 0x0060_0000_0000_0703;
const PAGE_RO_NX: u64 = 0x0060_0000_0000_0783;
const PAGE_RX: u64 = 0x0040_0000_0000_0783;
const UART_PAGE: u64 = 0x0060_0000_0900_0407;

pub(super) struct BootPlatform {
    memory: hm_kernel::boot_memory::MemoryMap,
    pool: Option<super::frames::Pool>,
}
pub(super) const BANNER: &str = "hematite: ARM64 Rust kernel entered at EL1\n";

impl Platform for BootPlatform {
    type Frame<'a> = super::frames::Frame<'a>;

    fn allocate_frame(&self) -> Result<Self::Frame<'_>, hm_kernel::frame_api::FrameError> {
        self.pool
            .as_ref()
            .ok_or(hm_kernel::frame_api::FrameError::Unsupported)?
            .allocate()
    }

    fn frame_stats(&self) -> Option<hm_kernel::frame_api::FrameStats> {
        self.pool.as_ref().map(super::frames::Pool::stats)
    }

    fn frame_exhaustion_check(&self) -> bool {
        self.pool
            .as_ref()
            .is_some_and(super::frames::Pool::exhaustion_check)
    }

    fn boot_memory(&self) -> Option<&hm_kernel::boot_memory::MemoryMap> {
        Some(&self.memory)
    }

    fn write(&mut self, text: &str) {
        for byte in text.bytes() {
            if byte == b'\n' {
                serial_byte(b'\r');
            }
            serial_byte(byte);
        }
    }

    fn read_byte(&mut self) -> Option<u8> {
        // SAFETY: Only the mapped PL011 flag/data registers are accessed as
        // aligned u32 volatile MMIO. They cannot initiate DMA or change maps.
        unsafe {
            if ((UART + 0x18) as *const u32).read_volatile() & (1 << 4) == 0 {
                Some((UART as *const u32).read_volatile() as u8)
            } else {
                None
            }
        }
    }

    fn checks(&self) -> BootChecks {
        let current_el: u64;
        let sctlr: u64;
        let tcr: u64;
        let mair: u64;
        let ttbr: u64;
        let pan: u64;
        // SAFETY: Entry checked EL1 and PAN support. These architectural
        // register reads do not change CPU state or dereference memory.
        unsafe {
            asm!("mrs {}, CurrentEL", out(reg) current_el, options(nomem, nostack, preserves_flags));
            asm!("mrs {}, sctlr_el1", out(reg) sctlr, options(nomem, nostack, preserves_flags));
            asm!("mrs {}, tcr_el1", out(reg) tcr, options(nomem, nostack, preserves_flags));
            asm!("mrs {}, mair_el1", out(reg) mair, options(nomem, nostack, preserves_flags));
            asm!("mrs {}, ttbr0_el1", out(reg) ttbr, options(nomem, nostack, preserves_flags));
            // Architectural encoding of PAN, usable with the baseline target.
            asm!("mrs {}, S3_0_C4_C2_3", out(reg) pan, options(nomem, nostack, preserves_flags));
        }
        let root = core::ptr::addr_of!(arm_root) as u64;
        let dev_l2 = core::ptr::addr_of!(arm_device_l2) as u64;
        let dev_l3 = core::ptr::addr_of!(arm_device_l3) as u64;
        let ram_l2 = core::ptr::addr_of!(arm_ram_l2) as u64;
        let ram_l3 = core::ptr::addr_of!(arm_ram_l3) as u64;
        let text = (core::ptr::addr_of!(__text_start) as usize)
            ..(core::ptr::addr_of!(__text_end) as usize);
        let rodata = (core::ptr::addr_of!(__rodata_start) as usize)
            ..(core::ptr::addr_of!(__rodata_end) as usize);
        let guard = core::ptr::addr_of!(__stack_guard) as usize;
        let mut mappings = ttbr == root && tcr == 0x8099_3519 && mair == 0xff;
        // SAFETY: Each loop index lies within its named linker-owned array.
        // These initialized tables are mapped and not mutated by safe policy;
        // raw volatile access respects their status as hardware-owned storage.
        unsafe {
            for index in 0..512usize {
                let root_expected = match index {
                    0 => dev_l2 | 3,
                    1 => ram_l2 | 3,
                    _ => 0,
                };
                mappings &= core::ptr::addr_of!(arm_root)
                    .cast::<u64>()
                    .add(index)
                    .read_volatile()
                    == root_expected;
                let dev_expected = if index == 72 { dev_l3 | 3 } else { 0 };
                mappings &= core::ptr::addr_of!(arm_device_l2)
                    .cast::<u64>()
                    .add(index)
                    .read_volatile()
                    == dev_expected;
                let uart_expected = if index == 0 { UART_PAGE } else { 0 };
                mappings &= core::ptr::addr_of!(arm_device_l3)
                    .cast::<u64>()
                    .add(index)
                    .read_volatile()
                    == uart_expected;
                let ram_expected = if index < 8 {
                    (ram_l3 + index as u64 * 4096) | 3
                } else {
                    0
                };
                mappings &= core::ptr::addr_of!(arm_ram_l2)
                    .cast::<u64>()
                    .add(index)
                    .read_volatile()
                    == ram_expected;
            }
        }
        for index in 0..4096usize {
            let address = RAM + index * 4096;
            let expected = if address == guard {
                0
            } else {
                address as u64
                    | if text.contains(&address) {
                        PAGE_RX
                    } else if rodata.contains(&address) {
                        PAGE_RO_NX
                    } else {
                        PAGE_RW_NX
                    }
            };
            mappings &= leaf(index) == expected;
        }
        let mmu = current_el == 4 && sctlr & 1 != 0;
        BootChecks {
            write_protect: mmu && mappings,
            nx: mmu && sctlr & (1 << 19) != 0 && mappings,
            // Bootstrap only: no EL0 mappings; every leaf is UXN, with PXN on
            // writable pages. This is not a claim of SMEP-equivalent user MM.
            user_execute_protect: mmu && mappings,
            user_access_protect: current_el == 4 && pan & (1 << 22) != 0 && sctlr & (1 << 23) == 0,
            wx: mmu && sctlr & (1 << 19) != 0 && mappings,
            stack_guard: mmu && mappings && leaf((guard - RAM) / 4096) == 0,
            // Root[0] has only the UART page. Full mapping check proves no
            // translation for virtual address zero or any alias of that page.
            null_guard: mmu && mappings,
        }
    }
}

fn leaf(index: usize) -> u64 {
    if index >= 4096 {
        return u64::MAX;
    }
    // SAFETY: Bounds select an entry in the initialized, mapped RAM leaf array;
    // all access is raw/volatile, without immutable aliases to hardware tables.
    unsafe {
        core::ptr::addr_of!(arm_ram_l3)
            .cast::<u64>()
            .add(index)
            .read_volatile()
    }
}

pub(super) fn init() -> BootPlatform {
    match initialize() {
        Ok(platform) => platform,
        Err(_) => {
            for byte in b"Boot memory rejected\r\n" {
                serial_byte(*byte);
            }
            exit(false)
        }
    }
}

fn initialize() -> Result<BootPlatform, ()> {
    use hm_kernel::boot_memory::Region;
    use hm_kernel::frame_plan::Plan;
    let memory = discover_memory()?;
    let mut platform = BootPlatform { memory, pool: None };
    let checks = platform.checks();
    if !checks.write_protect
        || !checks.wx
        || !checks.nx
        || !checks.stack_guard
        || !checks.user_access_protect
        || !checks.user_execute_protect
        || !checks.null_guard
    {
        return Err(());
    }
    let end = core::ptr::addr_of!(__kernel_end) as u64;
    if !(0x4020_0000..=0x4100_0000).contains(&end)
        || !platform
            .memory
            .ram()
            .iter()
            .any(|r| r.start <= RAM as u64 && r.end >= end)
    {
        return Err(());
    }
    let mut excluded = [Region { start: 0, end: 0 }; 34];
    excluded[0] = Region {
        start: RAM as u64,
        end: 0x4020_0000,
    };
    excluded[1] = Region {
        start: 0x4020_0000,
        end,
    };
    let reserved = platform.memory.reservations();
    excluded
        .get_mut(2..2 + reserved.len())
        .ok_or(())?
        .copy_from_slice(reserved);
    let plan = Plan::build(
        Region {
            start: RAM as u64,
            end: 0x4100_0000,
        },
        platform.memory.ram(),
        &excluded[..2 + reserved.len()],
    )
    .map_err(|_| ())?;
    // SAFETY: Actual QEMU boot metadata covers the live kernel and DTB. The
    // fixed profile has no initrd/firmware runtime or DMA. Plan admits only
    // advertised full RAM pages in the checked identity map, excluding every
    // reservation, all kernel/table/stack storage and the entire DTB prefix.
    // checks proved the full mapping tree: eligible leaves are exclusive
    // supervisor RW/NX normal RAM, with no alias. One CPU/interrupts masked;
    // no API changes mappings. Constructor permanently claims singleton use.
    // SAFETY: The checks and exclusions above prove the constructor contract;
    // repeated initialization is rejected before any physical memory access.
    platform.pool = Some(unsafe { super::frames::Pool::from_boot(plan) }.map_err(|_| ())?);
    Ok(platform)
}

fn discover_memory() -> Result<hm_kernel::boot_memory::MemoryMap, ()> {
    const LIMIT: usize = 2 * 1024 * 1024;
    // SAFETY: The pinned QEMU virt ELF profile initializes RAM at RAM. The
    // first 40 bytes are identity mapped, outside all linker-owned storage.
    // One CPU, masked interrupts and no DMA/writers keep this temporary
    // shared borrow immutable. No reference escapes this function.
    let header = unsafe { core::slice::from_raw_parts(RAM as *const u8, 40) };
    let size = u32::from_be_bytes([header[4], header[5], header[6], header[7]]) as usize;
    if !(40..=LIMIT).contains(&size) {
        return Err(());
    }
    // SAFETY: Size was bounded by the independent 2 MiB pre-kernel window,
    // not by the tree's RAM claims. QEMU initializes this RAM, bootstrap maps
    // it, and the linker reserves the complete window from code/tables/stack.
    // No allocator or concurrent writer exists; parsing returns owned values.
    let blob = unsafe { core::slice::from_raw_parts(RAM as *const u8, size) };
    hm_kernel::boot_memory::parse(blob).map_err(|_| ())
}

fn serial_byte(byte: u8) {
    for _ in 0..100_000 {
        // SAFETY: Fixed, mapped PL011 FR/DR registers in Device memory. Only
        // status polling and byte transmission occur, with no DMA capability.
        unsafe {
            if ((UART + 0x18) as *const u32).read_volatile() & (1 << 5) == 0 {
                (UART as *mut u32).write_volatile(u32::from(byte));
                return;
            }
        }
        core::hint::spin_loop();
    }
    exit(false)
}

pub(super) fn exit(success: bool) -> ! {
    let block: [u64; 2] = [0x20026, if success { 0 } else { 1 }];
    // SAFETY: SYS_EXIT_EXTENDED receives a live, aligned two-u64 block on the
    // mapped private stack. QEMU semihosting reads it synchronously. No caller
    // can supply a pointer, operation number or host file capability.
    unsafe {
        asm!("hlt #0xf000", inout("x0") 0x20u64 => _, inout("x1") block.as_ptr() => _, options(nostack));
    }
    loop {
        // SAFETY: Bootstrap deliberately masks all exceptions/interrupts and
        // has no scheduler; a missing semihost handler must never return pass.
        unsafe { asm!("wfi", options(nomem, nostack)) }
    }
}
