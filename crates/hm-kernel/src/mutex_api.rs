//! Owned mutual exclusion; implementations provide the synchronization proof.
#![forbid(unsafe_code)]

use crate::thread_api::ThreadContext;
use core::ops::DerefMut;

pub trait OwnedMutex<T> {
    type Guard<'a>: DerefMut<Target = T>
    where
        Self: 'a,
        T: 'a;

    fn try_lock(&self) -> Option<Self::Guard<'_>>;
    fn lock_cooperative<'a>(&'a self, context: &mut dyn ThreadContext) -> Self::Guard<'a>;
    fn get_mut(&mut self) -> &mut T;
    fn into_inner(self) -> T;
}
