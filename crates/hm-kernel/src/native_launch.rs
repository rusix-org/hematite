//! Safe monitor-facing native launch results; these values confer no authority.
#![forbid(unsafe_code)]
use crate::{Platform, elf, initramfs, startup_input::Input};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Availability {
    Unsupported,
    NotInitialized,
    Ready,
    Unavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeStatus {
    pub availability: Availability,
    pub completed: u64,
    pub generation: Option<u32>,
    pub free: Option<usize>,
}
impl NativeStatus {
    pub const fn unsupported() -> Self {
        Self {
            availability: Availability::Unsupported,
            completed: 0,
            generation: None,
            free: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    Exited(u8),
    Fault,
    Deadline,
    CallLimit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LaunchReport {
    pub outcome: Outcome,
    pub completed: u64,
    pub generation: u32,
    pub before: usize,
    pub after: usize,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LaunchError {
    Unsupported,
    Archive(initramfs::Error),
    Missing,
    Directory,
    Elf(elf::Error),
    Unavailable,
    Busy,
    Resources,
}
const UNSUPPORTED: &str = "Native launching is unsupported on this platform.\n";

#[inline(never)]
pub(crate) fn list_apps(platform: &mut impl Platform) {
    let Some(bytes) = platform.native_archive() else {
        platform.write(UNSUPPORTED);
        return;
    };
    let Ok(archive) = initramfs::Archive::parse(bytes) else {
        platform.write("Native application archive is invalid.\n");
        return;
    };
    platform.write("Native programs:\n");
    for entry in archive
        .entries()
        .filter(|entry| entry.kind() == initramfs::Kind::File)
    {
        platform.write("  ");
        platform.write(entry.path());
        platform.write("\n");
    }
}

/// Print only after synchronous core teardown; no writer is passed to execution.
#[inline(never)]
pub(crate) fn run_command(platform: &mut impl Platform, command: &str) {
    let (path, bytes) = match command.split_once(' ') {
        Some((path, bytes)) => (path, Some(bytes.as_bytes())),
        None => (command, None),
    };
    if path.is_empty() || path.bytes().any(|b| b.is_ascii_whitespace()) {
        platform.write("Usage: run <archive-path> [text].\n");
        return;
    }
    let result = if let Some(bytes) = bytes {
        let Ok(input) = Input::from_bytes(bytes) else {
            platform.write("Usage: run <archive-path> [text].\n");
            return;
        };
        platform.run_native_with_input(path, &input)
    } else {
        platform.run_native(path)
    };
    match result {
        Ok(report) => match report.outcome {
            Outcome::Exited(code) => {
                platform.write("Program exited (status ");
                number(platform, u64::from(code));
                platform.write(").\n");
            }
            Outcome::Fault => platform.write("Program faulted.\n"),
            Outcome::Deadline => platform.write("Program timed out.\n"),
            Outcome::CallLimit => platform.write("Program exceeded its call limit.\n"),
        },
        Err(error) => platform.write(match error {
            LaunchError::Unsupported => UNSUPPORTED,
            LaunchError::Archive(_) => "Native application archive is invalid.\n",
            LaunchError::Missing => "Program not found.\n",
            LaunchError::Directory => "Cannot launch a directory.\n",
            LaunchError::Elf(_) => "Program is not a supported native ELF.\n",
            LaunchError::Unavailable => "Native launcher is unavailable.\n",
            LaunchError::Busy => "Native launcher is busy.\n",
            LaunchError::Resources => "Insufficient resources to launch program.\n",
        }),
    }
}
#[inline(never)]
pub(crate) fn report_status(platform: &mut impl Platform) {
    let status = platform.native_status();
    platform.write("Native launcher: ");
    platform.write(match status.availability {
        Availability::Unsupported => "unsupported\n",
        Availability::NotInitialized => "not initialized\n",
        Availability::Ready => "ready\n",
        Availability::Unavailable => "unavailable\n",
    });
    platform.write("Native launches: ");
    number(platform, status.completed);
    platform.write("\n");
    platform.write("Last prepared generation: ");
    if let Some(generation) = status.generation {
        number(platform, u64::from(generation));
    } else {
        platform.write("none");
    }
    platform.write("\nFree frames: ");
    if let Some(free) = status.free {
        number(platform, free as u64);
    } else {
        platform.write("unavailable");
    }
    platform.write("\n");
}
fn number(platform: &mut impl Platform, mut value: u64) {
    let mut bytes = [b'0'; 20];
    let mut start = bytes.len();
    loop {
        start -= 1;
        bytes[start] = b'0' + (value % 10) as u8;
        value /= 10;
        if value == 0 {
            break;
        }
    }
    if let Ok(text) = core::str::from_utf8(&bytes[start..]) {
        platform.write(text);
    }
}
