#![no_std]
#![forbid(unsafe_code)]

//! Safe bootstrap policy. This monitor is kernel code, not Linux userspace.

use core::ops::Range;

/// Observations collected by the core from the active bootstrap mappings.
#[derive(Clone, Copy)]
pub struct BootChecks {
    pub write_protect: bool,
    pub nx: bool,
    pub smep: bool,
    pub smap: bool,
    pub wx: bool,
    pub stack_guard: bool,
    pub null_guard: bool,
}

/// The core retains all hardware authority; policy only exchanges owned values.
pub trait Platform {
    fn write(&mut self, text: &str);
    fn read_byte(&mut self) -> Option<u8>;
    fn checks(&self) -> BootChecks;
}

/// Return a bounded half-open range, rejecting both arithmetic and bound errors.
fn checked_range(start: usize, length: usize, limit: usize) -> Option<Range<usize>> {
    let end = start.checked_add(length)?;
    if start > limit || end > limit {
        None
    } else {
        Some(start..end)
    }
}

const LINE_CAPACITY: usize = 64;

#[derive(Clone, Copy, PartialEq, Eq)]
enum InputEvent {
    Ignored,
    Echo(u8),
    Erase,
    Ready,
}

struct InputLine {
    bytes: [u8; LINE_CAPACITY],
    length: usize,
    rejected: bool,
    skip_lf: bool,
}

impl InputLine {
    const fn new() -> Self {
        Self {
            bytes: [0; LINE_CAPACITY],
            length: 0,
            rejected: false,
            skip_lf: false,
        }
    }

    fn feed(&mut self, byte: u8) -> InputEvent {
        if self.skip_lf {
            self.skip_lf = false;
            if byte == b'\n' {
                return InputEvent::Ignored;
            }
        }
        if byte == b'\r' || byte == b'\n' {
            self.skip_lf = byte == b'\r';
            return InputEvent::Ready;
        }
        // Once a line is rejected, discard it through its terminator. Never
        // execute a valid-looking prefix after a truncation or invalid byte.
        if self.rejected {
            return InputEvent::Ignored;
        }
        if byte == 8 || byte == 127 {
            if let Some(previous) = self.length.checked_sub(1) {
                self.length = previous;
                return InputEvent::Erase;
            }
            return InputEvent::Ignored;
        }
        if !(b' '..=b'~').contains(&byte) {
            self.rejected = true;
            return InputEvent::Ignored;
        }
        let Some(range) = checked_range(self.length, 1, self.bytes.len()) else {
            self.rejected = true;
            return InputEvent::Ignored;
        };
        // get_mut additionally keeps this path non-panicking if the layout or
        // range helper changes in a future refactor.
        let Some(slot) = self.bytes.get_mut(range.start) else {
            self.rejected = true;
            return InputEvent::Ignored;
        };
        *slot = byte;
        self.length = range.end;
        InputEvent::Echo(byte)
    }

    fn command(&self) -> Option<&[u8]> {
        if self.rejected {
            None
        } else {
            self.bytes.get(..self.length)
        }
    }

    fn clear(&mut self) {
        self.length = 0;
        self.rejected = false;
        // Keep CRLF suppression across completion of the preceding line.
    }
}

fn test_checked_range() -> bool {
    checked_range(0, 0, 0) == Some(0..0)
        && checked_range(63, 1, 64) == Some(63..64)
        && checked_range(64, 0, 64) == Some(64..64)
        && checked_range(64, 1, 64).is_none()
        && checked_range(65, 0, 64).is_none()
        && checked_range(usize::MAX - 2, 4, usize::MAX).is_none()
}

fn test_monitor_bounds() -> bool {
    let mut line = InputLine::new();
    for _ in 0..LINE_CAPACITY {
        if line.feed(b'x') != InputEvent::Echo(b'x') {
            return false;
        }
    }
    if line.command().map(<[u8]>::len) != Some(LINE_CAPACITY)
        || line.feed(b'x') != InputEvent::Ignored
        || line.command().is_some()
        || line.feed(b'\r') != InputEvent::Ready
    {
        return false;
    }
    line.clear();
    if line.feed(b'\n') != InputEvent::Ignored || line.feed(8) != InputEvent::Ignored {
        return false;
    }
    for byte in b"helx" {
        line.feed(*byte);
    }
    if line.feed(127) != InputEvent::Erase {
        return false;
    }
    line.feed(b'p');
    if line.feed(b'\n') != InputEvent::Ready || line.command() != Some(&b"help"[..]) {
        return false;
    }
    line.clear();
    for byte in b"help" {
        line.feed(*byte);
    }
    line.feed(0xff);
    line.feed(8);
    line.command().is_none() && line.feed(b'\n') == InputEvent::Ready
}

/// Run the full bootstrap inventory. The core must also return a failure exit
/// status when this returns false; the serial summary alone is not success.
pub fn run_tests(platform: &mut impl Platform) -> bool {
    let checks = platform.checks();
    let cases = [
        ("write_protect", checks.write_protect),
        ("nx", checks.nx),
        ("smep", checks.smep),
        ("smap", checks.smap),
        ("wx", checks.wx),
        ("stack_guard", checks.stack_guard),
        ("null_guard", checks.null_guard),
        ("checked_range", test_checked_range()),
        ("monitor_bounds", test_monitor_bounds()),
    ];
    platform.write("HMTEST BEGIN 9\n");
    let mut passed = true;
    for (name, success) in cases {
        platform.write(if success {
            "HMTEST PASS "
        } else {
            "HMTEST FAIL "
        });
        platform.write(name);
        platform.write("\n");
        passed &= success;
    }
    platform.write("HMTEST END 9\n");
    passed
}

fn status(platform: &mut impl Platform) {
    let checks = platform.checks();
    for (name, enabled) in [
        ("write protection", checks.write_protect),
        ("NX", checks.nx),
        ("SMEP", checks.smep),
        ("SMAP", checks.smap),
        ("W^X mappings", checks.wx),
        ("stack guard", checks.stack_guard),
        ("null guard", checks.null_guard),
    ] {
        platform.write(name);
        platform.write(if enabled { ": OK\n" } else { ": FAILED\n" });
    }
    platform.write("Bootstrap only: one CPU; no userspace, storage, or networking.\n");
}

pub fn run_monitor(platform: &mut impl Platform) -> ! {
    platform.write("Hematite bootstrap monitor (local working name)\n");
    platform.write("Commands: help, status, test. This is not a Linux shell.\n");
    platform.write("hm> ");
    let mut line = InputLine::new();
    loop {
        let Some(byte) = platform.read_byte() else {
            core::hint::spin_loop();
            continue;
        };
        match line.feed(byte) {
            InputEvent::Ignored => {}
            InputEvent::Echo(byte) => {
                let bytes = [byte];
                if let Ok(text) = core::str::from_utf8(&bytes) {
                    platform.write(text);
                }
            }
            InputEvent::Erase => platform.write("\x08 \x08"),
            InputEvent::Ready => {
                platform.write("\n");
                match line.command() {
                    None => platform.write("Rejected: use at most 64 printable ASCII bytes.\n"),
                    Some(b"") => {}
                    Some(b"help") => platform.write("help   - show commands\nstatus - inspect boot protections\ntest   - run bootstrap tests\n"),
                    Some(b"status") => status(platform),
                    Some(b"test") => {
                        if run_tests(platform) {
                            platform.write("Bootstrap tests passed.\n");
                        } else {
                            platform.write("Bootstrap tests FAILED.\n");
                        }
                    }
                    Some(_) => platform.write("Unknown command; type help.\n"),
                }
                line.clear();
                platform.write("hm> ");
            }
        }
    }
}
