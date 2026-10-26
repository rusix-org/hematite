#![forbid(unsafe_code)]
use hm_kernel::process_budget::{Budget, Error, Finish, Snapshot};
// Only the counter-capacity case needs private-state access, through a
// self-check compiled from the actual source. All public cases use the rlib.
#[allow(dead_code)]
#[path = "../src/process_budget.rs"]
mod actual_source;

fn poisoned(budget: &mut Budget, before: Snapshot) {
    assert_eq!(budget.begin(0), Err(Error::State));
    assert_eq!(budget.finish(u64::MAX), Err(Error::State));
    assert_eq!(budget.stop(), Err(Error::State));
    assert_eq!(budget.snapshot(), before);
}
#[test]
fn configuration_bounds() {
    for (total, quantum) in [(0, 0), (0, 1), (1, 0), (1, 2), (u64::MAX, 0)] {
        assert_eq!(
            Budget::new(total, quantum).unwrap_err(),
            Error::Configuration
        );
    }
    for (total, quantum) in [(1, 1), (u64::MAX, 1), (u64::MAX, u64::MAX)] {
        let budget = Budget::new(total, quantum).unwrap();
        assert_eq!(
            budget.snapshot(),
            Snapshot {
                remaining: total,
                charged: 0,
                turns: 0
            }
        );
    }
}
#[test]
fn zero_and_one_elapsed() {
    let mut b = Budget::new(3, 1).unwrap();
    assert_eq!(b.begin(100), Ok(101));
    assert_eq!(b.finish(100), Ok(Finish::Ready));
    assert_eq!(
        b.snapshot(),
        Snapshot {
            remaining: 3,
            charged: 0,
            turns: 1
        }
    );
    assert_eq!(b.begin(100), Ok(101));
    assert_eq!(b.finish(101), Ok(Finish::Ready));
    assert_eq!(
        b.snapshot(),
        Snapshot {
            remaining: 2,
            charged: 1,
            turns: 2
        }
    );
}
#[test]
fn quantum_clamping() {
    let mut b = Budget::new(10, 4).unwrap();
    assert_eq!(b.begin(0), Ok(4));
    assert_eq!(b.finish(7), Ok(Finish::Ready));
    assert_eq!(b.begin(50), Ok(53));
    assert_eq!(b.finish(51), Ok(Finish::Ready));
    assert_eq!(b.begin(75), Ok(77));
    assert_eq!(b.finish(77), Ok(Finish::Exhausted));
    assert_eq!(
        b.snapshot(),
        Snapshot {
            remaining: 0,
            charged: 10,
            turns: 3
        }
    );
}
#[test]
fn exact_exhaustion() {
    for total in [1, 10, u64::MAX] {
        let mut b = Budget::new(total, total).unwrap();
        assert_eq!(b.begin(0), Ok(total));
        assert_eq!(b.finish(total), Ok(Finish::Exhausted));
        assert_eq!(
            b.snapshot(),
            Snapshot {
                remaining: 0,
                charged: total,
                turns: 1
            }
        );
    }
}
#[test]
fn overrun_saturation() {
    for end in [8, 1000, u64::MAX] {
        let mut b = Budget::new(7, 2).unwrap();
        assert_eq!(b.begin(0), Ok(2));
        assert_eq!(b.finish(end), Ok(Finish::Exhausted));
        assert_eq!(
            b.snapshot(),
            Snapshot {
                remaining: 0,
                charged: 7,
                turns: 1
            }
        );
    }
}
#[test]
fn repeated_turns() {
    let mut b = Budget::new(100, 3).unwrap();
    for turn in 0..40 {
        let now = turn * 2;
        assert_eq!(b.begin(now), Ok(now + 3));
        assert_eq!(b.finish(now + 1), Ok(Finish::Ready));
        let snapshot = b.snapshot();
        assert_eq!(
            snapshot,
            Snapshot {
                remaining: 99 - turn,
                charged: turn + 1,
                turns: turn + 1
            }
        );
        assert_eq!(snapshot.remaining.checked_add(snapshot.charged), Some(100));
        let mut detached = snapshot;
        detached.remaining = u64::MAX;
        detached.charged = u64::MAX;
        detached.turns = u64::MAX;
        assert_ne!(detached, b.snapshot());
        assert_eq!(b.snapshot(), snapshot);
    }
}
#[test]
fn parked_time_uncharged() {
    let mut b = Budget::new(100, 10).unwrap();
    assert_eq!(b.begin(0), Ok(10));
    assert_eq!(b.finish(5), Ok(Finish::Ready));
    let before = b.snapshot();
    let later = u64::MAX - 100;
    assert_eq!(b.begin(later), Ok(later + 10));
    assert_eq!(b.snapshot(), before);
    assert_eq!(b.finish(later + 2), Ok(Finish::Ready));
    assert_eq!(
        b.snapshot(),
        Snapshot {
            remaining: 93,
            charged: 7,
            turns: 2
        }
    );
}
#[test]
fn equal_timestamps() {
    let mut b = Budget::new(1, 1).unwrap();
    for turn in 1..=3 {
        assert_eq!(b.begin(50), Ok(51));
        assert_eq!(b.finish(50), Ok(Finish::Ready));
        assert_eq!(
            b.snapshot(),
            Snapshot {
                remaining: 1,
                charged: 0,
                turns: turn
            }
        );
    }
}
#[test]
fn backward_begin() {
    let mut b = Budget::new(100, 10).unwrap();
    b.begin(100).unwrap();
    b.finish(110).unwrap();
    let before = b.snapshot();
    assert_eq!(b.begin(109), Err(Error::Clock));
    assert_eq!(b.snapshot(), before);
    poisoned(&mut b, before);
}
#[test]
fn backward_finish() {
    let mut b = Budget::new(100, 10).unwrap();
    b.begin(10).unwrap();
    b.finish(12).unwrap();
    b.begin(200).unwrap();
    let before = b.snapshot();
    assert_eq!(b.finish(199), Err(Error::Clock));
    assert_eq!(b.snapshot(), before);
    poisoned(&mut b, before);
}
#[test]
fn deadline_overflow() {
    for (total, quantum, now) in [(10, 1, u64::MAX - 9), (u64::MAX, 1, 1), (1, 1, u64::MAX)] {
        let mut b = Budget::new(total, quantum).unwrap();
        let before = b.snapshot();
        assert_eq!(b.begin(now), Err(Error::Clock));
        assert_eq!(b.snapshot(), before);
        poisoned(&mut b, before);
    }
    let mut b = Budget::new(10, 1).unwrap();
    assert_eq!(b.begin(u64::MAX - 10), Ok(u64::MAX - 9));
    assert_eq!(b.finish(u64::MAX), Ok(Finish::Exhausted));
}
#[test]
fn invalid_transitions() {
    let mut ready = Budget::new(10, 1).unwrap();
    let before = ready.snapshot();
    assert_eq!(ready.finish(0), Err(Error::State));
    poisoned(&mut ready, before);
    for operation in 0..2 {
        let mut running = Budget::new(10, 1).unwrap();
        running.begin(0).unwrap();
        let before = running.snapshot();
        if operation == 0 {
            assert_eq!(running.begin(0), Err(Error::State));
        } else {
            assert_eq!(running.stop(), Err(Error::State));
        }
        poisoned(&mut running, before);
    }
    let mut completed = Budget::new(10, 1).unwrap();
    completed.begin(0).unwrap();
    completed.finish(1).unwrap();
    let before = completed.snapshot();
    assert_eq!(completed.finish(1), Err(Error::State));
    poisoned(&mut completed, before);
}
#[test]
fn terminal_no_revival() {
    for terminal in 0..3 {
        for operation in 0..3 {
            // Exhausted accepts stop; that success is separately covered.
            if terminal == 0 && operation == 2 {
                continue;
            }
            let mut b = Budget::new(1, 1).unwrap();
            match terminal {
                0 => {
                    b.begin(0).unwrap();
                    b.finish(1).unwrap();
                }
                1 => b.stop().unwrap(),
                _ => {
                    assert_eq!(b.finish(0), Err(Error::State));
                }
            }
            let before = b.snapshot();
            match operation {
                0 => assert_eq!(b.begin(1), Err(Error::State)),
                1 => assert_eq!(b.finish(1), Err(Error::State)),
                _ => assert_eq!(b.stop(), Err(Error::State)),
            }
            poisoned(&mut b, before);
        }
    }
}
#[test]
fn stop_ready_and_exhausted() {
    for exhausted in [false, true] {
        let mut b = Budget::new(10, 1).unwrap();
        b.begin(0).unwrap();
        b.finish(if exhausted { 10 } else { 1 }).unwrap();
        let before = b.snapshot();
        assert_eq!(b.stop(), Ok(()));
        assert_eq!(b.snapshot(), before);
        poisoned(&mut b, before);
    }
}
#[test]
fn turn_counter_overflow() {
    assert!(actual_source::check_turn_counter_overflow());
}
