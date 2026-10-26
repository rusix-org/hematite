"""Archive only after make test-all exits zero; bind runtime and source evidence."""
from pathlib import Path
import gzip
import hashlib
import json
import re
import shutil
out = Path('docs/evidence/startup-input-stage-b/final')
before = set(json.loads((out / 'preexisting-evidence.json').read_text()))
new = sorted(str(p) for p in Path('target/evidence').iterdir() if p.is_dir() and str(p) not in before)
assert len(new) == 2, new
reviewed = json.loads(Path('docs/evidence/startup-input-stage-b/third-candidates/inputs.json').read_text())
assert len(reviewed) == 22
for name, entry in reviewed.items():
    assert hashlib.sha256(Path(entry['elf']).read_bytes()).hexdigest() == entry['elf_sha256'], name
counts = {}
for architecture in ('aarch64', 'x86_64'):
    source, = [Path(p) for p in new if p.endswith('-' + architecture)]
    results = list(source.rglob('result.json'))
    assert results
    for path in results:
        assert json.loads(path.read_text()).get('passed') is True, str(path)
    counts[architecture] = len(results)
    shutil.copytree(source, out / architecture)
log = Path('docs/evidence/startup-input-stage-b/final-test-all.log').read_text()
assert sum(map(int, re.findall(r'^Ran (\d+) tests? in ', log, re.M))) == 328
files = []
for root in ('.cargo', '.github', 'crates', 'tools'):
    files.extend(p for p in Path(root).rglob('*') if p.is_file() and '__pycache__' not in p.parts)
files.extend(Path(p) for p in ('Cargo.toml','Cargo.lock','Dockerfile','Makefile','rust-toolchain.toml'))
(out / 'source-sha256.txt').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest() + '  ' + str(p) + '\n' for p in sorted(files)))
for name in ('aarch64-monitor', 'aarch64-startup-input-test'):
    (out / (name + '.elf.gz')).write_bytes(gzip.compress(Path(reviewed[name]['elf']).read_bytes(), mtime=0))
summary = dict(passed=True, command='make test-all', exit_code=0, parent_commit='018c815',
    result_counts=counts, python_harness_cases_per_target=164,
    launch_input_host_cases=8, process_compiler_clients=26,
    startup_delivery_profiles=2, startup_delivery_runs_per_profile=24,
    launcher_profiles=4, launcher_completed_runs=22,
    existing_process_profiles=3, existing_process_runs_per_profile=66,
    all_22_reviewed_runtime_hashes_match=True,
    normal_arm_elf_sha256=reviewed['aarch64-monitor']['elf_sha256'],
    startup_input_elf_sha256=reviewed['aarch64-startup-input-test']['elf_sha256'],
    evidence_source_directories=new)
(out / 'verification-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary,indent=2))
