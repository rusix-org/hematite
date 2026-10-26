//! Safe guest evidence for design 0022; AT translations are not EL0 execution.
#![forbid(unsafe_code)]
use super::frames::Pool;
use super::mappings::Mapper;
use super::user_space::{Access, Error, Factory, Probe, Space};
use core::fmt::Write;
use hm_kernel::elf::{Image, Permissions, USER_IMAGE_START as BASE};
use hm_kernel::mapping_api::OwnedMapping;

const FIXTURE: &[u8] = include_bytes!("../fixtures/native-elf");
const PAGE: usize = 4096;
const INDICES: [usize; 21] = [
    0, 1, 2, 3, 4, 496, 497, 498, 499, 500, 501, 502, 503, 504, 505, 506, 507, 508, 509, 510, 511,
];
type CheckResult = Result<(), ()>;

#[inline(never)]
pub fn run(
    pool: &Pool,
    mapper: &Mapper,
    factory: &Factory<'_>,
    output: &mut dyn Write,
    cpus: usize,
    kernel_text: u64,
    kernel_data: u64,
) -> bool {
    checks(
        pool,
        mapper,
        factory,
        output,
        cpus,
        kernel_text,
        kernel_data,
    )
    .is_ok()
}

fn require(condition: bool) -> CheckResult {
    if condition { Ok(()) } else { Err(()) }
}
fn free(pool: &Pool) -> usize {
    pool.stats().free
}
fn metric(
    output: &mut dyn Write,
    case: &str,
    before: usize,
    during: usize,
    after: usize,
    frames: usize,
    attempts: usize,
) -> CheckResult {
    writeln!(output, "HMSPACE METRIC case={case} before={before} during={during} after={after} frames={frames} attempts={attempts}").map_err(|_| ())?;
    writeln!(output, "HMSPACE PASS {case}").map_err(|_| ())
}

#[inline(never)]
fn checks(
    pool: &Pool,
    mapper: &Mapper,
    factory: &Factory<'_>,
    output: &mut dyn Write,
    cpus: usize,
    kernel_text: u64,
    kernel_data: u64,
) -> CheckResult {
    require(cpus == 1 || cpus == 2)?;
    let frame = pool.allocate().map_err(|_| ())?;
    let mut arena = mapper.map(frame).map_err(|_| ())?;
    require(arena.virtual_address() == 0x8000_0000)?;
    arena.bytes_mut().fill(0xa5);
    // This real alias borrow remains live through every alternate-root probe.
    let retained = arena.bytes();
    let baseline = free(pool);
    require(baseline >= 284)?;
    let image = Image::parse(FIXTURE).map_err(|_| ())?;
    require(image.page_count() == 5 && image.segments().len() == 3 && image.entry() == BASE)?;
    fixture_case(
        pool,
        factory,
        &image,
        output,
        baseline,
        cpus,
        arena.virtual_address(),
        arena.physical_address(),
        retained,
        kernel_text,
        kernel_data,
    )?;
    budget_case(pool, factory, &image, output, baseline)?;
    pressure_case(pool, factory, &image, output, baseline)?;
    let maximum_during = geometry_max(pool, factory, baseline)?;
    geometry_cross(pool, factory, baseline)?;
    metric(
        output,
        "geometry",
        baseline,
        maximum_during,
        free(pool),
        284,
        2,
    )?;
    forget_case(pool, factory, &image, output, baseline, retained)?;
    writeln!(output, "HMSPACE END 5").map_err(|_| ())?;
    Ok(())
}

// Keep borrowed evidence inputs explicit and large owners in separate stack frames.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn fixture_case(
    pool: &Pool,
    factory: &Factory<'_>,
    image: &Image<'_>,
    output: &mut dyn Write,
    baseline: usize,
    cpus: usize,
    arena_va: u64,
    arena_pa: u64,
    retained: &[u8; PAGE],
    kernel_text: u64,
    kernel_data: u64,
) -> CheckResult {
    let mut space = factory.load(image, 33).map_err(|_| ())?;
    require(space.frame_count() == 33 && free(pool) == baseline - 33)?;
    verify_bytes(&space, image)?;
    require(space.audit())?;
    writeln!(output, "HMSPACE BEGIN 5").map_err(|_| ())?;
    writeln!(
        output,
        "HMSPACE INFO cpus={cpus} peer_off=1 image_pages=5 required=33"
    )
    .map_err(|_| ())?;
    writeln!(output, "HMSPACE ARENA va={} pa={}", arena_va, arena_pa).map_err(|_| ())?;
    for i in INDICES {
        let va = BASE + (i * PAGE) as u64;
        let page = space.page(va).ok_or(())?;
        writeln!(
            output,
            "HMSPACE MAP va={va} pa={} flags={}",
            page.physical,
            flags(page.permissions)
        )
        .map_err(|_| ())?;
    }
    probes(
        &mut space,
        &INDICES,
        arena_va,
        arena_pa,
        retained,
        kernel_text,
        kernel_data,
        output,
    )?;
    require(retained.iter().all(|b| *b == 0xa5))?;
    let during = free(pool);
    drop(space);
    require(free(pool) == baseline)?;
    metric(output, "fixture", baseline, during, free(pool), 33, 1)?;

    Ok(())
}
#[inline(never)]
fn budget_case(
    pool: &Pool,
    factory: &Factory<'_>,
    image: &Image<'_>,
    output: &mut dyn Write,
    baseline: usize,
) -> CheckResult {
    for budget in 0..33 {
        require(matches!(factory.load(image, budget), Err(Error::Budget)))?;
        require(free(pool) == baseline)?;
    }
    metric(output, "budgets", baseline, free(pool), free(pool), 33, 33)?;

    Ok(())
}
#[inline(never)]
fn pressure_case(
    pool: &Pool,
    factory: &Factory<'_>,
    image: &Image<'_>,
    output: &mut dyn Write,
    baseline: usize,
) -> CheckResult {
    // The admitted fresh boot pool has one free run following the image and
    // the retained lowest-frame arena owner. Charge a real owned Block, not a
    // mock allocator or an artificially injected allocation error.
    let pressure = pool
        .allocate_heap((baseline - 8) * PAGE, PAGE)
        .map_err(|_| ())?;
    require(free(pool) == 8)?;
    require(matches!(factory.load(image, 33), Err(Error::Exhausted)))?;
    require(free(pool) == 8)?;
    let during = free(pool);
    drop(pressure);
    require(free(pool) == baseline)?;
    metric(output, "pressure", baseline, during, free(pool), 33, 1)?;

    Ok(())
}
#[inline(never)]
fn forget_case(
    pool: &Pool,
    factory: &Factory<'_>,
    image: &Image<'_>,
    output: &mut dyn Write,
    baseline: usize,
    retained: &[u8; PAGE],
) -> CheckResult {
    let forgotten = factory.load(image, 33).map_err(|_| ())?;
    verify_bytes(&forgotten, image)?;
    require(free(pool) == baseline - 33)?;
    let mut leaked_addresses = [0u64; 22];
    leaked_addresses[0] = forgotten.root_address();
    for (slot, index) in INDICES.iter().enumerate() {
        leaked_addresses[slot + 1] = forgotten
            .page(BASE + (*index * PAGE) as u64)
            .ok_or(())?
            .physical;
    }
    core::mem::forget(forgotten);
    let following = factory.load(image, 33).map_err(|_| ())?;
    require(free(pool) == baseline - 66 && !leaked_addresses.contains(&following.root_address()))?;
    verify_bytes(&following, image)?;
    for index in INDICES {
        let address = following
            .page(BASE + (index * PAGE) as u64)
            .ok_or(())?
            .physical;
        require(!leaked_addresses.contains(&address))?;
    }
    drop(following);
    require(free(pool) == baseline - 33 && retained.iter().all(|b| *b == 0xa5))?;
    metric(output, "forget", baseline, free(pool), free(pool), 33, 1)?;
    Ok(())
}

fn flags(p: Permissions) -> u32 {
    match p {
        Permissions::ReadExecute => 5,
        Permissions::ReadOnly => 4,
        Permissions::ReadWrite => 6,
    }
}

#[inline(never)]
fn verify_bytes(space: &Space<'_>, image: &Image<'_>) -> CheckResult {
    require(space.entry() == image.entry())?;
    let mut page = [0u8; PAGE];
    for segment in image.segments() {
        for va in (segment.page_start()..segment.page_end()).step_by(PAGE) {
            require(
                space
                    .page(va)
                    .is_some_and(|p| p.permissions == segment.permissions()),
            )?;
            space.read_into(va, &mut page).map_err(|_| ())?;
            for (offset, actual) in page.iter().enumerate() {
                let address = va + offset as u64;
                let expected = address
                    .checked_sub(segment.virtual_address())
                    .and_then(|i| usize::try_from(i).ok())
                    .and_then(|i| segment.file_bytes().get(i))
                    .copied()
                    .unwrap_or(0);
                require(*actual == expected)?;
            }
        }
    }
    for i in 496..512 {
        let va = BASE + (i * PAGE) as u64;
        require(
            space
                .page(va)
                .is_some_and(|p| p.permissions == Permissions::ReadWrite),
        )?;
        space.read_into(va, &mut page).map_err(|_| ())?;
        require(page.iter().all(|b| *b == 0))?;
    }
    require(space.page(BASE + 0x1ef000).is_none() && space.page(BASE + 0x100000).is_none())?;
    require(matches!(
        space.read_into(BASE + 0x1ef000, &mut page[..1]),
        Err(Error::Bounds)
    ))?;
    require(matches!(
        space.read_into(u64::MAX, &mut page[..2]),
        Err(Error::Bounds)
    ))?;
    // Crossing the final mapped stack byte into the absent next page must
    // fail before copying the first, otherwise-valid byte.
    page.fill(0x6c);
    require(matches!(
        space.read_into(BASE + 0x1fffff, &mut page[..2]),
        Err(Error::Bounds)
    ))?;
    require(page.iter().all(|b| *b == 0x6c))
}

#[derive(Clone, Copy)]
enum Expected {
    Address(u64),
    Fault(u64),
}
#[derive(Clone, Copy)]
struct Query {
    probe: Probe,
    boot: Expected,
    active: Expected,
}
impl Query {
    const EMPTY: Self = Self {
        probe: Probe {
            address: 0,
            access: Access::El0Read,
        },
        boot: Expected::Fault(5),
        active: Expected::Fault(7),
    };
}
fn matches_par(par: u64, expected: Expected) -> bool {
    match expected {
        Expected::Address(address) => {
            par & 1 == 0 && ((par & 0x000f_ffff_ffff_f000) | (address & 4095)) == address
        }
        Expected::Fault(status) => par & 1 == 1 && (par >> 1) & 63 == status && par & 0x300 == 0,
    }
}
// Keep borrowed evidence inputs explicit and large owners in separate stack frames.
#[allow(clippy::too_many_arguments)]
#[inline(never)]
fn probes(
    space: &mut Space<'_>,
    indices: &[usize; 21],
    arena_va: u64,
    arena_pa: u64,
    retained: &[u8; PAGE],
    kernel_text: u64,
    kernel_data: u64,
    output: &mut dyn Write,
) -> CheckResult {
    let oversized = [Probe {
        address: 0,
        access: Access::El0Read,
    }; 17];
    require(matches!(space.probe(&oversized), Err(Error::Bounds)))?;
    let mut queries = [Query::EMPTY; 71];
    let mut count = 0;
    for i in indices {
        let va = BASE + (*i * PAGE) as u64;
        let page = space.page(va).ok_or(())?;
        for (address, access, boot, active) in [
            (
                va + 17,
                Access::El0Read,
                Expected::Fault(5),
                Expected::Address(page.physical + 17),
            ),
            (
                va + 17,
                Access::El0Write,
                Expected::Fault(5),
                if page.permissions.writable() {
                    Expected::Address(page.physical + 17)
                } else {
                    Expected::Fault(15)
                },
            ),
            (
                page.physical + 17,
                Access::El1Write,
                Expected::Address(page.physical + 17),
                if page.permissions.executable() {
                    Expected::Fault(15)
                } else {
                    Expected::Address(page.physical + 17)
                },
            ),
        ] {
            queries[count] = Query {
                probe: Probe { address, access },
                boot,
                active,
            };
            count += 1;
        }
    }
    for address in [BASE + 0x1ef000, BASE + 0x100000, BASE + 0x10000] {
        queries[count] = Query {
            probe: Probe {
                address,
                access: Access::El0Read,
            },
            boot: Expected::Fault(5),
            active: Expected::Fault(7),
        };
        count += 1;
    }
    for address in [kernel_text, kernel_data, 0x0900_0000, 0x0800_0000] {
        queries[count] = Query {
            probe: Probe {
                address,
                access: Access::El0Read,
            },
            boot: Expected::Fault(15),
            active: Expected::Fault(15),
        };
        count += 1;
    }
    queries[count] = Query {
        probe: Probe {
            address: arena_va,
            access: Access::El1Read,
        },
        boot: Expected::Address(arena_pa),
        active: Expected::Fault(5),
    };
    require(count + 1 == queries.len())?;
    for pass in 0..2 {
        for (batch_id, chunk) in queries.chunks(16).enumerate() {
            let mut batch_input = [Probe {
                address: 0,
                access: Access::El0Read,
            }; 16];
            for (to, from) in batch_input.iter_mut().zip(chunk.iter()) {
                *to = from.probe;
            }
            let batch = space.probe(&batch_input[..chunk.len()]).map_err(|_| ())?;
            require(
                batch.count == chunk.len()
                    && batch.root_during == space.root_address()
                    && batch.root_restored != batch.root_during
                    && batch.controls_restored,
            )?;
            require(retained.iter().all(|b| *b == 0xa5))?;
            writeln!(
                output,
                "HMSPACE ROOT pass={pass} batch={batch_id} active={} restored={} controls={}",
                batch.root_during,
                batch.root_restored,
                usize::from(batch.controls_restored)
            )
            .map_err(|_| ())?;
            for (offset, (result, query)) in batch.results.iter().zip(chunk.iter()).enumerate() {
                require(
                    matches_par(result.before, query.boot)
                        && matches_par(result.active, query.active)
                        && matches_par(result.after, query.boot),
                )?;
                writeln!(output,"HMSPACE PROBE pass={pass} index={} va={} access={} before={} active={} after={}",
                    batch_id*16+offset,query.probe.address,query.probe.access as u64,
                    result.before,result.active,result.after).map_err(|_| ())?;
            }
        }
    }
    Ok(())
}

fn p16(bytes: &mut [u8], at: usize, value: u16) {
    bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
}
fn p32(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}
fn p64(bytes: &mut [u8], at: usize, value: u64) {
    bytes[at..at + 8].copy_from_slice(&value.to_le_bytes());
}
fn header(bytes: &mut [u8]) {
    bytes.fill(0);
    bytes[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    p16(bytes, 16, 2);
    p16(bytes, 18, 183);
    p32(bytes, 20, 1);
    p64(bytes, 24, BASE + 0x100);
    p64(bytes, 32, 64);
    p16(bytes, 52, 64);
    p16(bytes, 54, 56);
    p16(bytes, 56, 2);
}
fn segment(bytes: &mut [u8], at: usize, offset: u64, va: u64, file: u64, memory: u64, flags: u32) {
    p32(bytes, at, 1);
    p32(bytes, at + 4, flags);
    p64(bytes, at + 8, offset);
    p64(bytes, at + 16, va);
    p64(bytes, at + 32, file);
    p64(bytes, at + 40, memory);
    p64(bytes, at + 48, 4096);
}
#[inline(never)]
fn geometry_max(pool: &Pool, factory: &Factory<'_>, baseline: usize) -> Result<usize, ()> {
    let mut bytes = [0u8; 1024];
    header(&mut bytes);
    segment(&mut bytes, 64, 0x100, BASE + 0x100, 16, 16, 5);
    segment(
        &mut bytes,
        120,
        0x200,
        BASE + 0x1200,
        32,
        255 * 4096 - 0x200,
        6,
    );
    bytes[0x100..0x110].fill(0xc3);
    bytes[0x200..0x220].fill(0x7d);
    geometry_load(pool, factory, baseline, &bytes, 256, 284, false)
}
#[inline(never)]
fn geometry_cross(pool: &Pool, factory: &Factory<'_>, baseline: usize) -> CheckResult {
    let mut bytes = [0u8; 8192];
    header(&mut bytes);
    segment(&mut bytes, 64, 0x100, BASE + 0x100, 16, 16, 5);
    segment(&mut bytes, 120, 0xff8, BASE + 0x2ff8, 32, 5000, 6);
    bytes[0x100..0x110].fill(0x3c);
    for (i, b) in bytes[0xff8..0x1018].iter_mut().enumerate() {
        *b = i as u8 + 1;
    }
    geometry_load(pool, factory, baseline, &bytes, 4, 32, true)?;
    Ok(())
}
#[inline(never)]
fn geometry_load(
    pool: &Pool,
    factory: &Factory<'_>,
    baseline: usize,
    bytes: &[u8],
    pages: usize,
    frames: usize,
    cross: bool,
) -> Result<usize, ()> {
    let image = Image::parse(bytes).map_err(|_| ())?;
    require(image.page_count() == pages)?;
    let space = factory.load(&image, frames).map_err(|_| ())?;
    let during = free(pool);
    require(space.frame_count() == frames && during == baseline - frames && space.audit())?;
    verify_bytes(&space, &image)?;
    if cross {
        let mut crossing = [0u8; 32];
        space
            .read_into(BASE + 0x2ff8, &mut crossing)
            .map_err(|_| ())?;
        require(crossing.iter().enumerate().all(|(i, b)| *b == i as u8 + 1))?;
    }
    drop(space);
    require(free(pool) == baseline)?;
    Ok(during)
}
