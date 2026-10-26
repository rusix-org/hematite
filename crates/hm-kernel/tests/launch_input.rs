//! Test the actual monitor policy against a minimal effect recorder. Hardware
//! Platform integration is separately compiled and exercised in QEMU.
#![forbid(unsafe_code)]
use hm_kernel::{elf, initramfs, startup_input};
#[allow(dead_code)]
#[path = "../src/native_launch.rs"]
mod native_launch;
use native_launch::{LaunchError, LaunchReport, NativeStatus, Outcome};

trait Platform {
    fn native_archive(&self) -> Option<&'static [u8]> {
        None
    }
    fn run_native(&mut self, path: &str) -> Result<LaunchReport, LaunchError>;
    fn run_native_with_input(
        &mut self,
        path: &str,
        input: &startup_input::Input,
    ) -> Result<LaunchReport, LaunchError>;
    fn native_status(&self) -> NativeStatus {
        NativeStatus::unsupported()
    }
    fn write(&mut self, text: &str);
}
#[derive(Default)]
struct Recorder {
    calls: Vec<(String, Option<Vec<u8>>)>,
    output: String,
    error: Option<LaunchError>,
}
impl Recorder {
    fn record(&mut self, path: &str, input: Option<&[u8]>) -> Result<LaunchReport, LaunchError> {
        self.calls
            .push((path.to_owned(), input.map(<[u8]>::to_vec)));
        if let Some(error) = self.error {
            return Err(error);
        }
        Ok(LaunchReport {
            outcome: Outcome::Exited(0),
            completed: 1,
            generation: 1,
            before: 5,
            after: 5,
        })
    }
}
impl Platform for Recorder {
    fn run_native(&mut self, path: &str) -> Result<LaunchReport, LaunchError> {
        self.record(path, None)
    }
    fn run_native_with_input(
        &mut self,
        path: &str,
        input: &startup_input::Input,
    ) -> Result<LaunchReport, LaunchError> {
        self.record(path, input.as_bytes())
    }
    fn write(&mut self, text: &str) {
        self.output.push_str(text);
    }
}
#[test]
fn absent_dispatch() {
    let mut r = Recorder::default();
    native_launch::run_command(&mut r, "bin/echo");
    assert_eq!(r.calls, [("bin/echo".into(), None)]);
    assert_eq!(r.output, "Program exited (status 0).\n");
}
#[test]
fn empty_dispatch() {
    let mut r = Recorder::default();
    native_launch::run_command(&mut r, "bin/echo ");
    assert_eq!(r.calls, [("bin/echo".into(), Some(vec![]))]);
}
#[test]
fn spaces_preserved() {
    let mut r = Recorder::default();
    {
        let command = String::from("bin/echo  hi  ");
        native_launch::run_command(&mut r, &command);
    }
    assert_eq!(r.calls, [("bin/echo".into(), Some(b" hi  ".to_vec()))]);
}
#[test]
fn maximum_payload() {
    let mut r = Recorder::default();
    for n in [1, 51, 255, 256] {
        native_launch::run_command(&mut r, &format!("bin/echo {}", "A".repeat(n)));
        assert_eq!(
            r.calls.last().unwrap(),
            &("bin/echo".into(), Some(vec![b'A'; n]))
        );
    }
}
#[test]
fn oversize_rejected() {
    let mut r = Recorder::default();
    native_launch::run_command(&mut r, &format!("bin/echo {}", "A".repeat(257)));
    assert!(r.calls.is_empty());
    assert_eq!(r.output, "Usage: run <archive-path> [text].\n");
    native_launch::run_command(&mut r, "bin/echo ok");
    assert_eq!(r.calls, [("bin/echo".into(), Some(b"ok".to_vec()))]);
}
#[test]
fn empty_or_whitespace_path_rejected() {
    for command in [
        "",
        " ",
        " bin/echo",
        "bin\techo",
        "bin/echo\n",
        "\rbin/echo input",
    ] {
        let mut r = Recorder::default();
        native_launch::run_command(&mut r, command);
        assert!(r.calls.is_empty(), "{command:?}");
        assert_eq!(r.output, "Usage: run <archive-path> [text].\n");
    }
}
#[test]
fn core_errors_are_reported() {
    for (error, text) in [
        (LaunchError::Missing, "Program not found.\n"),
        (LaunchError::Directory, "Cannot launch a directory.\n"),
        (
            LaunchError::Resources,
            "Insufficient resources to launch program.\n",
        ),
    ] {
        let mut r = Recorder {
            error: Some(error),
            ..Recorder::default()
        };
        native_launch::run_command(&mut r, "bin/nope input");
        assert_eq!(r.calls, [("bin/nope".into(), Some(b"input".to_vec()))]);
        assert_eq!(r.output, text);
    }
}
#[test]
fn unsupported_input() {
    let mut r = Recorder {
        error: Some(LaunchError::Unsupported),
        ..Recorder::default()
    };
    native_launch::run_command(&mut r, "bin/echo text");
    assert_eq!(
        r.output,
        "Native launching is unsupported on this platform.\n"
    );
    assert_eq!(r.calls.len(), 1);
}
