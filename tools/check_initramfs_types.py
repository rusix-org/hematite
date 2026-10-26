#!/usr/bin/env python3
"""Compile actual initramfs clients and reject forged entries or escaped borrows."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = '''#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::initramfs::{Archive, Entry, Kind};
'''
PROBES = {
    'valid_parse_lookup': ('''pub fn client(input: &[u8]) -> Option<usize> {
        let archive = Archive::parse(input).ok()?;
        let _ = (archive.len(), archive.is_empty());
        for entry in archive.entries() { let _ = (entry.path(), entry.kind(), entry.permissions(), entry.bytes()); }
        Some(archive.get("bin/hello")?.bytes().len())
    }''', None),
    'valid_entry_after_archive_drop': ('''pub fn client(input: &[u8]) -> Option<Entry<'_>> {
        let archive = Archive::parse(input).ok()?;
        let entry = archive.get("bin/hello")?; drop(archive); Some(entry)
    }''', None),
    'valid_elf_composition': ('''pub fn client(input: &[u8]) -> Option<hm_kernel::elf::Image<'_>> {
        let archive = Archive::parse(input).ok()?;
        let entry = archive.get("bin/hello")?;
        if entry.kind() != Kind::File { return None; }
        hm_kernel::elf::Image::parse(entry.bytes()).ok()
    }''', None),
    'valid_entry_copy': ('''pub fn client(entry: Entry<'_>) -> usize {
        let copied = entry; entry.bytes().len() + copied.bytes().len()
    }''', None),
    'forge_archive': ('''pub fn client(archive: Archive<'_>) { let _ = Archive { ..archive }; }''', 'E0451'),
    'forge_entry': ('''pub fn client(entry: Entry<'_>) { let _ = Entry { ..entry }; }''', 'E0451'),
    'archive_outlive_input': ('''pub fn client() -> Archive<'static> {
        let bytes = [0u8; 128]; Archive::parse(&bytes).unwrap()
    }''', 'E0515'),
    'entry_outlive_input': ('''pub fn client() -> Entry<'static> {
        let bytes = [0u8; 128]; Archive::parse(&bytes).unwrap().get("bin/hello").unwrap()
    }''', 'E0515'),
    'mutate_input_while_entry_live': ('''pub fn client(input: &mut [u8]) -> usize {
        let archive = Archive::parse(input).unwrap(); let entry = archive.get("bin/hello").unwrap();
        drop(archive); input[0] = 0; entry.bytes().len()
    }''', 'E0506'),
    'mutate_payload': ('''pub fn client(entry: Entry<'_>) { entry.bytes()[0] = 0; }''', 'E0594'),
    'mutate_path': ('''pub fn client(entry: Entry<'_>) { entry.path().as_bytes()[0] = b'x'; }''', 'E0594'),
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
    source = Path(__file__).resolve().parents[1] / 'crates/hm-kernel/src/initramfs.rs'
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
              'scope': 'actual safe initramfs entry/input lifetime encapsulation; no archive-backed execution'}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
