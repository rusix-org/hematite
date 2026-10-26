"""Read concrete current ARM owner layouts from compiler-emitted constants; no guest."""
from pathlib import Path
import hashlib
import json
import subprocess
import sys
sys.path.insert(0, str(Path('tools').resolve()))
from check_frame_types import run
out = Path('docs/evidence/process-budget-stage-a/layout')
out.mkdir()
root = Path('crates/hm-core/src').resolve()
modules = ('frames','cpu_local','threads','preempt','secondary','user_space','process')
wrapper = out / 'hm_layout.rs'
wrapper.write_text('#![no_std]\n#![deny(unsafe_op_in_unsafe_fn)]\n' + ''.join(
    f'#[path={json.dumps(str(root / (name + ".rs")))}] mod {name};\n' for name in modules) +
    'pub use user_space::Space;\npub use frames::KernelBox;\npub use process::Process;\n')
library = Path('target/aarch64/boot-test/aarch64-unknown-none/release/libhm_kernel.rlib').resolve()
base = ['rustc','--edition=2024','--target','aarch64-unknown-none','-C','panic=abort',
    '--cfg','feature="startup-input-tests"','--cfg','feature="secondary-tests"','--cfg','feature="smp-tests"',
    '--extern',f'hm_kernel={library}','-L',f'dependency={library.parent / "deps"}']
compiled = out / 'libhm_layout.rlib'
code, _ = run(base + ['--crate-type=rlib','--crate-name=hm_layout',str(wrapper),'-o',str(compiled)],out,'library')
assert code == 0
source = out / 'sizes.rs'
source.write_text('''#![no_std]
#![forbid(unsafe_code)]
use hm_layout::{Process,Space,KernelBox};
pub fn process_size()->usize { core::mem::size_of::<Process<'static>>() }
pub fn space_size()->usize { core::mem::size_of::<Space<'static>>() }
pub fn box_size()->usize { core::mem::size_of::<KernelBox<'static,Process<'static>>>() }
pub fn slot_owner_size()->usize { core::mem::size_of::<Option<KernelBox<'static,Process<'static>>>>() }
pub fn owner_pages()->usize { core::mem::size_of::<Process<'static>>().div_ceil(4096) }
''')
asm = out / 'sizes.s'
code, _ = run(base + ['--crate-type=lib','--emit=asm','-C','opt-level=2','--extern',f'hm_layout={compiled}',
    str(source),'-o',str(asm)],out,'sizes')
assert code == 0
inputs = {str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [library,source,wrapper] +
    [root / (n+'.rs') for n in modules]}
(out / 'input.json').write_text(json.dumps(dict(sources=inputs,scope='compile-only actual ARM type sizes, not allocation or stack execution'),indent=2)+'\n')
print(asm.read_text())
