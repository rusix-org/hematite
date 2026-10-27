"""Serial compilation and emitted-code capture only; never starts a guest."""
import gzip
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

output = Path(sys.argv[1])
output.mkdir(parents=True, exist_ok=False)
prior = json.loads(Path('docs/evidence/rw-copy/rw-copy-candidates-first/inputs.json').read_text())
template = prior['aarch64-rw-copy-test']['command']
for variant in ('short', 'wall'):
    command = template.copy()
    command[command.index('--features') + 1] = 'idle-tests'
    prior['aarch64-idle-' + variant] = {'command': command,
        'profile_environment': {'HM_IDLE_PROFILE': variant}}
profiles = sys.argv[2:] or list(prior)
sources = [Path(__file__), Path('Cargo.lock'), Path('Cargo.toml'),
    Path('rust-toolchain.toml'), Path('.cargo/config.toml')]
sources += [path for path in Path('crates').rglob('*') if path.is_file()]

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

(output / 'sources.json').write_text(json.dumps({str(path): sha(path) for path in sorted(sources)}, indent=2) + '\n')
inputs = {}
for profile in profiles:
    command = prior[profile]['command'].copy()
    environment = os.environ.copy()
    environment.pop('HM_TASK_NEGATIVE', None)
    environment.pop('HM_IDLE_PROFILE', None)
    selected = prior[profile].get('profile_environment', {})
    for name, value in selected.items():
        if value is not None:
            environment[name] = value
    directory = Path('target/idle-candidates') / output.name / profile
    command[command.index('--target-dir') + 1] = str(directory)
    (output / (profile + '.command.json')).write_text(json.dumps(command, indent=2) + '\n')
    with (output / (profile + '.build.log')).open('wb') as log:
        result = subprocess.run(command, env=environment, stdout=log, stderr=subprocess.STDOUT, timeout=180)
    item = {'command': command, 'returncode': result.returncode, 'profile_environment': selected}
    inputs[profile] = item
    target = command[command.index('--target') + 1]
    elf = directory / target / 'release/hematite'
    if result.returncode == 0:
        assembly = list((elf.parent / 'deps').glob('hematite-*.s'))
        if len(assembly) != 1:
            raise RuntimeError('Expected one emitted assembly: ' + profile)
        item.update(elf=str(elf), elf_sha256=sha(elf), asm=str(assembly[0]), asm_sha256=sha(assembly[0]))
        with gzip.open(output / (profile + '.s.gz'), 'wb') as log:
            log.write(assembly[0].read_bytes())
        with (output / (profile + '.symbols.log')).open('wb') as log:
            subprocess.run(['readelf', '-sW', str(elf)], stdout=log, check=True)
    (output / 'inputs.json').write_text(json.dumps(inputs, indent=2) + '\n')
    print(profile, result.returncode, flush=True)
    if result.returncode:
        sys.exit(result.returncode)
