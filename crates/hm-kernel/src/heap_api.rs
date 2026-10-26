//! Fallible owned heap boundary; physical ownership remains in the core.
#![forbid(unsafe_code)]

use core::convert::Infallible;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeapError {
    InvalidLayout,
    Exhausted,
    Unsupported,
}

pub trait OwnedBlock {
    fn physical_address(&self) -> u64;
    fn bytes(&self) -> &[u8];
    fn bytes_mut(&mut self) -> &mut [u8];
}

/// Platforms without a heap cannot construct this owner.
pub enum UnsupportedBlock {}
impl OwnedBlock for UnsupportedBlock {
    fn physical_address(&self) -> u64 {
        match *self {}
    }
    fn bytes(&self) -> &[u8] {
        match *self {}
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        match *self {}
    }
}

/// The uninhabited field prevents successful typed allocation on unsupported
/// platforms while permitting the same generic safe policy interface.
pub struct UnsupportedBox<T> {
    never: Infallible,
    _value: PhantomData<T>,
}
impl<T> Deref for UnsupportedBox<T> {
    type Target = T;
    fn deref(&self) -> &T {
        match self.never {}
    }
}
impl<T> DerefMut for UnsupportedBox<T> {
    fn deref_mut(&mut self) -> &mut T {
        match self.never {}
    }
}
