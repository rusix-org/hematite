//! Copied idle observations and bounded boot-only case driver; no device access.
#![forbid(unsafe_code)]
use super::idle::{self, BootTest, Case, Causes, Observation, Snapshot};
use core::fmt::{self, Write};

const CPU_NAMES: [&str; 26] = [
    "root",
    "vbar",
    "sp0",
    "daif",
    "cntkctl",
    "cpacr",
    "tpidr0",
    "tpidrro",
    "par",
    "sctlr",
    "tcr",
    "mair",
    "ttbr1",
    "elr",
    "spsr",
    "esr",
    "far",
    "fpcr",
    "fpsr",
    "tpidr1",
    "cval",
    "ctl",
    "currentel",
    "spsel",
    "dit",
    "ssbs",
];
const SHARED_NAMES: [&str; 9] = [
    "group0", "group1", "cfg1", "cfg2", "prio7", "prio8", "target8", "pmr", "bpr",
];

fn snapshot(out: &mut impl Write, index: usize, phase: &str, saved: &Snapshot) -> fmt::Result {
    write!(out, "HMIDLE SNAPSHOT index={index} phase={phase}")?;
    for (name, value) in CPU_NAMES.into_iter().zip(saved.controls) {
        write!(out, " {name}={value:#x}")?;
    }
    shared(
        out,
        &saved.fields,
        saved.enable,
        saved.imsc,
        saved.dctl,
        saved.cctl,
    )?;
    writeln!(out)
}
fn shared(
    out: &mut impl Write,
    fields: &[u32; 9],
    enable: [u32; 2],
    imsc: u32,
    dctl: u32,
    cctl: u32,
) -> fmt::Result {
    for (name, value) in SHARED_NAMES.into_iter().zip(fields) {
        write!(out, " {name}={value:#x}")?;
    }
    write!(
        out,
        " enable0={:#x} enable1={:#x} imsc={imsc:#x} dctl={dctl:#x} cctl={cctl:#x}",
        enable[0], enable[1]
    )
}
fn causes(
    out: &mut impl Write,
    index: usize,
    phase: &str,
    sampled: bool,
    value: Causes,
    count: u64,
) -> fmt::Result {
    writeln!(
        out,
        "HMIDLE CAUSE index={index} phase={phase} sampled={} fr={:#x} ris={:#x} mis={:#x} rsr={:#x} ctl={:#x} count={count}",
        u8::from(sampled),
        value.fr,
        value.ris,
        value.mis,
        value.rsr,
        value.ctl
    )
}
fn report(
    out: &mut impl Write,
    test: &BootTest<'_>,
    index: usize,
    value: &Observation,
) -> fmt::Result {
    writeln!(
        out,
        "HMIDLE COUNTS index={index} frequency={} start={} wall_end={} limit={} pre_arm={} precheck={} wake={} final={} samples={} poisoned=0",
        test.frequency(),
        test.start(),
        test.wall_end(),
        value.limit,
        value.initial,
        value.precheck,
        value.post,
        value.final_count,
        value.samples
    )?;
    snapshot(out, index, "entry", &value.saved)?;
    let setup = &value.setup;
    write!(
        out,
        "HMIDLE SETUP index={index} changed={:#x} cval={:#x} ctl={:#x}",
        setup.changed, setup.cval, setup.ctl
    )?;
    shared(
        out,
        &setup.fields,
        setup.enable,
        setup.imsc,
        setup.dctl,
        setup.cctl,
    )?;
    writeln!(out)?;
    let armed = setup.changed != 0;
    causes(
        out,
        index,
        "precheck",
        armed,
        value.pre,
        if armed { value.precheck } else { 0 },
    )?;
    causes(out, index, "post", value.entered, value.after, value.post)?;
    let (wfi, resume) = idle::leaf_addresses();
    writeln!(
        out,
        "HMIDLE WAIT index={index} entered={} expired={} wfi={wfi:#x} resume={resume:#x} acks={} spurious={}",
        u8::from(value.entered),
        u8::from(value.final_count >= value.limit),
        value.ack_count,
        u8::from(value.spurious)
    )?;
    for (ordinal, ack) in value.acks[..value.ack_count].iter().enumerate() {
        let before = ack.before;
        let after = ack.after;
        writeln!(
            out,
            "HMIDLE ACK index={index} ordinal={ordinal} iar={:#x} enable={} source={:#x} pending_before={} active_before={} pending_after={} active_after={} counter_before={} counter_after={} polls_before={} polls_after={}",
            ack.iar,
            u8::from(before.enable != 0),
            ack.source,
            u8::from(before.pending != 0),
            u8::from(before.active != 0),
            u8::from(after.pending != 0),
            u8::from(after.active != 0),
            before.counter,
            after.counter,
            before.iterations,
            after.iterations
        )?;
    }
    for (id, quiet) in [30, 33].into_iter().zip(value.quiet) {
        writeln!(
            out,
            "HMIDLE CLEANUP index={index} id={id} enable={} pending={} active={} counter={} polls={}",
            u8::from(quiet.enable != 0),
            u8::from(quiet.pending != 0),
            u8::from(quiet.active != 0),
            quiet.counter,
            quiet.iterations
        )?;
    }
    snapshot(out, index, "restored", &value.restored)?;
    writeln!(out, "HMIDLE RESTORED index={index} controls=1")
}

pub(super) fn run(mut test: BootTest<'_>, out: &mut impl Write) -> bool {
    let count = test.cases().len();
    if writeln!(out, "HMIDLE BEGIN profile={} cases={count}", test.profile()).is_err() {
        return false;
    }
    for (index, case) in test.cases().iter().copied().enumerate() {
        if writeln!(out, "HMIDLE CASE index={index} name={}", case.name()).is_err() {
            return false;
        }
        if matches!(case, Case::PendingByte | Case::UartWait)
            && writeln!(out, "HMIDLE READY name={}", case.name()).is_err()
        {
            return false;
        }
        let value = match test.wait(case) {
            Ok(value) => value,
            Err(error) => {
                let _ = writeln!(out, "HMIDLE ERROR index={index} stage=wait error={error:?}");
                return false;
            }
        };
        if report(out, &test, index, &value).is_err() {
            return false;
        }
        let pump = match test.pump_one() {
            Ok(pump) => pump,
            Err(error) => {
                let _ = writeln!(out, "HMIDLE ERROR index={index} stage=pump error={error:?}");
                return false;
            }
        };
        let payload = match (pump.count(), pump.dr()) {
            (0, 0) => "none",
            (1, 0x50) => "50",
            (1, 0x55) => "55",
            _ => return false,
        };
        if writeln!(
            out,
            "HMIDLE PUMP index={index} count={} dr={:#x} hex={payload} empty={}",
            pump.count(),
            pump.dr(),
            u8::from(pump.empty())
        )
        .is_err()
        {
            return false;
        }
        let expected = match case {
            Case::PendingByte => 0x50,
            Case::UartWait => 0x55,
            _ => 0,
        };
        let entered = matches!(
            case,
            Case::UartWait | Case::TimerWait | Case::ProductionWall
        );
        if pump.dr() != expected
            || pump.count() != u8::from(expected != 0)
            || value.entered != entered
            || value.ack_count > 2
            || (matches!(case, Case::TimerWait | Case::ProductionWall)
                && (value.post < value.limit || value.after.ctl & 4 == 0))
        {
            return false;
        }
        if writeln!(out, "HMIDLE PASS index={index}").is_err() {
            return false;
        }
    }
    if writeln!(out, "HMIDLE END cases={count}").is_err() {
        return false;
    }
    test.finish().is_ok()
}
