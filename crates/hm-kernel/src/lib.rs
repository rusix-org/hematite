#![no_std]
#![forbid(unsafe_code)]

//! Safe bootstrap policy. This monitor is kernel code, not Linux userspace.

pub mod boot_memory;
mod boot_memory_tests;
pub mod capability;
mod capability_tests;
pub mod cpu_topology;
mod cpu_topology_tests;
pub mod frame_api;
pub mod frame_plan;
mod frame_plan_tests;
pub mod heap_api;
mod heap_tests;
pub mod mapping_api;
mod mapping_tests;
pub mod mutex_api;
mod mutex_tests;
mod preemption_tests;
pub mod thread_api;
mod thread_tests;
pub mod wait_api;
mod wait_tests;

use core::ops::Range;

/// Observations collected by the core from the active bootstrap mappings.
#[derive(Clone, Copy)]
pub struct BootChecks {
    pub write_protect: bool,
    pub nx: bool,
    /// Architecture-specific observation: x86 SMEP; ARM bootstrap
    /// privileged-only mappings with UXN preventing execution by userspace.
    pub user_execute_protect: bool,
    /// Architecture-specific observation: x86 SMAP; ARM PAN together with
    /// the bootstrap's privileged-only mappings.
    pub user_access_protect: bool,
    pub wx: bool,
    pub stack_guard: bool,
    pub null_guard: bool,
}

/// The core retains all hardware authority; policy only exchanges owned values.
pub trait Platform {
    /// Expected boot profile, independent of the discovered device tree.
    fn expected_cpu_count(&self) -> usize {
        1
    }
    fn secondary_memory_check(&self) -> Option<bool> {
        None
    }
    fn gic_ownership_check(&self) -> Option<bool> {
        None
    }
    fn cpu_local_check(&self) -> Option<bool> {
        None
    }
    fn cpu_topology(&self) -> Option<cpu_topology::Topology> {
        None
    }
    type Mutex<T>: mutex_api::OwnedMutex<T>;
    fn new_mutex<T>(&self, value: T) -> Self::Mutex<T>;
    fn mutex_checks(&self) -> Option<MutexChecks> {
        None
    }
    fn run_threads(
        &self,
        _jobs: &[thread_api::ThreadJob],
    ) -> Result<thread_api::RunReport, thread_api::ThreadError> {
        Err(thread_api::ThreadError::Unsupported)
    }
    fn thread_checks(&self) -> Option<ThreadChecks> {
        None
    }
    fn run_preemptive(
        &self,
        _jobs: &[thread_api::ThreadJob],
    ) -> Result<thread_api::PreemptReport, thread_api::ThreadError> {
        Err(thread_api::ThreadError::Unsupported)
    }
    fn preempt_core_check(&self) -> Option<bool> {
        None
    }
    fn preemption_irq_count(&self) -> Option<u64> {
        None
    }
    fn timer_checks(&self) -> Option<TimerChecks> {
        None
    }
    type Mapping<'a>: mapping_api::OwnedMapping<Frame = Self::Frame<'a>>
    where
        Self: 'a;
    fn map_frame<'a>(
        &'a self,
        frame: Self::Frame<'a>,
    ) -> Result<Self::Mapping<'a>, (mapping_api::MapError, Self::Frame<'a>)> {
        Err((mapping_api::MapError::Unsupported, frame))
    }
    fn mapping_stats(&self) -> Option<mapping_api::MapStats> {
        None
    }
    fn mapping_hardware_check(&self) -> bool {
        false
    }
    fn fault_checks(&self) -> Option<FaultChecks> {
        None
    }
    type HeapBlock<'a>: heap_api::OwnedBlock
    where
        Self: 'a;
    type HeapBox<'a, T: 'a>: core::ops::DerefMut<Target = T>
    where
        Self: 'a;
    fn allocate_heap(
        &self,
        size: usize,
        align: usize,
    ) -> Result<Self::HeapBlock<'_>, heap_api::HeapError>;
    fn box_value<'a, T: 'a>(
        &'a self,
        value: T,
    ) -> Result<Self::HeapBox<'a, T>, (heap_api::HeapError, T)>;
    fn heap_exhaustion_check(&self) -> bool {
        false
    }
    type Frame<'a>: frame_api::OwnedFrame
    where
        Self: 'a;
    fn allocate_frame(&self) -> Result<Self::Frame<'_>, frame_api::FrameError>;
    fn frame_stats(&self) -> Option<frame_api::FrameStats> {
        None
    }
    fn frame_exhaustion_check(&self) -> bool {
        false
    }
    fn write(&mut self, text: &str);
    fn read_byte(&mut self) -> Option<u8>;
    fn checks(&self) -> BootChecks;
    fn boot_memory(&self) -> Option<&boot_memory::MemoryMap> {
        None
    }
}

/// Actual fixed-site ARM exception observations; not a user-memory API.
pub struct FaultChecks {
    pub null_read: bool,
    pub stack_guard: bool,
    pub exception_guard: bool,
    pub read_only: bool,
    pub no_execute: bool,
    pub context: bool,
}

/// Scoped ARM IRQ observations; no scheduler or interrupt callback authority.
pub struct TimerChecks {
    pub delivery: bool,
    pub context: bool,
    pub quiescent: bool,
    pub timeout: bool,
}

/// Core observations supplement the safe callback execution tests.
pub struct ThreadChecks {
    pub context: bool,
    pub reentrant: bool,
    pub guards: bool,
}

/// Observations from actual shared-state cooperative workers on ARM.
pub struct MutexChecks {
    pub contention: bool,
    pub publication: bool,
    pub completion: bool,
}

fn mutex_workers(platform: &impl Platform) -> bool {
    let frames = platform.frame_stats();
    let mappings = platform.mapping_stats();
    for _ in 0..2 {
        let Some(checks) = platform.mutex_checks() else {
            return cfg!(target_arch = "x86_64");
        };
        if !cfg!(target_arch = "aarch64")
            || !checks.contention
            || !checks.publication
            || !checks.completion
            || platform.frame_stats() != frames
            || platform.mapping_stats() != mappings
        {
            return false;
        }
    }
    let c = platform.checks();
    c.write_protect
        && c.nx
        && c.wx
        && c.stack_guard
        && c.null_guard
        && c.user_access_protect
        && c.user_execute_protect
}

fn thread_core(platform: &impl Platform) -> bool {
    let frames = platform.frame_stats();
    let mappings = platform.mapping_stats();
    for _ in 0..2 {
        let Some(checks) = platform.thread_checks() else {
            return cfg!(target_arch = "x86_64");
        };
        if !cfg!(target_arch = "aarch64")
            || !checks.context
            || !checks.reentrant
            || !checks.guards
            || platform.frame_stats() != frames
            || platform.mapping_stats() != mappings
        {
            return false;
        }
    }
    let c = platform.checks();
    c.write_protect
        && c.nx
        && c.wx
        && c.stack_guard
        && c.null_guard
        && c.user_access_protect
        && c.user_execute_protect
}

fn timer_contract(platform: &impl Platform) -> bool {
    use frame_api::OwnedFrame;
    use mapping_api::OwnedMapping;
    if cfg!(target_arch = "x86_64") {
        return platform.timer_checks().is_none();
    }
    let Ok(mut frame) = platform.allocate_frame() else {
        return false;
    };
    frame.bytes_mut().fill(0x93);
    let Ok(mut mapped) = platform.map_frame(frame) else {
        return false;
    };
    let frames = platform.frame_stats();
    let mappings = platform.mapping_stats();
    // A live exclusive borrow crosses the scoped interrupt operation. The
    // handler must never access this storage or its ownership metadata.
    let bytes = mapped.bytes_mut();
    for _ in 0..2 {
        let Some(checks) = platform.timer_checks() else {
            return false;
        };
        if !checks.delivery
            || !checks.context
            || !checks.quiescent
            || !checks.timeout
            || bytes.iter().any(|b| *b != 0x93)
            || platform.frame_stats() != frames
            || platform.mapping_stats() != mappings
        {
            return false;
        }
    }
    let c = platform.checks();
    c.write_protect
        && c.nx
        && c.wx
        && c.stack_guard
        && c.null_guard
        && c.user_access_protect
        && c.user_execute_protect
}

fn fault_contract(platform: &mut impl Platform) -> bool {
    for _ in 0..2 {
        let Some(checks) = platform.fault_checks() else {
            return cfg!(target_arch = "x86_64");
        };
        if !cfg!(target_arch = "aarch64") {
            return false;
        }
        for (name, passed) in [
            ("null read", checks.null_read),
            ("normal guard", checks.stack_guard),
            ("exception guard", checks.exception_guard),
            ("read-only write", checks.read_only),
            ("NX execution", checks.no_execute),
            ("context preservation", checks.context),
        ] {
            if !passed {
                platform.write("Fault check failed: ");
                platform.write(name);
                platform.write("\n");
                return false;
            }
        }
    }
    let checks = platform.checks();
    checks.stack_guard && checks.wx && checks.nx
}

pub fn report_memory(platform: &mut impl Platform) {
    let Some(memory) = platform.boot_memory() else {
        return;
    };
    let total = memory.total_bytes();
    platform.write("Boot RAM bytes: 0x");
    write_hex(platform, total);
    platform.write(" (discovered RAM)\n");
    if let Some(stats) = platform.frame_stats() {
        platform.write("Frame pool pages: 0x");
        write_hex(platform, stats.total as u64);
        platform.write(" (4 KiB each; boot reservations excluded)\n");
    }
}

fn write_hex(platform: &mut impl Platform, value: u64) {
    let mut digits = [b'0'; 16];
    for (index, digit) in digits.iter_mut().enumerate() {
        *digit = b"0123456789abcdef"[((value >> ((15 - index) * 4)) & 15) as usize];
    }
    if let Ok(text) = core::str::from_utf8(&digits) {
        platform.write(text);
    }
}

/// Return a bounded half-open range, rejecting both arithmetic and bound errors.
fn checked_range(start: usize, length: usize, limit: usize) -> Option<Range<usize>> {
    let end = start.checked_add(length)?;
    if start > limit || end > limit {
        None
    } else {
        Some(start..end)
    }
}

const LINE_CAPACITY: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum InputEvent {
    Ignored,
    Echo(u8),
    Erase,
    Ready,
}

struct InputLine {
    bytes: [u8; LINE_CAPACITY],
    length: usize,
    rejected: bool,
    skip_lf: bool,
}

impl InputLine {
    const fn new() -> Self {
        Self {
            bytes: [0; LINE_CAPACITY],
            length: 0,
            rejected: false,
            skip_lf: false,
        }
    }

    fn feed(&mut self, byte: u8) -> InputEvent {
        if self.skip_lf {
            self.skip_lf = false;
            if byte == b'\n' {
                return InputEvent::Ignored;
            }
        }
        if byte == b'\r' || byte == b'\n' {
            self.skip_lf = byte == b'\r';
            return InputEvent::Ready;
        }
        // Once a line is rejected, discard it through its terminator. Never
        // execute a valid-looking prefix after a truncation or invalid byte.
        if self.rejected {
            return InputEvent::Ignored;
        }
        if byte == 8 || byte == 127 {
            if let Some(previous) = self.length.checked_sub(1) {
                self.length = previous;
                return InputEvent::Erase;
            }
            return InputEvent::Ignored;
        }
        if !(b' '..=b'~').contains(&byte) {
            self.rejected = true;
            return InputEvent::Ignored;
        }
        let Some(range) = checked_range(self.length, 1, self.bytes.len()) else {
            self.rejected = true;
            return InputEvent::Ignored;
        };
        // get_mut additionally keeps this path non-panicking if the layout or
        // range helper changes in a future refactor.
        let Some(slot) = self.bytes.get_mut(range.start) else {
            self.rejected = true;
            return InputEvent::Ignored;
        };
        *slot = byte;
        self.length = range.end;
        InputEvent::Echo(byte)
    }

    fn command(&self) -> Option<&[u8]> {
        if self.rejected {
            None
        } else {
            self.bytes.get(..self.length)
        }
    }

    fn clear(&mut self) {
        self.length = 0;
        self.rejected = false;
        // Keep CRLF suppression across completion of the preceding line.
    }
}

fn test_checked_range() -> bool {
    checked_range(0, 0, 0) == Some(0..0)
        && checked_range(63, 1, 64) == Some(63..64)
        && checked_range(64, 0, 64) == Some(64..64)
        && checked_range(64, 1, 64).is_none()
        && checked_range(65, 0, 64).is_none()
        && checked_range(usize::MAX - 2, 4, usize::MAX).is_none()
}

fn test_monitor_bounds() -> bool {
    let mut line = InputLine::new();
    for _ in 0..LINE_CAPACITY {
        if line.feed(b'x') != InputEvent::Echo(b'x') {
            return false;
        }
    }
    if line.command().map(<[u8]>::len) != Some(LINE_CAPACITY)
        || line.feed(b'x') != InputEvent::Ignored
        || line.command().is_some()
        || line.feed(b'\r') != InputEvent::Ready
    {
        return false;
    }
    line.clear();
    if line.feed(b'\n') != InputEvent::Ignored || line.feed(8) != InputEvent::Ignored {
        return false;
    }
    for byte in b"helx" {
        line.feed(*byte);
    }
    if line.feed(127) != InputEvent::Erase {
        return false;
    }
    line.feed(b'p');
    if line.feed(b'\n') != InputEvent::Ready || line.command() != Some(&b"help"[..]) {
        return false;
    }
    line.clear();
    for byte in b"help" {
        line.feed(*byte);
    }
    line.feed(0xff);
    line.feed(8);
    line.command().is_none() && line.feed(b'\n') == InputEvent::Ready
}

/// Run the full bootstrap inventory. The core must also return a failure exit
/// status when this returns false; the serial summary alone is not success.
// Keep the test inventory's large fixture frame out of the monitor loop.
#[inline(never)]
pub fn run_tests(platform: &mut impl Platform) -> bool {
    let checks = platform.checks();
    let cases = [
        ("write_protect", checks.write_protect),
        ("nx", checks.nx),
        ("user_execute_protect", checks.user_execute_protect),
        ("user_access_protect", checks.user_access_protect),
        ("wx", checks.wx),
        ("stack_guard", checks.stack_guard),
        ("null_guard", checks.null_guard),
        ("checked_range", test_checked_range()),
        ("monitor_bounds", test_monitor_bounds()),
        ("cap_authority", capability_tests::authority()),
        ("cap_limits", capability_tests::limits()),
        ("cap_lifetime", capability_tests::lifetime()),
        ("cap_generation", capability_tests::generation()),
        ("boot_memory_valid", boot_memory_tests::valid()),
        ("boot_memory_malformed", boot_memory_tests::malformed()),
        ("cpu_topology_valid", cpu_topology_tests::valid()),
        ("cpu_topology_malformed", cpu_topology_tests::malformed()),
        (
            "cpu_topology_live",
            if cfg!(target_arch = "aarch64") {
                platform.cpu_topology().is_some_and(|topology| {
                    match platform.expected_cpu_count() {
                        1 => topology.cpu_count() == 1 && topology.affinities() == [0],
                        2 => topology.cpu_count() == 2 && topology.affinities() == [0, 1],
                        _ => false,
                    }
                })
            } else {
                platform.cpu_topology().is_none()
            },
        ),
        ("frame_ranges", frame_plan_tests::ranges()),
        ("frame_boundaries", frame_plan_tests::boundaries()),
        ("frame_contract", frame_api::contract(platform)),
        ("heap_blocks", heap_tests::blocks(platform)),
        ("heap_values", heap_tests::values(platform)),
        ("event_contract", wait_tests::events()),
        ("mutex_contract", mutex_tests::contract(platform)),
        ("mutex_lifetime", mutex_tests::lifetime(platform)),
        ("mutex_workers", mutex_workers(platform)),
        ("thread_contract", thread_tests::contract(platform)),
        ("thread_limits", thread_tests::limits(platform)),
        ("thread_core", thread_core(platform)),
        ("timer_contract", timer_contract(platform)),
        ("fault_contract", fault_contract(platform)),
        ("mapping_contract", mapping_tests::contract(platform)),
        ("mapping_limits", mapping_tests::limits(platform)),
        ("mapping_hardware", mapping_tests::hardware(platform)),
        (
            "cpu_local_state",
            platform
                .cpu_local_check()
                .unwrap_or(cfg!(target_arch = "x86_64")),
        ),
        (
            "gic_ownership",
            platform
                .gic_ownership_check()
                .unwrap_or(cfg!(target_arch = "x86_64")),
        ),
        (
            "secondary_memory",
            platform
                .secondary_memory_check()
                .unwrap_or(cfg!(target_arch = "x86_64")),
        ),
        (
            "cap_orderings",
            capability_tests::orderings(|message| platform.write(message)),
        ),
    ];
    platform.write("HMTEST BEGIN 39\n");
    let mut passed = true;
    for (name, success) in cases {
        platform.write(if success {
            "HMTEST PASS "
        } else {
            "HMTEST FAIL "
        });
        platform.write(name);
        platform.write("\n");
        passed &= success;
    }
    platform.write("HMTEST END 39\n");
    passed
}

/// Separate strict inventory for timer-driven scheduling; ordinary boot tests
/// retain their fixed diagnostic IRQ count.
#[inline(never)]
pub fn run_preemption_tests(platform: &mut impl Platform) -> bool {
    let before = platform.preemption_irq_count();
    let cases = [
        ("preempt_progress", preemption_tests::progress(platform)),
        ("preempt_mixed", preemption_tests::mixed(platform)),
        ("preempt_limits", preemption_tests::limits(platform)),
        ("wait_workers", wait_tests::workers(platform)),
        (
            "preempt_core",
            platform
                .preempt_core_check()
                .unwrap_or(cfg!(target_arch = "x86_64")),
        ),
    ];
    platform.write("HMPREEMPT BEGIN 5\n");
    let mut passed = true;
    for (name, success) in cases {
        platform.write(if success {
            "HMPREEMPT PASS "
        } else {
            "HMPREEMPT FAIL "
        });
        platform.write(name);
        platform.write("\n");
        passed &= success;
    }
    platform.write("HMPREEMPT END 5\n");
    if let (Some(before), Some(after)) = (before, platform.preemption_irq_count()) {
        platform.write("HMPREEMPT IRQ ");
        write_hex(platform, after.wrapping_sub(before));
        platform.write("\n");
    }
    passed
}

fn status(platform: &mut impl Platform) {
    let checks = platform.checks();
    for (name, enabled) in [
        ("write protection", checks.write_protect),
        ("NX", checks.nx),
        ("user execution protection", checks.user_execute_protect),
        ("user access protection", checks.user_access_protect),
        ("W^X mappings", checks.wx),
        ("stack guard", checks.stack_guard),
        ("null guard", checks.null_guard),
    ] {
        platform.write(name);
        platform.write(if enabled { ": OK\n" } else { ": FAILED\n" });
    }
    platform.write("Bootstrap only: one CPU; no userspace, storage, or networking.\n");
}

pub fn run_monitor(platform: &mut impl Platform) -> ! {
    platform.write("Hematite bootstrap monitor (local working name)\n");
    platform.write("Commands: help, status, test. This is not a Linux shell.\n");
    platform.write("hm> ");
    let mut line = InputLine::new();
    loop {
        let Some(byte) = platform.read_byte() else {
            core::hint::spin_loop();
            continue;
        };
        match line.feed(byte) {
            InputEvent::Ignored => {}
            InputEvent::Echo(byte) => {
                let bytes = [byte];
                if let Ok(text) = core::str::from_utf8(&bytes) {
                    platform.write(text);
                }
            }
            InputEvent::Erase => platform.write("\x08 \x08"),
            InputEvent::Ready => {
                platform.write("\n");
                match line.command() {
                    None => platform.write("Rejected: use at most 64 printable ASCII bytes.\n"),
                    Some(b"") => {}
                    Some(b"help") => platform.write("help   - show commands\nstatus - inspect boot protections\ntest   - run bootstrap tests\n"),
                    Some(b"status") => status(platform),
                    Some(b"test") => {
                        if run_tests(platform) {
                            platform.write("Bootstrap tests passed.\n");
                        } else {
                            platform.write("Bootstrap tests FAILED.\n");
                        }
                    }
                    Some(_) => platform.write("Unknown command; type help.\n"),
                }
                line.clear();
                platform.write("hm> ");
            }
        }
    }
}
