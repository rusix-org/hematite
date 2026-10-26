//! Fixed archive paths and independent payload expectations for design 0027.
#![forbid(unsafe_code)]
pub const ARCHIVE: &[u8] = include_bytes!("../fixtures/startup-input-initramfs.cpio");
#[derive(Clone, Copy)]
pub enum Payload {
    Absent,
    Empty,
    One,
    Bytes255,
    Binary256,
    Write,
    Execute,
    Loop,
}
impl Payload {
    pub fn present(self) -> bool {
        !matches!(self, Self::Absent)
    }
    pub fn length(self) -> usize {
        match self {
            Self::Absent | Self::Empty => 0,
            Self::Bytes255 => 255,
            Self::Binary256 => 256,
            Self::Execute => 4,
            _ => 1,
        }
    }
    pub fn byte(self, index: usize) -> u8 {
        match self {
            Self::One => 0x41,
            Self::Write => 0x57,
            Self::Loop => 0x4c,
            Self::Bytes255 | Self::Binary256 => index as u8,
            Self::Execute => [0xc0, 3, 0x5f, 0xd6].get(index).copied().unwrap_or(0),
            Self::Absent | Self::Empty => 0,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Case {
    pub name: &'static str,
    pub fixture: &'static str,
    pub path: &'static str,
    pub payload: Payload,
}
pub const CASES: [Case; 12] = [
    Case {
        name: "absent",
        fixture: "absent",
        path: "bin/absent",
        payload: Payload::Absent,
    },
    Case {
        name: "empty",
        fixture: "empty",
        path: "bin/empty",
        payload: Payload::Empty,
    },
    Case {
        name: "one",
        fixture: "one",
        path: "bin/one",
        payload: Payload::One,
    },
    Case {
        name: "bytes255",
        fixture: "bytes255",
        path: "bin/bytes255",
        payload: Payload::Bytes255,
    },
    Case {
        name: "binary256",
        fixture: "binary256",
        path: "bin/binary256",
        payload: Payload::Binary256,
    },
    Case {
        name: "after_binary_absent",
        fixture: "absent",
        path: "bin/absent",
        payload: Payload::Absent,
    },
    Case {
        name: "payload_write",
        fixture: "payload_write",
        path: "bin/payload_write",
        payload: Payload::Write,
    },
    Case {
        name: "after_write_absent",
        fixture: "absent",
        path: "bin/absent",
        payload: Payload::Absent,
    },
    Case {
        name: "payload_execute",
        fixture: "payload_execute",
        path: "bin/payload_execute",
        payload: Payload::Execute,
    },
    Case {
        name: "after_execute_absent",
        fixture: "absent",
        path: "bin/absent",
        payload: Payload::Absent,
    },
    Case {
        name: "input_loop",
        fixture: "input_loop",
        path: "bin/input_loop",
        payload: Payload::Loop,
    },
    Case {
        name: "after_loop_absent",
        fixture: "absent",
        path: "bin/absent",
        payload: Payload::Absent,
    },
];
