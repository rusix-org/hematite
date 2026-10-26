//! Bounded in-guest topology fixtures; no firmware calls or CPU authority.
#![forbid(unsafe_code)]

use crate::cpu_topology::{Conduit, Topology, parse};

const START: usize = 80;
const NAMES: &[u8] = b"#address-cells\0#size-cells\0device_type\0reg\0status\0enable-method\0compatible\0method\0cpu_on\0cpu_off\0cpu_suspend\0migrate\0";

struct Blob {
    bytes: [u8; 4096],
    cursor: usize,
    len: usize,
    okay: bool,
}

impl Blob {
    fn new() -> Self {
        Self {
            bytes: [0; 4096],
            cursor: START,
            len: 0,
            okay: true,
        }
    }

    fn at(&mut self, position: usize, value: u32) {
        if let Some(destination) = self.bytes.get_mut(position..position.saturating_add(4)) {
            destination.copy_from_slice(&value.to_be_bytes());
        } else {
            self.okay = false;
        }
    }

    fn raw(&mut self, value: &[u8]) {
        let end = self.cursor.saturating_add(value.len());
        if let Some(destination) = self.bytes.get_mut(self.cursor..end) {
            destination.copy_from_slice(value);
            self.cursor = end;
        } else {
            self.okay = false;
        }
    }

    fn word(&mut self, value: u32) {
        self.raw(&value.to_be_bytes());
    }

    fn align(&mut self) {
        while self.okay && !self.cursor.is_multiple_of(4) {
            self.raw(&[0]);
        }
    }

    fn begin(&mut self, name: &[u8]) {
        self.word(1);
        self.raw(name);
        self.raw(&[0]);
        self.align();
    }

    fn end(&mut self) {
        self.word(2);
    }

    fn property(&mut self, name: &[u8], value: &[u8]) -> usize {
        let Some(offset) = NAMES
            .windows(name.len() + 1)
            .position(|part| &part[..name.len()] == name && part[name.len()] == 0)
        else {
            self.okay = false;
            return 0;
        };
        let start = self.cursor;
        self.word(3);
        self.word(value.len() as u32);
        self.word(offset as u32);
        self.raw(value);
        self.align();
        start
    }

    fn cell(&mut self, name: &[u8], value: u32) {
        self.property(name, &value.to_be_bytes());
    }

    fn root(&mut self) {
        self.begin(b"");
        self.cell(b"#address-cells", 2);
        self.cell(b"#size-cells", 2);
    }

    fn cpus(&mut self, cells: u32) {
        self.begin(b"cpus");
        self.cell(b"#address-cells", cells);
        self.cell(b"#size-cells", 0);
    }

    fn cpu(&mut self, id: u64, cells: u32, method: bool) {
        self.begin(match id {
            0 => b"cpu@0",
            1 => b"cpu@1",
            _ => b"cpu@2",
        });
        self.property(b"device_type", b"cpu\0");
        if cells == 2 {
            self.property(b"reg", &id.to_be_bytes());
        } else {
            self.cell(b"reg", id as u32);
        }
        if method {
            self.property(b"enable-method", b"psci\0");
        }
        self.end();
    }

    fn psci(&mut self) {
        self.begin(b"psci");
        self.property(b"compatible", b"arm,psci-1.0\0arm,psci-0.2\0arm,psci\0");
        self.property(b"method", b"hvc\0");
        self.cell(b"cpu_on", 0xc4000003);
        self.cell(b"cpu_off", 0x84000002);
        self.cell(b"cpu_suspend", 0xc4000001);
        self.cell(b"migrate", 0xc4000005);
        self.end();
    }

    fn finish(&mut self) {
        self.word(9);
        let end = self.cursor;
        self.raw(NAMES);
        self.len = self.cursor;
        self.at(0, 0xd00dfeed);
        self.at(4, self.len as u32);
        self.at(8, START as u32);
        self.at(12, end as u32);
        self.at(16, 40);
        self.at(20, 17);
        self.at(24, 16);
        self.at(28, 0);
        self.at(32, NAMES.len() as u32);
        self.at(36, (end - START) as u32);
    }

    fn data(&self) -> &[u8] {
        &self.bytes[..self.len]
    }
    fn rejected(&self) -> bool {
        self.okay && parse(self.data()).is_err()
    }
}

#[inline(never)]
fn basic(ids: &[u64], cells: u32, psci: bool, method: bool) -> Blob {
    let mut b = Blob::new();
    b.root();
    b.cpus(cells);
    for id in ids {
        b.cpu(*id, cells, method);
    }
    b.end();
    if psci {
        b.psci();
    }
    b.end();
    b.finish();
    b
}

fn sound(topology: &Topology) -> bool {
    let ids = topology.affinities();
    topology.cpu_count() == ids.len()
        && (ids == [0] || ids == [0, 1])
        && (ids.len() == 1 || topology.psci().is_some())
        && topology
            .psci()
            .is_none_or(|psci| psci.conduit == Conduit::Hvc)
}

#[inline(never)]
pub(super) fn valid() -> bool {
    for (ids, cells, psci, method) in [
        (&[0][..], 1, false, false),
        (&[0][..], 1, true, false),
        (&[0][..], 2, true, true),
        (&[0, 1][..], 1, true, true),
        (&[1, 0][..], 2, true, true),
    ] {
        let b = basic(ids, cells, psci, method);
        let before = b.bytes;
        let Ok(topology) = parse(b.data()) else {
            return false;
        };
        if !b.okay
            || !sound(&topology)
            || b.bytes != before
            || topology.cpu_count() != ids.len()
            || topology.psci().is_some() != psci
            || topology.psci().is_some_and(|p| !p.advertised_1_0)
        {
            return false;
        }
    }
    extra_valid()
}

#[inline(never)]
fn extra_valid() -> bool {
    let mut b = Blob::new();
    // Complete nonempty reservation followed by an in-bounds zero terminator.
    b.at(44, 0x4000);
    b.at(52, 4096);
    b.root();
    b.word(4);
    b.cpus(1);
    b.begin(b"cpu@0");
    b.property(b"device_type", b"cpu\0");
    b.cell(b"reg", 0);
    let property = b.property(b"status", b"okay\0");
    // QEMU property alignment bytes do not belong to the property value.
    b.bytes[property + 17..property + 20].copy_from_slice(&[0xff, 3, 9]);
    b.property(b"enable-method", b"psci\0");
    b.end();
    b.begin(b"cpu@1");
    b.property(b"device_type", b"cpu\0");
    b.cell(b"reg", 1);
    b.property(b"status", b"ok\0");
    b.property(b"enable-method", b"psci\0");
    b.end();
    // A cpu-map's references are unrelated to the actual CPU descriptors.
    b.begin(b"cpu-map");
    b.begin(b"cluster0");
    b.begin(b"core0");
    b.end();
    b.end();
    b.end();
    b.end();
    b.begin(b"psci");
    b.property(b"compatible", b"arm,psci-0.2\0");
    b.property(b"method", b"hvc\0");
    b.property(b"status", b"okay\0");
    b.end();
    b.begin(b"unrelated");
    b.property(b"compatible", b"test,device\0");
    b.end();
    b.end();
    b.finish();
    b.okay
        && parse(b.data()).is_ok_and(|t| {
            sound(&t) && t.affinities() == [0, 1] && t.psci().is_some_and(|p| !p.advertised_1_0)
        })
}

// One small blob per frame prevents fixture accumulation on the boot stack.
#[inline(never)]
fn invalid_cpu(case: u8) -> bool {
    let mut b = Blob::new();
    b.root();
    if case == 17 {
        b.begin(b"bus");
    }
    b.begin(b"cpus");
    if case != 0 {
        b.cell(b"#address-cells", if case == 1 { 3 } else { 1 });
    }
    if case != 2 {
        b.cell(b"#size-cells", u32::from(case == 3));
    }
    if case == 4 {
        b.cell(b"#address-cells", 1);
    }
    if case == 5 {
        b.property(b"status", b"disabled\0");
    }
    b.begin(if case == 6 { b"cpu" } else { b"cpu@0" });
    if case != 7 {
        b.property(
            b"device_type",
            if case == 8 { b"device\0" } else { b"cpu\0" },
        );
    }
    match case {
        9 => {}
        10 => {
            b.property(b"reg", &[0; 8]);
        }
        11 => {
            b.cell(b"reg", 1);
        }
        _ => {
            b.cell(b"reg", 0);
        }
    }
    b.property(
        b"enable-method",
        if case == 12 {
            b"spin-table\0"
        } else {
            b"psci\0"
        },
    );
    if case == 13 {
        b.property(b"status", b"disabled\0");
    }
    if case == 14 {
        b.cell(b"reg", 0);
    }
    if case == 15 {
        b.property(b"enable-method", b"psci\0");
    }
    if case == 16 {
        b.begin(b"child");
        b.end();
    }
    b.end();
    b.end();
    if case == 17 {
        b.end();
    }
    if case == 18 {
        b.cpus(1);
        b.cpu(0, 1, true);
        b.end();
    }
    if case == 19 {
        b.property(b"status", b"okay\0");
    }
    b.psci();
    b.end();
    b.finish();
    b.rejected()
}

#[inline(never)]
fn invalid_psci(case: u8) -> bool {
    let mut b = Blob::new();
    b.root();
    b.cpus(1);
    b.cpu(0, 1, true);
    b.cpu(1, 1, true);
    b.end();
    if case != 0 {
        if case == 13 {
            b.begin(b"bus");
        }
        b.begin(b"psci");
        if case != 1 {
            b.property(
                b"compatible",
                match case {
                    2 => b"arm,psci\0",
                    3 => b"arm,psci-0.2",
                    4 => b"arm,psci-0.2\0\0",
                    5 => b"vendor,unknown\0",
                    _ => b"arm,psci-0.2\0",
                },
            );
        }
        if case != 6 {
            b.property(b"method", if case == 7 { b"smc\0" } else { b"hvc\0" });
        }
        if case == 8 {
            b.property(b"method", b"hvc\0");
        }
        if case == 9 {
            b.property(b"compatible", b"arm,psci-1.0\0");
        }
        if case == 10 {
            b.cell(b"cpu_on", 0x84000003);
        }
        if case == 11 {
            b.property(b"status", b"disabled\0");
        }
        if case == 12 {
            b.begin(b"child");
            b.end();
        }
        b.end();
        if case == 13 {
            b.end();
        }
        if case == 14 {
            b.psci();
        }
        if case >= 15 {
            if case == 16 {
                b.begin(b"bus");
            }
            b.begin(if case == 17 {
                b"psci"
            } else {
                b"legacy-firmware"
            });
            b.property(b"compatible", b"arm,psci\0");
            b.property(b"method", b"hvc\0");
            b.end();
            if case == 16 {
                b.end();
            }
        }
    }
    b.end();
    b.finish();
    b.rejected()
}

#[inline(never)]
fn invalid_header(offset: usize, value: u32) -> bool {
    let mut b = basic(&[0, 1], 1, true, true);
    b.at(offset, value);
    b.rejected()
}

#[inline(never)]
fn invalid_structure(case: u8) -> bool {
    let mut b = basic(&[0, 1], 1, true, true);
    match case {
        0 => {
            b.bytes[START + 5] = 1;
        }
        1 => {
            b.at(START, 7);
        }
        2 => {
            b.at(START + 16, u32::MAX);
        }
        3 => {
            b.at(START + 12, u32::MAX);
        }
        4 => {
            b.at(16, (START - 8) as u32);
        }
        5 => {
            b.at(40, u32::MAX);
            b.at(44, u32::MAX);
            b.at(52, 1);
        }
        6 => {
            b = Blob::new();
            b.root();
            for _ in 0..33 {
                b.begin(b"a");
            }
            for _ in 0..33 {
                b.end();
            }
            b.cpus(1);
            b.cpu(0, 1, false);
            b.end();
            b.end();
            b.finish();
        }
        7 => {
            b.at(b.len - NAMES.len() - 4, 4);
        }
        8 => {
            b.at(b.len - NAMES.len() - 8, 4);
        }
        9 => {
            b.at(b.len - NAMES.len() - 4, 2);
        }
        _ => {
            b = Blob::new();
            b.root();
            b.cpus(1);
            b.cpu(0, 1, false);
            b.end();
            b.end();
            b.begin(b"another-root");
            b.end();
            b.finish();
        }
    }
    b.rejected()
}

#[inline(never)]
fn mutations() -> bool {
    let mut b = basic(&[0, 1], 1, true, true);
    if !b.okay || parse(b.data()).is_err() {
        return false;
    }
    for end in 0..b.len {
        if parse(&b.bytes[..end]).is_ok() {
            return false;
        }
    }
    for offset in 0..b.len {
        b.bytes[offset] ^= 0xff;
        if let Ok(topology) = parse(b.data())
            && !sound(&topology)
        {
            return false;
        }
        b.bytes[offset] ^= 0xff;
    }
    true
}

#[inline(never)]
pub(super) fn malformed() -> bool {
    if !mutations() {
        return false;
    }
    for (ids, cells, psci, method) in [
        (&[][..], 1, true, true),
        (&[1][..], 1, true, true),
        (&[0, 0][..], 1, true, true),
        (&[0, 2][..], 1, true, true),
        (&[0, 1, 2][..], 1, true, true),
        (&[0, 1][..], 1, true, false),
        (&[0, 1][..], 1, false, true),
    ] {
        if !basic(ids, cells, psci, method).rejected() {
            return false;
        }
    }
    for case in 0..20 {
        if !invalid_cpu(case) {
            return false;
        }
    }
    for case in 0..18 {
        if !invalid_psci(case) {
            return false;
        }
    }
    for case in 0..11 {
        if !invalid_structure(case) {
            return false;
        }
    }
    for (offset, value) in [
        (0, 0),
        (4, 39),
        (4, 0x200001),
        (8, 65),
        (8, u32::MAX),
        (12, START as u32),
        (16, 41),
        (16, START as u32),
        (20, 16),
        (20, 18),
        (24, 17),
        (28, 1),
        (32, u32::MAX),
        (36, u32::MAX),
    ] {
        if !invalid_header(offset, value) {
            return false;
        }
    }
    true
}
