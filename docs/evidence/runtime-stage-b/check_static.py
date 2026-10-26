"""Run static gates in the pinned container; no guests."""
import json
from pathlib import Path
import sys

sys.path.insert(0, 'tools')
from build_initramfs import run

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=False)
for arch in ['aarch64', 'x86_64']:
    run(['cargo', 'clippy', '--workspace', '--all-features', '--locked', '--offline',
         '--target', arch + '-unknown-none', '--target-dir', 'target/group-stage-b/clippy-' + arch,
         '--', '-D', 'warnings'], out, arch)
run(['cargo', 'fmt', '--all', '--', '--check'], out, 'format')
run(['python3', 'tools/check_safety.py'], out, 'safety')
result = {'passed': True, 'scope': 'ARM/x86 all-feature Clippy, formatting, structural safety hygiene; no runtime acceptance'}
(out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result))
