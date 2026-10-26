//! Safe callbacks exercising actual stack switching through Platform.
#![forbid(unsafe_code)]

use crate::Platform;
use crate::frame_api::OwnedFrame;
use crate::mapping_api::OwnedMapping;
use crate::thread_api::{
    MAX_THREADS, RunReport, TRACE_CAPACITY, ThreadContext, ThreadError, ThreadJob,
};
use core::cell::Cell;
use core::sync::atomic::{AtomicUsize, Ordering};

// Used only to observe that rejected oversized runs invoke no callback.
static REJECTED_INVOCATIONS: AtomicUsize = AtomicUsize::new(0);

struct DropWitness<'a>(&'a Cell<u64>);

impl Drop for DropWitness<'_> {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_add(1));
    }
}

fn result(input: u64, id: usize) -> u64 {
    input.rotate_left(7) ^ ((id as u64) << 32) ^ (1u64 << 48)
}

#[inline(never)]
fn worker(context: &mut dyn ThreadContext, input: u64) -> u64 {
    let id = context.id();
    if id >= MAX_THREADS {
        return u64::MAX;
    }
    let rounds = input as u8;
    let pattern = ((input >> 8) as u8).wrapping_add(id as u8);
    let mut local = [pattern; 256];
    let dropped = Cell::new(0u64);
    {
        let _witness = DropWitness(&dropped);
        for round in 0..rounds {
            let offset = usize::from(round);
            local[offset] = pattern.wrapping_add(1);
            core::hint::black_box(&mut local);
            context.yield_now();
            if context.id() != id || dropped.get() != 0 {
                return u64::MAX;
            }
            for (index, byte) in local.iter().enumerate() {
                let expected = if index <= offset {
                    pattern.wrapping_add(1)
                } else {
                    pattern
                };
                if *byte != expected {
                    return u64::MAX;
                }
            }
        }
    }
    if dropped.get() != 1 {
        return u64::MAX;
    }
    result(input, id)
}

fn forbidden_callback(_context: &mut dyn ThreadContext, input: u64) -> u64 {
    REJECTED_INVOCATIONS.fetch_add(1, Ordering::SeqCst);
    input
}

fn unsupported(platform: &impl Platform) -> bool {
    cfg!(target_arch = "x86_64")
        && matches!(platform.run_threads(&[]), Err(ThreadError::Unsupported))
}

fn protections(platform: &impl Platform) -> bool {
    let checks = platform.checks();
    checks.write_protect
        && checks.nx
        && checks.wx
        && checks.stack_guard
        && checks.null_guard
        && checks.user_execute_protect
        && checks.user_access_protect
}

#[inline(never)]
pub fn contract(platform: &impl Platform) -> bool {
    if cfg!(target_arch = "x86_64") {
        return unsupported(platform);
    }
    let before_frames = platform.frame_stats();
    let before_mappings = platform.mapping_stats();
    let Ok(mut frame) = platform.allocate_frame() else {
        return false;
    };
    frame.bytes_mut().fill(0x9d);
    let Ok(mut mapped) = platform.map_frame(frame) else {
        return false;
    };
    let frames = platform.frame_stats();
    let mappings = platform.mapping_stats();
    let bytes = mapped.bytes_mut();
    let inputs = [0x1202, 0x3400, 0x5601, 0x7803];
    let jobs = inputs.map(|input| ThreadJob {
        entry: worker,
        input,
    });
    let expected_trace = [0, 1, 2, 3, 0, 2, 3, 0, 3, 3];
    for _ in 0..3 {
        let Ok(report) = platform.run_threads(&jobs) else {
            return false;
        };
        if report.completed != 4
            || report.yields != [2, 0, 1, 3]
            || report.dispatches != 10
            || report.trace_len != expected_trace.len()
            || report.trace[..expected_trace.len()] != expected_trace
            || report.truncated
            || report.trace[expected_trace.len()..]
                .iter()
                .any(|id| *id != 0)
            || report.results != core::array::from_fn(|id| result(inputs[id], id))
            || bytes.iter().any(|byte| *byte != 0x9d)
            || platform.frame_stats() != frames
            || platform.mapping_stats() != mappings
            || !protections(platform)
        {
            return false;
        }
    }
    drop(mapped);
    platform.frame_stats() == before_frames && platform.mapping_stats() == before_mappings
}

#[inline(never)]
pub fn limits(platform: &impl Platform) -> bool {
    if cfg!(target_arch = "x86_64") {
        return unsupported(platform);
    }
    let frames = platform.frame_stats();
    let mappings = platform.mapping_stats();
    if platform.run_threads(&[]) != Ok(RunReport::empty()) {
        return false;
    }
    REJECTED_INVOCATIONS.store(0, Ordering::SeqCst);
    let oversized = [ThreadJob {
        entry: forbidden_callback,
        input: 19,
    }; MAX_THREADS + 1];
    if !matches!(platform.run_threads(&oversized), Err(ThreadError::TooMany))
        || REJECTED_INVOCATIONS.load(Ordering::SeqCst) != 0
        || platform.frame_stats() != frames
        || platform.mapping_stats() != mappings
    {
        return false;
    }
    // Exactly full trace must not be labeled truncated; one additional
    // dispatch must be recorded in counters even though it cannot fit.
    for rounds in [63u64, 64] {
        let input = 0x9a00 | rounds;
        let Ok(report) = platform.run_threads(&[ThreadJob {
            entry: worker,
            input,
        }]) else {
            return false;
        };
        if report.completed != 1
            || report.results != [result(input, 0), 0, 0, 0]
            || report.yields != [rounds, 0, 0, 0]
            || report.dispatches != rounds + 1
            || report.trace_len != TRACE_CAPACITY
            || report.trace.iter().any(|id| *id != 0)
            || report.truncated != (rounds == 64)
        {
            return false;
        }
    }
    let jobs = core::array::from_fn::<_, MAX_THREADS, _>(|id| ThreadJob {
        entry: worker,
        input: 0xb000 | ((20 + id) as u64),
    });
    let Ok(report) = platform.run_threads(&jobs) else {
        return false;
    };
    // All four workers are still runnable through the first 64 activations.
    // They must nevertheless continue past the full trace until all complete.
    if report.completed != MAX_THREADS
        || report.results != core::array::from_fn(|id| result(jobs[id].input, id))
        || report.yields != [20, 21, 22, 23]
        || report.dispatches != 90
        || report.trace_len != TRACE_CAPACITY
        || !report.truncated
        || report
            .trace
            .iter()
            .enumerate()
            .any(|(index, id)| *id != index % MAX_THREADS)
    {
        return false;
    }
    // A fresh leaf callback after trace exhaustion proves completion releases
    // the execution lease and resets all per-run accounting.
    let input = 0xcc00;
    let Ok(report) = platform.run_threads(&[ThreadJob {
        entry: worker,
        input,
    }]) else {
        return false;
    };
    report.completed == 1
        && report.results == [result(input, 0), 0, 0, 0]
        && report.yields == [0; MAX_THREADS]
        && report.dispatches == 1
        && report.trace_len == 1
        && report.trace == [0; TRACE_CAPACITY]
        && !report.truncated
        && platform.run_threads(&[]) == Ok(RunReport::empty())
        && platform.frame_stats() == frames
        && platform.mapping_stats() == mappings
        && protections(platform)
}
