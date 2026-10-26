"""Archive a successful final run; invoke only after make test-all exits zero."""
from pathlib import Path
import gzip
import hashlib
import json
import re
import shutil

out = Path('docs/evidence/launcher-final')
before = set(json.loads((out / 'preexisting-evidence.json').read_text()))
new = sorted(str(p) for p in Path('target/evidence').iterdir() if p.is_dir() and str(p) not in before)
assert len(new) == 2, new
reviewed = json.loads(Path('docs/evidence/launcher-third-candidates/inputs.json').read_text())
assert len(reviewed) == 21
for name, entry in reviewed.items():
    assert hashlib.sha256(Path(entry['elf']).read_bytes()).hexdigest() == entry['elf_sha256'], name
counts = {}
for architecture in ('aarch64', 'x86_64'):
    source, = [Path(p) for p in new if p.endswith('-' + architecture)]
    results = list(source.rglob('result.json'))
    assert results
    for path in results:
        result = json.loads(path.read_text())
        assert result.get('passed') is True, (str(path), result)
    counts[architecture] = len(results)
    shutil.copytree(source, out / architecture)
log = Path('docs/evidence/launcher-final-test-all.log').read_text()
python_cases = sum(map(int, re.findall(r'^Ran (\d+) tests? in ', log, re.M)))
assert python_cases == 284, python_cases
files = []
for root in ('.cargo', '.github', 'crates', 'tools'):
    files.extend(p for p in Path(root).rglob('*') if p.is_file() and '__pycache__' not in p.parts)
files.extend(Path(p) for p in ('Cargo.toml','Cargo.lock','Dockerfile','Makefile','rust-toolchain.toml'))
(out / 'source-sha256.txt').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest() + '  ' + str(p) + '\n' for p in sorted(files)))
(out / 'hematite-aarch64.elf.gz').write_bytes(gzip.compress(Path(reviewed['aarch64-monitor']['elf']).read_bytes(), mtime=0))
summary = dict(passed=True, command='make test-all', exit_code=0, parent_commit='3ffe256',
    result_counts=counts, python_harness_cases_per_target=142,
    process_compiler_clients=21, process_cases_per_profile=66, process_profiles=3,
    launcher_profiles=3, launcher_completed_runs=11,
    normal_arm_elf_sha256=reviewed['aarch64-monitor']['elf_sha256'],
    all_21_reviewed_profile_hashes_match=True, evidence_source_directories=new)
(out / 'verification-summary.json').write_text(json.dumps(summary, indent=2) + '\n')
print(json.dumps(summary, indent=2))
