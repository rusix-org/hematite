"""Strict fresh ten-profile run against the reviewed compiled candidates."""
import json
from pathlib import Path
import subprocess
import sys
sys.path.insert(0, 'tools')
from build_task_negative_cases import PROFILES
out = Path(sys.argv[1]); out.mkdir(parents=True, exist_ok=False)
inputs = json.loads(Path('docs/evidence/task-negative/candidates-first/inputs.json').read_text())
results = {}
for case in PROFILES:
    command = ['python3', 'tools/test_task_negative.py', '--case', case, '--kernel',
               inputs['aarch64-negative-' + case]['elf'], '--fixtures',
               'crates/hm-core/fixtures/task-negative-cases/manifest.json', '--archive',
               'crates/hm-core/fixtures/task-negative-initramfs.cpio', '--layout',
               'docs/evidence/task-negative-layout-fresh/result.json', '--output', str(out / case)]
    (out / (case + '.command.json')).write_text(json.dumps(command, indent=2) + '\n')
    with (out / (case + '.output.log')).open('w') as log:
        run = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT)
    result = json.loads((out / case / 'result.json').read_text())
    results[case] = dict(returncode=run.returncode, passed=result.get('passed'), errors=result.get('errors', result.get('error')))
    (out / 'results.json').write_text(json.dumps(results, indent=2) + '\n')
    print(case, results[case], flush=True)
    if run.returncode != 0 or not result.get('passed'):
        raise SystemExit(1)
(out / 'result.json').write_text(json.dumps({'passed': len(results) == len(PROFILES), 'profiles': results}, indent=2)+'\n')
