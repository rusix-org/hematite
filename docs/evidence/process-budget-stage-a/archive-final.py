"""Archive a successful full suite; require exact unchanged runtime identities."""
from pathlib import Path
import hashlib
import json
import re
import shutil
out=Path('docs/evidence/process-budget-stage-a/final')
before=set(json.loads((out/'preexisting-evidence.json').read_text()))
new=sorted(str(p) for p in Path('target/evidence').iterdir() if p.is_dir() and str(p) not in before)
assert len(new)==2,new
baseline=json.loads(Path('docs/evidence/startup-input-stage-b/third-candidates/inputs.json').read_text())
assert len(baseline)==22
for name,entry in baseline.items():
    assert hashlib.sha256(Path(entry['elf']).read_bytes()).hexdigest()==entry['elf_sha256'],name
counts={}
for arch in ('aarch64','x86_64'):
    source,=[Path(p) for p in new if p.endswith('-'+arch)]
    results=list(source.rglob('result.json'))
    assert results
    for path in results:
        assert json.loads(path.read_text()).get('passed') is True,str(path)
    counts[arch]=len(results)
    shutil.copytree(source,out/arch)
log=Path('docs/evidence/process-budget-stage-a/final-test-all.log').read_text()
assert sum(map(int,re.findall(r'^Ran (\d+) tests? in ',log,re.M)))==338
files=[]
for root in ('.cargo','.github','crates','tools'):
    files.extend(p for p in Path(root).rglob('*') if p.is_file() and '__pycache__' not in p.parts)
files.extend(Path(p) for p in ('Cargo.toml','Cargo.lock','Dockerfile','Makefile','rust-toolchain.toml'))
(out/'source-sha256.txt').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest()+'  '+str(p)+'\n' for p in sorted(files)))
summary=dict(passed=True,command='make test-all',exit_code=0,parent_commit='06afd3d',result_counts=counts,
    python_harness_cases_per_target=169,process_budget_host_cases=15,process_budget_compiler_clients_per_target=10,
    all_22_runtime_elfs_identical_to_verified_startup_delivery=True,evidence_source_directories=new,
    scope='safe persistent turn accounting; resumable/group runtime remains unimplemented')
(out/'verification-summary.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps(summary,indent=2))
