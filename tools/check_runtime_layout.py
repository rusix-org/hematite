#!/usr/bin/env python3
"""Extract actual ARM owner sizes through Rust, GNU assembler and object bytes."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

from build_initramfs import run

ROOT = Path(__file__).resolve().parents[1]
MODULES = ('frames', 'cpu_local', 'threads', 'preempt', 'secondary', 'user_space', 'process', 'process_group')


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--library', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output, library = args.output.resolve(), args.library.resolve()
    if output.exists() or not library.is_file():
        parser.error('require an existing hm-kernel library and a fresh output directory')
    output.mkdir(parents=True)
    source = ROOT / 'crates/hm-core/src'
    wrapper, sizes = output / 'hm_layout.rs', output / 'sizes.rs'
    # The local module copy adds a read-only witness inside the original privacy
    # boundary. Its implementation bytes otherwise remain identical.
    original_group = (source / 'process_group.rs').read_text()
    old_path = '#[path = "process_task.rs"]'
    new_path = f'#[path = {json.dumps(str(source / "process_task.rs"))}]'
    if original_group.count(old_path) != 1:
        parser.error('expected one exact task module path in original Group')
    appendix = """
pub const TASK_LAYOUT: [usize; 6] = [
    core::mem::size_of::<Image<'static>>(),
    core::mem::size_of::<Image<'static>>().div_ceil(4096),
    core::mem::size_of::<task::Authority<'static>>(),
    core::mem::size_of::<task::Authority<'static>>().div_ceil(4096),
    core::mem::size_of::<Group<'static, 'static>>(),
    task::METADATA_FRAMES,
];
"""
    group_copy = output / 'process_group.rs'
    group_copy.write_text(original_group.replace(old_path, new_path, 1) + appendix)
    transformed = group_copy.read_text()
    if not transformed.endswith(appendix) or transformed[:-len(appendix)].replace(new_path, old_path, 1) != original_group:
        parser.error('generated Group differs beyond the declared witness transformation')
    wrapper.write_text('#![no_std]\n#![deny(unsafe_op_in_unsafe_fn)]\n' + ''.join(
        f'#[path={json.dumps(str(group_copy if name == "process_group" else source / (name + ".rs")))}] mod {name};\n' for name in MODULES) +
        'pub use process::Process;\npub use user_space::Space;\npub use frames::KernelBox;\npub use process_group::TASK_LAYOUT;\n')
    sizes.write_text('''#![no_std]
#![forbid(unsafe_code)]
use hm_layout::{Process,Space,KernelBox,TASK_LAYOUT};
#[used]
pub static LAYOUT: [usize;11] = [
    core::mem::size_of::<Process<'static>>(),
    core::mem::size_of::<Space<'static>>(),
    core::mem::size_of::<KernelBox<'static,Process<'static>>>(),
    core::mem::size_of::<Option<KernelBox<'static,Process<'static>>>>(),
    core::mem::size_of::<Process<'static>>().div_ceil(4096),
    TASK_LAYOUT[0], TASK_LAYOUT[1], TASK_LAYOUT[2],
    TASK_LAYOUT[3], TASK_LAYOUT[4], TASK_LAYOUT[5],
];
''')
    paths = [source / (name + '.rs') for name in MODULES]
    paths += [source / name for name in ('process_layout.rs', 'process_task.rs', 'process_task_turn.rs',
              'process-aarch64.S', 'user-space-aarch64.S',
              'threads-aarch64.S', 'preempt-aarch64.S', 'preempt-probe-aarch64.S', 'secondary-aarch64.S')]
    paths += [wrapper, sizes, group_copy, library, ROOT / 'rust-toolchain.toml', ROOT / 'Cargo.lock', Path(__file__).resolve()]
    (output / 'input.json').write_text(json.dumps({'sources': {str(path): sha(path) for path in paths},
        'group_transformation': {'replace_once': [old_path, new_path], 'append': appendix},
        'scope': 'actual ARM compiler layout only; no stack or allocation execution'}, indent=2) + '\n')
    result = {'passed': False}
    try:
        run(['rustc', '--version'], output, 'toolchain')
        dependencies = library.parent if library.parent.name == 'deps' else library.parent / 'deps'
        base = ['rustc', '--edition=2024', '--target', 'aarch64-unknown-none', '-C', 'panic=abort',
                '--cfg', 'feature="secondary-tests"', '--cfg', 'feature="smp-tests"',
                '--extern', f'hm_kernel={library}', '-L', f'dependency={dependencies}']
        compiled = output / 'libhm_layout.rlib'
        run(base + ['--crate-type=rlib', '--crate-name=hm_layout', str(wrapper), '-o', str(compiled)], output, 'library')
        assembly = output / 'sizes.s'
        run(base + ['--crate-type=lib', '--emit=asm', '-C', 'opt-level=2', '--extern', f'hm_layout={compiled}',
                    str(sizes), '-o', str(assembly)], output, 'sizes')
        sections = re.findall(r'\.section\s+(\.rodata\.[^\s,]*LAYOUT[^\s,]*)', assembly.read_text())
        if len(sections) != 1:
            raise ValueError('Expected exactly one retained layout section')
        obj = output / 'sizes.o'
        run(['aarch64-linux-gnu-as', str(assembly), '-o', str(obj)], output, 'assemble')
        binary = output / 'layout.bin'
        run(['aarch64-linux-gnu-objcopy', '--dump-section', sections[0] + '=' + str(binary),
             str(obj), str(output / 'sizes-extracted.o')], output, 'extract')
        raw = binary.read_bytes()
        if len(raw) != 88:
            raise ValueError('Layout witness must contain exactly eleven u64 values')
        values = struct.unpack('<11Q', raw)
        if not (0 < values[0] <= 4 * 4096 and values[4] == (values[0] + 4095) // 4096):
            raise ValueError('Process size violates reviewed box bound')
        if values[6] != (values[5] + 4095) // 4096 or values[8] != (values[7] + 4095) // 4096 or values[10] != 2 * values[6] + values[8]:
            raise ValueError('Task layout metadata charge differs from actual compiler sizes')
        result = {'passed': True, 'fields': dict(zip(('process_bytes', 'space_bytes', 'box_bytes',
                  'optional_box_bytes', 'box_backing_pages'), values[:5])),
                  'task_fields': dict(zip(('image_bytes', 'image_pages', 'authority_bytes',
                    'authority_pages', 'group_bytes', 'metadata_pages'), values[5:])), 'assembly_sha256': sha(assembly),
                  'bytes_sha256': sha(binary), 'scope': 'current ARM compiler layout constants only; not stack usage or runtime allocation proof'}
    except (OSError, ValueError, RuntimeError) as error:
        result['error'] = str(error)
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
