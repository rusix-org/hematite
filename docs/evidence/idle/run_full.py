"""Capture the full ARM/x86 suite, including the strict real-300s idle gate."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import signal
import sys
import tarfile
import time

out = Path(sys.argv[1])
out.mkdir(exist_ok=False)

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def save(name, value):
    (out / name).write_text(json.dumps(value, indent=2) + '\n')

paths = [path for root in ('crates', 'tools', '.cargo') for path in Path(root).rglob('*')
    if path.is_file() and '__pycache__' not in path.parts]
paths += [Path(path) for path in ('Cargo.toml', 'Cargo.lock', 'Makefile', 'rust-toolchain.toml', 'Dockerfile')]
paths.append(Path(__file__))
sources = {str(path): digest(path) for path in sorted(paths)}
command = ['make', 'test-all']
save('inputs.json', {'command': command, 'timeout_seconds': 1800,
    'head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(), 'sources': sources})
before = set(Path('target/evidence').iterdir())
start = time.monotonic()
timed_out = False
with (out / 'output.log').open('w') as log:
    process = subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    try:
        returncode = process.wait(timeout=1800)
    except subprocess.TimeoutExpired:
        timed_out = True
        os.killpg(process.pid, signal.SIGTERM)
        try:
            returncode = process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            returncode = process.wait(timeout=5)
changed = [name for name, sha in sources.items() if not Path(name).is_file() or digest(Path(name)) != sha]
passed = returncode == 0 and not timed_out and not changed
save('result.json', {'passed': passed, 'returncode': returncode, 'timed_out': timed_out,
    'elapsed_seconds': time.monotonic() - start, 'changed_inputs': changed})
archives = []
for directory in sorted(set(Path('target/evidence').iterdir()) - before):
    if directory.is_dir():
        archive = out / (directory.name + '.tar.gz')
        with tarfile.open(archive, 'w:gz') as tar:
            tar.add(directory, arcname=directory.name)
        archives.append({'source': str(directory), 'archive': str(archive), 'sha256': digest(archive)})
save('archives.json', archives)
print((out / 'result.json').read_text())
raise SystemExit(0 if passed else 1)
