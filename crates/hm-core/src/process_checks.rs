//! Safe end-to-end process checks; hardware evidence is independently parsed.
#![forbid(unsafe_code)]
use super::frames::Pool;
use super::process::{Executor, Reason, STACK_TOP};
use super::process_inputs::CASES;
use super::user_space::{Factory, Space};
use core::fmt::Write;
use hm_kernel::elf::{Image, Permissions, USER_IMAGE_START};

type Checked = Result<(), ()>;
fn require(ok: bool) -> Checked {
    if ok { Ok(()) } else { Err(()) }
}

#[inline(never)]
pub fn run(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    cpus: usize,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> bool {
    checks(pool, factory, executor, cpus, root, vectors, output).is_ok()
}
#[inline(never)]
fn checks(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    cpus: usize,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> Checked {
    require(cpus == 1 || cpus == 2)?;
    writeln!(output, "HMPROC BEGIN 66").map_err(|_| ())?;
    writeln!(output,"HMPROC INFO wire=1 cpus={cpus} peer_off=1 abi=1 max_write=256 byte_budget=4096 call_limit=64 timer_frequency={} timer_ticks={}",executor.frequency(),executor.ticks()).map_err(|_|())?;
    let baseline = pool.stats().free;
    for pass in 0..2 {
        for (number, (name, bytes)) in CASES.iter().enumerate() {
            one(
                pool, factory, executor, pass, number, name, bytes, baseline, root, vectors, output,
            )?;
        }
    }
    prepare_failure(pool, factory, executor, baseline, false)?;
    prepare_failure(pool, factory, executor, baseline, true)?;
    prepare_drop(pool, factory, executor, baseline)?;
    writeln!(
        output,
        "HMPROC PREPARE budget=1 exhausted=1 drop=1 before={baseline} after={}",
        pool.stats().free
    )
    .map_err(|_| ())?;
    writeln!(output, "HMPROC END 66").map_err(|_| ())
}
// Separate owners from other cases to bound simultaneously live stack storage.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn one(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    pass: usize,
    number: usize,
    name: &str,
    bytes: &[u8],
    baseline: usize,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> Checked {
    require(pool.stats().free == baseline)?;
    let image = Image::parse(bytes).map_err(|_| ())?;
    let owned = image.page_count() + 29;
    let space = factory.load(&image, owned - 1).map_err(|_| ())?;
    verify(&space, &image)?;
    run_loaded(
        pool, executor, space, pass, number, name, baseline, root, vectors, output,
    )
}
#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn run_loaded(
    pool: &Pool,
    executor: &mut Executor,
    space: Space<'_>,
    pass: usize,
    number: usize,
    name: &str,
    baseline: usize,
    root: u64,
    vectors: u64,
    output: &mut dyn Write,
) -> Checked {
    let owned = space.frame_count() + 1;
    let process = executor
        .prepare(space, name != "nogrant", 1)
        .map_err(|_| ())?;
    require(process.frame_count() == owned && pool.stats().free == baseline - owned)?;
    let index = pass * 33 + number;
    writeln!(output,"HMPROC RUN index={index} pass={pass} case={name} fixture={name} entry={} root={} sp={} handle={} before={baseline} during={} owned={owned}",
        process.entry(),process.root_address(),STACK_TOP,process.handle(),pool.stats().free).map_err(|_|())?;
    let report = executor.run(process).map_err(|_| ())?;
    let expected = match number {
        8..=17 | 28 | 31 | 32 => Reason::Fault,
        18 => Reason::Deadline,
        22 | 23 => Reason::CallLimit,
        _ => Reason::Exit,
    };
    let reason = match report.reason {
        Reason::Exit => "exit",
        Reason::Fault => "fault",
        Reason::Deadline => "deadline",
        Reason::CallLimit => "calllimit",
    };
    writeln!(output,"HMPROC STOP index={index} reason={reason} code={} calls={} written={} esr={} far={} elr={} spsr={} irqs={}",
        report.code,report.calls,report.written,report.esr,report.far,report.elr,report.spsr,report.irqs).map_err(|_|())?;
    require(
        report.reason == expected
            && report.code == 0
            && report.controls
            && report.root == root
            && report.vbar == vectors,
    )?;
    require(pool.stats().free == baseline)?;
    writeln!(output,"HMPROC RESTORE index={index} root={} vbar={} controls=1 inactive=1 before={baseline} after={}",report.root,report.vbar,pool.stats().free).map_err(|_|())?;
    writeln!(output, "HMPROC PASS index={index} pass={pass} case={name}").map_err(|_| ())
}
#[inline(never)]
fn verify(space: &Space<'_>, image: &Image<'_>) -> Checked {
    require(space.audit() && space.entry() == image.entry())?;
    let mut buffer = [0; 256];
    for segment in image.segments() {
        for va in (segment.page_start()..segment.page_end()).step_by(4096) {
            require(
                space
                    .page(va)
                    .is_some_and(|p| p.permissions == segment.permissions()),
            )?;
            for offset in (0..4096).step_by(256) {
                space.read_into(va + offset, &mut buffer).map_err(|_| ())?;
                for (i, byte) in buffer.iter().enumerate() {
                    let expected = (va + offset + i as u64)
                        .checked_sub(segment.virtual_address())
                        .and_then(|n| usize::try_from(n).ok())
                        .and_then(|n| segment.file_bytes().get(n))
                        .copied()
                        .unwrap_or(0);
                    require(*byte == expected)?;
                }
            }
        }
    }
    let mut count = 0;
    for i in 0..512 {
        let va = USER_IMAGE_START + i * 4096;
        if let Some(page) = space.page(va) {
            count += 1;
            require(page.physical != space.root_address())?;
            for j in 0..i {
                require(
                    space
                        .page(USER_IMAGE_START + j * 4096)
                        .is_none_or(|other| other.physical != page.physical),
                )?;
            }
            if i >= 496 {
                require(page.permissions == Permissions::ReadWrite)?;
                for offset in (0..4096).step_by(256) {
                    space.read_into(va + offset, &mut buffer).map_err(|_| ())?;
                    require(buffer.iter().all(|b| *b == 0))?;
                }
            }
        }
    }
    require(count == image.page_count() + 16 && space.page(USER_IMAGE_START + 0x100000).is_none())
}

#[inline(never)]
fn prepare_failure(
    pool: &Pool,
    factory: &Factory<'_>,
    executor: &mut Executor,
    baseline: usize,
    exhausted: bool,
) -> Checked {
    let image = Image::parse(CASES[0].1).map_err(|_| ())?;
    let space = factory
        .load(&image, image.page_count() + 28)
        .map_err(|_| ())?;
    check_prepare_failure(pool, executor, space, baseline, exhausted)
}
#[inline(never)]
fn check_prepare_failure(
    pool: &Pool,
    executor: &mut Executor,
    space: Space<'_>,
    baseline: usize,
    exhausted: bool,
) -> Checked {
    let identity = (space.root_address(), space.entry(), space.frame_count());
    let free = pool.stats().free;
    let pressure = if exhausted {
        Some(pool.allocate_heap(free * 4096, 4096).map_err(|_| ())?)
    } else {
        None
    };
    let expected_free = if exhausted { 0 } else { free };
    require(pool.stats().free == expected_free)?;
    let result = executor.prepare(space, true, usize::from(exhausted));
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
                returned.root_address(),
                returned.entry(),
                returned.frame_count(),
            ) == identity,
    )?;
    require(
        returned.page(super::process::STARTUP).is_none() && pool.stats().free == expected_free,
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
) -> Checked {
    let image = Image::parse(CASES[0].1).map_err(|_| ())?;
    let space = factory
        .load(&image, image.page_count() + 28)
        .map_err(|_| ())?;
    drop_loaded(pool, executor, space, baseline)
}
#[inline(never)]
fn drop_loaded(pool: &Pool, executor: &mut Executor, space: Space<'_>, baseline: usize) -> Checked {
    let before = space.frame_count();
    let process = executor.prepare(space, true, 1).map_err(|_| ())?;
    require(process.frame_count() == before + 1 && pool.stats().free == baseline - before - 1)?;
    drop(process);
    require(pool.stats().free == baseline)
}
