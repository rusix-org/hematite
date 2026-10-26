#!/usr/bin/env python3
"""Compile actual initramfs clients and reject forged entries or escaped borrows."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = """#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::{boot_image::{admit, AdmissionError}, elf::Image};
"""
PROBES = {
    "valid_image": ("pub fn client(input: &[u8]) -> Result<Image<'_>, AdmissionError> { admit(input, \"bin/hello\") }", None),
    'temporary_path': ("pub fn client(input: &[u8]) -> Image<'_> { let path = *b\"bin/hello\"; admit(input, core::str::from_utf8(&path).unwrap()).unwrap() }", None),
    'image_after_local_plan': ("pub fn client(input: &[u8]) -> Image<'_> { let image = admit(input, \"bin/hello\").unwrap(); image }", None),
    'escape_input': ("pub fn client() -> Image<'static> { let bytes = [0u8;128]; admit(&bytes, \"bin/hello\").unwrap() }", 'E0515'),
    'mutate_live_input': ('pub fn client(input: &mut [u8]) -> u64 { let image = admit(input, "bin/hello").unwrap(); input[0] = 0; image.entry() }', 'E0506'),
    'forge_image': ("pub fn client(image: Image<'_>) { let _ = Image { ..image }; }", 'E0451'),
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
    sources = [root / ('crates/hm-kernel/src/' + name + '.rs') for name in ('boot_image', 'initramfs', 'elf')] + [Path(__file__).resolve()]
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
              'scope': 'actual safe boot image borrowed-input encapsulation; no execution'}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
