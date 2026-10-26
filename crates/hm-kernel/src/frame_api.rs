//! Safe physical-frame policy boundary. Implementations own the memory proof.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    Unsupported,
    Exhausted,
    AlreadyInitialized,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameStats {
    pub total: usize,
    pub free: usize,
}

pub trait OwnedFrame {
    fn physical_address(&self) -> u64;
    fn bytes(&self) -> &[u8; 4096];
    fn bytes_mut(&mut self) -> &mut [u8; 4096];
}

/// No frame can exist on a platform without an allocator.
pub enum UnsupportedFrame {}
impl OwnedFrame for UnsupportedFrame {
    fn physical_address(&self) -> u64 {
        match *self {}
    }
    fn bytes(&self) -> &[u8; 4096] {
        match *self {}
    }
    fn bytes_mut(&mut self) -> &mut [u8; 4096] {
        match *self {}
    }
}

/// Actual byte access and ownership on ARM; explicit Unsupported on x86.
#[inline(never)]
pub fn contract(platform: &impl crate::Platform) -> bool {
    let Some(before) = platform.frame_stats() else {
        return cfg!(target_arch = "x86_64")
            && matches!(platform.allocate_frame(), Err(FrameError::Unsupported));
    };
    if !cfg!(target_arch = "aarch64") || before.free < 3 {
        return false;
    }
    let Ok(mut first) = platform.allocate_frame() else {
        return false;
    };
    let Ok(mut second) = platform.allocate_frame() else {
        return false;
    };
    let address = first.physical_address();
    if address == second.physical_address()
        || !address.is_multiple_of(4096)
        || !second.physical_address().is_multiple_of(4096)
        || first.bytes().iter().any(|b| *b != 0)
        || second.bytes().iter().any(|b| *b != 0)
    {
        return false;
    }
    first.bytes_mut().fill(0xa5);
    second.bytes_mut().fill(0x5a);
    if first.bytes().iter().any(|b| *b != 0xa5) {
        return false;
    }
    drop(first);
    let Ok(first) = platform.allocate_frame() else {
        return false;
    };
    if first.physical_address() != address
        || first.bytes().iter().any(|b| *b != 0)
        || second.bytes().iter().any(|b| *b != 0x5a)
    {
        return false;
    }
    drop(first);
    if !platform.frame_exhaustion_check() || second.bytes().iter().any(|b| *b != 0x5a) {
        return false;
    }
    drop(second);
    let Some(after) = platform.frame_stats() else {
        return false;
    };
    // The first diagnostic intentionally leaks one page; repeat diagnostics
    // must preserve all remaining capacity and the same pool identity.
    after.total == before.total
        && (after.free == before.free || after.free.checked_add(1) == Some(before.free))
        && platform.frame_exhaustion_check()
        && platform.frame_stats() == Some(after)
}
