#!/usr/bin/env python3
"""Compile actual task-wire clients and reject bypasses of validated private state."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = """#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::task_wire::{Spawn, Grant, Error};
"""
PROBES = {
    'owned_copy_detaches_source': ("""pub fn client(bytes: &mut [u8]) -> Result<Spawn, Error> {
        let spawn = Spawn::decode(bytes)?; bytes.fill(0); let copy = spawn;
        let _ = spawn.input(); Ok(copy)
    }""", None),
    'grant_metadata_is_copy': ("""pub fn client(bytes: &[u8]) -> Result<(Grant, Grant), Error> {
        let grant = Grant::decode(bytes)?; Ok((grant, grant))
    }""", None),
    'forge_spawn': ('pub fn client(value: Spawn) { let _ = Spawn { ..value }; }', 'E0451'),
    'forge_grant': ('pub fn client(value: Grant) { let _ = Grant { ..value }; }', 'E0451'),
    'mutate_spawn': ('pub fn client(mut value: Spawn) { value.max_frames = 0; }', 'E0616'),
    'mutate_grant': ('pub fn client(mut value: Grant) { value.handle = 0; }', 'E0616'),
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
    sources = [root / ('crates/hm-kernel/src/' + name + '.rs') for name in ('task_wire',)] + [Path(__file__).resolve()]
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
              'scope': 'actual private task-wire metadata and owned copies; no hardware authority'}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
