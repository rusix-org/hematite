#!/usr/bin/env python3
"""Compile actual native-policy clients and reject forged session state."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = '''#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::native::{Issuer, Session, Request, Outcome, Host, CopyError};
'''
PROBES = {
    'valid_issue_startup': ('''pub fn client() -> Option<[u8;64]> {
        let mut issuer = Issuer::new(); let session = issuer.issue(true).ok()?;
        let _ = (session.calls(), session.written(), session.may_resume(), session.termination());
        Some(session.startup())
    }''', None),
    'valid_dispatch_host': ('''struct Sink;
    impl Host for Sink {
        fn copy_from_user(&mut self, _:u64, out:&mut[u8]) -> Result<(), CopyError> { out.fill(7); Ok(()) }
        fn write_console(&mut self, _: &[u8]) {}
    }
    pub fn client(session: &mut Session, request: Request) -> Outcome {
        session.dispatch(request, &mut Sink)
    }''', None),
    'valid_move_session': ('''pub fn client(mut issuer: Issuer) -> Option<Session> { issuer.issue(false).ok() }''', None),
    'forge_issuer': ('''pub fn client() { let _ = Issuer { next: 1 }; }''', 'E0451'),
    'forge_session': ('''pub fn client() { let _ = Session { console: Some(1), calls: 0, written: 0, stopped: None }; }''', 'E0451'),
    'issuer_clone': ('''pub fn client(i:&Issuer) { let _ = <Issuer as Clone>::clone(i); }''', 'E0277'),
    'session_clone': ('''pub fn client(s:&Session) { let _ = <Session as Clone>::clone(s); }''', 'E0277'),
    'change_generation': ('''pub fn client(i:&mut Issuer) { i.next = 1; }''', 'E0616'),
    'change_grant': ('''pub fn client(s:&mut Session) { s.console = Some(1); }''', 'E0616'),
    'change_calls': ('''pub fn client(s:&mut Session) { s.calls = 0; }''', 'E0616'),
    'change_budget': ('''pub fn client(s:&mut Session) { s.written = 0; }''', 'E0616'),
    'revive_session': ('''pub fn client(s:&mut Session) { s.stopped = None; }''', 'E0616'),
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
    source = Path(__file__).resolve().parents[1] / 'crates/hm-kernel/src/native.rs'
    (output / 'input.json').write_text(json.dumps({'library': str(library),
        'library_sha256': hashlib.sha256(library.read_bytes()).hexdigest(), 'target': args.target,
        'source_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'expected_probes': list(PROBES)}, indent=2) + '\n')
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
              'scope': 'actual safe native-policy type encapsulation; no hardware authority or EL0 execution'}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
