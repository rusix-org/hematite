//! Hosted parallel stress of the actual dependency-free kernel mutex module.
#![forbid(unsafe_code)]

use hm_mutex::Mutex;
use std::cell::Cell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

const WORKERS: usize = 4;
const ITERATIONS: usize = 20_000;

fn contribution(id: usize, iteration: usize) -> u64 {
    let token = ((id as u64) << 32) | iteration as u64;
    token.wrapping_mul(0x9e37_79b9_7f4a_7c15).rotate_left(((id + iteration) % 64) as u32)
}

// Cell makes this value !Sync; moving it between exclusive owners is allowed.
// Sharing it is sound only through the real mutex's exclusive guard.
struct State {
    count: Cell<u64>,
    checksum: Cell<u64>,
    entered: Cell<bool>,
    drops: Arc<AtomicUsize>,
}

impl Drop for State {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn main() {
    let drops = Arc::new(AtomicUsize::new(0));
    let shared = Arc::new(Mutex::new(State {
        count: Cell::new(0),
        checksum: Cell::new(0),
        entered: Cell::new(false),
        drops: Arc::clone(&drops),
    }));
    let barrier = Arc::new(Barrier::new(WORKERS + 1));
    let rejected = Arc::new(AtomicUsize::new(0));
    let held = shared.try_lock().expect("initial exclusive acquisition");
    let mut workers = Vec::new();
    for id in 0..WORKERS {
        let shared = Arc::clone(&shared);
        let barrier = Arc::clone(&barrier);
        let rejected = Arc::clone(&rejected);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            assert!(shared.try_lock().is_none(), "owner must exclude every worker");
            rejected.fetch_add(1, Ordering::Release);
            let mut retries = 0usize;
            for iteration in 0..ITERATIONS {
                let guard = loop {
                    if let Some(guard) = shared.try_lock() {
                        break guard;
                    }
                    retries = retries.saturating_add(1);
                    std::thread::yield_now();
                };
                assert!(!guard.entered.replace(true), "overlapping critical sections");
                let before = guard.count.get();
                if iteration % 127 == 0 {
                    std::thread::yield_now();
                }
                guard.count.set(before.checked_add(1).expect("bounded count"));
                guard.checksum.set(guard.checksum.get().wrapping_add(contribution(id, iteration)));
                assert!(guard.entered.replace(false));
                drop(guard);
            }
            retries
        }));
    }
    barrier.wait();
    let deadline = Instant::now() + Duration::from_secs(5);
    while rejected.load(Ordering::Acquire) != WORKERS {
        assert!(Instant::now() < deadline, "workers failed to observe held lock");
        std::thread::yield_now();
    }
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(held);
    let retries = workers.into_iter().map(|w| w.join().expect("worker completed"))
        .fold(0usize, usize::saturating_add);
    let mutex = match Arc::try_unwrap(shared) {
        Ok(mutex) => mutex,
        Err(_) => panic!("worker retained mutex ownership"),
    };
    let value = mutex.into_inner();
    let expected = (0..WORKERS).flat_map(|id| (0..ITERATIONS).map(move |iteration| {
        contribution(id, iteration)
    })).fold(0u64, u64::wrapping_add);
    assert_eq!(value.count.get(), (WORKERS * ITERATIONS) as u64);
    assert_eq!(value.checksum.get(), expected);
    assert!(!value.entered.get());
    assert_eq!(drops.load(Ordering::SeqCst), 0);
    drop(value);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    println!("HMMUTEX HOST PASS workers={WORKERS} iterations={ITERATIONS} updates={} rejected={} retries={retries} drops=1",
             WORKERS * ITERATIONS, rejected.load(Ordering::Acquire));
}
