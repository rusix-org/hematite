"""Three fixed same-binary trials; failures remain failures, no retry-to-pass policy."""
import hashlib
import json
from pathlib import Path
import resource
import sys
import time
sys.path.insert(0, 'tools')
import test_task_cases as cases
import test_runtime as runtime

out = Path(sys.argv[1]); out.mkdir(parents=True, exist_ok=False)
kernel = Path('target/aarch64/task-test/aarch64-unknown-none/release/hematite').resolve()
prior = Path('target/evidence/20261027T015846-174706867-aarch64/task-cases')
original = json.loads((prior / 'tasks-128m/input.json').read_text())
metadata = json.loads((prior / 'metadata.json').read_text())
sha = hashlib.sha256(kernel.read_bytes()).hexdigest()
assert sha == original['kernel_sha256'], 'Require the exact failed kernel'
fixtures, archives = cases.builder.verify(Path('crates/hm-core/fixtures/task-cases/manifest.json'),
    Path('crates/hm-core/fixtures/task-cases-initramfs.cpio'), Path('crates/hm-core/fixtures/task-cases-large-initramfs.cpio'),
    prior.parent / 'runtime-layout/result.json', out, kernel)
symbols = metadata['symbols']; layout = metadata['layout']['result']
fixups = runtime.fixup_table(kernel.read_bytes(), symbols)
(out / 'input.json').write_text(json.dumps(dict(kernel_sha256=sha, original_input=original,
    trials=3, memory_mb=128, qemu_changes=[], budget_changes=[],
    sources={str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in
        [Path(__file__), Path('tools/test_task_cases.py'), Path('tools/test_runtime.py')]}), indent=2)+'\n')
def snapshot():
    result = {}
    for name in ('/sys/fs/cgroup/cpu.stat', '/proc/pressure/cpu', '/proc/pressure/memory'):
        p = Path(name)
        result[name] = p.read_text() if p.exists() else None
    return result
results = []
for index in range(3):
    before, usage, start = snapshot(), resource.getrusage(resource.RUSAGE_CHILDREN), time.monotonic()
    result = cases.run_profile(out / f'trial-{index+1}', kernel, fixtures, symbols, fixups, 128, layout,
        dict(original_kernel_sha256=sha, trial=index+1, archives=archives))
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    results.append(dict(trial=index+1, passed=result['passed'], errors=result['errors'],
        wall_seconds=time.monotonic()-start, qemu_user_seconds=after.ru_utime-usage.ru_utime,
        qemu_system_seconds=after.ru_stime-usage.ru_stime, before=before, after=snapshot()))
    (out / 'results.json').write_text(json.dumps(results, indent=2)+'\n')
    print(json.dumps({k:v for k,v in results[-1].items() if k not in ('before','after')}), flush=True)
# Investigation completion is separate from whether its three guests passed.
(out / 'completed.json').write_text(json.dumps({'completed_trials':len(results), 'passed_trials':sum(r['passed'] for r in results),
    'scope':'unchanged-kernel reproducibility investigation; original full failure remains unresolved'}, indent=2)+'\n')
