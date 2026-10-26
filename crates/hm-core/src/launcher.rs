//! Persistent ordinary-monitor launch authority; design 0026.
use super::frames::Pool;
use super::process::{Error, Executor, Reason};
use super::user_space::{Factory, Space};
use core::sync::atomic::Ordering;
use hm_kernel::boot_image::{self, AdmissionError};
use hm_kernel::cpu_topology::Topology;
use hm_kernel::native_launch::{Availability, LaunchError, LaunchReport, NativeStatus, Outcome};

pub(super) static ARCHIVE: &[u8] = include_bytes!("../fixtures/apps-initramfs.cpio");
enum Authority {
    NotInitialized,
    Ready(Executor),
    Unavailable,
}
/// Executor's private CPU affinity also makes this state non-Send/non-Sync.
pub(super) struct State {
    authority: Authority,
    completed: u64,
    generation: Option<u32>,
}
impl State {
    pub(super) const fn new() -> Self {
        Self {
            authority: Authority::NotInitialized,
            completed: 0,
            generation: None,
        }
    }
    pub(super) fn status(&self, pool: Option<&Pool>) -> NativeStatus {
        NativeStatus {
            availability: match &self.authority {
                Authority::NotInitialized => Availability::NotInitialized,
                Authority::Ready(_) => Availability::Ready,
                Authority::Unavailable => Availability::Unavailable,
            },
            completed: self.completed,
            generation: self.generation,
            free: pool.map(|p| p.stats().free),
        }
    }
    /// Caller proves Pool/topology belong to the actual permanent BootPlatform,
    /// with reviewed immutable roots, no DMA, and no external peer CPU starts.
    #[inline(never)]
    // SAFETY: Only BootPlatform supplies this private entry's actual boot proof.
    pub(super) unsafe fn run(
        &mut self,
        pool: &Pool,
        topology: &Topology,
        path: &str,
    ) -> Result<LaunchReport, LaunchError> {
        if topology.affinities() != [0] {
            return Err(LaunchError::Unsupported);
        }
        let image = admit(path)?;
        if super::threads::BUSY.load(Ordering::Acquire) {
            return Err(LaunchError::Busy);
        }
        super::user_space::context().map_err(|_| LaunchError::Unavailable)?;
        if self.completed == u64::MAX {
            self.authority = Authority::Unavailable;
            return Err(LaunchError::Unavailable);
        }
        if matches!(self.authority, Authority::NotInitialized) {
            let checks = super::timer::checks();
            if !(checks.delivery && checks.context && checks.quiescent && checks.timeout) {
                self.authority = Authority::Unavailable;
                return Err(LaunchError::Unavailable);
            }
            // SAFETY: Actual BootPlatform proof is required by run; complete
            // bounded timer preflight above establishes delivery/quiescence.
            self.authority = match unsafe { Executor::from_boot_terminal(topology) } {
                Ok(executor) => Authority::Ready(executor),
                Err(_) => Authority::Unavailable,
            };
        }
        let Authority::Ready(executor) = &mut self.authority else {
            return Err(LaunchError::Unavailable);
        };
        let factory = {
            // SAFETY: Same actual Pool/topology and immutable-root proof as run;
            // Factory is scoped to this invocation, not stored self-referentially.
            unsafe { Factory::from_boot(pool, topology) }
        }
        .map_err(|_| LaunchError::Unavailable)?;
        let before = pool.stats().free;
        let result = load_and_run(executor, &mut self.generation, &factory, &image);
        let after = pool.stats().free;
        if before != after {
            self.authority = Authority::Unavailable;
            return Err(LaunchError::Unavailable);
        }
        if matches!(result, Err(LaunchError::Unavailable)) {
            self.authority = Authority::Unavailable;
        }
        let outcome = result?;
        self.completed = self
            .completed
            .checked_add(1)
            .ok_or(LaunchError::Unavailable)?;
        let generation = self.generation.ok_or(LaunchError::Unavailable)?;
        Ok(LaunchReport {
            outcome,
            completed: self.completed,
            generation,
            before,
            after,
        })
    }
}

// Keep the complete archive plan out of State::run's live launch call chain.
#[inline(never)]
fn admit(path: &str) -> Result<hm_kernel::elf::Image<'static>, LaunchError> {
    boot_image::admit(ARCHIVE, path).map_err(admission_error)
}

#[inline(never)]
fn load_and_run(
    executor: &mut Executor,
    generation: &mut Option<u32>,
    factory: &Factory<'_>,
    image: &hm_kernel::elf::Image<'_>,
) -> Result<Outcome, LaunchError> {
    let budget = image
        .page_count()
        .checked_add(28)
        .ok_or(LaunchError::Resources)?;
    let space = factory.load(image, budget).map_err(|error| match error {
        super::user_space::Error::Budget | super::user_space::Error::Exhausted => {
            LaunchError::Resources
        }
        super::user_space::Error::Busy => LaunchError::Busy,
        _ => LaunchError::Unavailable,
    })?;
    run_loaded(executor, generation, space)
}
#[inline(never)]
fn run_loaded(
    executor: &mut Executor,
    generation: &mut Option<u32>,
    space: Space<'_>,
) -> Result<Outcome, LaunchError> {
    let prepared = executor.prepare(space, true, 1);
    let process = match prepared {
        Ok(process) => process,
        // Borrow the failure metadata; retain its Space in the owned Result
        // until return drops it, without a second large by-value error owner.
        Err(ref failure) => return Err(core_error(failure.0)),
    };
    let handle = process.handle();
    let issued = (handle >> 32) as u32;
    if issued == 0 || handle as u32 != 1 {
        return Err(LaunchError::Unavailable);
    }
    *generation = Some(issued);
    let report = executor.run(process).map_err(core_error)?;
    if !report.controls {
        return Err(LaunchError::Unavailable);
    }
    Ok(match report.reason {
        Reason::Exit => Outcome::Exited(report.code),
        Reason::Fault => Outcome::Fault,
        Reason::Deadline => Outcome::Deadline,
        Reason::CallLimit => Outcome::CallLimit,
    })
}
fn core_error(error: Error) -> LaunchError {
    match error {
        Error::Busy => LaunchError::Busy,
        Error::Budget | Error::Exhausted => LaunchError::Resources,
        _ => LaunchError::Unavailable,
    }
}
fn admission_error(error: AdmissionError) -> LaunchError {
    match error {
        AdmissionError::Archive(error) => LaunchError::Archive(error),
        AdmissionError::Missing => LaunchError::Missing,
        AdmissionError::Directory => LaunchError::Directory,
        AdmissionError::Elf(error) => LaunchError::Elf(error),
    }
}
