#!/usr/bin/env python3
"""Capture the full regression command, inputs, output and new raw evidence."""
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile
import time

out = Path('docs/evidence/task-matrix-full-first')
out.mkdir(exist_ok=False)
def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()
def save(name, value):
    (out / name).write_text(json.dumps(value, indent=2) + '\n')
paths = [p for root in ('crates', 'tools', '.cargo') for p in Path(root).rglob('*')
         if p.is_file() and '__pycache__' not in p.parts]
paths += [Path(p) for p in ('Cargo.toml', 'Cargo.lock', 'Makefile', 'rust-toolchain.toml')]
sources = {str(p): digest(p) for p in sorted(paths)}
save('inputs.json', {'command': ['make', 'test-all'], 'head': subprocess.check_output(
    ['git', 'rev-parse', 'HEAD'], text=True).strip(), 'sources': sources})
before = set(Path('target/evidence').iterdir())
start = time.monotonic()
with (out / 'output.log').open('w') as log:
    run = subprocess.run(['make', 'test-all'], stdout=log, stderr=subprocess.STDOUT)
changed = [p for p, sha in sources.items() if digest(Path(p)) != sha]
save('result.json', {'returncode': run.returncode, 'elapsed_seconds': time.monotonic()-start,
                     'changed_inputs': changed})
archives = []
for directory in sorted(set(Path('target/evidence').iterdir()) - before):
    if directory.is_dir():
        archive = out / (directory.name + '.tar.gz')
        with tarfile.open(archive, 'w:gz') as tar:
            tar.add(directory, arcname=directory.name)
        archives.append({'source': str(directory), 'archive': str(archive), 'sha256': digest(archive)})
save('archives.json', archives)
print((out / 'result.json').read_text())
raise SystemExit(run.returncode or bool(changed))
