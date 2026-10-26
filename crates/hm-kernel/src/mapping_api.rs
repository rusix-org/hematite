//! Owned kernel data mappings; implementations retain backing through invalidation.
#![forbid(unsafe_code)]

use crate::frame_api::{OwnedFrame, UnsupportedFrame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MapError {
    Unsupported,
    Exhausted,
    CorruptState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MapStats {
    pub total: usize,
    pub free: usize,
}

/// Access is tied to the owner. Consuming unmap returns its original backing.
pub trait OwnedMapping {
    type Frame: OwnedFrame;
    fn virtual_address(&self) -> u64;
    fn physical_address(&self) -> u64;
    fn bytes(&self) -> &[u8; 4096];
    fn bytes_mut(&mut self) -> &mut [u8; 4096];
    fn unmap(self) -> Self::Frame;
}

/// No mapping exists when the architecture has no mapping implementation.
pub enum UnsupportedMapping {}
impl OwnedMapping for UnsupportedMapping {
    type Frame = UnsupportedFrame;
    fn virtual_address(&self) -> u64 {
        match *self {}
    }
    fn physical_address(&self) -> u64 {
        match *self {}
    }
    fn bytes(&self) -> &[u8; 4096] {
        match *self {}
    }
    fn bytes_mut(&mut self) -> &mut [u8; 4096] {
        match *self {}
    }
    fn unmap(self) -> Self::Frame {
        match self {}
    }
}
