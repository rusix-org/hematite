//! Persistent ordinary-monitor launch authority; design 0026.
use super::frames::Pool;
use super::process::{Error, Executor, Reason};
use super::process_group::{End, Group, TaskId, Turn};
use super::user_space::{Factory, Space};
use core::sync::atomic::Ordering;
use hm_kernel::boot_image::{self, AdmissionError};
use hm_kernel::cpu_topology::Topology;
use hm_kernel::native_launch::{
    Availability, GroupLaunchReport, GroupOutcome, LaunchError, LaunchReport, NativeStatus, Outcome,
};

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
    /// Same permanent BootPlatform proof as run; both owners remain local.
    #[inline(never)]
    // SAFETY: Only BootPlatform passes its actual pool/topology/root authority.
    pub(super) unsafe fn run_group(
        &mut self,
        pool: &Pool,
        topology: &Topology,
        paths: [&str; 2],
    ) -> Result<GroupLaunchReport, LaunchError> {
        if topology.affinities() != [0] {
            return Err(LaunchError::Unsupported);
        }
        if super::threads::BUSY.load(Ordering::Acquire) {
            return Err(LaunchError::Busy);
        }
        super::user_space::context().map_err(|_| LaunchError::Unavailable)?;
        if self.completed.checked_add(2).is_none() {
            self.authority = Authority::Unavailable;
            return Err(LaunchError::Unavailable);
        }
        // SAFETY: Preserve the actual permanent BootPlatform proof above.
        unsafe { self.initialize(topology) }?;
        let Authority::Ready(executor) = &mut self.authority else {
            return Err(LaunchError::Unavailable);
        };
        let factory = {
            // SAFETY: Factory receives this invocation's actual Pool/topology
            // and immutable mappings, with no external peer starts or DMA.
            unsafe { Factory::from_boot(pool, topology) }
        }
        .map_err(|_| LaunchError::Unavailable)?;
        let before = pool.stats().free;
        let result = run_pair(
            executor,
            factory,
            paths,
            &mut self.completed,
            &mut self.generation,
            before,
        );
        let after = pool.stats().free;
        if before != after {
            self.authority = Authority::Unavailable;
            return Err(LaunchError::Unavailable);
        }
        if matches!(result, Err(LaunchError::Unavailable))
            || result
                .as_ref()
                .is_ok_and(|report| report.outcomes.contains(&GroupOutcome::Unavailable))
        {
            self.authority = Authority::Unavailable;
        }
        result.map(|mut report| {
            report.after = after;
            report
        })
    }

    // SAFETY: Caller proves permanent BootPlatform topology and boot mappings.
    unsafe fn initialize(&mut self, topology: &Topology) -> Result<(), LaunchError> {
        if matches!(self.authority, Authority::NotInitialized) {
            let checks = super::timer::checks();
            if !(checks.delivery && checks.context && checks.quiescent && checks.timeout) {
                self.authority = Authority::Unavailable;
                return Err(LaunchError::Unavailable);
            }
            // SAFETY: Caller supplies actual boot authority and bounded timer
            // preflight above establishes delivery and restored quiescence.
            self.authority = match unsafe { Executor::from_boot_terminal(topology) } {
                Ok(executor) => Authority::Ready(executor),
                Err(_) => Authority::Unavailable,
            };
        }
        match self.authority {
            Authority::Ready(_) => Ok(()),
            _ => Err(LaunchError::Unavailable),
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
        // SAFETY: Preserve the caller's actual BootPlatform proof.
        unsafe { self.run_inner(pool, topology, path, None) }
    }
    /// Same actual BootPlatform proof as run; input is copied before execution.
    #[inline(never)]
    // SAFETY: Only BootPlatform supplies this private entry's boot authority.
    pub(super) unsafe fn run_with_input(
        &mut self,
        pool: &Pool,
        topology: &Topology,
        path: &str,
        input: &hm_kernel::startup_input::Input,
    ) -> Result<LaunchReport, LaunchError> {
        // SAFETY: Preserve the caller's actual BootPlatform proof.
        unsafe { self.run_inner(pool, topology, path, Some(input)) }
    }
    #[inline(never)]
    // SAFETY: Private callers preserve the actual Pool/topology/root proof.
    unsafe fn run_inner(
        &mut self,
        pool: &Pool,
        topology: &Topology,
        path: &str,
        input: Option<&hm_kernel::startup_input::Input>,
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
        // SAFETY: Preserve run's actual permanent BootPlatform proof.
        unsafe { self.initialize(topology) }?;
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
        let result = load_and_run(executor, &mut self.generation, &factory, &image, input);
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

/// Both admissions precede execution; Group drop reclaims every inactive owner.
#[inline(never)]
fn run_pair(
    executor: &mut Executor,
    factory: Factory<'_>,
    paths: [&str; 2],
    completed: &mut u64,
    generation: &mut Option<u32>,
    before: usize,
) -> Result<GroupLaunchReport, LaunchError> {
    let mut created = Group::new(executor, factory, 512);
    let group = match &mut created {
        Ok(group) => group,
        Err(error) => return Err(group_error(*error)),
    };
    let first = admit_group(group, paths[0], b"worker-a");
    *generation = group.last_prepared_generation().or(*generation);
    let first = first?;
    let second = admit_group(group, paths[1], b"worker-b");
    *generation = group.last_prepared_generation().or(*generation);
    let ids = [first, second?];
    let mut outcomes = [GroupOutcome::Unavailable; 2];
    let mut order = [0; 2];
    let mut finished = [false; 2];
    let mut count = 0;
    // Each task has at most 256 accounted turns and one no-entry termination.
    for _ in 0..514 {
        match group.turn().map_err(group_error)? {
            Turn::Idle => return Err(LaunchError::Unavailable),
            Turn::Progress(_) => {}
            Turn::Completed(completion) => {
                let id = completion.report().task().id();
                let index = ids
                    .iter()
                    .position(|candidate| *candidate == id)
                    .ok_or(LaunchError::Unavailable)?;
                if finished[index] || count >= 2 {
                    return Err(LaunchError::Unavailable);
                }
                group.reap(id).map_err(group_error)?;
                *completed = completed.checked_add(1).ok_or(LaunchError::Unavailable)?;
                finished[index] = true;
                outcomes[index] = group_outcome(completion.end());
                order[count] = index;
                count += 1;
                if count == 2 {
                    return Ok(GroupLaunchReport {
                        outcomes,
                        completion_order: order,
                        completed: *completed,
                        generation: generation.ok_or(LaunchError::Unavailable)?,
                        before,
                        // Outer State checks the actual Pool after Group drop.
                        after: before,
                    });
                }
                if completion.end() == End::Unavailable {
                    for (index, id) in ids.iter().enumerate() {
                        if !finished[index] {
                            group.request_cancel(*id).map_err(group_error)?;
                        }
                    }
                }
            }
        }
    }
    Err(LaunchError::Unavailable)
}

// Keep image and copied-input temporaries out of the retained scheduling frame.
#[inline(never)]
fn admit_group(group: &mut Group<'_, '_>, path: &str, bytes: &[u8]) -> Result<TaskId, LaunchError> {
    let image = admit(path);
    let input = hm_kernel::startup_input::Input::from_bytes(bytes);
    match (&image, &input) {
        (Ok(image), Ok(input)) => group.admit(image, true, Some(input)).map_err(group_error),
        (Err(error), _) => Err(*error),
        _ => Err(LaunchError::Unavailable),
    }
}

fn group_outcome(end: End) -> GroupOutcome {
    match end {
        End::Exited(code) => GroupOutcome::Exited(code),
        End::Fault => GroupOutcome::Fault,
        End::Budget => GroupOutcome::Budget,
        End::CallLimit => GroupOutcome::CallLimit,
        End::TurnLimit => GroupOutcome::TurnLimit,
        End::Cancelled => GroupOutcome::Cancelled,
        End::Unavailable => GroupOutcome::Unavailable,
    }
}

fn group_error(error: super::process_group::Error) -> LaunchError {
    use super::process_group::Error as E;
    match error {
        E::Unsupported => LaunchError::Unsupported,
        E::Busy => LaunchError::Busy,
        E::Bounds | E::Full | E::Resources => LaunchError::Resources,
        E::Prepare(error) => core_error(error),
        E::Load(super::user_space::Error::Budget | super::user_space::Error::Exhausted) => {
            LaunchError::Resources
        }
        E::Load(super::user_space::Error::Busy) => LaunchError::Busy,
        _ => LaunchError::Unavailable,
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
    input: Option<&hm_kernel::startup_input::Input>,
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
    run_loaded(executor, generation, space, input)
}
#[inline(never)]
fn run_loaded(
    executor: &mut Executor,
    generation: &mut Option<u32>,
    space: Space<'_>,
    input: Option<&hm_kernel::startup_input::Input>,
) -> Result<Outcome, LaunchError> {
    let prepared = match input {
        Some(input) => executor.prepare_with_input(space, true, 1, input),
        None => executor.prepare(space, true, 1),
    };
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
