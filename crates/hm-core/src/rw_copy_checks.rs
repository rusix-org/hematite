//! Inactive owned-RW copy checks; no user execution or hardware authority minted here.
#![forbid(unsafe_code)]
use super::frames::Pool;
use super::user_space::{Access, Error, Factory, Probe, Space};
use core::fmt::Write;
use hm_kernel::elf::{Image, Permissions};
const B: u64 = 0x100000000;
const P: u64 = 4096;
const ELF: &[u8] = include_bytes!("../fixtures/rw-copy.elf");
const CASES: [&str; 12] = [
    "lengths",
    "page_end",
    "cross_rw",
    "cross_stack",
    "tail_ro",
    "tail_rx",
    "tail_absent",
    "protected",
    "holes",
    "bounds",
    "startup",
    "translations",
];
type Check = Result<(), ()>;
fn require(value: bool) -> Check {
    if value { Ok(()) } else { Err(()) }
}
#[inline(never)]
fn free(pool: &Pool) -> usize {
    pool.stats().free
}
#[inline(never)]
pub fn run(pool: &Pool, factory: &Factory<'_>, output: &mut dyn Write) -> bool {
    let result = checks(pool, factory, output);
    if result.is_err() {
        let _ = writeln!(output, "HMRWCOPY FAIL");
    }
    result.is_ok()
}
#[inline(never)]
fn checks(pool: &Pool, factory: &Factory<'_>, output: &mut dyn Write) -> Check {
    let baseline = free(pool);
    writeln!(output, "HMRWCOPY BEGIN passes=2 cases=12").map_err(|_| ())?;
    for pass in 0..2 {
        for (case, name) in CASES.iter().enumerate() {
            require(free(pool) == baseline)?;
            writeln!(
                output,
                "HMRWCOPY CASE pass={pass} name={name} before={baseline}"
            )
            .map_err(|_| ())?;
            one(pool, factory, output, pass, case, baseline)?;
            require(free(pool) == baseline)?;
            writeln!(
                output,
                "HMRWCOPY PASS pass={pass} name={name} frames=35 during={} after={baseline}",
                baseline.checked_sub(35).ok_or(())?
            )
            .map_err(|_| ())?;
        }
    }
    writeln!(output, "HMRWCOPY END cases=24").map_err(|_| ())
}
fn header() -> [u8; 64] {
    let mut bytes = [0; 64];
    bytes[..8].copy_from_slice(b"HEMATITE");
    bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
    bytes[12..16].copy_from_slice(&64u32.to_le_bytes());
    bytes[32..40].copy_from_slice(&256u64.to_le_bytes());
    bytes[40..48].copy_from_slice(&4096u64.to_le_bytes());
    bytes[48..56].copy_from_slice(&64u64.to_le_bytes());
    bytes
}
#[inline(never)]
fn one(
    pool: &Pool,
    factory: &Factory<'_>,
    output: &mut dyn Write,
    pass: usize,
    case: usize,
    baseline: usize,
) -> Check {
    let image = Image::parse(ELF).map_err(|_| ())?;
    require(image.page_count() == 6 && image.segments().len() == 5 && image.entry() == B + P)?;
    let mut loaded = factory.load(&image, 34);
    let space = loaded.as_mut().map_err(|_| ())?;
    require(space.frame_count() == 34)?;
    space.install_startup(&header(), 1, None).map_err(|_| ())?;
    require(
        space.frame_count() == 35
            && free(pool) == baseline.checked_sub(35).ok_or(())?
            && space.audit(),
    )?;
    for (slot, permission) in [
        (0, Permissions::ReadWrite),
        (1, Permissions::ReadExecute),
        (2, Permissions::ReadWrite),
        (3, Permissions::ReadWrite),
        (4, Permissions::ReadOnly),
        (6, Permissions::ReadWrite),
        (256, Permissions::ReadOnly),
    ] {
        require(
            space
                .page(B + slot * P)
                .is_some_and(|p| p.permissions == permission),
        )?;
    }
    require(space.page(B + 5 * P).is_none() && space.page(B + 7 * P).is_none())?;
    fixture_bytes(space, false)?;
    startup(space)?;
    exercise(pool, space, output, pass, case)?;
    startup(space)?;
    fixture_bytes(space, true)?;
    require(space.audit() && space.frame_count() == 35 && free(pool) == baseline - 35)
}
#[inline(never)]
fn fixture_bytes(space: &Space<'_>, protected_only: bool) -> Check {
    let mut bytes = [0; 256];
    for (slot, pattern) in [
        (0, 0x10u8),
        (1, 0x21),
        (2, 0x32),
        (3, 0x43),
        (4, 0x54),
        (6, 0x76),
    ] {
        if protected_only && slot != 1 && slot != 4 {
            continue;
        }
        for offset in (0..4096).step_by(256) {
            space
                .read_into(B + slot * P + offset as u64, &mut bytes)
                .map_err(|_| ())?;
            for (i, b) in bytes.iter().enumerate() {
                let expected = if slot == 1 && offset + i < 4 {
                    [0, 0, 0, 0x14][offset + i]
                } else {
                    pattern
                };
                require(*b == expected)?;
            }
        }
    }
    Ok(())
}
#[inline(never)]
fn startup(space: &Space<'_>) -> Check {
    let h = header();
    let mut bytes = [0; 256];
    for offset in (0..4096).step_by(256) {
        space
            .read_into(B + 256 * P + offset as u64, &mut bytes)
            .map_err(|_| ())?;
        for (i, b) in bytes.iter().enumerate() {
            require(*b == if offset + i < 64 { h[offset + i] } else { 0 })?;
        }
    }
    Ok(())
}
#[inline(never)]
fn operation(pool: &Pool, space: &mut Space<'_>, va: u64, len: usize, success: bool) -> Check {
    let baseline = free(pool);
    let mut input = [0u8; 257];
    for (i, b) in input.iter_mut().enumerate() {
        *b = i as u8;
    }
    let mut old = [0u8; 256];
    let mut mapped = [false; 256];
    for i in 0..len.min(256) {
        if let Some(address) = va.checked_add(i as u64)
            && space.page(address).is_some()
        {
            space
                .read_into(address, &mut old[i..i + 1])
                .map_err(|_| ())?;
            mapped[i] = true;
        }
    }
    let mut neighbors = [(0u64, 0u8, false); 3];
    for (n, address) in [
        va.checked_sub(1),
        va.checked_add(len as u64),
        if len == 257 {
            va.checked_add(256)
        } else {
            None
        },
    ]
    .into_iter()
    .enumerate()
    {
        if let Some(address) = address
            && space.page(address).is_some()
        {
            let mut byte = [0];
            space.read_into(address, &mut byte).map_err(|_| ())?;
            neighbors[n] = (address, byte[0], true);
        }
    }
    let expected = if success { Ok(()) } else { Err(Error::Bounds) };
    require(space.validate_rw(va, len) == expected)?;
    require(space.copy_into_rw(va, &input[..len]) == expected)?;
    for i in 0..len.min(256) {
        if mapped[i] {
            let mut byte = [0];
            space.read_into(va + i as u64, &mut byte).map_err(|_| ())?;
            require(byte[0] == if success { input[i] } else { old[i] })?;
        }
    }
    for (address, old, valid) in neighbors {
        if valid {
            let mut byte = [0];
            space.read_into(address, &mut byte).map_err(|_| ())?;
            require(byte[0] == old)?;
        }
    }
    require(space.frame_count() == 35 && free(pool) == baseline)
}
#[inline(never)]
fn exercise(
    pool: &Pool,
    space: &mut Space<'_>,
    output: &mut dyn Write,
    pass: usize,
    case: usize,
) -> Check {
    match case {
        0 => {
            for len in [1, 255, 256] {
                operation(pool, space, B + 2 * P + 32, len, true)?;
            }
        }
        1 => operation(pool, space, B + 2 * P + 3840, 256, true)?,
        2 => operation(pool, space, B + 2 * P + 3968, 256, true)?,
        3 => operation(pool, space, B + 496 * P + 3968, 256, true)?,
        4 => operation(pool, space, B + 3 * P + 3968, 256, false)?,
        5 => operation(pool, space, B + 3968, 256, false)?,
        6 => operation(pool, space, B + 6 * P + 3968, 256, false)?,
        7 => {
            for slot in [1, 4, 256] {
                operation(pool, space, B + slot * P, 1, false)?;
            }
        }
        8 => {
            for slot in [5, 7, 255, 257, 495] {
                operation(pool, space, B + slot * P, 1, false)?;
            }
        }
        9 => {
            for len in [0, 257] {
                operation(pool, space, B + 2 * P, len, false)?;
            }
            for (va, len) in [
                (u64::MAX, 2),
                (B - 1, 1),
                (B + 512 * P, 1),
                (B + 512 * P - 1, 2),
            ] {
                operation(pool, space, va, len, false)?;
            }
        }
        10 => {
            startup(space)?;
            operation(pool, space, B + 256 * P + 64, 256, false)?;
            startup(space)?;
        }
        11 => {
            let before = translations(space, output, pass, 0)?;
            operation(pool, space, B + 2 * P + 3968, 256, true)?;
            let after = translations(space, output, pass, 1)?;
            require(before == after)?;
        }
        _ => return Err(()),
    }
    Ok(())
}
#[inline(never)]
fn translations(
    space: &mut Space<'_>,
    output: &mut dyn Write,
    pass: usize,
    stage: usize,
) -> Result<[[u64; 3]; 12], ()> {
    let slots = [2, 3, 4, 1, 256, 495];
    let queries = core::array::from_fn::<_, 12, _>(|i| Probe {
        address: B + slots[i / 2] * P,
        access: if i % 2 == 0 {
            Access::El0Read
        } else {
            Access::El0Write
        },
    });
    let batch = space.probe(&queries).map_err(|_| ())?;
    require(
        batch.count == 12
            && batch.controls_restored
            && batch.root_during == space.root_address()
            && batch.root_during != batch.root_restored,
    )?;
    writeln!(
        output,
        "HMRWCOPY ROOT pass={pass} stage={stage} during={:#x} restored={:#x} controls=1",
        batch.root_during, batch.root_restored
    )
    .map_err(|_| ())?;
    let mut results = [[0; 3]; 12];
    for (i, (query, result)) in queries.iter().zip(batch.results.iter()).enumerate() {
        let physical = space.page(query.address).map_or(0, |p| p.physical);
        let succeeds = i < 4 || (i < 10 && i % 2 == 0);
        require(
            result.before & 1 == 1 && result.after & 1 == 1 && (result.active & 1 == 0) == succeeds,
        )?;
        if succeeds {
            require(result.active & 0x0000_ffff_ffff_f000 == physical)?;
        }
        results[i] = [result.before, result.active, result.after];
        writeln!(output,"HMRWCOPY PROBE pass={pass} stage={stage} index={i} va={:#x} access={} before={:#x} active={:#x} after={:#x} physical={physical:#x}",query.address,if i%2==0 {"r"}else{"w"},result.before,result.active,result.after).map_err(|_| ())?;
    }
    Ok(results)
}
