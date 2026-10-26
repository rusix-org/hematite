#!/usr/bin/env python3
"""Compile actual startup-input clients and reject forged values or escaped borrows."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = """#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::startup_input::{Input, View, Error, decode, MAX_INPUT_SIZE, EXTENSION_OFFSET,
    EXTENSION_SIZE, HEADER_SIZE, PAYLOAD_OFFSET};
"""
PROBES = {
    'owned_source_independence': ("""pub fn client() -> Input {
        let mut source = [7u8; MAX_INPUT_SIZE];
        let input = Input::from_bytes(&source).unwrap();
        source.fill(9); drop(source); input
    }""", None),
    'move_encode_drop_then_decode': ("""pub fn client<'a>(source: &[u8], destination: &'a mut [u8]) -> View<'a> {
        let input = Input::from_bytes(source).unwrap(); let moved = input;
        moved.encode_into(destination).unwrap(); drop(moved); decode(destination).unwrap()
    }""", None),
    'view_copy_and_payload_borrow': ("""pub fn client(extension: &[u8]) -> &[u8] {
        let view = decode(extension).unwrap(); let copied = view;
        let _ = view.is_present(); copied.bytes()
    }""", None),
    'constants_absent_and_errors': ("""pub fn client(destination: &mut [u8]) -> Result<(), Error> {
        let input = Input::absent(); let _ = input.as_bytes();
        let _ = [MAX_INPUT_SIZE, EXTENSION_OFFSET, EXTENSION_SIZE, HEADER_SIZE, PAYLOAD_OFFSET];
        let _ = [Error::Length, Error::Size, Error::Magic, Error::Version, Error::Header, Error::Reserved, Error::Padding];
        input.encode_into(destination)
    }""", None),
    'forge_input': ('pub fn client(input: Input) { let _ = Input { ..input }; }', 'E0451'),
    'forge_view': ("pub fn client(view: View<'_>) { let _ = View { ..view }; }", 'E0451'),
    'clone_input': ('pub fn client(input: Input) { let _ = input.clone(); }', 'E0599'),
    'copy_input': ('pub fn client(input: Input) { let moved = input; let _ = input.as_bytes(); drop(moved); }', 'E0382'),
    'input_bytes_escape_owner': ("""pub fn client() -> &'static [u8] {
        let input = Input::from_bytes(b"owned").unwrap(); input.as_bytes().unwrap()
    }""", 'E0515'),
    'view_escape_extension': ("""pub fn client() -> View<'static> {
        let extension = [0u8; EXTENSION_SIZE]; decode(&extension).unwrap()
    }""", 'E0515'),
    'view_bytes_escape_extension': ("""pub fn client() -> &'static [u8] {
        let extension = [0u8; EXTENSION_SIZE]; decode(&extension).unwrap().bytes()
    }""", 'E0515'),
    'mutate_input_while_borrowed': ("""pub fn client(mut input: Input) -> usize {
        let bytes = input.as_bytes().unwrap(); input = Input::absent(); bytes.len()
    }""", 'E0506'),
    'mutate_extension_while_view_live': ("""pub fn client(extension: &mut [u8]) -> usize {
        let view = decode(extension).unwrap(); extension[0] = 0; view.bytes().len()
    }""", 'E0506'),
    'mutate_view_payload': ("pub fn client(view: View<'_>) { view.bytes()[0] = 0; }", 'E0594'),
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
    sources = [root / ('crates/hm-kernel/src/' + name + '.rs') for name in ('startup_input',)] + [Path(__file__).resolve()]
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
              'scope': 'actual owned startup input and immutable extension view lifetimes; no runtime integration'}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
