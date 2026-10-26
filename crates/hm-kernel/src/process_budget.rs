//! Persistent active-turn accounting for design 0028. This policy has no hardware
//! authority: its caller must establish quiescence before stopping an owner.
#![forbid(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    Configuration,
    State,
    Clock,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Finish {
    Ready,
    Exhausted,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Snapshot {
    pub remaining: u64,
    pub charged: u64,
    pub turns: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Ready,
    Running(u64),
    Exhausted,
    Stopped,
    Poisoned,
}
#[derive(Debug)]
pub struct Budget {
    remaining: u64,
    charged: u64,
    turns: u64,
    quantum: u64,
    last_finish: Option<u64>,
    state: State,
}
impl Budget {
    pub fn new(total: u64, quantum: u64) -> Result<Self, Error> {
        if total == 0 || quantum == 0 || quantum > total {
            return Err(Error::Configuration);
        }
        Ok(Self {
            remaining: total,
            charged: 0,
            turns: 0,
            quantum,
            last_finish: None,
            state: State::Ready,
        })
    }
    /// Admit a turn and return its absolute quantum comparator. Both that
    /// comparator and the whole remaining budget must fit the counter domain.
    pub fn begin(&mut self, now: u64) -> Result<u64, Error> {
        if self.state != State::Ready || self.turns == u64::MAX {
            return self.reject(Error::State);
        }
        if self.last_finish.is_some_and(|last| now < last)
            || now.checked_add(self.remaining).is_none()
        {
            return self.reject(Error::Clock);
        }
        let Some(deadline) = now.checked_add(self.remaining.min(self.quantum)) else {
            return self.reject(Error::Clock);
        };
        self.state = State::Running(now);
        Ok(deadline)
    }
    /// Charge elapsed active-turn ticks, excluding any parked interval. A
    /// bounded overrun saturates the charge at the admitted total budget.
    pub fn finish(&mut self, now: u64) -> Result<Finish, Error> {
        let State::Running(start) = self.state else {
            return self.reject(Error::State);
        };
        let Some(elapsed) = now.checked_sub(start) else {
            return self.reject(Error::Clock);
        };
        let Some(turns) = self.turns.checked_add(1) else {
            return self.reject(Error::State);
        };
        let charge = elapsed.min(self.remaining);
        let Some(charged) = self.charged.checked_add(charge) else {
            return self.reject(Error::State);
        };
        let Some(remaining) = self.remaining.checked_sub(charge) else {
            return self.reject(Error::State);
        };
        // Commit only after every fallible calculation: rejected operations
        // poison admission while retaining the complete diagnostic snapshot.
        self.remaining = remaining;
        self.charged = charged;
        self.turns = turns;
        self.last_finish = Some(now);
        self.state = if remaining == 0 {
            State::Exhausted
        } else {
            State::Ready
        };
        Ok(if remaining == 0 {
            Finish::Exhausted
        } else {
            Finish::Ready
        })
    }
    pub fn stop(&mut self) -> Result<(), Error> {
        if !matches!(self.state, State::Ready | State::Exhausted) {
            return self.reject(Error::State);
        }
        self.state = State::Stopped;
        Ok(())
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            remaining: self.remaining,
            charged: self.charged,
            turns: self.turns,
        }
    }
    fn reject<T>(&mut self, error: Error) -> Result<T, Error> {
        self.state = State::Poisoned;
        Err(error)
    }
}

// Dedicated integration tests import this exact source. The production rlib
// contains no seam, and the parent-only function exposes no state constructor.
#[cfg(test)]
#[allow(dead_code)]
pub(super) fn check_turn_counter_overflow() -> bool {
    let Ok(mut budget) = Budget::new(10, 1) else {
        return false;
    };
    budget.turns = u64::MAX - 1;
    if budget.begin(0) != Ok(1) || budget.finish(0) != Ok(Finish::Ready) {
        return false;
    }
    let before = budget.snapshot();
    if before.turns != u64::MAX
        || budget.begin(0) != Err(Error::State)
        || budget.snapshot() != before
        || budget.finish(0) != Err(Error::State)
        || budget.stop() != Err(Error::State)
        || budget.snapshot() != before
    {
        return false;
    }
    // Defensive finish guard, unreachable through the public API because
    // begin already rejects full capacity. No partial charge may be committed.
    let Ok(mut budget) = Budget::new(10, 1) else {
        return false;
    };
    budget.turns = u64::MAX;
    budget.state = State::Running(0);
    let before = budget.snapshot();
    budget.finish(1) == Err(Error::State)
        && budget.snapshot() == before
        && budget.begin(1) == Err(Error::State)
        && budget.stop() == Err(Error::State)
        && budget.snapshot() == before
}
