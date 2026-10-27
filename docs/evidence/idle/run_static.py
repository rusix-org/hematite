"""Preserve serial static-check commands/results without starting guests."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

output = Path(sys.argv[1])
output.mkdir(parents=True, exist_ok=False)
commands = [('fmt', ['cargo', 'fmt', '--all', '--', '--check']),
    ('safety', ['python3', 'tools/check_safety.py']),
    ('repository', ['python3', 'tools/check_repository.py'])]
for target in ('aarch64-unknown-none', 'x86_64-unknown-none'):
    commands.append((target, ['cargo', 'clippy', '--workspace', '--all-features',
        '--locked', '--offline', '--target', target, '--target-dir',
        'target/idle-static/' + output.name + '/' + target, '--', '-D', 'warnings']))
sources = [path for path in Path('crates').rglob('*') if path.is_file()]
(output / 'sources.json').write_text(json.dumps({str(path): hashlib.sha256(path.read_bytes()).hexdigest()
    for path in sorted(sources)}, indent=2) + '\n')
environment = os.environ.copy()
environment['CARGO_BUILD_JOBS'] = '2'
environment.pop('HM_TASK_NEGATIVE', None)
environment.pop('HM_IDLE_PROFILE', None)
results = []
for name, command in commands:
    with (output / (name + '.log')).open('wb') as log:
        result = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT,
            env=environment, timeout=180)
    results.append({'name': name, 'command': command, 'returncode': result.returncode})
    passed = len(results) == len(commands) and all(item['returncode'] == 0 for item in results)
    (output / 'result.json').write_text(json.dumps({'passed': passed, 'results': results}, indent=2) + '\n')
    print(name, result.returncode, flush=True)
    if result.returncode:
        sys.exit(result.returncode)
