"""Serialize pinned profile builds; preserve exact compiler evidence before guests."""
from pathlib import Path
import gzip
import hashlib
import json
import subprocess

out = Path('docs/evidence/launcher-third-candidates')
out.mkdir()
arm = [('monitor', None), ('boot-test', 'boot-tests'), ('fault-test', 'unhandled-fault-test'),
       ('nested-test', 'nested-fault-test'), ('thread-overflow-test', 'thread-overflow-test'),
       ('preemption-test', 'preemption-tests'), ('preemption-boundary-test', 'preemption-boundary-test'),
       ('preemption-no-timer-test', 'preemption-no-timer-test'), ('preemption-overflow-test', 'preemption-overflow-test'),
       ('wait-all-blocked-test', 'wait-all-blocked-test'), ('space-test', 'space-tests'), ('process-test', 'process-tests'),
       ('secondary-test', 'secondary-tests'), ('secondary-stale-test', 'secondary-stale-generation-test'),
       ('secondary-off-return-test', 'secondary-cpu-off-return-test'), ('smp-test', 'smp-tests'),
       ('smp-no-peer-timer-test', 'smp-no-peer-timer-test'), ('smp-peer-overflow-test', 'smp-peer-overflow-test'),
       ('smp-all-blocked-test', 'smp-all-blocked-test')]
records = {}
for arch, profiles in [('aarch64', arm), ('x86_64', arm[:2])]:
    for profile, feature in profiles:
        name = arch + '-' + profile
        target = arch + '-unknown-none'
        directory = 'target/' + arch + '/' + profile
        command = ['cargo', 'rustc', '--locked', '--offline', '-p', 'hm-core', '--release', '--target', target, '--target-dir', directory]
        if feature: command += ['--features', feature]
        command += ['--', '--emit=link,asm']
        (out / (name + '.command.json')).write_text(json.dumps(command, indent=2) + '\n')
        with (out / (name + '.build.log')).open('wb') as stream:
            result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT, timeout=120)
        if result.returncode: raise SystemExit(name + ' build failed')
        root = Path(directory) / target / 'release'
        elf = root / 'hematite'
        asm = max((root / 'deps').glob('hematite-*.s'), key=lambda p: p.stat().st_mtime_ns)
        (out / (name + '.s.gz')).write_bytes(gzip.compress(asm.read_bytes(), mtime=0))
        symbols = subprocess.check_output(['readelf', '-sW', str(elf)], timeout=20)
        (out / (name + '.symbols.log')).write_bytes(symbols)
        records[name] = {'command': command, 'returncode': result.returncode, 'elf': str(elf),
            'elf_sha256': hashlib.sha256(elf.read_bytes()).hexdigest(), 'asm': str(asm),
            'asm_sha256': hashlib.sha256(asm.read_bytes()).hexdigest()}
        (out / 'inputs.json').write_text(json.dumps(records, indent=2) + '\n')
        print(name + ' compiled; no guest executed', flush=True)
