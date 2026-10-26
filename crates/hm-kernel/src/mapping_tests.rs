//! Exercise actual owned mappings through the safe platform boundary.
#![forbid(unsafe_code)]

use crate::Platform;
use crate::frame_api::{FrameError, OwnedFrame};
use crate::heap_api::OwnedBlock;
use crate::mapping_api::{MapError, OwnedMapping};

fn protections(platform: &impl Platform) -> bool {
    let c = platform.checks();
    c.write_protect
        && c.nx
        && c.wx
        && c.stack_guard
        && c.null_guard
        && c.user_access_protect
        && c.user_execute_protect
}

fn unsupported(platform: &impl Platform) -> bool {
    cfg!(target_arch = "x86_64")
        && platform.mapping_stats().is_none()
        && matches!(platform.allocate_frame(), Err(FrameError::Unsupported))
}

#[inline(never)]
pub fn contract(platform: &impl Platform) -> bool {
    let Some(maps) = platform.mapping_stats() else {
        return unsupported(platform);
    };
    let Some(frames) = platform.frame_stats() else {
        return false;
    };
    if !cfg!(target_arch = "aarch64") || maps.free < 2 || frames.free < 6 {
        return false;
    }
    let Ok(mut first) = platform.allocate_frame() else {
        return false;
    };
    let Ok(mut second) = platform.allocate_frame() else {
        return false;
    };
    let Ok(mut ordinary) = platform.allocate_frame() else {
        return false;
    };
    let Ok(mut block) = platform.allocate_heap(4097, 4096) else {
        return false;
    };
    first.bytes_mut().fill(0x17);
    second.bytes_mut().fill(0x29);
    ordinary.bytes_mut().fill(0x3b);
    block.bytes_mut().fill(0x4d);
    let first_pa = first.physical_address();
    let second_pa = second.physical_address();
    let Ok(mut first) = platform.map_frame(first) else {
        return false;
    };
    let Ok(second) = platform.map_frame(second) else {
        return false;
    };
    let slot = first.virtual_address();
    if first.physical_address() != first_pa
        || second.physical_address() != second_pa
        || first_pa == second_pa
        || first.virtual_address() == second.virtual_address()
        || first.virtual_address() == first_pa
        || !slot.is_multiple_of(4096)
        || first.bytes().iter().any(|b| *b != 0x17)
        || second.bytes().iter().any(|b| *b != 0x29)
        || platform.mapping_stats().map(|s| s.free) != maps.free.checked_sub(2)
        || platform.frame_stats().map(|s| s.free) != frames.free.checked_sub(5)
        || !protections(platform)
    {
        return false;
    }
    first.bytes_mut().fill(0x65);
    let old = first.unmap();
    if old.physical_address() != first_pa || old.bytes().iter().any(|b| *b != 0x65) {
        return false;
    }
    // Keep old backing allocated while repeatedly reusing its virtual slot
    // with a different page. A stale translation would corrupt old's pattern.
    for pattern in 0x70..0x80u8 {
        let Ok(mut fresh) = platform.allocate_frame() else {
            return false;
        };
        let fresh_pa = fresh.physical_address();
        fresh.bytes_mut().fill(pattern);
        let Ok(mut mapped) = platform.map_frame(fresh) else {
            return false;
        };
        if mapped.virtual_address() != slot
            || mapped.physical_address() != fresh_pa
            || fresh_pa == first_pa
            || mapped.bytes().iter().any(|b| *b != pattern)
        {
            return false;
        }
        mapped.bytes_mut().fill(pattern.wrapping_add(1));
        let fresh = mapped.unmap();
        if fresh.bytes().iter().any(|b| *b != pattern.wrapping_add(1))
            || old.bytes().iter().any(|b| *b != 0x65)
            || second.bytes().iter().any(|b| *b != 0x29)
            || ordinary.bytes().iter().any(|b| *b != 0x3b)
            || block.bytes().iter().any(|b| *b != 0x4d)
        {
            return false;
        }
        drop(fresh);
    }
    drop(second);
    let Ok(reused) = platform.allocate_frame() else {
        return false;
    };
    if reused.physical_address() != second_pa || reused.bytes().iter().any(|b| *b != 0) {
        return false;
    }
    drop(reused);
    drop(old);
    drop(ordinary);
    drop(block);
    platform.mapping_stats() == Some(maps)
        && platform.frame_stats() == Some(frames)
        && protections(platform)
}

#[inline(never)]
pub fn limits<P: Platform>(platform: &P) -> bool {
    let Some(maps) = platform.mapping_stats() else {
        return unsupported(platform);
    };
    let Some(frames) = platform.frame_stats() else {
        return false;
    };
    if !cfg!(target_arch = "aarch64")
        || maps.total != 64
        || maps.free > 64
        || frames.free <= maps.free
    {
        return false;
    }
    let mut held: [Option<P::Mapping<'_>>; 64] = core::array::from_fn(|_| None);
    for (index, entry) in held.iter_mut().take(maps.free).enumerate() {
        let Ok(mut frame) = platform.allocate_frame() else {
            return false;
        };
        frame.bytes_mut().fill(index as u8);
        let Ok(mapping) = platform.map_frame(frame) else {
            return false;
        };
        *entry = Some(mapping);
    }
    let Ok(mut extra) = platform.allocate_frame() else {
        return false;
    };
    extra.bytes_mut().fill(0xee);
    let address = extra.physical_address();
    let frame_count = platform.frame_stats();
    let Err((error, extra)) = platform.map_frame(extra) else {
        return false;
    };
    if error != MapError::Exhausted
        || extra.physical_address() != address
        || extra.bytes().iter().any(|b| *b != 0xee)
        || platform.frame_stats() != frame_count
        || platform.mapping_stats().map(|s| s.free) != Some(0)
        || !protections(platform)
    {
        return false;
    }
    for (index, mapping) in held.iter().take(maps.free).enumerate() {
        let Some(mapping) = mapping else {
            return false;
        };
        if mapping.bytes().iter().any(|b| *b != index as u8) {
            return false;
        }
    }
    drop(extra);
    drop(held);
    platform.mapping_stats() == Some(maps)
        && platform.frame_stats() == Some(frames)
        && protections(platform)
}

#[inline(never)]
pub fn hardware(platform: &impl Platform) -> bool {
    let Some(maps) = platform.mapping_stats() else {
        return unsupported(platform) && !platform.mapping_hardware_check();
    };
    let Some(frames) = platform.frame_stats() else {
        return false;
    };
    if !cfg!(target_arch = "aarch64") || !platform.mapping_hardware_check() {
        return false;
    }
    let (Some(after_maps), Some(after_frames)) = (platform.mapping_stats(), platform.frame_stats())
    else {
        return false;
    };
    // Only the first call retains one deliberately forgotten mapping/frame.
    let unchanged = after_maps == maps && after_frames == frames;
    let one_leak = after_maps.total == maps.total
        && after_frames.total == frames.total
        && after_maps.free.checked_add(1) == Some(maps.free)
        && after_frames.free.checked_add(1) == Some(frames.free);
    (unchanged || one_leak)
        && platform.mapping_hardware_check()
        && platform.mapping_stats() == Some(after_maps)
        && platform.frame_stats() == Some(after_frames)
        && protections(platform)
}
