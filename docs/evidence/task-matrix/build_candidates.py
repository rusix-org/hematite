"""Serial candidate compilation and emitted-code capture; never starts a guest."""
import gzip
import hashlib
import json
from pathlib import Path
import subprocess
import sys

output = Path(sys.argv[1])
output.mkdir(parents=True, exist_ok=False)
prior = json.loads(Path('docs/evidence/startup-input-stage-b/third-candidates/inputs.json').read_text())
runtime = prior['aarch64-startup-input-test']['command'].copy()
runtime[runtime.index('--features') + 1] = 'runtime-tests'
prior['aarch64-runtime-test'] = {'command': runtime}
task = runtime.copy()
task[task.index('--features') + 1] = 'task-tests'
prior['aarch64-task-test'] = {'command': task}
profiles = sys.argv[2:] or list(prior)
inputs = {}
sources = [Path(__file__), Path('Cargo.lock'), Path('Cargo.toml'), Path('rust-toolchain.toml'), Path('.cargo/config.toml')]
sources += [p for p in Path('crates').rglob('*') if p.is_file()]
def sha(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()
(output / 'sources.json').write_text(json.dumps({str(p): sha(p) for p in sorted(sources)}, indent=2)+'\n')
for profile in profiles:
    command = prior[profile]['command'].copy()
    directory = Path('target/group-stage-b') / output.name / profile
    command[command.index('--target-dir')+1] = str(directory)
    (output / (profile+'.command.json')).write_text(json.dumps(command, indent=2)+'\n')
    with (output / (profile+'.build.log')).open('wb') as log:
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, timeout=180)
    item = {'command': command, 'returncode': result.returncode}
    inputs[profile] = item
    target = command[command.index('--target')+1]
    elf = directory / target / 'release/hematite'
    if result.returncode == 0:
        assembly = list((elf.parent / 'deps').glob('hematite-*.s'))
        if len(assembly) != 1:
            raise RuntimeError('Expected one emitted assembly: '+profile)
        item.update(elf=str(elf), elf_sha256=sha(elf), asm=str(assembly[0]), asm_sha256=sha(assembly[0]))
        with gzip.open(output / (profile+'.s.gz'), 'wb') as log:
            log.write(assembly[0].read_bytes())
        with (output / (profile+'.symbols.log')).open('wb') as log:
            subprocess.run(['readelf', '-sW', str(elf)], stdout=log, check=True)
    (output / 'inputs.json').write_text(json.dumps(inputs, indent=2)+'\n')
    print(profile, result.returncode, flush=True)
    if result.returncode:
        sys.exit(result.returncode)
