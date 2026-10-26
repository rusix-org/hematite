//! Owned, inactive ELF spaces and bounded CPU0 translation probes; design 0022.
use super::frames::{Frame, Pool};
use core::arch::{asm, global_asm};
use core::marker::PhantomData;
use core::sync::atomic::Ordering;
use hm_kernel::cpu_topology::{Conduit, Topology};
use hm_kernel::elf::{Image, Permissions, USER_IMAGE_START};
use hm_kernel::frame_api::OwnedFrame;

const PAGE: usize = 4096;
const TABLES: usize = 12;
const RAM: u64 = 0x4000_0000;
const RW: u64 = 0x0060_0000_0000_0703;
const RO: u64 = 0x0060_0000_0000_0783;
pub const MAX_PROBES: usize = 16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Context,
    Topology,
    Busy,
    Peer,
    Budget,
    Exhausted,
    Bounds,
    Corrupt,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PageInfo {
    pub physical: u64,
    pub permissions: Permissions,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u64)]
pub enum Access {
    El0Read,
    El0Write,
    El1Read,
    El1Write,
}
#[derive(Clone, Copy, Debug)]
#[repr(C)]
pub struct Probe {
    pub address: u64,
    pub access: Access,
}
#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct ProbeResult {
    pub before: u64,
    pub active: u64,
    pub after: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct ProbeBatch {
    pub count: usize,
    pub results: [ProbeResult; MAX_PROBES],
    pub root_during: u64,
    pub root_restored: u64,
    pub controls_restored: bool,
}

// SAFETY: Boot owns these permanent aligned tables; Factory's construction
// proof guarantees no concurrent mutation. No Rust reference is formed.
unsafe extern "C" {
    static arm_root: [u64; 512];
    static arm_device_l2: [u64; 512];
    static arm_ram_l3: [u64; 4096];
    fn arm_space_probe(
        root: u64,
        queries: *const Probe,
        results: *mut ProbeResult,
        count: usize,
        roots: *mut u64,
    );
}
// SAFETY: The assembly is a bounded leaf operation using only private copied
// buffers with verified common identity mappings and retained roots; design 0022.
global_asm!(include_str!("user-space-aarch64.S"));

pub struct Factory<'pool> {
    pool: &'pool Pool,
    cpus: usize,
    _local: PhantomData<*mut ()>,
}
pub struct Space<'pool> {
    pool: &'pool Pool,
    startup: bool,
    tables: [Option<Frame<'pool>>; TABLES],
    pages: [Option<Frame<'pool>>; 512],
    permissions: [Option<Permissions>; 512],
    entry: u64,
    count: usize,
    cpus: usize,
    _local: PhantomData<*mut ()>,
}

impl<'pool> Factory<'pool> {
    pub(super) fn reborrow(&self) -> Factory<'pool> {
        Factory {
            pool: self.pool,
            cpus: self.cpus,
            _local: PhantomData,
        }
    }
    pub(super) fn pool(&self) -> &'pool Pool {
        self.pool
    }
    pub(super) fn cpus(&self) -> usize {
        self.cpus
    }
    /// The caller proves these are the actual BootPlatform pool/topology and
    /// immutable boot tables, with no external CPU starts or DMA; design 0022.
    // SAFETY: This private constructor binds the stronger platform proof to
    // every subsequently loaded Space; generic Pool/Topology values do not.
    pub(crate) unsafe fn from_boot(pool: &'pool Pool, topology: &Topology) -> Result<Self, Error> {
        context()?;
        let cpus = match topology.affinities() {
            [0] => 1,
            [0, 1] if topology.psci().is_some_and(|p| p.conduit == Conduit::Hvc) => 2,
            _ => return Err(Error::Topology),
        };
        Ok(Self {
            pool,
            cpus,
            _local: PhantomData,
        })
    }

    #[inline(never)]
    pub fn load(&self, image: &Image<'_>, frame_budget: usize) -> Result<Space<'pool>, Error> {
        context()?;
        let mut space = Space {
            pool: self.pool,
            startup: false,
            tables: [const { None }; TABLES],
            pages: [const { None }; 512],
            permissions: [None; 512],
            entry: image.entry(),
            count: 0,
            cpus: self.cpus,
            _local: PhantomData,
        };
        for i in 0..TABLES {
            space.tables[i] = Some(self.allocate(&mut space.count, frame_budget)?);
        }
        for segment in image.segments() {
            for va in (segment.page_start()..segment.page_end()).step_by(PAGE) {
                let index = slot(va).ok_or(Error::Bounds)?;
                if space.pages[index].is_some() {
                    return Err(Error::Corrupt);
                }
                space.pages[index] = Some(self.allocate(&mut space.count, frame_budget)?);
                space.permissions[index] = Some(segment.permissions());
            }
            let mut va = segment.virtual_address();
            let mut input = segment.file_bytes();
            while !input.is_empty() {
                let index = slot(va).ok_or(Error::Bounds)?;
                let offset = (va as usize) & (PAGE - 1);
                let n = input.len().min(PAGE - offset);
                let frame = space.pages[index].as_mut().ok_or(Error::Corrupt)?;
                frame.bytes_mut()[offset..offset + n].copy_from_slice(&input[..n]);
                input = &input[n..];
                va = va.checked_add(n as u64).ok_or(Error::Bounds)?;
            }
        }
        for i in 496..512 {
            space.pages[i] = Some(self.allocate(&mut space.count, frame_budget)?);
            space.permissions[i] = Some(Permissions::ReadWrite);
        }
        space.build_tables()?;
        if !space.audit() {
            return Err(Error::Corrupt);
        }
        Ok(space)
    }

    fn allocate(&self, count: &mut usize, budget: usize) -> Result<Frame<'pool>, Error> {
        if *count >= budget {
            return Err(Error::Budget);
        }
        let frame = self.pool.allocate().map_err(|_| Error::Exhausted)?;
        *count += 1;
        Ok(frame)
    }
}

impl Space<'_> {
    #[cfg(feature = "runtime-tests")]
    pub(super) fn runtime_release_word(&self) -> Result<u64, Error> {
        context()?;
        const VA: u64 = 0x1_0000_2000;
        if self
            .page(VA)
            .is_none_or(|page| page.permissions != Permissions::ReadWrite)
        {
            return Err(Error::Bounds);
        }
        let mut bytes = [0; 8];
        self.read_into(VA, &mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }
    #[cfg(feature = "runtime-tests")]
    pub(super) fn release_runtime_probe(&mut self) -> Result<(), Error> {
        if self.runtime_release_word()? != 0 {
            return Err(Error::Bounds);
        }
        let index = slot(0x1_0000_2000).ok_or(Error::Bounds)?;
        let frame = self.pages[index].as_mut().ok_or(Error::Bounds)?;
        frame.bytes_mut()[..8].copy_from_slice(&1u64.to_le_bytes());
        Ok(())
    }

    pub(super) fn cpus(&self) -> usize {
        self.cpus
    }
    pub(super) fn install_startup(
        &mut self,
        bytes: &[u8; 64],
        budget: usize,
        input: Option<&hm_kernel::startup_input::Input>,
    ) -> Result<(), Error> {
        context()?;
        if !self.audit()
            || self.startup
            || self.pages[256].is_some()
            || self.word(11, 256) != Some(0)
        {
            return Err(Error::Corrupt);
        }
        if budget == 0 {
            return Err(Error::Budget);
        }
        let mut page = self.pool.allocate().map_err(|_| Error::Exhausted)?;
        let pa = page.physical_address();
        if ram_index(pa).is_none_or(|i| boot_ram(i) != pa | RW) {
            return Err(Error::Corrupt);
        }
        page.bytes_mut()[..64].copy_from_slice(bytes);
        if let Some(input) = input {
            input
                .encode_into(&mut page.bytes_mut()[64..])
                .map_err(|_| Error::Corrupt)?;
        }
        let Some(table) = self.tables[11].as_mut() else {
            return Err(Error::Corrupt);
        };
        table.bytes_mut()[256 * 8..257 * 8]
            .copy_from_slice(&(pa | attributes(Permissions::ReadOnly)).to_le_bytes());
        self.pages[256] = Some(page);
        self.permissions[256] = Some(Permissions::ReadOnly);
        self.startup = true;
        self.count += 1;
        Ok(())
    }
    pub(super) fn prepare_instructions(&self) -> Result<(), Error> {
        context()?;
        let ctr: u64;
        // SAFETY: EL1 read-only cache geometry observation.
        unsafe {
            asm!("mrs {}, ctr_el0",out(reg) ctr,options(nomem,nostack,preserves_flags));
        }
        let line = 4u64 << ((ctr >> 16) & 15);
        if line > 4096 || !self.startup || !self.audit() {
            return Err(Error::Corrupt);
        }
        for (frame, permission) in self.pages.iter().zip(self.permissions.iter()) {
            if *permission == Some(Permissions::ReadExecute) {
                let Some(frame) = frame else {
                    return Err(Error::Corrupt);
                };
                let start = frame.physical_address();
                for offset in (0..4096).step_by(line as usize) {
                    // SAFETY: Retained executable Frame has an aligned accessible
                    // boot identity alias; no user executes and peer is OFF.
                    unsafe {
                        asm!("dc cvau, {}",in(reg) start+offset,options(nostack,preserves_flags));
                    }
                }
            }
        }
        // SAFETY: CPU0-only instruction visibility after all cleans; full local
        // I-cache invalidation covers future EL0 virtual instruction aliases.
        unsafe {
            asm!(
                "dsb sy",
                "ic iallu",
                "dsb sy",
                "isb",
                options(nostack, preserves_flags)
            );
        }
        Ok(())
    }
    pub fn entry(&self) -> u64 {
        self.entry
    }
    pub fn frame_count(&self) -> usize {
        self.count
    }
    pub fn root_address(&self) -> u64 {
        self.table_address(0)
    }
    pub fn page(&self, va: u64) -> Option<PageInfo> {
        let i = slot(va)?;
        Some(PageInfo {
            physical: self.pages[i].as_ref()?.physical_address(),
            permissions: self.permissions[i]?,
        })
    }
    pub fn read_into(&self, mut va: u64, output: &mut [u8]) -> Result<(), Error> {
        context()?;
        let end = va.checked_add(output.len() as u64).ok_or(Error::Bounds)?;
        let mut cursor = va;
        while cursor < end {
            self.page(cursor).ok_or(Error::Bounds)?;
            cursor = ((cursor / PAGE as u64) + 1) * PAGE as u64;
        }
        let mut remaining = output;
        while !remaining.is_empty() {
            let i = slot(va).ok_or(Error::Bounds)?;
            let offset = va as usize & (PAGE - 1);
            let n = remaining.len().min(PAGE - offset);
            let frame = self.pages[i].as_ref().ok_or(Error::Corrupt)?;
            remaining[..n].copy_from_slice(&frame.bytes()[offset..offset + n]);
            remaining = &mut remaining[n..];
            va += n as u64;
        }
        Ok(())
    }
    fn table_address(&self, i: usize) -> u64 {
        self.tables[i]
            .as_ref()
            .map_or(0, OwnedFrame::physical_address)
    }
    fn set(&mut self, table: usize, index: usize, value: u64) -> Result<(), Error> {
        let frame = self.tables[table].as_mut().ok_or(Error::Corrupt)?;
        frame.bytes_mut()[index * 8..index * 8 + 8].copy_from_slice(&value.to_le_bytes());
        Ok(())
    }
    fn word(&self, table: usize, index: usize) -> Option<u64> {
        let b = self
            .tables
            .get(table)?
            .as_ref()?
            .bytes()
            .get(index * 8..index * 8 + 8)?;
        Some(u64::from_le_bytes(b.try_into().ok()?))
    }
    fn build_tables(&mut self) -> Result<(), Error> {
        self.set(0, 0, device_root() | 3)?;
        self.set(0, 1, self.table_address(1) | 3)?;
        self.set(0, 4, self.table_address(10) | 3)?;
        self.set(10, 0, self.table_address(11) | 3)?;
        for i in 0..8 {
            self.set(1, i, self.table_address(2 + i) | 3)?;
        }
        for i in 0..4096 {
            self.set(2 + i / 512, i % 512, boot_ram(i))?;
        }
        for i in 0..512 {
            if let Some(page) = self.page(USER_IMAGE_START + i as u64 * PAGE as u64) {
                self.set(11, i, page.physical | attributes(page.permissions))?;
                let index = ram_index(page.physical).ok_or(Error::Corrupt)?;
                if boot_ram(index) != page.physical | RW {
                    return Err(Error::Corrupt);
                }
                if page.permissions.executable() {
                    self.set(2 + index / 512, index % 512, page.physical | RO)?;
                }
            }
        }
        Ok(())
    }
    pub fn audit(&self) -> bool {
        if context().is_err() {
            return false;
        }
        if (self.startup && self.pages[256].is_none())
            || self.count != TABLES + self.pages.iter().filter(|p| p.is_some()).count()
            || self.count > if self.startup { 285 } else { 284 }
        {
            return false;
        }
        for table in &self.tables {
            let Some(frame) = table else {
                return false;
            };
            let pa = frame.physical_address();
            let Some(i) = ram_index(pa) else {
                return false;
            };
            if boot_ram(i) != pa | RW {
                return false;
            }
        }
        for i in 0..512 {
            let expected_root = match i {
                0 => device_root() | 3,
                1 => self.table_address(1) | 3,
                4 => self.table_address(10) | 3,
                _ => 0,
            };
            let expected_ram = if i < 8 {
                self.table_address(2 + i) | 3
            } else {
                0
            };
            if self.word(0, i) != Some(expected_root)
                || self.word(1, i) != Some(expected_ram)
                || self.word(10, i)
                    != Some(if i == 0 {
                        self.table_address(11) | 3
                    } else {
                        0
                    })
            {
                return false;
            }
            let expected = match (&self.pages[i], self.permissions[i]) {
                (None, None) => 0,
                (Some(frame), Some(p))
                    if i < 256
                        || (i == 256 && self.startup && p == Permissions::ReadOnly)
                        || (i >= 496 && p == Permissions::ReadWrite) =>
                {
                    let pa = frame.physical_address();
                    if ram_index(pa).is_none_or(|j| boot_ram(j) != pa | RW) {
                        return false;
                    }
                    pa | attributes(p)
                }
                _ => return false,
            };
            if self.word(11, i) != Some(expected) {
                return false;
            }
            if i >= 496 && self.pages[i].is_none() {
                return false;
            }
        }
        for i in 0..4096 {
            let pa = RAM + i as u64 * PAGE as u64;
            let executable = self
                .pages
                .iter()
                .zip(self.permissions.iter())
                .any(|(f, p)| {
                    *p == Some(Permissions::ReadExecute)
                        && f.as_ref().is_some_and(|f| f.physical_address() == pa)
                });
            let expected = if executable { pa | RO } else { boot_ram(i) };
            if self.word(2 + i / 512, i % 512) != Some(expected) {
                return false;
            }
        }
        self.page(self.entry)
            .is_some_and(|p| p.permissions.executable())
    }
    fn buffer_visible(&self, start: u64, bytes: usize) -> bool {
        let Some(end) = start.checked_add(bytes as u64) else {
            return false;
        };
        if bytes == 0 || start < RAM || end > RAM + 16 * 1024 * 1024 {
            return false;
        }
        let first = ((start - RAM) / PAGE as u64) as usize;
        let last = ((end - 1 - RAM) / PAGE as u64) as usize;
        (first..=last).all(|i| {
            let expected = (RAM + i as u64 * PAGE as u64) | RW;
            boot_ram(i) == expected && self.word(2 + i / 512, i % 512) == Some(expected)
        })
    }
    #[inline(never)]
    pub fn probe(&mut self, queries: &[Probe]) -> Result<ProbeBatch, Error> {
        if queries.len() > MAX_PROBES {
            return Err(Error::Bounds);
        }
        context()?;
        if super::threads::BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(Error::Busy);
        }
        let _lease = Lease;
        context()?;
        if self.cpus == 2
            && ((hvc(0x8400_000a, 0xc400_0004, 0) as i64) < 0 || hvc(0xc400_0004, 1, 0) != 1)
        {
            return Err(Error::Peer);
        }
        if !self.audit() {
            return Err(Error::Corrupt);
        }
        let before = controls();
        let mut local = [Probe {
            address: 0,
            access: Access::El0Read,
        }; MAX_PROBES];
        local[..queries.len()].copy_from_slice(queries);
        let mut results = [ProbeResult::default(); MAX_PROBES];
        let mut roots = [0u64; 2];
        // Validate actual storage, including optimizer return-slot placement.
        // A caller can keep return storage in the omitted owned arena.
        if !self.buffer_visible(local.as_ptr() as u64, core::mem::size_of_val(&local))
            || !self.buffer_visible(results.as_ptr() as u64, core::mem::size_of_val(&results))
            || !self.buffer_visible(roots.as_ptr() as u64, core::mem::size_of_val(&roots))
        {
            return Err(Error::Context);
        }
        // SAFETY: All tables/frames are retained and audited; CPU0 has masked
        // exceptions and BUSY, peer OFF. Assembly gets only bounded private
        // buffers whose actual addresses retain identical RW identity mappings;
        // it returns under the original root after full TLBI.
        unsafe {
            arm_space_probe(
                self.root_address(),
                local.as_ptr(),
                results.as_mut_ptr(),
                queries.len(),
                roots.as_mut_ptr(),
            );
        }
        let restored = controls() == before && roots[1] == boot_root();
        if !restored {
            fatal();
        }
        Ok(ProbeBatch {
            count: queries.len(),
            results,
            root_during: roots[0],
            root_restored: roots[1],
            controls_restored: restored,
        })
    }
}

struct Lease;
impl Drop for Lease {
    fn drop(&mut self) {
        super::threads::BUSY.store(false, Ordering::Release);
    }
}
fn slot(va: u64) -> Option<usize> {
    let index = va.checked_sub(USER_IMAGE_START)? / PAGE as u64;
    (index < 512).then_some(index as usize)
}
fn ram_index(pa: u64) -> Option<usize> {
    let offset = pa.checked_sub(RAM)?;
    (offset < 16 * 1024 * 1024 && offset.is_multiple_of(PAGE as u64))
        .then_some((offset / PAGE as u64) as usize)
}
fn attributes(p: Permissions) -> u64 {
    match p {
        Permissions::ReadOnly => 0x0060_0000_0000_07c3,
        Permissions::ReadWrite => 0x0060_0000_0000_0743,
        Permissions::ReadExecute => 0x0020_0000_0000_07c3,
    }
}
fn boot_root() -> u64 {
    core::ptr::addr_of!(arm_root) as u64
}
fn device_root() -> u64 {
    core::ptr::addr_of!(arm_device_l2) as u64
}
fn boot_ram(i: usize) -> u64 {
    // SAFETY: Only bounded private callers use indices below4096; Factory's
    // platform proof and unchanged hardware-update controls keep these immutable.
    unsafe {
        core::ptr::addr_of!(arm_ram_l3)
            .cast::<u64>()
            .add(i)
            .read_volatile()
    }
}
fn controls() -> [u64; 10] {
    let (root, ttbr1, tcr, mair, sctlr, vbar, sp0, daif, pan, par);
    // SAFETY: Read-only EL1 observations, no pointers or register writes.
    unsafe {
        asm!("mrs {0}, ttbr0_el1", "mrs {1}, ttbr1_el1", "mrs {2}, tcr_el1",
        "mrs {3}, mair_el1", "mrs {4}, sctlr_el1", "mrs {5}, vbar_el1",
        "mrs {6}, sp_el0", "mrs {7}, daif", "mrs {8}, S3_0_C4_C2_3", "mrs {9}, par_el1",
        out(reg) root,out(reg) ttbr1,out(reg) tcr,out(reg) mair,out(reg) sctlr,
        out(reg) vbar,out(reg) sp0,out(reg) daif,out(reg) pan,out(reg) par,
        options(nomem,nostack,preserves_flags));
    }
    [root, ttbr1, tcr, mair, sctlr, vbar, sp0, daif, pan, par]
}
pub(super) fn context() -> Result<(), Error> {
    if super::cpu_local::current_id() != Some(0) {
        return Err(Error::Context);
    }
    let c = controls();
    if c[0] != boot_root()
        || c[1] != 0
        || c[2] != 0x80993519
        || c[3] != 0xff
        || c[4] != 0x3058180d
        || c[7] != 0x3c0
        || c[8] != (1 << 22)
    {
        return Err(Error::Context);
    }
    Ok(())
}
fn hvc(function: u64, a: u64, b: u64) -> u64 {
    let result;
    // SAFETY: Only PSCI_FEATURES and AFFINITY_INFO are called with admitted
    // HVC boot authority under BUSY; SMCCC caller clobbers are explicit.
    unsafe {
        asm!("hvc #0", inlateout("x0") function => result,
        inlateout("x1") a => _, inlateout("x2") b => _, inlateout("x3") 0u64 => _,
        clobber_abi("C"),options(nostack));
    }
    result
}
fn fatal() -> ! {
    loop {
        // SAFETY: A failed restoration is terminal and cannot release owners.
        unsafe {
            asm!("wfe", options(nomem, nostack));
        }
    }
}
const _: () = {
    assert!(core::mem::size_of::<Probe>() == 16);
    assert!(core::mem::size_of::<ProbeResult>() == 24);
};
