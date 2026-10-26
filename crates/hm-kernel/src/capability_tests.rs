//! In-guest checks for the bounded capability experiment, not a userspace ABI.
#![forbid(unsafe_code)]

use crate::capability::{Access, Completion, Config, Error, Owner, Rights, with_model};

const ALICE: Owner = Owner(1);
const BOB: Owner = Owner(2);

fn config() -> Config {
    Config {
        byte_budget: 4096,
        max_in_flight: 64,
        generation_limit: u32::MAX,
    }
}

// Rejections preserve accounting/retirement counters and structural invariants.
// These observations do not prove equality of all private model state.
macro_rules! rejected {
    ($model:ident, $request:expr, $error:pat) => {{
        let before = $model.stats();
        matches!($request, Err($error)) && $model.stats() == before && $model.audit()
    }};
    ($model:ident, $request:expr) => {{
        let before = $model.stats();
        $request.is_err() && $model.stats() == before && $model.audit()
    }};
}

pub fn authority() -> bool {
    matches!(
        with_model(config(), |mut m| {
            let Ok(root) = m.create(ALICE, 16, Rights::ALL) else {
                return false;
            };
            if !rejected!(m, m.begin(BOB, root, Access::Read), Error::WrongOwner)
                || !rejected!(m, m.release(BOB, root))
                || !rejected!(m, m.revoke(BOB, root))
                || !rejected!(m, m.grant(BOB, root, BOB, Rights::READ))
            {
                return false;
            }
            let Ok(read) = m.grant(ALICE, root, BOB, Rights::READ) else {
                return false;
            };
            if !rejected!(m, m.begin(BOB, read, Access::Write), Error::Denied)
                || !rejected!(m, m.grant(BOB, read, ALICE, Rights::READ))
                || !rejected!(m, m.revoke(BOB, read))
            {
                return false;
            }
            let Ok(delegator) = m.grant(ALICE, root, BOB, Rights::READ.union(Rights::GRANT)) else {
                return false;
            };
            if !rejected!(m, m.grant(BOB, delegator, ALICE, Rights::ALL)) {
                return false;
            }
            let Ok(child) = m.grant(BOB, delegator, ALICE, Rights::READ) else {
                return false;
            };
            let Ok(duplicate) = m.grant(ALICE, root, ALICE, Rights::ALL) else {
                return false;
            };
            if !rejected!(m, m.revoke(ALICE, duplicate), Error::Denied) {
                return false;
            }
            let Ok(none) = m.grant(ALICE, root, BOB, Rights::NONE) else {
                return false;
            };
            if !rejected!(m, m.begin(BOB, none, Access::Read)) {
                return false;
            }
            let Ok(op) = m.begin(ALICE, child, Access::Read) else {
                return false;
            };
            if !rejected!(m, m.cancel(BOB, op)) {
                return false;
            }
            if !matches!(m.complete(op), Ok(Completion::Completed))
                || m.revoke(ALICE, root).is_err()
            {
                return false;
            }
            rejected!(m, m.begin(BOB, read, Access::Read))
                && rejected!(m, m.begin(ALICE, duplicate, Access::Read))
                && rejected!(m, m.begin(ALICE, child, Access::Read))
                && m.stats().resources == 0
                && m.stats().bytes == 0
                && m.audit()
        }),
        Ok(true)
    )
}

pub fn limits() -> bool {
    // Leave space for callers and test locals on the 64 KiB bootstrap stack.
    if core::mem::size_of::<crate::capability::Model<'_>>() > 16 * 1024 {
        return false;
    }
    let bytes = matches!(
        with_model(
            Config {
                byte_budget: 8,
                ..config()
            },
            |mut m| {
                let Ok(root) = m.create(ALICE, 8, Rights::ALL) else {
                    return false;
                };
                rejected!(m, m.create(ALICE, 1, Rights::ALL), Error::ByteBudget)
                    && m.release(ALICE, root).is_ok()
                    && m.stats().bytes == 0
                    && m.audit()
            }
        ),
        Ok(true)
    );
    let overflow = matches!(
        with_model(
            Config {
                byte_budget: u64::MAX,
                ..config()
            },
            |mut m| {
                let Ok(root) = m.create(ALICE, u64::MAX, Rights::ALL) else {
                    return false;
                };
                rejected!(m, m.create(ALICE, 1, Rights::ALL), Error::Overflow)
                    && m.stats().bytes == u64::MAX
                    && m.release(ALICE, root).is_ok()
                    && m.stats().bytes == 0
                    && m.audit()
            }
        ),
        Ok(true)
    );
    let resources = matches!(
        with_model(config(), |mut m| {
            for _ in 0..64 {
                if m.create(ALICE, 1, Rights::ALL).is_err() || !m.audit() {
                    return false;
                }
            }
            m.stats().resources == 64 && rejected!(m, m.create(ALICE, 1, Rights::ALL))
        }),
        Ok(true)
    );
    let grants = matches!(
        with_model(config(), |mut m| {
            let Ok(root) = m.create(ALICE, 1, Rights::ALL) else {
                return false;
            };
            for _ in 1..128 {
                if m.grant(ALICE, root, BOB, Rights::READ).is_err() || !m.audit() {
                    return false;
                }
            }
            m.stats().grants == 128
                && rejected!(m, m.create(ALICE, 1, Rights::ALL), Error::GrantSlots)
                && rejected!(m, m.grant(ALICE, root, BOB, Rights::READ))
                && m.revoke(ALICE, root).is_ok()
                && m.stats().bytes == 0
                && m.audit()
        }),
        Ok(true)
    );
    let operations = matches!(
        with_model(config(), |mut m| {
            let Ok(root) = m.create(ALICE, 1, Rights::ALL) else {
                return false;
            };
            for _ in 0..64 {
                if m.begin(ALICE, root, Access::Write).is_err() || !m.audit() {
                    return false;
                }
            }
            m.stats().operations == 64 && rejected!(m, m.begin(ALICE, root, Access::Read))
        }),
        Ok(true)
    );
    let inflight = matches!(
        with_model(
            Config {
                max_in_flight: 1,
                ..config()
            },
            |mut m| {
                let Ok(root) = m.create(ALICE, 1, Rights::ALL) else {
                    return false;
                };
                let Ok(op) = m.begin(ALICE, root, Access::Read) else {
                    return false;
                };
                if !rejected!(m, m.begin(ALICE, root, Access::Read)) || m.complete(op).is_err() {
                    return false;
                }
                m.begin(ALICE, root, Access::Read).is_ok() && m.audit()
            }
        ),
        Ok(true)
    );
    let zero_inflight = matches!(
        with_model(
            Config {
                max_in_flight: 0,
                ..config()
            },
            |mut m| {
                let Ok(root) = m.create(ALICE, 1, Rights::ALL) else {
                    return false;
                };
                rejected!(m, m.begin(ALICE, root, Access::Read), Error::OperationLimit)
                    && m.release(ALICE, root).is_ok()
                    && m.audit()
            }
        ),
        Ok(true)
    );
    bytes
        && zero_inflight
        && overflow
        && resources
        && grants
        && operations
        && inflight
        && with_model(
            Config {
                generation_limit: 0,
                ..config()
            },
            |_| (),
        )
        .is_err()
        && with_model(
            Config {
                max_in_flight: 65,
                ..config()
            },
            |_| (),
        )
        .is_err()
}

pub fn lifetime() -> bool {
    let independent_delegate = matches!(
        with_model(config(), |mut m| {
            let baseline = m.stats();
            let Ok(root) = m.create(ALICE, 8, Rights::ALL) else {
                return false;
            };
            let Ok(child) = m.grant(ALICE, root, BOB, Rights::READ) else {
                return false;
            };
            if m.release(ALICE, root).is_err()
                || !m.audit()
                || m.stats().bytes != 8
                || m.stats().resources != 1
                || m.stats().grants != 1
            {
                return false;
            }
            let Ok(op) = m.begin(BOB, child, Access::Read) else {
                return false;
            };
            if m.release(BOB, child).is_err()
                || !m.audit()
                || m.stats().bytes != 8
                || m.stats().grants != 0
                || m.stats().operations != 1
            {
                return false;
            }
            matches!(m.complete(op), Ok(Completion::Completed))
                && m.stats() == baseline
                && m.audit()
        }),
        Ok(true)
    );
    independent_delegate
        && matches!(
            with_model(config(), |mut m| {
                let baseline = m.stats();
                let Ok(root) = m.create(ALICE, 32, Rights::ALL) else {
                    return false;
                };
                let Ok(child) = m.grant(ALICE, root, BOB, Rights::READ) else {
                    return false;
                };
                let Ok(first) = m.begin(ALICE, root, Access::Write) else {
                    return false;
                };
                let Ok(second) = m.begin(BOB, child, Access::Read) else {
                    return false;
                };
                if m.release(BOB, child).is_err()
                    || m.cancel(ALICE, first).is_err()
                    || m.cancel(ALICE, first).is_err()
                    || m.revoke(ALICE, root).is_err()
                {
                    return false;
                }
                if m.stats().bytes != 32
                    || m.stats().resources != 1
                    || m.stats().grants != 0
                    || m.stats().operations != 2
                    || !m.audit()
                {
                    return false;
                }
                if !rejected!(m, m.release(BOB, child))
                    || !rejected!(m, m.begin(ALICE, root, Access::Read))
                    || !matches!(m.complete(first), Ok(Completion::Cancelled))
                {
                    return false;
                }
                if m.stats().bytes != 32
                    || !rejected!(m, m.complete(first))
                    || !matches!(m.complete(second), Ok(Completion::Completed))
                {
                    return false;
                }
                if m.stats() != baseline || !rejected!(m, m.cancel(BOB, second)) {
                    return false;
                }
                let Ok(reused) = m.create(ALICE, 32, Rights::ALL) else {
                    return false;
                };
                let Ok(reused_op) = m.begin(ALICE, reused, Access::Read) else {
                    return false;
                };
                rejected!(m, m.release(ALICE, root))
                    && rejected!(m, m.complete(first), Error::StaleOperation)
                    && rejected!(m, m.complete(second))
                    && m.complete(reused_op).is_ok()
                    && m.release(ALICE, reused).is_ok()
                    && m.stats() == baseline
                    && m.audit()
            }),
            Ok(true)
        )
}

pub fn generation() -> bool {
    let resources = matches!(
        with_model(
            Config {
                generation_limit: 1,
                ..config()
            },
            |mut m| {
                for _ in 0..64 {
                    let Ok(root) = m.create(ALICE, 1, Rights::ALL) else {
                        return false;
                    };
                    if m.release(ALICE, root).is_err() || !rejected!(m, m.release(ALICE, root)) {
                        return false;
                    }
                }
                let s = m.stats();
                s.resources == 0
                    && s.bytes == 0
                    && s.retired_resources == 64
                    && s.retired_grants == 64
                    && rejected!(m, m.create(ALICE, 1, Rights::ALL))
            }
        ),
        Ok(true)
    );
    let grants = matches!(
        with_model(
            Config {
                generation_limit: 1,
                ..config()
            },
            |mut m| {
                let Ok(root) = m.create(ALICE, 1, Rights::ALL) else {
                    return false;
                };
                for _ in 1..128 {
                    let Ok(child) = m.grant(ALICE, root, BOB, Rights::READ) else {
                        return false;
                    };
                    if m.release(BOB, child).is_err() || !m.audit() {
                        return false;
                    }
                }
                m.stats().retired_grants == 127
                    && rejected!(m, m.grant(ALICE, root, BOB, Rights::READ))
                    && m.release(ALICE, root).is_ok()
                    && m.stats().retired_grants == 128
                    && m.audit()
            }
        ),
        Ok(true)
    );
    let operations = matches!(
        with_model(
            Config {
                generation_limit: 1,
                ..config()
            },
            |mut m| {
                let Ok(root) = m.create(ALICE, 1, Rights::ALL) else {
                    return false;
                };
                for _ in 0..64 {
                    let Ok(op) = m.begin(ALICE, root, Access::Read) else {
                        return false;
                    };
                    if !matches!(m.complete(op), Ok(Completion::Completed))
                        || !rejected!(m, m.complete(op))
                    {
                        return false;
                    }
                }
                m.stats().retired_operations == 64
                    && rejected!(m, m.begin(ALICE, root, Access::Read))
                    && m.release(ALICE, root).is_ok()
                    && m.stats().bytes == 0
                    && m.audit()
            }
        ),
        Ok(true)
    );
    resources && grants && operations
}

pub fn orderings(mut log: impl FnMut(&str)) -> bool {
    const NAMES: [&str; 4] = ["revoke", "cancel", "complete", "release"];
    let mut count = 0;
    for a in 0..4 {
        for b in 0..4 {
            for c in 0..4 {
                for d in 0..4 {
                    if a == b || a == c || a == d || b == c || b == d || c == d {
                        continue;
                    }
                    let order = [a, b, c, d];
                    let passed = matches!(
                        with_model(config(), |mut m| {
                            let Ok(root) = m.create(ALICE, 8, Rights::ALL) else {
                                return false;
                            };
                            let Ok(op) = m.begin(ALICE, root, Access::Read) else {
                                return false;
                            };
                            let mut grant_live = true;
                            let mut op_live = true;
                            let mut cancelled = false;
                            for action in order {
                                match action {
                                    0 | 3 => {
                                        let before = m.stats();
                                        let result = if action == 0 {
                                            m.revoke(ALICE, root)
                                        } else {
                                            m.release(ALICE, root)
                                        };
                                        if result.is_ok() != grant_live {
                                            return false;
                                        }
                                        if !grant_live && m.stats() != before {
                                            return false;
                                        }
                                        grant_live = false;
                                        if !rejected!(m, m.revoke(ALICE, root))
                                            || !rejected!(m, m.release(ALICE, root))
                                        {
                                            return false;
                                        }
                                    }
                                    1 => {
                                        if op_live {
                                            if m.cancel(ALICE, op).is_err()
                                                || m.cancel(ALICE, op).is_err()
                                            {
                                                return false;
                                            }
                                            cancelled = true;
                                        } else if !rejected!(m, m.cancel(ALICE, op)) {
                                            return false;
                                        }
                                    }
                                    2 => {
                                        let expected = if cancelled {
                                            Completion::Cancelled
                                        } else {
                                            Completion::Completed
                                        };
                                        if !matches!(m.complete(op), Ok(result) if result == expected)
                                        {
                                            return false;
                                        }
                                        op_live = false;
                                        if !rejected!(m, m.complete(op))
                                            || !rejected!(m, m.cancel(ALICE, op))
                                        {
                                            return false;
                                        }
                                    }
                                    _ => return false,
                                }
                                let s = m.stats();
                                let live = grant_live || op_live;
                                if !m.audit()
                                    || s.resources != usize::from(live)
                                    || s.grants != usize::from(grant_live)
                                    || s.operations != usize::from(op_live)
                                    || s.bytes != if live { 8 } else { 0 }
                                {
                                    return false;
                                }
                            }
                            m.stats().resources == 0 && m.stats().bytes == 0 && m.audit()
                        }),
                        Ok(true)
                    );
                    if !passed {
                        log("CAPMODEL failed ordering:");
                        for action in order {
                            log(" ");
                            log(NAMES[action]);
                        }
                        log("\n");
                        return false;
                    }
                    count += 1;
                }
            }
        }
    }
    count == 24
}
