//! Quiescent finalization and scheduling of task-enabled groups.
use super::*;

impl Group<'_, '_> {
    fn management(&self) -> Management {
        let a = self.authority();
        Management {
            root: a.root,
            woken: None,
            reaped: 0,
            children_remaining: a.caps.iter().filter(|c| c.is_some()).count() as u8,
            closing: a.closing.is_some(),
            live_frames: self.charged,
            closed_root: None,
        }
    }
    fn refresh_management(&self, m: &mut Management) {
        m.children_remaining = self.authority().caps.iter().filter(|c| c.is_some()).count() as u8;
        m.closing = self.authority().closing.is_some();
        m.live_frames = self.charged;
    }
    fn release_owner(&mut self, index: usize) {
        let info = self.slots[index].info.unwrap_or_else(|| process::fatal());
        if let Some(owner) = self.slots[index].owner.take() {
            drop(owner);
            self.charged = self
                .charged
                .checked_sub(info.process_frames + info.box_frames)
                .unwrap_or_else(|| process::fatal());
        }
    }
    fn close_root(&mut self, end: End, evidence: RootEvidence, m: &mut Management) {
        if self.authority().closing.is_some() {
            return;
        }
        let id = self.authority().root;
        let mut info = self.slots[id.slot()].info();
        info.state = TaskState::Closing;
        let closure = RootClosure {
            root: info,
            end,
            evidence,
        };
        self.authority_mut().launch = None;
        let _ = self.authority_mut().waiting.take();
        let _ = self.slots[id.slot()]
            .budget
            .as_mut()
            .unwrap_or_else(|| process::fatal())
            .stop();
        self.release_owner(id.slot());
        self.authority_mut().closing = Some(closure);
        let caps = self.authority().caps;
        for cap in caps.into_iter().flatten() {
            let index = self.index(cap.child).unwrap_or_else(|_| process::fatal());
            if self.slots[index].completion.is_none() {
                self.slots[index].cancel = true;
            }
        }
        m.closed_root = Some(closure);
        self.refresh_management(m);
    }
    fn manage(&mut self, m: &mut Management) -> Result<(), Error> {
        if self.authority().closing.is_some() {
            let caps = self.authority().caps;
            for (slot, cap) in caps.into_iter().enumerate() {
                if let Some(cap) = cap {
                    let index = self.index(cap.child)?;
                    if self.slots[index].completion.is_some() {
                        self.consume_child(Pending {
                            child: cap.child,
                            slot,
                            generation: cap.generation,
                        })?;
                        m.reaped += 1;
                    }
                }
            }
        } else if !self.slots[self.authority().root.slot()].cancel {
            let pending = self.authority().waiting.as_ref().map(|w| w.pending);
            if let Some(pending) = pending {
                let root = self.authority().root;
                let index = self.index(pending.child)?;
                let cap = self.authority().caps[pending.slot].ok_or(Error::Unavailable)?;
                if cap.child != pending.child || cap.generation != pending.generation {
                    return Err(Error::Unavailable);
                }
                if self.slots[index].completion.is_some() {
                    if self.authority().dispatching.is_some()
                        || self.selected_owner(root.slot()).has_session()
                    {
                        return Err(Error::Unavailable);
                    }
                    self.authority_mut().dispatching = Some(root);
                    let waiting = self
                        .authority_mut()
                        .waiting
                        .take()
                        .ok_or(Error::Unavailable)?;
                    let done = self.consume_child(pending)?;
                    m.reaped += 1;
                    let reply = response(Status::Ok, completion_word(done.end));
                    let (session, _) = waiting.session.wake(reply);
                    #[cfg(feature = "task-negative-tests")]
                    let installed = self.negative_install_wake(root, pending, session);
                    #[cfg(not(feature = "task-negative-tests"))]
                    let installed = self.selected_owner_mut(root.slot()).put_session(session);
                    if installed.is_err() {
                        return Err(Error::Unavailable);
                    }
                    self.selected_owner_mut(root.slot()).task_reply(reply);
                    self.authority_mut().dispatching = None;
                    m.woken = Some(root);
                }
            }
        }
        self.refresh_management(m);
        Ok(())
    }
    fn fail_management(&mut self, m: &mut Management, trigger: Option<TaskId>) {
        self.executor.retire();
        self.authority_mut().dispatching = None;
        self.close_root(
            End::Unavailable,
            RootEvidence::Administrative { trigger },
            m,
        );
    }
    pub(crate) fn task_turn(&mut self) -> Result<Turn, Error> {
        // A detached Session may exist only inside a retained turn. Residual
        // Dispatching is a broken internal invariant, never entry authority.
        if self.authority().dispatching.is_some() {
            process::fatal();
        }
        if self.authority().published.is_some() {
            return Ok(Turn::Idle);
        }
        let lease = match Lease::acquire() {
            Ok(l) => l,
            Err(e) => {
                if e != process::Error::Busy {
                    self.executor.retire();
                }
                return Err(lease_error(e));
            }
        };
        let mut management = self.management();
        if self.executor.retired() && self.authority().closing.is_none() {
            self.close_root(
                End::Unavailable,
                RootEvidence::Administrative { trigger: None },
                &mut management,
            );
            if self.manage(&mut management).is_err() {
                process::fatal();
            }
            self.emit_administrative(&management);
            return Ok(Turn::Task(TaskTurn::Administrative { management }));
        }
        if self.authority().closing.is_some() {
            if self.manage(&mut management).is_err() {
                process::fatal();
            }
            if self.authority().caps.iter().all(Option::is_none) {
                let completion = TaskCompletion {
                    closure: self.authority().closing.unwrap_or_else(|| process::fatal()),
                    live_frames: self.charged,
                };
                self.authority_mut().published = Some(completion);
                self.emit_publication(&completion, &management);
                return Ok(Turn::Task(TaskTurn::RootCompleted {
                    completion,
                    management,
                }));
            }
        }
        let root = self.authority().root;
        let selected = (0..CAPACITY)
            .map(|n| (self.cursor + n) % CAPACITY)
            .find(|i| {
                let slot = &self.slots[*i];
                slot.owner.is_some()
                    && (slot.cancel
                        || (!self.executor.retired()
                            && !(root.slot() == *i && self.authority().waiting.is_some())))
            });
        let Some(index) = selected else {
            self.fail_management(&mut management, None);
            self.emit_administrative(&management);
            return Ok(Turn::Task(TaskTurn::Administrative { management }));
        };
        if !self.slots[index].cancel && process::admission().is_err() {
            self.fail_management(&mut management, None);
            self.emit_administrative(&management);
            return Ok(Turn::Task(TaskTurn::Administrative { management }));
        }
        self.cursor = (index + 1) % CAPACITY;
        self.slots[index].ordinal = self.slots[index]
            .ordinal
            .checked_add(1)
            .unwrap_or_else(|| process::fatal());
        let info = self.slots[index].info();
        let prior = info.budget;
        let saved = process::observe();
        let (calls, written) = if index == root.slot() {
            if let Some(wait) = &self.authority().waiting {
                (wait.session.calls(), wait.session.written())
            } else {
                (
                    self.selected_owner(index).calls(),
                    self.selected_owner(index).written(),
                )
            }
        } else {
            (
                self.selected_owner(index).calls(),
                self.selected_owner(index).written(),
            )
        };
        let owner = self.selected_owner(index);
        let (esr, far, elr, spsr) = owner.trap();
        let mut report = StepReport {
            task: info,
            event: Event::None,
            ordinal: self.slots[index].ordinal,
            entered: false,
            entry_pc: owner.pc(),
            elr,
            esr,
            far,
            spsr,
            calls,
            written,
            irqs: self.slots[index].irqs,
            spurious: self.slots[index].spurious,
            boot_root: saved[0],
            vbar: saved[1],
            controls: true,
            charged_delta: 0,
            timing: None,
            live_frames: self.charged,
        };
        let initial = report;
        let mut capture = TaskCapture::new();
        let mut work = Work::new();
        work.counters = Counters { calls, written };
        let before = self.factory.pool().stats().free;
        let mut end = if self.slots[index].cancel {
            Some(End::Cancelled)
        } else if prior.turns >= 256 {
            Some(End::TurnLimit)
        } else {
            None
        };
        if end.is_none() {
            end = self.execute_task_turn(index, &lease, &mut report, &mut work, &mut capture);
        }
        report.task = self.slots[index].info();
        report.calls = work.counters.calls;
        report.written = work.counters.written;
        report.irqs = self.slots[index].irqs;
        report.spurious = self.slots[index].spurious;
        report.charged_delta = report
            .task
            .budget
            .charged
            .checked_sub(prior.charged)
            .unwrap_or_else(|| process::fatal());
        if end.is_none()
            && let Some(settlement) = work.settlement.take()
        {
            match settlement {
                Settlement::Reply { session, outcome } => {
                    if self.authority().dispatching != Some(info.id)
                        || self.selected_owner_mut(index).put_session(session).is_err()
                    {
                        end = Some(End::Unavailable);
                    } else if let Outcome::Reply(reply) = outcome {
                        self.selected_owner_mut(index).task_reply(reply);
                    } else {
                        process::fatal();
                    }
                }
                Settlement::Park(waiting) => {
                    if info.id != root
                        || self.authority().waiting.is_some()
                        || self.authority().dispatching != Some(root)
                    {
                        end = Some(End::Unavailable);
                    } else {
                        self.authority_mut().waiting = Some(waiting);
                        report.task.state = TaskState::Waiting;
                    }
                }
            }
        }
        let _ = work.settlement.take();
        self.authority_mut().dispatching = None;
        if let Some(reason) = end {
            if reason == End::Unavailable {
                self.executor.retire();
            }
            if info.id == root {
                report.task.state = TaskState::Closing;
                self.close_root(reason, RootEvidence::Executed(report), &mut management);
            } else {
                let _ = self.slots[index]
                    .budget
                    .as_mut()
                    .unwrap_or_else(|| process::fatal())
                    .stop();
                self.release_owner(index);
                report.task.state = TaskState::Completed;
                report.live_frames = self.charged;
                self.slots[index].completion = Some(Completion {
                    end: reason,
                    report,
                });
            }
        }
        if self.executor.retired() && self.authority().closing.is_none() {
            self.close_root(
                End::Unavailable,
                RootEvidence::Administrative {
                    trigger: Some(info.id),
                },
                &mut management,
            );
        }
        if self.manage(&mut management).is_err() {
            self.fail_management(&mut management, Some(info.id));
        }
        #[cfg(feature = "task-negative-tests")]
        self.negative_take_wake(&mut capture);
        self.refresh_management(&mut management);
        report.live_frames = self.charged;
        // Freeze executed closure only after this Step's bounded management, so
        // the immutable evidence is exactly the externally returned report.
        if let Some(mut closure) = management.closed_root
            && matches!(closure.evidence, RootEvidence::Executed(_))
        {
            closure.evidence = RootEvidence::Executed(report);
            self.authority_mut().closing = Some(closure);
            management.closed_root = Some(closure);
        }
        if self.executor.tracing() {
            self.emit_task_step(&initial, &report, end, &capture, &management, before);
        }
        Ok(Turn::Task(TaskTurn::Step {
            report,
            terminal: end,
            management,
        }))
    }
    fn execute_task_turn(
        &mut self,
        index: usize,
        lease: &Lease,
        report: &mut StepReport,
        work: &mut Work,
        capture: &mut TaskCapture,
    ) -> Option<End> {
        let remaining = self.slots[index]
            .budget
            .as_ref()
            .unwrap_or_else(|| process::fatal())
            .snapshot()
            .remaining;
        let start = process::task_counter(self, self.selected_owner(index), work, capture);
        if start > process::MAX_COUNT || remaining > process::MAX_COUNT - start {
            self.executor.retire();
            return Some(End::Unavailable);
        }
        let comparator = match self.slots[index]
            .budget
            .as_mut()
            .unwrap_or_else(|| process::fatal())
            .begin(start)
        {
            Ok(v) => v,
            Err(_) => {
                self.executor.retire();
                return Some(End::Unavailable);
            }
        };
        let hard_end = start + remaining;
        let mut last = start;
        let mut end = None;
        let mut clock_failed = false;
        if process::admission().is_err() {
            end = Some(End::Unavailable);
        }
        if end.is_none() && !self.slots[index].cached {
            if self.selected_owner(index).prepare_instructions().is_err() {
                end = Some(End::Unavailable);
            } else {
                self.slots[index].cached = true;
            }
        }
        if end.is_none() && !self.selected_owner(index).executable_pc() {
            end = Some(End::Fault);
        }
        if end.is_none() {
            let saved = process::observe();
            report.boot_root = saved[0];
            report.vbar = saved[1];
            let now = process::task_counter(self, self.selected_owner(index), work, capture);
            if now < last || now > process::MAX_COUNT {
                clock_failed = true;
                end = Some(End::Unavailable);
            } else if now >= hard_end {
                end = Some(End::Budget);
            } else {
                last = now;
                let owner = self.slots[index]
                    .owner
                    .as_mut()
                    .unwrap_or_else(|| process::fatal());
                let kind = self.executor.group_slice(owner, comparator, &saved, lease);
                report.entered = true;
                let (esr, far, elr, spsr) = owner.trap();
                report.esr = esr;
                report.far = far;
                report.elr = elr;
                report.spsr = spsr;
                report.event = match kind {
                    1 => Event::Svc,
                    2 => Event::Fault,
                    3 => Event::Timer,
                    4 => Event::Spurious,
                    _ => process::fatal(),
                };
                match kind {
                    2 => end = Some(End::Fault),
                    3 => {
                        self.slots[index].irqs = self.slots[index]
                            .irqs
                            .checked_add(1)
                            .unwrap_or_else(|| process::fatal())
                    }
                    4 => {
                        self.slots[index].spurious += 1;
                        if self.slots[index].spurious > 8 {
                            end = Some(End::Unavailable);
                        }
                    }
                    1 => {
                        let now =
                            process::task_counter(self, self.selected_owner(index), work, capture);
                        if now < last || now > process::MAX_COUNT {
                            clock_failed = true;
                            end = Some(End::Unavailable);
                        } else if now >= hard_end {
                            end = Some(End::Budget);
                        } else {
                            last = now;
                            self.dispatch_task(index, work, capture, lease);
                            if work.failed {
                                end = Some(End::Unavailable);
                            } else if let Some(Settlement::Reply { session, outcome }) =
                                &work.settlement
                            {
                                end = match outcome {
                                    Outcome::Stop(native::Stop::Exit(code)) => {
                                        Some(End::Exited(*code))
                                    }
                                    Outcome::Stop(native::Stop::CallLimit) => Some(End::CallLimit),
                                    Outcome::Reply(_) => {
                                        session.termination().map(|_| End::CallLimit)
                                    }
                                };
                            }
                        }
                    }
                    _ => process::fatal(),
                }
            }
        }
        #[cfg(feature = "task-negative-tests")]
        if !clock_failed && end.is_none() {
            clock_failed |=
                self.negative_final_boundary(index, report, work, capture, hard_end, &mut last);
        }
        let finish = process::task_counter(self, self.selected_owner(index), work, capture);
        let budget = self.slots[index]
            .budget
            .as_mut()
            .unwrap_or_else(|| process::fatal());
        let result = if clock_failed || finish < last || finish > process::MAX_COUNT {
            let _ = budget.stop();
            Err(hm_kernel::process_budget::Error::Clock)
        } else {
            budget.finish(finish)
        };
        report.timing = Some(Timing {
            start,
            end: finish,
            comparator,
            hard_end,
            accounted: result.is_ok(),
        });
        // Retirement invalidates any provisional reply/park before installation;
        // valid elapsed time remains accounted even when the platform is lost.
        if result.is_err() || self.executor.retired() {
            end = Some(End::Unavailable);
        } else if result == Ok(Finish::Exhausted) && end.is_none() {
            end = Some(End::Budget);
        }
        end
    }
}

fn end_name(end: Option<End>) -> (&'static str, u8) {
    match end {
        None => ("none", 0),
        Some(End::Exited(c)) => ("exit", c),
        Some(End::Fault) => ("fault", 0),
        Some(End::Budget) => ("budget", 0),
        Some(End::CallLimit) => ("call_limit", 0),
        Some(End::TurnLimit) => ("turn_limit", 0),
        Some(End::Cancelled) => ("cancelled", 0),
        Some(End::Unavailable) => ("unavailable", 0),
    }
}
fn state_name(state: TaskState) -> &'static str {
    match state {
        TaskState::Ready => "ready",
        TaskState::CancelPending => "cancel_pending",
        TaskState::Waiting => "waiting",
        TaskState::Closing => "closing",
        TaskState::Completed => "completed",
    }
}
fn bytes_record(group: u64, turn: u64, part: &str, bytes: &[u8]) {
    let _ = write!(
        Uart,
        "HMTASK COPY group={group} turn={turn} part={part} length={} hex=",
        bytes.len()
    );
    for b in bytes {
        let _ = write!(Uart, "{b:02x}");
    }
    let _ = writeln!(Uart);
}
fn closure_record(kind: &str, group: u64, closure: RootClosure, charged: Option<usize>) {
    let (end, code) = end_name(Some(closure.end));
    let (evidence, turn, trigger) = match closure.evidence {
        RootEvidence::Executed(r) => ("executed", r.ordinal, None),
        RootEvidence::Administrative { trigger } => ("administrative", 0, trigger),
    };
    let _ = write!(
        Uart,
        "HMTASK {kind} group={group} root_slot={} root_generation={} end={end} code={code} evidence={evidence} turn={turn} trigger_group={} trigger_slot={} trigger_generation={}",
        closure.root.id.slot(),
        closure.root.id.generation(),
        trigger.map_or(0, TaskId::group),
        trigger.map_or(0, TaskId::slot),
        trigger.map_or(0, TaskId::generation)
    );
    if let Some(charged) = charged {
        let _ = write!(Uart, " charged={charged}");
    }
    let _ = writeln!(Uart);
}
impl Group<'_, '_> {
    fn emit_management(&mut self, m: &Management) {
        let sequence = self
            .authority()
            .sequence
            .checked_add(1)
            .unwrap_or_else(|| process::fatal());
        self.authority_mut().sequence = sequence;
        let _ = writeln!(
            Uart,
            "HMTASK MANAGE group={} sequence={sequence} woken={} reaped={} remaining_children={} closing={} charged={}",
            self.namespace,
            u8::from(m.woken.is_some()),
            m.reaped,
            m.children_remaining,
            u8::from(m.closing),
            m.live_frames
        );
    }
    fn emit_administrative(&mut self, m: &Management) {
        if !self.executor.tracing() {
            return;
        }
        let _ = writeln!(Uart, "HMTASK ADMIN group={} reason=retired", self.namespace);
        if let Some(c) = m.closed_root {
            closure_record("CLOSE", self.namespace, c, None);
        }
        self.emit_management(m);
    }
    fn emit_publication(&mut self, c: &TaskCompletion, m: &Management) {
        if !self.executor.tracing() {
            return;
        }
        self.emit_management(m);
        closure_record("PUBLISH", self.namespace, c.closure, Some(c.live_frames));
    }
    fn emit_task_step(
        &mut self,
        initial: &StepReport,
        r: &StepReport,
        end: Option<End>,
        capture: &TaskCapture,
        m: &Management,
        before: usize,
    ) {
        let group = self.namespace;
        let turn = r.ordinal;
        let prior = initial.task.budget;
        let _ = writeln!(
            Uart,
            "HMTASK STEP group={group} turn={turn} slot={} generation={} state={} calls={} written={} irqs={} remaining={} charged={} turns={} before={before}",
            r.task.id.slot(),
            r.task.id.generation(),
            state_name(initial.task.state),
            initial.calls,
            initial.written,
            initial.irqs,
            prior.remaining,
            prior.charged,
            prior.turns
        );
        if let Some(t) = r.timing {
            let _ = writeln!(
                Uart,
                "HMTASK CLOCK group={group} turn={turn} begin={} remaining={} comparator={} whole_end={}",
                t.start, prior.remaining, t.comparator, t.hard_end
            );
        }
        if r.entered {
            let _ = writeln!(
                Uart,
                "HMTASK ENTER group={group} turn={turn} pc={} root={}",
                r.entry_pc, r.task.root
            );
            let kind = match r.event {
                Event::Svc => "svc",
                Event::Timer => "irq",
                Event::Fault => "fault",
                Event::Spurious => "spurious",
                Event::None => process::fatal(),
            };
            let _ = writeln!(
                Uart,
                "HMTASK TRAP group={group} turn={turn} kind={kind} esr={} far={} elr={} spsr={}",
                r.esr, r.far, r.elr, r.spsr
            );
        }
        capture.output.emit_task_call(group, turn);
        if capture.descriptor_copied {
            bytes_record(group, turn, "descriptor", &capture.descriptor);
        }
        if capture.input_copied != 0 {
            bytes_record(group, turn, "input", &capture.input[..capture.input_copied]);
        }
        if let Some(c) = capture.commit {
            let _ = writeln!(
                Uart,
                "HMTASK COMMIT group={group} turn={turn} cap_slot={} cap_generation={} child_slot={} child_generation={} program={} console_generation={} entry={} root={} process_frames={} box_frames={} before={} after={}",
                c.token as u32,
                c.token >> 32,
                c.child.slot(),
                c.child.generation(),
                c.program,
                c.console,
                c.entry,
                c.root,
                c.frames,
                BOX_FRAMES,
                c.before,
                c.after
            );
        }
        capture.output.emit_task_output(group, turn);
        if let Some(c) = capture.consumed {
            let _ = writeln!(
                Uart,
                "HMTASK CONSUME group={group} turn={turn} child_slot={} child_generation={} cap_slot={} cap_generation={} outcome={}",
                c.pending.child.slot(),
                c.pending.child.generation(),
                c.pending.slot + 3,
                c.pending.generation,
                c.outcome
            );
        }
        if let Some(s) = capture.settle {
            let _ = writeln!(
                Uart,
                "HMTASK SETTLE group={group} turn={turn} kind={} status={} result={} child_slot={} child_generation={} cap_slot={} cap_generation={}",
                s.kind,
                s.status,
                s.result,
                s.pending.map_or(0, |p| p.child.slot()),
                s.pending.map_or(0, |p| p.child.generation()),
                s.pending.map_or(0, |p| p.slot + 3),
                s.pending.map_or(0, |p| p.generation)
            );
        }
        #[cfg(feature = "task-negative-tests")]
        if let Some(fact) = capture.injection
            && !matches!(fact, negative::Injection::WakeInstall)
        {
            fact.emit(group, turn);
        }
        let state = r.task.budget;
        if let Some(t) = r.timing {
            let _ = writeln!(
                Uart,
                "HMTASK CHARGE group={group} turn={turn} end={} accounted={} remaining={} charged={} turns={}",
                t.end,
                u8::from(t.accounted),
                state.remaining,
                state.charged,
                state.turns
            );
        }
        let (event, (terminal, code)) = (
            match r.event {
                Event::None => "none",
                Event::Svc => "svc",
                Event::Timer => "timer",
                Event::Spurious => "spurious",
                Event::Fault => "fault",
            },
            end_name(end),
        );
        let _ = writeln!(
            Uart,
            "HMTASK RETURN group={group} turn={turn} event={event} terminal={terminal} state={} entered={} code={code} calls={} written={} irqs={} elr={} remaining={} charged={} turns={} root={} vbar={} controls=1 before={before} after={}",
            state_name(r.task.state),
            u8::from(r.entered),
            r.calls,
            r.written,
            r.irqs,
            r.elr,
            state.remaining,
            state.charged,
            state.turns,
            r.boot_root,
            r.vbar,
            self.factory.pool().stats().free
        );
        #[cfg(feature = "task-negative-tests")]
        if let Some(fact @ negative::Injection::WakeInstall) = capture.injection {
            fact.emit(group, turn);
        }
        if let Some(c) = m.closed_root {
            closure_record("CLOSE", group, c, None);
        }
        self.emit_management(m);
    }
}
