"""Three predeclared optimized trials, retaining each independent guest verdict."""
import hashlib
import json
from pathlib import Path
import resource
import sys
import time
sys.path.insert(0, 'tools')
import test_task_cases as cases
import test_runtime as runtime

out, inventory, witness = map(Path, sys.argv[1:])
out.mkdir(parents=True, exist_ok=False)
candidate = json.loads(inventory.read_text())['aarch64-task-test']
kernel = Path(candidate['elf']).resolve()
sha = hashlib.sha256(kernel.read_bytes()).hexdigest()
assert candidate['returncode'] == 0 and candidate['elf_sha256'] == sha
_, layout = runtime.verify_layout(witness.resolve())
fixtures, archives = cases.builder.verify(Path('crates/hm-core/fixtures/task-cases/manifest.json'),
    Path('crates/hm-core/fixtures/task-cases-initramfs.cpio'), Path('crates/hm-core/fixtures/task-cases-large-initramfs.cpio'),
    witness, out, kernel)
symbols = runtime.read_symbols(kernel, out, 'kernel-symbols', runtime.SYMBOLS)
fixups = runtime.fixup_table(kernel.read_bytes(), symbols)
(out / 'input.json').write_text(json.dumps(dict(kernel_sha256=sha, candidate=candidate,
    trials=3, memory_mb=128, qemu_changes=[], budget_changes=[], layout=layout,
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
    result = cases.run_profile(out / f'trial-{index+1}', kernel, fixtures, symbols, fixups, 128, layout['result'],
        dict(candidate_kernel_sha256=sha, trial=index+1, archives=archives))
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    results.append(dict(trial=index+1, passed=result['passed'], errors=result['errors'],
        wall_seconds=time.monotonic()-start, qemu_user_seconds=after.ru_utime-usage.ru_utime,
        qemu_system_seconds=after.ru_stime-usage.ru_stime, before=before, after=snapshot()))
    (out / 'results.json').write_text(json.dumps(results, indent=2)+'\n')
    print(json.dumps({k:v for k,v in results[-1].items() if k not in ('before','after')}), flush=True)
(out / 'completed.json').write_text(json.dumps({'completed_trials':len(results), 'passed_trials':sum(r['passed'] for r in results),
    'scope':'three optimized-kernel trials; original failures preserved; not full regression acceptance'}, indent=2)+'\n')
raise SystemExit(not all(r['passed'] for r in results))
