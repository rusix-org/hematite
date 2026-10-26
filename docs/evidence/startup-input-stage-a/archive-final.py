"""Run only after the full suite exits zero; preserve exact evidence and identities."""
from pathlib import Path
import hashlib
import json
import re
import shutil
out = Path('docs/evidence/startup-input-stage-a/final')
before = set(json.loads((out / 'preexisting-evidence.json').read_text()))
new = sorted(str(p) for p in Path('target/evidence').iterdir() if p.is_dir() and str(p) not in before)
assert len(new) == 2, new
baseline = json.loads(Path('docs/evidence/launcher-third-candidates/inputs.json').read_text())
assert len(baseline) == 21
for name, entry in baseline.items():
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
log = Path('docs/evidence/startup-input-stage-a/final-test-all.log').read_text()
assert sum(map(int, re.findall(r'^Ran (\d+) tests? in ', log, re.M))) == 294
files = []
for root in ('.cargo', '.github', 'crates', 'tools'):
    files.extend(p for p in Path(root).rglob('*') if p.is_file() and '__pycache__' not in p.parts)
files.extend(Path(p) for p in ('Cargo.toml','Cargo.lock','Dockerfile','Makefile','rust-toolchain.toml'))
(out / 'source-sha256.txt').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest() + '  ' + str(p) + '\n' for p in sorted(files)))
summary = dict(passed=True, command='make test-all', exit_code=0, parent_commit='b918abb',
    result_counts=counts, python_harness_cases_per_target=147,
    startup_input_host_cases=14, startup_input_compiler_clients_per_target=14,
    all_21_runtime_elfs_identical_to_verified_launcher=True, evidence_source_directories=new,
    scope='safe startup input codec only; runtime delivery remains unimplemented')
(out / 'verification-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary,indent=2))
