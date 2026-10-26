from pathlib import Path
import hashlib
import json
import sys
sys.path.insert(0, str(Path('tools').resolve()))
import build_apps
import build_startup_probes
root = Path('docs/evidence/startup-input-stage-b/first-artifacts')
root.mkdir()
for name, builder, manifest, archive, kernel in (
    ('normal', build_apps, 'apps', 'apps-initramfs.cpio', 'monitor'),
    ('delivery', build_startup_probes, 'startup-input', 'startup-input-initramfs.cpio', 'startup-input-test'),
):
    output = root / name
    output.mkdir()
    elf = Path(f'target/aarch64/{kernel}/aarch64-unknown-none/release/hematite').resolve()
    items, result = builder.verify(Path(f'crates/hm-core/fixtures/{manifest}/manifest.json').resolve(),
        Path('crates/hm-core/fixtures', archive).resolve(), output, elf)
    report = dict(passed=True, items=len(items), archive=result, kernel_sha256=hashlib.sha256(elf.read_bytes()).hexdigest())
    (output / 'result.json').write_text(json.dumps(report,indent=2)+'\n')
    print(name, json.dumps(report))
