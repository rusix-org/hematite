//! Fixed, one-use failure boundaries. Absent from ordinary kernels; design 0030.
use super::*;
use process::NegativeCase;

static ARCHIVE: &[u8] = include_bytes!("../fixtures/task-negative-initramfs.cpio");
// The generated witness also carries host-checked layout facts. Only the loop
// bounds are consumed by hooks; the independent artifact gate checks every field.
#[allow(dead_code)]
mod fixture {
    include!("../fixtures/task-negative-cases/layout.rs");
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HookStatus {
    Armed,
    Fired,
    ConsumedFailed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SeedKind {
    None,
    TaskCapabilityMax,
    ChildSlotsMax,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NegativeObservation {
    case: NegativeCase,
    state: HookStatus,
    fired: u8,
    seed: SeedKind,
}
impl NegativeObservation {
    pub(crate) fn case(self) -> NegativeCase {
        self.case
    }
    pub(crate) fn state(self) -> HookStatus {
        self.state
    }
    pub(crate) fn fired(self) -> u8 {
        self.fired
    }
    pub(crate) fn seed(self) -> SeedKind {
        self.seed
    }
    pub(crate) fn succeeded(self) -> bool {
        self.state == HookStatus::Fired && self.fired == 1
    }
}
#[derive(Clone, Copy)]
pub(super) enum Injection {
    FinalBudget,
    Retire,
    Abandon,
    WakeInstall,
    ChildRetire,
}
impl Injection {
    pub(super) fn emit(self, group: u64, turn: u64) {
        let kind = match self {
            Self::FinalBudget => "final_budget",
            Self::Retire => "retire",
            Self::Abandon => "abandon",
            Self::WakeInstall => "wake_install",
            Self::ChildRetire => "child_retire",
        };
        let _ = writeln!(Uart, "HMTASK INJECT group={group} turn={turn} kind={kind}");
    }
}
pub(super) struct HookState {
    observation: NegativeObservation,
    first: Option<(TaskId, u32)>,
    wake_fact: Option<Injection>,
}
impl HookState {
    pub(super) fn new(case: NegativeCase) -> Self {
        let seed = match case {
            NegativeCase::CapabilityExhaustion => SeedKind::TaskCapabilityMax,
            NegativeCase::SlotExhaustion => SeedKind::ChildSlotsMax,
            _ => SeedKind::None,
        };
        let seeded = seed != SeedKind::None;
        Self {
            observation: NegativeObservation {
                case,
                state: if seeded {
                    HookStatus::Fired
                } else {
                    HookStatus::Armed
                },
                fired: u8::from(seeded),
                seed,
            },
            first: None,
            wake_fact: None,
        }
    }
    fn armed(&self, case: NegativeCase) -> bool {
        self.observation.case == case && self.observation.state == HookStatus::Armed
    }
    fn consume(&mut self) {
        if self.observation.state != HookStatus::Armed {
            process::fatal();
        }
        self.observation.state = HookStatus::ConsumedFailed;
    }
    fn succeed(&mut self) {
        if self.observation.state != HookStatus::ConsumedFailed || self.observation.fired != 0 {
            process::fatal();
        }
        self.observation.state = HookStatus::Fired;
        self.observation.fired = 1;
    }
}
impl<'e, 'p> Group<'e, 'p> {
    /// Immutable canonical bytes; this conveys no hook provenance.
    pub(crate) fn negative_archive() -> &'static [u8] {
        ARCHIVE
    }
    #[inline(never)]
    pub(crate) fn new_negative_task(
        executor: &'e mut Executor,
        factory: Factory<'p>,
    ) -> Result<Self, Error> {
        let case = executor.take_negative_profile().ok_or(Error::Mode)?;
        let path = match case {
            NegativeCase::FinalBudgetSpawn => "bin/final_budget_spawn",
            NegativeCase::FinalBudgetPark => "bin/final_budget_park",
            NegativeCase::PlatformReply => "bin/platform_reply",
            NegativeCase::PlatformPark => "bin/platform_park",
            NegativeCase::AbandonDispatch => "bin/abandon_dispatch",
            NegativeCase::WakeInstallFailure => "bin/wake_install_failure",
            NegativeCase::CapabilityExhaustion => "bin/capability_exhaustion",
            NegativeCase::SlotExhaustion => "bin/slot_exhaustion",
            NegativeCase::ChildRetireReady => "bin/child_retire_ready",
            NegativeCase::ChildRetireWaiting => "bin/child_retire_waiting",
        };
        let image = hm_kernel::boot_image::admit(ARCHIVE, path).map_err(Error::Catalog)?;
        Self::new_task_inner(executor, factory, &image, ARCHIVE, true, Some(case))
    }
    pub(crate) fn negative_observation(&self) -> Result<NegativeObservation, Error> {
        self.task
            .as_ref()
            .and_then(|a| a.negative.as_ref())
            .map(|h| h.observation)
            .ok_or(Error::Mode)
    }
    fn negative_hook(&self) -> Option<&HookState> {
        self.authority().negative.as_ref()
    }
    fn negative_hook_mut(&mut self) -> &mut HookState {
        self.authority_mut()
            .negative
            .as_mut()
            .unwrap_or_else(|| process::fatal())
    }
    fn negative_fire(&mut self, capture: &mut TaskCapture, fact: Injection) {
        let hook = self.negative_hook_mut();
        hook.consume();
        hook.succeed();
        if capture.injection.replace(fact).is_some() {
            process::fatal();
        }
    }
    pub(super) fn negative_commit(&mut self, child: TaskId, program: u32) {
        if let Some(hook) = self.authority_mut().negative.as_mut()
            && hook.first.is_none()
        {
            hook.first = Some((child, program));
        }
    }
    pub(super) fn negative_abandon(
        &mut self,
        index: usize,
        guard: &TaskAdmission,
        capture: &mut TaskCapture,
    ) -> bool {
        let matches = self
            .negative_hook()
            .is_some_and(|h| h.armed(NegativeCase::AbandonDispatch))
            && self.slots[index].info().id == self.authority().root
            && guard.calls() == 2
            && guard.operation() == TaskOperation::Spawn;
        if matches {
            self.negative_fire(capture, Injection::Abandon);
        }
        matches
    }
    /// Called only by manage after validating/consuming the actual pending child.
    pub(super) fn negative_install_wake(
        &mut self,
        root: TaskId,
        pending: Pending,
        session: Session,
    ) -> Result<(), Session> {
        let matches = self.negative_hook().is_some_and(|h| {
            h.armed(NegativeCase::WakeInstallFailure) && h.first == Some((pending.child, 1))
        }) && root == self.authority().root
            && self.authority().dispatching == Some(root)
            && !self.selected_owner(root.slot()).has_session();
        if matches {
            let hook = self.negative_hook_mut();
            hook.consume();
            hook.succeed();
            if hook.wake_fact.replace(Injection::WakeInstall).is_some() {
                process::fatal();
            }
            Err(session)
        } else {
            self.selected_owner_mut(root.slot()).put_session(session)
        }
    }
    pub(super) fn negative_take_wake(&mut self, capture: &mut TaskCapture) {
        let fact = self
            .authority_mut()
            .negative
            .as_mut()
            .and_then(|h| h.wake_fact.take());
        if let Some(fact) = fact
            && capture.injection.replace(fact).is_some()
        {
            process::fatal();
        }
    }
    /// Returns a sticky invalid-clock flag; poll exhaustion with valid clocks
    /// retires without fabricating a clock error or a successful injection.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn negative_final_boundary(
        &mut self,
        index: usize,
        report: &StepReport,
        work: &Work,
        capture: &mut TaskCapture,
        hard_end: u64,
        last: &mut u64,
    ) -> bool {
        let Some(hook) = self.negative_hook() else {
            return false;
        };
        if hook.observation.state != HookStatus::Armed {
            return false;
        }
        let case = hook.observation.case;
        let id = self.slots[index].info().id;
        let root = self.authority().root;
        let reply = id == root
            && work.counters.calls == 1
            && capture.commit.is_some()
            && matches!(&work.settlement, Some(Settlement::Reply { outcome: Outcome::Reply(r), .. }) if r.status == Status::Ok);
        let park = id == root
            && work.counters.calls == 2
            && matches!(&work.settlement, Some(Settlement::Park(w)) if hook.first == Some((w.pending.child, 2)));
        let child = hook.first == Some((id, 2))
            && report.entered
            && report.event == Event::Timer
            && (fixture::LOOP_START..fixture::LOOP_END).contains(&report.elr)
            && self.authority().closing.is_none()
            && !self.slots[root.slot()].cancel;
        let ready = self.authority().waiting.is_none()
            && self.authority().dispatching.is_none()
            && self.slots[root.slot()]
                .owner
                .as_ref()
                .is_some_and(|p| p.has_session());
        let waiting = self
            .authority()
            .waiting
            .as_ref()
            .is_some_and(|w| w.pending.child == id)
            && self.authority().dispatching.is_none();
        match case {
            NegativeCase::FinalBudgetSpawn if reply => {}
            NegativeCase::FinalBudgetPark if park => {}
            NegativeCase::PlatformReply if reply => {
                self.negative_fire(capture, Injection::Retire);
                self.executor.retire();
                return false;
            }
            NegativeCase::PlatformPark if park => {
                self.negative_fire(capture, Injection::Retire);
                self.executor.retire();
                return false;
            }
            NegativeCase::ChildRetireReady if child && ready => {
                self.negative_fire(capture, Injection::ChildRetire);
                self.executor.retire();
                return false;
            }
            NegativeCase::ChildRetireWaiting if child && waiting => {
                self.negative_fire(capture, Injection::ChildRetire);
                self.executor.retire();
                return false;
            }
            _ => return false,
        }
        self.negative_hook_mut().consume();
        for _ in 0..(1u32 << 24) {
            let now = process::task_counter(self, self.selected_owner(index), work, capture);
            if now < *last || now > process::MAX_COUNT {
                self.executor.retire();
                return true;
            }
            *last = now;
            if now >= hard_end {
                self.negative_hook_mut().succeed();
                if capture.injection.replace(Injection::FinalBudget).is_some() {
                    process::fatal();
                }
                return false;
            }
        }
        self.executor.retire();
        false
    }
}
