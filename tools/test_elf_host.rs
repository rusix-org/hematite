//! Exercise the actual safe parser against a separately linked ELF and mutations.
#![forbid(unsafe_code)]

use hm_kernel::elf::{Image, Permissions};
use std::fs;
use std::path::Path;

const START: u64 = 0x1_0000_0000;
const END: u64 = START + 1024 * 1024;
const PAGE: u64 = 4096;
const RANDOM_CASES: usize = 4096;
const MUTATIONS: usize = 32;

fn require(okay: bool, context: &str) -> Result<(), String> {
    if okay { Ok(()) } else { Err(context.to_owned()) }
}
fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let part = bytes.get(offset..offset + 2).ok_or("fixture u16 range")?;
    Ok(u16::from_le_bytes([part[0], part[1]]))
}
fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let part = bytes.get(offset..offset + 4).ok_or("fixture u32 range")?;
    Ok(u32::from_le_bytes(part.try_into().map_err(|_| "fixture u32 width")?))
}
fn u64_at(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let part = bytes.get(offset..offset + 8).ok_or("fixture u64 range")?;
    Ok(u64::from_le_bytes(part.try_into().map_err(|_| "fixture u64 width")?))
}
fn flags(permission: Permissions) -> u32 {
    match permission {
        Permissions::ReadOnly => 4,
        Permissions::ReadWrite => 6,
        Permissions::ReadExecute => 5,
    }
}
fn fingerprint(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)
    })
}

// Independently assert every accepted plan's bounded observable invariants.
fn check_plan(image: &Image<'_>, bytes: &[u8]) -> Result<(), String> {
    require(!image.segments().is_empty() && image.segments().len() <= 8, "segment count")?;
    let mut pages = 0;
    let mut previous_end = START;
    let mut entry_found = false;
    for segment in image.segments() {
        let start = segment.virtual_address();
        let end = start.checked_add(segment.memory_size()).ok_or("memory overflow")?;
        let file_end = start.checked_add(segment.file_bytes().len() as u64).ok_or("file virtual overflow")?;
        require(segment.memory_size() != 0 && start >= START && end <= END, "memory window")?;
        require(file_end <= end, "file exceeds memory")?;
        let rounded_start = start / PAGE * PAGE;
        let rounded_end = end.checked_add(PAGE - 1).ok_or("rounding overflow")? / PAGE * PAGE;
        require(segment.page_start() == rounded_start && segment.page_end() == rounded_end,
                "rounded page range")?;
        require(rounded_start >= previous_end && rounded_end <= END, "page overlap/order")?;
        previous_end = rounded_end;
        pages += ((rounded_end - rounded_start) / PAGE) as usize;
        require(segment.permissions().readable(), "read permission")?;
        require(!(segment.permissions().writable() && segment.permissions().executable()), "W^X")?;
        if !segment.file_bytes().is_empty() {
            let address = segment.file_bytes().as_ptr() as usize;
            let input = bytes.as_ptr() as usize;
            let offset = address.checked_sub(input).ok_or("file borrow precedes input")?;
            let file_limit = offset.checked_add(segment.file_bytes().len()).ok_or("borrow overflow")?;
            require(file_limit <= bytes.len(), "file borrow exceeds input")?;
            require(offset as u64 % PAGE == start % PAGE, "page congruence")?;
        }
        if segment.permissions().executable() && image.entry() >= start
            && image.entry().checked_add(4).is_some_and(|value| value <= file_end) {
            entry_found = true;
        }
    }
    require(pages <= 256 && image.page_count() == pages, "page accounting")?;
    require(image.entry().is_multiple_of(4) && entry_found, "file-backed aligned entry")
}

fn fixture(bytes: &[u8]) -> Result<usize, String> {
    let image = Image::parse(bytes).map_err(|error| format!("fixture parse: {error:?}"))?;
    check_plan(&image, bytes)?;
    require(image.segments().len() == 3, "fixture must contain three LOADs")?;
    require(image.segments().iter().map(|s| flags(s.permissions())).collect::<Vec<_>>() == [5, 4, 6],
            "fixture RX/R/RW segments")?;
    require(image.segments()[2].memory_size() > image.segments()[2].file_bytes().len() as u64,
            "fixture must contain BSS")?;
    println!("HMELF IMAGE entry={} pages={} segments={}", image.entry(), image.page_count(), image.segments().len());
    for segment in image.segments() {
        // This checks the plan's proposed initialized/BSS content, not a kernel loader.
        let mut planned = vec![0u8; usize::try_from(segment.memory_size()).map_err(|_| "memory usize")?];
        planned[..segment.file_bytes().len()].copy_from_slice(segment.file_bytes());
        require(planned[..segment.file_bytes().len()] == *segment.file_bytes(), "planned file bytes")?;
        require(planned[segment.file_bytes().len()..].iter().all(|byte| *byte == 0), "planned zero fill")?;
        println!("HMELF LOAD va={} filesz={} memsz={} flags={} page_start={} page_end={} file_hash={}",
            segment.virtual_address(), segment.file_bytes().len(), segment.memory_size(),
            flags(segment.permissions()), segment.page_start(), segment.page_end(), fingerprint(segment.file_bytes()));
    }
    let table = usize::try_from(u64_at(bytes, 32)?).map_err(|_| "phoff usize")?;
    let count = usize::from(u16_at(bytes, 56)?);
    let mut required_end = table.checked_add(count.checked_mul(56).ok_or("PH count overflow")?).ok_or("PH range overflow")?;
    for index in 0..count {
        let header = table + index * 56;
        if u32_at(bytes, header)? == 1 {
            let end = u64_at(bytes, header + 8)?.checked_add(u64_at(bytes, header + 32)?).ok_or("required file overflow")?;
            required_end = required_end.max(usize::try_from(end).map_err(|_| "required usize")?);
        }
    }
    Ok(required_end)
}

fn mutate_reject(bytes: &[u8], offset: usize, replacement: &[u8], label: &str) -> Result<(), String> {
    let mut changed = bytes.to_vec();
    changed.get_mut(offset..offset + replacement.len()).ok_or("mutation range")?.copy_from_slice(replacement);
    require(Image::parse(&changed).is_err(), label)
}
fn mutations(bytes: &[u8]) -> Result<(), String> {
    let ph = usize::try_from(u64_at(bytes, 32)?).map_err(|_| "phoff usize")?;
    let va = u64_at(bytes, ph + 16)?;
    let memsz = u64_at(bytes, ph + 40)?;
    let file_offset = u64_at(bytes, ph + 8)?;
    let second_va = u64_at(bytes, ph + 56 + 16)?;
    let changes: Vec<(usize, Vec<u8>, &str)> = vec![
        (0, vec![0], "magic"), (4, vec![1], "class"), (5, vec![2], "encoding"),
        (6, vec![0], "identification version"), (7, vec![3], "OSABI"),
        (8, vec![1], "ABI version"), (9, vec![1], "reserved identification"),
        (16, 3u16.to_le_bytes().to_vec(), "type"),
        (18, 62u16.to_le_bytes().to_vec(), "machine"),
        (20, 2u32.to_le_bytes().to_vec(), "header version"),
        (48, 1u32.to_le_bytes().to_vec(), "architecture flags"),
        (52, 63u16.to_le_bytes().to_vec(), "header size"),
        (54, 55u16.to_le_bytes().to_vec(), "program header size"),
        (56, 0u16.to_le_bytes().to_vec(), "zero program headers"),
        (56, 17u16.to_le_bytes().to_vec(), "program header capacity"),
        (56, u16::MAX.to_le_bytes().to_vec(), "extended program count"),
        (32, 63u64.to_le_bytes().to_vec(), "program table in header"),
        (32, u64::MAX.to_le_bytes().to_vec(), "program table overflow"),
        (24, (va + 1).to_le_bytes().to_vec(), "unaligned entry"),
        (24, second_va.to_le_bytes().to_vec(), "nonexecutable entry"),
        (24, (va + memsz).to_le_bytes().to_vec(), "entry past executable bytes"),
        (ph + 4, 7u32.to_le_bytes().to_vec(), "writable executable"),
        (ph + 4, 0u32.to_le_bytes().to_vec(), "missing read permission"),
        (ph + 32, (memsz + 1).to_le_bytes().to_vec(), "file larger than memory"),
        (ph + 8, u64::MAX.to_le_bytes().to_vec(), "file interval overflow"),
        (ph + 40, 0u64.to_le_bytes().to_vec(), "empty LOAD"),
        (ph + 48, 3u64.to_le_bytes().to_vec(), "invalid alignment"),
        (ph + 8, (file_offset + 1).to_le_bytes().to_vec(), "page incongruence"),
        (ph + 16, END.to_le_bytes().to_vec(), "outside image window"),
        (ph + 56 + 16, va.to_le_bytes().to_vec(), "overlapping pages"),
        (ph, 3u32.to_le_bytes().to_vec(), "interpreter"),
        (ph, 2u32.to_le_bytes().to_vec(), "dynamic section"),
    ];
    require(changes.len() == MUTATIONS, "mutation inventory")?;
    for (offset, replacement, label) in changes { mutate_reject(bytes, offset, &replacement, label)?; }
    Ok(())
}
fn truncations(bytes: &[u8], required_end: usize) -> Result<(), String> {
    let full = Image::parse(bytes).map_err(|_| "full truncation reference")?;
    for length in 0..=bytes.len() {
        match Image::parse(&bytes[..length]) {
            Ok(image) => {
                require(length >= required_end, "accepted missing loaded byte")?;
                check_plan(&image, &bytes[..length])?;
                require(image.entry() == full.entry() && image.page_count() == full.page_count()
                    && image.segments().len() == full.segments().len(), "truncated metadata differs")?;
                for (a, b) in image.segments().iter().zip(full.segments()) {
                    require(a.virtual_address() == b.virtual_address() && a.memory_size() == b.memory_size()
                        && flags(a.permissions()) == flags(b.permissions()) && a.file_bytes() == b.file_bytes(),
                        "truncated segment differs")?;
                }
            }
            Err(_) => require(length < required_end, "rejected complete load input")?,
        }
    }
    Ok(())
}
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}
fn random_cases(bytes: &[u8]) -> Result<(), String> {
    let mut state = 0x4e41_5449_5645_454cu64;
    for index in 0..RANDOM_CASES {
        let mut sample = if index % 2 == 0 {
            vec![0; (next(&mut state) as usize) % 2049]
        } else { bytes.to_vec() };
        if index % 2 == 0 {
            for byte in &mut sample { *byte = next(&mut state) as u8; }
        } else {
            for _ in 0..1 + index % 8 {
                let offset = next(&mut state) as usize % sample.len();
                sample[offset] ^= (next(&mut state) as u8) | 1;
            }
            if index % 3 == 0 { sample.truncate(next(&mut state) as usize % sample.len()); }
        }
        if let Ok(image) = Image::parse(&sample) { check_plan(&image, &sample)?; }
    }
    Ok(())
}
fn run(path: &Path) -> Result<(), String> {
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    require(bytes.len() <= 1024 * 1024, "bounded fixture input")?;
    let required_end = fixture(&bytes)?;
    let mut largest = bytes.clone();
    largest.resize(1024 * 1024, 0);
    let maximal = Image::parse(&largest).map_err(|_| "maximum-sized input rejected")?;
    check_plan(&maximal, &largest)?;
    largest.push(0);
    require(Image::parse(&largest).is_err(), "oversized input accepted")?;
    mutations(&bytes)?;
    truncations(&bytes, required_end)?;
    random_cases(&bytes)?;
    println!("HMELF HOST PASS cases=5 mutations={MUTATIONS} truncations={} random={RANDOM_CASES}", bytes.len() + 1);
    Ok(())
}
fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 2 { eprintln!("expected one fixture ELF"); std::process::exit(2); }
    if let Err(error) = run(Path::new(&args[1])) {
        eprintln!("ELF host check failed: {error}");
        std::process::exit(1);
    }
}
