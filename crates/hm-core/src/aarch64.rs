use core::arch::{asm, global_asm};
use hm_kernel::{BootChecks, Platform};

// SAFETY: Direct-ELF entry checks EL1/reset state, owns all linker storage,
// establishes the MMU and stack, then calls the shared Rust entry at its ABI.
global_asm!(include_str!("boot-aarch64.S"), rust_entry = sym super::boot_main,
    cpu_affinity_mask = const super::cpu_local::AFFINITY_MASK,
    cpu_stride = const super::cpu_local::STRIDE,
    cpu_words = const super::cpu_local::STRIDE / 8,
    cpu_canary = const super::cpu_local::CANARY);

// SAFETY: These objects have exact sizes/alignment supplied by the linker and
// assembly. Tables are mutable hardware storage; no Rust references escape.
// Address-only linker markers are zero-sized, not immutable byte objects.
unsafe extern "C" {
    #[cfg(any(
        feature = "process-tests",
        feature = "startup-input-tests",
        feature = "runtime-tests"
    ))]
    static arm_vectors: [u8; 0];
    static mut arm_root: [u64; 512];
    static mut arm_secondary_root: [u64; 512];
    static mut arm_device_l2: [u64; 512];
    static mut arm_device_l3: [u64; 512];
    static mut arm_gic_l3: [u64; 512];
    static mut arm_ram_l2: [u64; 512];
    static mut arm_ram_l3: [u64; 4096];
    static mut arm_owned_l2: [u64; 512];
    #[cfg(feature = "space-tests")]
    static __bss_start: [u8; 0];
    static __text_start: [u8; 0];
    static __text_end: [u8; 0];
    static __rodata_start: [u8; 0];
    static __rodata_end: [u8; 0];
    static __stack_guard: [u8; 0];
    static __exception_stack_guard: [u8; 0];
    static __exception_stack_top: [u8; 0];
    static __secondary_stack_guard: [u8; 0];
    static __secondary_stack_top: [u8; 0];
    static __secondary_exception_stack_guard: [u8; 0];
    static __secondary_exception_stack_top: [u8; 0];
    static __secondary_thread0_guard: [u8; 0];
    static __secondary_thread0_top: [u8; 0];
    static __secondary_thread1_guard: [u8; 0];
    static __secondary_thread1_top: [u8; 0];
    static __secondary_thread2_guard: [u8; 0];
    static __secondary_thread2_top: [u8; 0];
    static __secondary_thread3_guard: [u8; 0];
    static __secondary_thread3_top: [u8; 0];
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
    topology: Option<hm_kernel::cpu_topology::Topology>,
    pool: Option<super::frames::Pool>,
    mapper: Option<super::mappings::Mapper>,
    runner: Option<super::threads::Runner>,
    native: super::launcher::State,
}
pub(super) const BANNER: &str = "hematite: ARM64 Rust kernel entered at EL1\n";

#[cfg(feature = "runtime-tests")]
#[cfg_attr(
    any(
        feature = "secondary-tests",
        feature = "process-tests",
        feature = "startup-input-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
pub(super) fn runtime_tests(platform: &mut BootPlatform) -> bool {
    struct Output;
    impl core::fmt::Write for Output {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            for byte in text.bytes() {
                if byte == b'\n' {
                    serial_byte(b'\r');
                }
                serial_byte(byte);
            }
            Ok(())
        }
    }
    let (Some(pool), Some(topology)) = (platform.pool.as_ref(), platform.topology.as_ref()) else {
        return false;
    };
    if topology.affinities() != [0] {
        return false;
    }
    let checks = super::timer::checks();
    let bits = u8::from(checks.delivery)
        | (u8::from(checks.context) << 1)
        | (u8::from(checks.quiescent) << 2)
        | (u8::from(checks.timeout) << 3);
    let _ = core::fmt::write(
        &mut Output,
        format_args!("HMRT PREFLIGHT checks={bits} irqs=8\n"),
    );
    if bits != 15 {
        return false;
    }
    // SAFETY: Actual private BootPlatform supplies the Pool exclusions,
    // immutable mappings and CPU0-only topology, with no external CPU or DMA.
    let Ok(factory) = (unsafe { super::user_space::Factory::from_boot(pool, topology) }) else {
        return false;
    };
    // SAFETY: Same permanent boot proof and completed timer delivery/quiescence
    // preflight; the permanent latch binds this boot's sole execution namespace.
    let Ok(mut executor) = (unsafe { super::process::Executor::from_boot(topology) }) else {
        return false;
    };
    super::runtime_checks::run(
        pool,
        &factory,
        &mut executor,
        core::ptr::addr_of!(arm_root) as u64,
        core::ptr::addr_of!(arm_vectors) as u64,
        &mut Output,
    )
}

impl Platform for BootPlatform {
    fn run_native_group(
        &mut self,
        paths: [&str; 2],
    ) -> Result<hm_kernel::native_launch::GroupLaunchReport, hm_kernel::native_launch::LaunchError>
    {
        let (Some(pool), Some(topology)) = (self.pool.as_ref(), self.topology.as_ref()) else {
            return Err(hm_kernel::native_launch::LaunchError::Unavailable);
        };
        // SAFETY: Private initialized BootPlatform fields bind actual mappings,
        // Pool exclusions and topology; no external CPU starts or DMA exist.
        // State admits both programs and restores/reclaims before returning.
        unsafe { self.native.run_group(pool, topology, paths) }
    }

    fn native_archive(&self) -> Option<&'static [u8]> {
        self.topology
            .as_ref()
            .filter(|t| t.affinities() == [0])
            .map(|_| super::launcher::ARCHIVE)
    }
    fn native_status(&self) -> hm_kernel::native_launch::NativeStatus {
        if self.topology.as_ref().is_none_or(|t| t.affinities() != [0]) {
            return hm_kernel::native_launch::NativeStatus::unsupported();
        }
        self.native.status(self.pool.as_ref())
    }
    fn run_native(
        &mut self,
        path: &str,
    ) -> Result<hm_kernel::native_launch::LaunchReport, hm_kernel::native_launch::LaunchError> {
        let (Some(pool), Some(topology)) = (self.pool.as_ref(), self.topology.as_ref()) else {
            return Err(hm_kernel::native_launch::LaunchError::Unavailable);
        };
        // SAFETY: These private fields belong to this initialized BootPlatform;
        // boot verified immutable mappings, Pool exclusions and CPU topology.
        // No external peer starts or DMA exist; State performs masked-context,
        // BUSY, timer and existing process admission before executing any ELF.
        unsafe { self.native.run(pool, topology, path) }
    }

    fn run_native_with_input(
        &mut self,
        path: &str,
        input: &hm_kernel::startup_input::Input,
    ) -> Result<hm_kernel::native_launch::LaunchReport, hm_kernel::native_launch::LaunchError> {
        let (Some(pool), Some(topology)) = (self.pool.as_ref(), self.topology.as_ref()) else {
            return Err(hm_kernel::native_launch::LaunchError::Unavailable);
        };
        // SAFETY: The same private initialized BootPlatform proof as run_native;
        // input is owned/validated and no caller pointer is retained in Process.
        unsafe { self.native.run_with_input(pool, topology, path, input) }
    }

    fn expected_cpu_count(&self) -> usize {
        if (cfg!(feature = "space-tests") || cfg!(feature = "process-tests"))
            && !cfg!(feature = "secondary-tests")
        {
            self.topology.as_ref().map_or(0, |t| t.affinities().len())
        } else if cfg!(feature = "secondary-tests") {
            2
        } else {
            1
        }
    }
    fn cpu_topology(&self) -> Option<hm_kernel::cpu_topology::Topology> {
        self.topology
    }
    type Mutex<T> = super::mutex::Mutex<T>;

    fn new_mutex<T>(&self, value: T) -> Self::Mutex<T> {
        super::mutex::Mutex::new(value)
    }

    fn mutex_checks(&self) -> Option<hm_kernel::MutexChecks> {
        self.runner.as_ref().map(super::mutex_checks::checks)
    }

    fn run_threads(
        &self,
        jobs: &[hm_kernel::thread_api::ThreadJob],
    ) -> Result<hm_kernel::thread_api::RunReport, hm_kernel::thread_api::ThreadError> {
        self.runner
            .as_ref()
            .ok_or(hm_kernel::thread_api::ThreadError::Unsupported)?
            .run(jobs)
    }

    fn thread_checks(&self) -> Option<hm_kernel::ThreadChecks> {
        self.runner.as_ref().map(super::threads::Runner::checks)
    }
    fn run_preemptive(
        &self,
        jobs: &[hm_kernel::thread_api::ThreadJob],
    ) -> Result<hm_kernel::thread_api::PreemptReport, hm_kernel::thread_api::ThreadError> {
        self.runner
            .as_ref()
            .ok_or(hm_kernel::thread_api::ThreadError::Unsupported)?
            .run_preemptive(jobs)
    }
    fn preempt_core_check(&self) -> Option<bool> {
        self.runner.as_ref().map(|runner| {
            super::preempt_checks::checks(runner)
                && super::preempt::context_checks(runner)
                && super::preempt::control_checks()
                && super::preempt::wait_boundary_check()
                && super::cpu_local::checks()
        })
    }
    fn secondary_memory_check(&self) -> Option<bool> {
        Some(secondary_memory_checks(self.pool.as_ref()))
    }
    fn gic_ownership_check(&self) -> Option<bool> {
        Some(super::preempt::gic_checks())
    }
    fn cpu_local_check(&self) -> Option<bool> {
        Some(super::cpu_local::checks())
    }
    fn preemption_irq_count(&self) -> Option<u64> {
        Some(super::preempt::irq_count())
    }
    fn timer_checks(&self) -> Option<hm_kernel::TimerChecks> {
        Some(super::timer::checks())
    }
    type Mapping<'a> = super::mappings::MappedPage<'a, 'a>;

    fn map_frame<'a>(
        &'a self,
        frame: Self::Frame<'a>,
    ) -> Result<Self::Mapping<'a>, (hm_kernel::mapping_api::MapError, Self::Frame<'a>)> {
        match self.mapper.as_ref() {
            Some(mapper) => mapper.map(frame),
            None => Err((hm_kernel::mapping_api::MapError::Unsupported, frame)),
        }
    }

    fn mapping_stats(&self) -> Option<hm_kernel::mapping_api::MapStats> {
        self.mapper.as_ref().map(super::mappings::Mapper::stats)
    }

    fn mapping_hardware_check(&self) -> bool {
        match (self.mapper.as_ref(), self.pool.as_ref()) {
            (Some(mapper), Some(pool)) => mapper.hardware_check(pool),
            _ => false,
        }
    }

    fn fault_checks(&self) -> Option<hm_kernel::FaultChecks> {
        Some(super::exceptions::checks())
    }

    type HeapBlock<'a> = super::frames::Block<'a>;
    type HeapBox<'a, T: 'a> = super::frames::KernelBox<'a, T>;

    fn allocate_heap(
        &self,
        size: usize,
        align: usize,
    ) -> Result<Self::HeapBlock<'_>, hm_kernel::heap_api::HeapError> {
        self.pool
            .as_ref()
            .ok_or(hm_kernel::heap_api::HeapError::Unsupported)?
            .allocate_heap(size, align)
    }

    fn box_value<'a, T: 'a>(
        &'a self,
        value: T,
    ) -> Result<Self::HeapBox<'a, T>, (hm_kernel::heap_api::HeapError, T)> {
        match self.pool.as_ref() {
            Some(pool) => pool.box_value(value),
            None => Err((hm_kernel::heap_api::HeapError::Unsupported, value)),
        }
    }

    fn heap_exhaustion_check(&self) -> bool {
        self.pool
            .as_ref()
            .is_some_and(super::frames::Pool::heap_exhaustion_check)
    }

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
        let exception_sp: u64;
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
            asm!("mrs {}, sp_el0", out(reg) exception_sp, options(nomem, nostack, preserves_flags));
        }
        let root = core::ptr::addr_of!(arm_root) as u64;
        let dev_l2 = core::ptr::addr_of!(arm_device_l2) as u64;
        let dev_l3 = core::ptr::addr_of!(arm_device_l3) as u64;
        let gic_l3 = core::ptr::addr_of!(arm_gic_l3) as u64;
        let ram_l2 = core::ptr::addr_of!(arm_ram_l2) as u64;
        let ram_l3 = core::ptr::addr_of!(arm_ram_l3) as u64;
        let owned_l2 = core::ptr::addr_of!(arm_owned_l2) as u64;
        let text = (core::ptr::addr_of!(__text_start) as usize)
            ..(core::ptr::addr_of!(__text_end) as usize);
        let rodata = (core::ptr::addr_of!(__rodata_start) as usize)
            ..(core::ptr::addr_of!(__rodata_end) as usize);
        let guard = core::ptr::addr_of!(__stack_guard) as usize;
        let exception_guard = core::ptr::addr_of!(__exception_stack_guard) as usize;
        let mut mappings = secondary_layout_checks()
            && secondary_root_matches()
            && ttbr == root
            && tcr == 0x8099_3519
            && mair == 0xff
            && super::mappings::tables_match(self.mapper.as_ref());
        // SAFETY: Each loop index lies within its named linker-owned array.
        // These initialized tables are mapped and not mutated by safe policy;
        // raw volatile access respects their status as hardware-owned storage.
        unsafe {
            for index in 0..512usize {
                let root_expected = match index {
                    0 => dev_l2 | 3,
                    1 => ram_l2 | 3,
                    2 => owned_l2 | 3,
                    _ => 0,
                };
                mappings &= core::ptr::addr_of!(arm_root)
                    .cast::<u64>()
                    .add(index)
                    .read_volatile()
                    == root_expected;
                let dev_expected = match index {
                    64 => gic_l3 | 3,
                    72 => dev_l3 | 3,
                    _ => 0,
                };
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
                let gic_expected = match index {
                    0 => 0x0060_0000_0800_0407,
                    16 => 0x0060_0000_0801_0407,
                    _ => 0,
                };
                mappings &= core::ptr::addr_of!(arm_gic_l3)
                    .cast::<u64>()
                    .add(index)
                    .read_volatile()
                    == gic_expected;
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
        let secondary = secondary_ranges();
        for index in 0..4096usize {
            let address = RAM + index * 4096;
            let expected = if address == guard
                || address == exception_guard
                || super::threads::guards().contains(&address)
                || secondary.iter().any(|&(guard, _)| address == guard)
            {
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
            stack_guard: mmu
                && mappings
                && leaf((guard - RAM) / 4096) == 0
                && leaf((exception_guard - RAM) / 4096) == 0
                && exception_sp == core::ptr::addr_of!(__exception_stack_top) as u64,
            // Root[0] has only UART and GIC pages. Full mapping check proves no
            // translation for virtual address zero or any alias of that page.
            null_guard: mmu && mappings,
        }
    }
}

fn secondary_ranges() -> [(usize, usize); 6] {
    [
        (
            core::ptr::addr_of!(__secondary_stack_guard) as usize,
            core::ptr::addr_of!(__secondary_stack_top) as usize,
        ),
        (
            core::ptr::addr_of!(__secondary_exception_stack_guard) as usize,
            core::ptr::addr_of!(__secondary_exception_stack_top) as usize,
        ),
        (
            core::ptr::addr_of!(__secondary_thread0_guard) as usize,
            core::ptr::addr_of!(__secondary_thread0_top) as usize,
        ),
        (
            core::ptr::addr_of!(__secondary_thread1_guard) as usize,
            core::ptr::addr_of!(__secondary_thread1_top) as usize,
        ),
        (
            core::ptr::addr_of!(__secondary_thread2_guard) as usize,
            core::ptr::addr_of!(__secondary_thread2_top) as usize,
        ),
        (
            core::ptr::addr_of!(__secondary_thread3_guard) as usize,
            core::ptr::addr_of!(__secondary_thread3_top) as usize,
        ),
    ]
}

fn secondary_root_matches() -> bool {
    let root = core::ptr::addr_of!(arm_secondary_root) as usize;
    let primary = core::ptr::addr_of!(arm_root) as usize;
    let text = core::ptr::addr_of!(__text_start) as usize;
    let first_stack = core::ptr::addr_of!(__stack_guard) as usize;
    if !root.is_multiple_of(4096)
        || root == primary
        || root < text
        || root.checked_add(4096).is_none_or(|end| end > first_stack)
    {
        return false;
    }
    for index in 0..512 {
        let expected = match index {
            0 => core::ptr::addr_of!(arm_device_l2) as u64 | 3,
            1 => core::ptr::addr_of!(arm_ram_l2) as u64 | 3,
            _ => 0,
        };
        // SAFETY: Exact assembly-owned root, bounded entry index; this table
        // is fully initialized before MMU/caches and never changed thereafter.
        let actual = unsafe {
            core::ptr::addr_of!(arm_secondary_root)
                .cast::<u64>()
                .add(index)
                .read_volatile()
        };
        if actual != expected {
            return false;
        }
    }
    let Some(offset) = root.checked_sub(RAM) else {
        return false;
    };
    leaf(offset / 4096) == (root as u64 | PAGE_RW_NX)
}

fn secondary_layout_checks() -> bool {
    let end = core::ptr::addr_of!(__kernel_end) as usize;
    let mut previous_top = super::threads::tops()[3];
    if !(RAM..=0x4100_0000).contains(&end) {
        return false;
    }
    for (index, (guard, top)) in secondary_ranges().into_iter().enumerate() {
        let usable = if index == 1 { 16384 } else { 65536 };
        if guard != previous_top
            || guard < RAM
            || top > end
            || !guard.is_multiple_of(4096)
            || !top.is_multiple_of(4096)
            || top.checked_sub(guard) != Some(4096 + usable)
        {
            return false;
        }
        if leaf((guard - RAM) / 4096) != 0 {
            return false;
        }
        // Size/range checks above bound every addition and leaf index.
        for page in ((guard + 4096)..top).step_by(4096) {
            if leaf((page - RAM) / 4096) != (page as u64 | PAGE_RW_NX) {
                return false;
            }
        }
        previous_top = top;
    }
    previous_top == end
}

fn secondary_memory_checks(pool: Option<&super::frames::Pool>) -> bool {
    let Some(pool) = pool else {
        return false;
    };
    if !secondary_layout_checks() || !secondary_root_matches() || !pool.exclusion_checks() {
        return false;
    }
    let root = core::ptr::addr_of!(arm_secondary_root) as u64;
    let Some(root_end) = root.checked_add(4096) else {
        return false;
    };
    pool.excludes_region(root, root_end)
        && secondary_ranges()
            .into_iter()
            .all(|(guard, top)| pool.excludes_region(guard as u64, top as u64))
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
    let (memory, topology) = discover_metadata()?;
    let mut platform = BootPlatform {
        memory,
        topology,
        pool: None,
        mapper: None,
        runner: None,
        native: super::launcher::State::new(),
    };
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
    // supervisor RW/NX normal RAM, initially without aliases. Identity maps
    // in the boot tables never change. Consuming Frame mappings may add
    // matching-attribute aliases; design 0022 also permits private roots to
    // tighten owned RX aliases during assembly-only probes, with full root/TLB
    // restoration before Rust owner byte access resumes.
    // Construction occurs with one CPU/interrupts masked. Admitted secondary
    // CPUs install the same coherent identity subtrees before pool access;
    // allocator metadata masks local IRQs and synchronizes between CPUs.
    // Constructor permanently claims singleton use.
    // SAFETY: The checks and exclusions above prove the constructor contract;
    // repeated initialization is rejected before any physical memory access.
    platform.pool = Some(unsafe { super::frames::Pool::from_boot(plan) }.map_err(|_| ())?);
    // SAFETY: The full initial checks proved the permanent arena hierarchy
    // and zero leaves. Bootstrap is EL1, one CPU, exceptions masked, no DMA.
    // The mapper owns all later leaf changes; identity mappings stay fixed.
    platform.mapper = Some(unsafe { super::mappings::Mapper::from_boot() }?);
    // SAFETY: Full initial checks verified immutable worker/guard mappings;
    // bootstrap supplies EL1h, masked DAIF, FP/SIMD and the single active CPU.
    platform.runner = Some(unsafe { super::threads::Runner::from_boot() });
    Ok(platform)
}

#[cfg(all(
    feature = "thread-overflow-test",
    not(any(feature = "nested-fault-test", feature = "unhandled-fault-test"))
))]
pub(super) fn thread_overflow(platform: &BootPlatform) -> ! {
    match platform.runner.as_ref() {
        Some(runner) => super::threads::overflow(runner),
        None => exit(false),
    }
}

#[cfg(all(
    feature = "preemption-overflow-test",
    not(any(
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test"
    ))
))]
pub(super) fn preemption_overflow(platform: &BootPlatform) -> ! {
    match platform.runner.as_ref() {
        Some(runner) => super::preempt::overflow(runner),
        None => exit(false),
    }
}

#[cfg(all(
    feature = "wait-all-blocked-test",
    not(any(
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test"
    ))
))]
pub(super) fn wait_all_blocked(platform: &BootPlatform) -> ! {
    match platform.runner.as_ref() {
        Some(runner) => super::preempt::all_blocked(runner),
        None => exit(false),
    }
}

#[cfg(feature = "wait-all-blocked-test")]
pub(super) fn wait_blocked_marker() {
    for byte in b"HMWAIT BLOCKED\n" {
        serial_byte(*byte);
    }
}

fn discover_metadata() -> Result<
    (
        hm_kernel::boot_memory::MemoryMap,
        Option<hm_kernel::cpu_topology::Topology>,
    ),
    (),
> {
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
    let memory = hm_kernel::boot_memory::parse(blob).map_err(|_| ())?;
    // Descriptive metadata only. Unsupported topology grants no SMP authority;
    // this increment never starts a secondary CPU.
    let topology = hm_kernel::cpu_topology::parse(blob).ok();
    Ok((memory, topology))
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

#[cfg(feature = "secondary-tests")]
#[cfg_attr(
    any(
        feature = "smp-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
pub(super) fn secondary_tests(platform: &mut BootPlatform) -> bool {
    fn emit(platform: &mut BootPlatform, args: core::fmt::Arguments<'_>) {
        struct Output<'a>(&'a mut BootPlatform);
        impl core::fmt::Write for Output<'_> {
            fn write_str(&mut self, text: &str) -> core::fmt::Result {
                self.0.write(text);
                Ok(())
            }
        }
        let _ = core::fmt::write(&mut Output(platform), args);
    }
    let Some(topology) = platform.topology.as_ref() else {
        platform.write("HMSMP REJECT TOPOLOGY\n");
        return false;
    };
    if !super::secondary::admitted(topology) {
        platform.write("HMSMP REJECT TOPOLOGY\n");
        return false;
    }
    // SAFETY: initialize validated the actual boot DTB, complete immutable
    // roots/stack mappings and reserved frame exclusions. This private CPU0
    // entry supplies machine authority; no safe caller can fabricate it.
    let Some(mut runner) = (unsafe { super::secondary::Runner::from_boot(topology) }) else {
        platform.write("HMSMP REJECT PSCI\n");
        return false;
    };
    platform.write("HMSMP BEGIN 2\n");
    let info = runner.info();
    emit(
        platform,
        format_args!(
            "HMSMP INFO version={} cpu_on={} cpu_off={} affinity_info={}\n",
            info.version, info.cpu_on, info.cpu_off, info.affinity_info
        ),
    );
    if cfg!(feature = "secondary-stale-generation-test") {
        platform.write("HMSMP EXPECT STALE-GENERATION\n");
    } else if cfg!(feature = "secondary-cpu-off-return-test") {
        platform.write("HMSMP EXPECT CPU-OFF-RETURN\n");
    }
    for _ in 0..2 {
        let Ok(session) = runner.run() else {
            return false;
        };
        emit(
            platform,
            format_args!(
                "HMSMP SESSION generation={} cpu={} on={} returned=1 off=1 checks={} root={} sp={}\n",
                session.generation,
                session.cpu,
                session.on,
                session.checks,
                session.root,
                session.sp
            ),
        );
        if session.checks != 255 || session.cpu != 1 || session.on != 0 {
            return false;
        }
    }
    platform.write("HMSMP PASS lifecycle\nHMSMP END 2\n");
    true
}

#[cfg(feature = "smp-tests")]
#[cfg_attr(
    any(
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
pub(super) fn smp_tests(platform: &mut BootPlatform) -> bool {
    #[cfg(not(any(
        feature = "smp-no-peer-timer-test",
        feature = "smp-peer-overflow-test",
        feature = "smp-all-blocked-test"
    )))]
    struct Output;
    #[cfg(not(any(
        feature = "smp-no-peer-timer-test",
        feature = "smp-peer-overflow-test",
        feature = "smp-all-blocked-test"
    )))]
    impl core::fmt::Write for Output {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            for byte in text.bytes() {
                if byte == b'\n' {
                    serial_byte(b'\r');
                }
                serial_byte(byte);
            }
            Ok(())
        }
    }
    let Some(topology) = platform.topology.as_ref() else {
        platform.write("HMSCHED REJECT TOPOLOGY\n");
        return false;
    };
    if !super::secondary::admitted(topology) {
        platform.write("HMSCHED REJECT TOPOLOGY\n");
        return false;
    }
    // SAFETY: Actual boot topology and full roots/stack/frame exclusions were
    // validated by initialize. No allocator or mapping authority reaches peers.
    let Some(mut runner) = (unsafe { super::secondary::Runner::from_boot(topology) }) else {
        platform.write("HMSCHED REJECT PSCI\n");
        return false;
    };
    #[cfg(feature = "smp-no-peer-timer-test")]
    {
        platform.write("HMSCHED EXPECT NO-PEER-TIMER\n");
        super::smp_checks::no_peer_timer(&mut runner);
    }
    #[cfg(all(
        not(feature = "smp-no-peer-timer-test"),
        feature = "smp-peer-overflow-test"
    ))]
    {
        platform.write("HMSCHED EXPECT PEER-OVERFLOW\n");
        super::smp_checks::peer_overflow(&mut runner);
    }
    #[cfg(all(
        not(any(feature = "smp-no-peer-timer-test", feature = "smp-peer-overflow-test")),
        feature = "smp-all-blocked-test"
    ))]
    {
        platform.write("HMSCHED EXPECT ALL-BLOCKED\n");
        super::smp_checks::all_blocked(&mut runner);
    }
    #[cfg(not(any(
        feature = "smp-no-peer-timer-test",
        feature = "smp-peer-overflow-test",
        feature = "smp-all-blocked-test"
    )))]
    {
        let Some(pool) = platform.pool.as_ref() else {
            return false;
        };
        super::smp_checks::run(&mut runner, pool, &mut Output)
    }
}

#[cfg(any(feature = "smp-no-peer-timer-test", feature = "smp-all-blocked-test"))]
fn smp_marker(args: core::fmt::Arguments<'_>) {
    if super::cpu_local::current_id() != Some(0) {
        exit(false);
    }
    struct Output;
    impl core::fmt::Write for Output {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            for byte in text.bytes() {
                if byte == b'\n' {
                    serial_byte(b'\r');
                }
                serial_byte(byte);
            }
            Ok(())
        }
    }
    let _ = core::fmt::write(&mut Output, args);
}
#[cfg(feature = "smp-no-peer-timer-test")]
pub(super) fn smp_peer_started_marker(sp: u64) {
    smp_marker(format_args!("HMSCHED STARTED cpu=1 sp={sp}\n"));
}
#[cfg(feature = "smp-all-blocked-test")]
pub(super) fn smp_both_blocked_marker() {
    smp_marker(format_args!("HMSCHED BLOCKED cpu0=1 cpu1=1\n"));
}

#[cfg(feature = "space-tests")]
#[cfg_attr(
    any(
        feature = "secondary-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
pub(super) fn space_tests(platform: &mut BootPlatform) -> bool {
    struct Output;
    impl core::fmt::Write for Output {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            for byte in text.bytes() {
                if byte == b'\n' {
                    serial_byte(b'\r');
                }
                serial_byte(byte);
            }
            Ok(())
        }
    }
    let (Some(pool), Some(mapper), Some(topology)) = (
        platform.pool.as_ref(),
        platform.mapper.as_ref(),
        platform.topology.as_ref(),
    ) else {
        return false;
    };
    // SAFETY: initialize validated this actual DTB topology, boot mappings and
    // reserved exclusions before minting the unique pool/mapper. This private
    // coordinator entry has not started peers or DMA; boot tables are immutable.
    let Ok(factory) = (unsafe { super::user_space::Factory::from_boot(pool, topology) }) else {
        return false;
    };
    super::space_checks::run(
        pool,
        mapper,
        &factory,
        &mut Output,
        topology.affinities().len(),
        core::ptr::addr_of!(__text_start) as u64,
        core::ptr::addr_of!(__bss_start) as u64,
    )
}

#[cfg(feature = "process-tests")]
#[cfg_attr(
    any(
        feature = "secondary-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
pub(super) fn process_tests(platform: &mut BootPlatform) -> bool {
    struct Output;
    impl core::fmt::Write for Output {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            for byte in text.bytes() {
                if byte == b'\n' {
                    serial_byte(b'\r');
                }
                serial_byte(byte);
            }
            Ok(())
        }
    }
    let checks = super::timer::checks();
    let bits = u8::from(checks.delivery)
        | (u8::from(checks.context) << 1)
        | (u8::from(checks.quiescent) << 2)
        | (u8::from(checks.timeout) << 3);
    let _ = core::fmt::write(
        &mut Output,
        format_args!("HMPROC PREFLIGHT checks={bits} irqs=8\n"),
    );
    if bits != 15 {
        return false;
    }
    let (Some(pool), Some(topology)) = (platform.pool.as_ref(), platform.topology.as_ref()) else {
        return false;
    };
    // SAFETY: The private initialized BootPlatform proved actual boot roots,
    // frame exclusions and topology; timer preflight above proved delivery and
    // quiescence. No peers or DMA have started; all core mappings remain owned.
    let Ok(factory) = (unsafe { super::user_space::Factory::from_boot(pool, topology) }) else {
        return false;
    };
    // SAFETY: Same verified BootPlatform authority and completed preflight;
    // the permanent executor latch admits only this private process namespace.
    let Ok(mut executor) = (unsafe { super::process::Executor::from_boot(topology) }) else {
        return false;
    };
    super::process_checks::run(
        pool,
        &factory,
        &mut executor,
        topology.affinities().len(),
        core::ptr::addr_of!(arm_root) as u64,
        core::ptr::addr_of!(arm_vectors) as u64,
        &mut Output,
    )
}

#[cfg(feature = "startup-input-tests")]
#[cfg_attr(
    any(
        feature = "secondary-tests",
        feature = "process-tests",
        feature = "nested-fault-test",
        feature = "unhandled-fault-test",
        feature = "thread-overflow-test",
        feature = "preemption-overflow-test",
        feature = "wait-all-blocked-test"
    ),
    allow(dead_code)
)]
pub(super) fn startup_input_tests(platform: &mut BootPlatform) -> bool {
    struct Output;
    impl core::fmt::Write for Output {
        fn write_str(&mut self, text: &str) -> core::fmt::Result {
            for byte in text.bytes() {
                if byte == b'\n' {
                    serial_byte(b'\r');
                }
                serial_byte(byte);
            }
            Ok(())
        }
    }
    let (Some(pool), Some(topology)) = (platform.pool.as_ref(), platform.topology.as_ref()) else {
        return false;
    };
    if topology.affinities() != [0]
        || !super::startup_input_checks::reject_before(pool, &mut Output)
    {
        return false;
    }
    let checks = super::timer::checks();
    let bits = u8::from(checks.delivery)
        | (u8::from(checks.context) << 1)
        | (u8::from(checks.quiescent) << 2)
        | (u8::from(checks.timeout) << 3);
    let _ = core::fmt::write(
        &mut Output,
        format_args!("HMINPUT PREFLIGHT checks={bits} irqs=8\n"),
    );
    if bits != 15 {
        return false;
    }
    // SAFETY: The private BootPlatform supplies actual Pool/topology and
    // immutable boot roots; no external CPU/DMA exists in this one-CPU profile.
    let Ok(factory) = (unsafe { super::user_space::Factory::from_boot(pool, topology) }) else {
        return false;
    };
    // SAFETY: Same boot proof and completed delivery/quiescence preflight;
    // the permanent latch owns this boot's sole real process issuer.
    let Ok(mut executor) = (unsafe { super::process::Executor::from_boot(topology) }) else {
        return false;
    };
    super::startup_input_checks::run(
        pool,
        &factory,
        &mut executor,
        core::ptr::addr_of!(arm_root) as u64,
        core::ptr::addr_of!(arm_vectors) as u64,
        &mut Output,
    )
}
