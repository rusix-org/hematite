#!/usr/bin/env python3
"""Compile actual input codec and owned READ clients; no hardware authority is exercised."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = """#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::native::{Issuer, Session, InputAdmission, ReadAdmission, ParkedRead, ReadError, Request, Outcome, Host, CopyError};
use hm_kernel::interactive_wire::{self, Grant};
struct Empty;
impl Host for Empty {
    fn copy_from_user(&mut self, _: u64, _: &mut [u8]) -> Result<(), CopyError> { Err(CopyError) }
    fn write_console(&mut self, _: &[u8]) {}
}
fn request() -> Request { Request { version: 1, operation: 6, argument: 0x100000005, address: 0, length: 1 } }
"""
PROBES = {
    'grant_owned_copy': ("""pub fn client() -> Grant {
        let mut bytes = [0; interactive_wire::GRANT_SIZE];
        let grant = Grant::new(0x100000005).unwrap();
        grant.encode_into(&mut bytes).unwrap();
        let decoded = Grant::decode(&bytes).unwrap();
        bytes.fill(0); let copied = decoded; let _ = decoded.input_token(); copied
    }""", None),
    'guard_outlives_host_and_request': ("""pub fn client() -> ReadAdmission {
        let mut issuer = Issuer::new(); let mut host = Empty; let request = request();
        match issuer.issue_interactive().unwrap().admit_input(request, &mut host) {
            InputAdmission::Read(guard) => guard, _ => panic!("no read"),
        }
    }""", None),
    'park_and_finish_owned': ("""pub fn client(guard: ReadAdmission) -> (Session, Outcome) {
        let parked = match guard.park() { Ok(p) => p, Err(_) => panic!("capacity") };
        match parked.finish(1) { Ok(pair) => pair, Err(_) => panic!("count") }
    }""", None),
    'invalid_finish_retains_owner': ("""pub fn client(parked: ParkedRead) -> ParkedRead {
        match parked.finish(0) { Err(p) => p, Ok(_) => panic!("invalid") }
    }""", None),
    'reject_returns_session': ("pub fn client(guard: ReadAdmission) -> Session { guard.reject(ReadError::Address).0 }", None),
    'forge_grant': ('pub fn client(value: Grant) { let _ = Grant { ..value }; }', 'E0451'),
    'forge_session': ('pub fn client(value: Session) { let _ = Session { ..value }; }', 'E0451'),
    'forge_guard': ('pub fn client(value: ReadAdmission) { let _ = ReadAdmission { ..value }; }', 'E0451'),
    'forge_parked': ('pub fn client(value: ParkedRead) { let _ = ParkedRead { ..value }; }', 'E0451'),
    'clone_guard': ('pub fn client(value: ReadAdmission) { let _ = value.clone(); }', 'E0599'),
    'clone_parked': ('pub fn client(value: ParkedRead) { let _ = value.clone(); }', 'E0599'),
    'copy_guard': ('fn copy<T: Copy>() {} pub fn client() { copy::<ReadAdmission>(); }', 'E0277'),
    'copy_parked': ('fn copy<T: Copy>() {} pub fn client() { copy::<ParkedRead>(); }', 'E0277'),
    'reuse_after_reject': ('pub fn client(value: ReadAdmission) { let _ = value.reject(ReadError::Handle); let _ = value.calls(); }', 'E0382'),
    'reuse_after_park': ('pub fn client(value: ReadAdmission) { let _ = value.park(); let _ = value.calls(); }', 'E0382'),
    'finish_twice': ('pub fn client(value: ParkedRead) { let _ = value.finish(1); let _ = value.finish(1); }', 'E0382'),
    'reuse_session_after_admission': ('pub fn client(session: Session) { let _ = session.admit_input(request(), &mut Empty); let _ = session.calls(); }', 'E0382'),
    'extract_guard_session': ('pub fn client(value: ReadAdmission) { let _ = value.session; }', 'E0616'),
    'extract_parked_session': ('pub fn client(value: ParkedRead) { let _ = value.session; }', 'E0616'),
    'mutate_profile': ('pub fn client(mut value: Session) { value.profile = value.profile; }', 'E0616'),
    'escape_guard_reference': ("pub fn client<'a>(value: ReadAdmission) -> &'a ReadAdmission { &value }", 'E0515'),
    'move_borrowed_parked': ('pub fn client(value: ParkedRead) { let borrow = &value; let _ = value.finish(1); let _ = borrow.calls(); }', 'E0505'),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--library', type=Path, required=True)
    parser.add_argument('--target', choices=['aarch64-unknown-none', 'x86_64-unknown-none'], required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    library, output = args.library.resolve(), args.output.resolve()
    if not library.is_file() or output.exists():
        parser.error('require built hm-kernel library and fresh evidence directory')
    output.mkdir(parents=True)
    root = Path(__file__).resolve().parents[1]
    sources = [root / ('crates/hm-kernel/src/' + name + '.rs') for name in ('native', 'task_wire', 'interactive_wire', 'startup_input', 'lib')] + [root / name for name in ('rust-toolchain.toml', 'Cargo.lock', 'Cargo.toml', 'crates/hm-kernel/Cargo.toml', 'docs/design/0031-input-safe-api.md', 'tools/check_frame_types.py', 'tools/check_mapping_types.py')] + [Path(__file__).resolve()]
    (output / 'input.json').write_text(json.dumps({'library': str(library),
        'library_sha256': hashlib.sha256(library.read_bytes()).hexdigest(), 'target': args.target,
        'sources': {str(source.relative_to(root)): hashlib.sha256(source.read_bytes()).hexdigest() for source in sources}, 'expected_probes': list(PROBES)}, indent=2) + '\n')
    results = {}
    for name, (body, rejection) in PROBES.items():
        client, metadata = output / f'{name}.rs', output / f'{name}.rmeta'
        client.write_text(PREAMBLE + body + '\n')
        command = ['rustc', '--edition=2024', '--crate-type=lib', '--emit=metadata', '--error-format=json',
                   '--target', args.target, '-C', 'panic=abort', '--extern', f'hm_kernel={library}',
                   '-L', f'dependency={library.parent / "deps"}', '-o', str(metadata), str(client)]
        code, diagnostics = run(command, output, name)
        passed = code == 0 and metadata.is_file() if rejection is None else (
            code is not None and code != 0 and rejected_only_for(diagnostics, rejection))
        results[name] = {'passed': passed, 'returncode': code, 'expected_rejection': rejection}
    result = {'passed': len(results) == len(PROBES) and all(r['passed'] for r in results.values()),
              'probes': results, 'target': args.target,
              'scope': 'actual Grant and owned READ policy boundary; no RX lease, writable user copy or wake authority'}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
