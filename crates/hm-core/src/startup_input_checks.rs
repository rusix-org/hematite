//! Actual owned input delivery/containment checks; design 0027 Stage B.
#![forbid(unsafe_code)]
use super::frames::Pool;
use super::process::{Executor, Process, Reason};
use super::startup_input_inputs::{ARCHIVE, CASES, Case, Payload};
use super::user_space::{Factory, Space};
use core::fmt::Write;
use hm_kernel::startup_input::{Error as InputError, Input};
type Checked<T = ()> = Result<T, ()>;
fn require(ok: bool) -> Checked {
    if ok { Ok(()) } else { Err(()) }
}

#[inline(never)]
pub fn reject_before(pool: &Pool, output: &mut dyn Write) -> bool {
    reject_length(pool, output, false, 0).is_ok()
}
#[inline(never)]
fn reject_length(
    pool: &Pool,
    output: &mut dyn Write,
    initialized: bool,
    generation: u32,
) -> Checked {
    let before = pool.stats().free;
    require(matches!(
        Input::from_bytes(&[0xa5; 257]),
        Err(InputError::Length)
    ))?;
    let after = pool.stats().free;
    require(before == after)?;
    writeln!(output, "HMINPUT REJECT phase={} error=length initialized={} generation={generation} before={before} after={after}",
        if initialized {"after"} else {"before"}, u8::from(initialized)).map_err(|_| ())
}
#[inline(never)]
pub fn run(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> bool {
    checks(pool, factory, executor, root, vectors, output).is_ok()
}
#[inline(never)]
fn checks(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> Checked {
    let baseline = pool.stats().free;
    writeln!(output, "HMINPUT BEGIN 24").map_err(|_| ())?;
    writeln!(
        output,
        "HMINPUT INFO wire=1 extension=1 max_input=256 cpus=1 timer_frequency={} timer_ticks={}",
        executor.frequency(),
        executor.ticks()
    )
    .map_err(|_| ())?;
    let mut generation = 0;
    for pass in 0..2 {
        for (number, case) in CASES.iter().enumerate() {
            generation = one(
                pool,
                factory,
                executor,
                pass * 12 + number,
                *case,
                baseline,
                root,
                vectors,
                output,
            )?;
        }
    }
    require(generation == 24)?;
    reject_length(pool, output, true, generation)?;
    prepare_failure(pool, factory, executor, baseline, false)?;
    prepare_failure(pool, factory, executor, baseline, true)?;
    let generation = prepare_drop(pool, factory, executor, baseline)?;
    require(generation == 27 && pool.stats().free == baseline)?;
    writeln!(output,"HMINPUT PREPARE budget=1 exhausted=1 drop=1 generation={generation} before={baseline} after={}",pool.stats().free).map_err(|_| ())?;
    writeln!(output, "HMINPUT END 24").map_err(|_| ())
}
// Archive admission must not retain a large temporary on the owned-load chain.
#[inline(never)]
fn admit(path: &str) -> Checked<hm_kernel::elf::Image<'static>> {
    hm_kernel::boot_image::admit(ARCHIVE, path).map_err(|_| ())
}
#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn one(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    index: usize,
    case: Case,
    baseline: usize,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> Checked<u32> {
    require(pool.stats().free == baseline)?;
    let image = admit(case.path)?;
    let space = factory
        .load(&image, image.page_count() + 28)
        .map_err(|_| ())?;
    loaded(
        pool, executor, space, index, case, baseline, root, vectors, output,
    )
}
#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn loaded(
    pool: &Pool,
    executor: &mut Executor,
    space: Space<'_>,
    index: usize,
    case: Case,
    baseline: usize,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> Checked<u32> {
    let process = {
        let payload = case.payload;
        let mut source = [0; 256];
        for (i, byte) in source.iter_mut().enumerate().take(payload.length()) {
            *byte = payload.byte(i);
        }
        let input = if payload.present() {
            Input::from_bytes(&source[..payload.length()]).map_err(|_| ())?
        } else {
            Input::absent()
        };
        let prepared = executor.prepare_with_input(space, true, 1, &input);
        // Mutate the original source after preparation, then end both source
        // and Input scopes before readback or EL0 entry. No Process is returned
        // through another large owning helper frame.
        source.fill(0xa5);
        require(source.iter().all(|b| *b == 0xa5))?;
        match prepared {
            Ok(process) => process,
            Err(ref _failure) => return Err(()),
        }
    };
    let handle = process.handle();
    let generation = (handle >> 32) as u32;
    let owned = process.frame_count();
    require(
        generation as usize == index + 1
            && handle as u32 == 1
            && pool.stats().free == baseline - owned,
    )?;
    writeln!(output,"HMINPUT RUN index={index} pass={} case={} fixture={} present={} length={} generation={generation} entry={} root={} before={baseline} during={} owned={owned}",
        index / 12, case.name,case.fixture,u8::from(case.payload.present()),case.payload.length(),process.entry(),process.root_address(),pool.stats().free).map_err(|_| ())?;
    verify_startup(&process, case.payload, handle)?;
    writeln!(
        output,
        "HMINPUT READBACK index={index} base=1 extension=1 zero_tail=1 source_detached=1"
    )
    .map_err(|_| ())?;
    let report = executor.run(process).map_err(|_| ())?;
    let (reason, code) = match case.payload {
        Payload::Write | Payload::Execute => (Reason::Fault, 0),
        Payload::Loop => (Reason::Deadline, 0),
        Payload::Absent => (Reason::Exit, 10),
        Payload::Empty => (Reason::Exit, 11),
        _ => (Reason::Exit, 12),
    };
    let name = match report.reason {
        Reason::Exit => "exit",
        Reason::Fault => "fault",
        Reason::Deadline => "deadline",
        Reason::CallLimit => "calllimit",
    };
    writeln!(output,"HMINPUT STOP index={index} reason={name} code={} calls={} written={} esr={} far={} elr={} spsr={} irqs={}",report.code,report.calls,report.written,report.esr,report.far,report.elr,report.spsr,report.irqs).map_err(|_| ())?;
    require(
        report.reason == reason
            && report.code == code
            && report.controls
            && report.root == root
            && report.vbar == vectors
            && pool.stats().free == baseline,
    )?;
    require(
        report.calls == if reason == Reason::Exit { 3 } else { 0 }
            && report.written
                == if reason == Reason::Exit {
                    case.payload.length() + 1
                } else {
                    0
                }
            && report.irqs == u64::from(reason == Reason::Deadline),
    )?;
    writeln!(output,"HMINPUT RESTORE index={index} root={} vbar={} controls=1 inactive=1 before={baseline} after={}",report.root,report.vbar,pool.stats().free).map_err(|_| ())?;
    writeln!(
        output,
        "HMINPUT PASS index={index} pass={} case={}",
        index / 12,
        case.name
    )
    .map_err(|_| ())?;
    Ok(generation)
}
#[inline(never)]
fn verify_startup(process: &Process<'_>, payload: Payload, handle: u64) -> Checked {
    let mut buffer = [0u8; 256];
    for offset in (0..4096).step_by(256) {
        process.copy_startup(offset, &mut buffer).map_err(|_| ())?;
        for (i, byte) in buffer.iter().enumerate() {
            require(*byte == expected_byte(offset + i, payload, handle))?;
        }
    }
    let mut sentinel = [0xa5; 2];
    for offset in [4095, usize::MAX] {
        require(
            matches!(
                process.copy_startup(offset, &mut sentinel),
                Err(super::user_space::Error::Bounds)
            ) && sentinel == [0xa5; 2],
        )?;
    }
    require(matches!(
        process.copy_startup(4097, &mut []),
        Err(super::user_space::Error::Bounds)
    ))?;
    process.copy_startup(4096, &mut []).map_err(|_| ())
}
fn expected_byte(offset: usize, payload: Payload, handle: u64) -> u8 {
    match offset {
        0..=7 => b"HEMATITE"[offset],
        8 | 16 | 33 => 1,
        12 | 48 => 64,
        41 => 16,
        24..=31 => handle.to_le_bytes()[offset - 24],
        64..=71 if payload.present() => b"HMINPUT\0"[offset - 64],
        72 if payload.present() => 1,
        76 if payload.present() => 24,
        80..=83 if payload.present() => (payload.length() as u32).to_le_bytes()[offset - 80],
        offset if payload.present() && (88..88 + payload.length()).contains(&offset) => {
            payload.byte(offset - 88)
        }
        _ => 0,
    }
}
#[inline(never)]
fn prepare_failure(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    baseline: usize,
    exhausted: bool,
) -> Checked {
    let image = admit(CASES[0].path)?;
    let space = factory
        .load(&image, image.page_count() + 28)
        .map_err(|_| ())?;
    failed_loaded(pool, executor, space, baseline, exhausted)
}
#[inline(never)]
fn failed_loaded(
    pool: &Pool,
    executor: &mut Executor,
    space: Space<'_>,
    baseline: usize,
    exhausted: bool,
) -> Checked {
    let identity = (space.entry(), space.root_address(), space.frame_count());
    let free = pool.stats().free;
    let pressure = if exhausted {
        Some(pool.allocate_heap(free * 4096, 4096).map_err(|_| ())?)
    } else {
        None
    };
    let expected_free = if exhausted { 0 } else { free };
    require(pool.stats().free == expected_free)?;
    let input = Input::from_bytes(b"failure").map_err(|_| ())?;
    let result = executor.prepare_with_input(space, true, usize::from(exhausted), &input);
    let Err((error, returned)) = &result else {
        return Err(());
    };
    require(
        *error
            == if exhausted {
                super::process::Error::Exhausted
            } else {
                super::process::Error::Budget
            },
    )?;
    require(
        returned.audit()
            && (
                returned.entry(),
                returned.root_address(),
                returned.frame_count(),
            ) == identity
            && returned.page(super::process::STARTUP).is_none()
            && pool.stats().free == expected_free,
    )?;
    drop(pressure);
    drop(result);
    require(pool.stats().free == baseline)
}
#[inline(never)]
fn prepare_drop(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    baseline: usize,
) -> Checked<u32> {
    let image = admit(CASES[0].path)?;
    let space = factory
        .load(&image, image.page_count() + 28)
        .map_err(|_| ())?;
    drop_loaded(pool, executor, space, baseline)
}
#[inline(never)]
fn drop_loaded(
    pool: &Pool,
    executor: &mut Executor,
    space: Space<'_>,
    baseline: usize,
) -> Checked<u32> {
    let process = {
        let mut source = [0x57];
        let input = Input::from_bytes(&source).map_err(|_| ())?;
        let prepared = executor.prepare_with_input(space, true, 1, &input);
        source.fill(0xa5);
        require(source == [0xa5])?;
        match prepared {
            Ok(process) => process,
            Err(ref _failure) => return Err(()),
        }
    };
    let generation = (process.handle() >> 32) as u32;
    require(
        generation == 27
            && process.handle() as u32 == 1
            && pool.stats().free == baseline - process.frame_count(),
    )?;
    verify_startup(&process, Payload::Write, process.handle())?;
    drop(process);
    require(pool.stats().free == baseline)?;
    Ok(generation)
}
