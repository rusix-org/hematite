//! Owned mutual exclusion; design 0011 specifies ordering and guard lifetimes.
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};
use hm_kernel::mutex_api::OwnedMutex;
use hm_kernel::thread_api::ThreadContext;

pub struct Mutex<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: Only a successful Acquire exchange grants a guard, and its Release
// unlock publishes all value accesses. Guards cannot be sent/shared; shared
// owners therefore access T sequentially. T: Send permits that transfer.
unsafe impl<T: Send> Sync for Mutex<T> {}

impl<T> Mutex<T> {
    pub const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    pub fn try_lock(&self) -> Option<Guard<'_, T>> {
        self.locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| Guard {
                mutex: self,
                _not_send_sync: PhantomData,
            })
    }

    /// Yield on contention. Reentrant acquisition or forgetting a guard can
    /// wait forever; fairness is not promised. Never block from an IRQ handler.
    pub fn lock_cooperative<'a>(&'a self, context: &mut dyn ThreadContext) -> Guard<'a, T> {
        loop {
            if let Some(guard) = self.try_lock() {
                return guard;
            }
            context.yield_now();
        }
    }

    pub fn get_mut(&mut self) -> &mut T {
        self.value.get_mut()
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }
}

#[must_use = "dropping the guard releases the mutex"]
pub struct Guard<'a, T> {
    mutex: &'a Mutex<T>,
    _not_send_sync: PhantomData<*mut ()>,
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        // SAFETY: This guard uniquely owns the acquired lock. Its shared borrow
        // prevents mutation/unlock while this reference remains usable.
        unsafe { &*self.mutex.value.get() }
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        // SAFETY: Lock ownership excludes other guards, and the exclusive guard
        // borrow excludes other value references until this reference ends.
        unsafe { &mut *self.mutex.value.get() }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.mutex.locked.store(false, Ordering::Release);
    }
}

impl<T> OwnedMutex<T> for Mutex<T> {
    type Guard<'a>
        = Guard<'a, T>
    where
        Self: 'a,
        T: 'a;
    fn try_lock(&self) -> Option<Self::Guard<'_>> {
        Mutex::try_lock(self)
    }
    fn lock_cooperative<'a>(&'a self, context: &mut dyn ThreadContext) -> Self::Guard<'a> {
        Mutex::lock_cooperative(self, context)
    }
    fn get_mut(&mut self) -> &mut T {
        Mutex::get_mut(self)
    }
    fn into_inner(self) -> T {
        Mutex::into_inner(self)
    }
}
