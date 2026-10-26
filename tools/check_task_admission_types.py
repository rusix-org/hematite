#!/usr/bin/env python3
"""Compile actual owned Session clients and reject forged, copied or reused owners."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = """#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::native::{Session, Admission, TaskAdmission, ParkedSession, Request, Response, Status, Host, CopyError};
struct Empty;
impl Host for Empty {
    fn copy_from_user(&mut self, _: u64, _: &mut [u8]) -> Result<(), CopyError> { Err(CopyError) }
    fn write_console(&mut self, _: &[u8]) {}
}
fn request() -> Request { Request { version: 1, operation: 4, argument: 0, address: 0, length: 0 } }
fn response() -> Response { Response { status: Status::Ok, written: 0 } }
"""
PROBES = {
    'guard_outlives_host': ("""pub fn client(session: Session) -> TaskAdmission {
        let mut host = Empty;
        match session.admit_task(request(), &mut host) {
            Admission::Task(guard) => guard, _ => panic!("no task"),
        }
    }""", None),
    'parked_session_is_owned': ("""pub fn client(guard: TaskAdmission) -> ParkedSession {
        match guard.park_wait() { Ok(parked) => parked, Err(_) => panic!("not wait") }
    }""", None),
    'reply_returns_original_owner': ("""pub fn client(guard: TaskAdmission) -> Session {
        let (session, _) = guard.reply(response()); session
    }""", None),
    'forge_guard': ('pub fn client(value: TaskAdmission) { let _ = TaskAdmission { ..value }; }', 'E0451'),
    'forge_parked': ('pub fn client(value: ParkedSession) { let _ = ParkedSession { ..value }; }', 'E0451'),
    'clone_guard': ('pub fn client(value: TaskAdmission) { let _ = value.clone(); }', 'E0599'),
    'clone_parked': ('pub fn client(value: ParkedSession) { let _ = value.clone(); }', 'E0599'),
    'reuse_after_reply': ('pub fn client(value: TaskAdmission) { let _ = value.reply(response()); let _ = value.calls(); }', 'E0382'),
    'reuse_after_park': ('pub fn client(value: TaskAdmission) { let _ = value.park_wait(); let _ = value.calls(); }', 'E0382'),
    'wake_twice': ('pub fn client(value: ParkedSession) { let _ = value.wake(response()); let _ = value.wake(response()); }', 'E0382'),
    'reuse_session_after_admission': ('pub fn client(session: Session) { let _ = session.admit_task(request(), &mut Empty); let _ = session.calls(); }', 'E0382'),
    'extract_guard_session': ('pub fn client(value: TaskAdmission) { let _ = value.session; }', 'E0616'),
    'extract_parked_session': ('pub fn client(value: ParkedSession) { let _ = value.session; }', 'E0616'),
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
    sources = [root / ('crates/hm-kernel/src/' + name + '.rs') for name in ('native', 'task_wire')] + [Path(__file__).resolve()]
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
              'scope': 'actual owned Session admission boundary; no core integration or wake authority'}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
