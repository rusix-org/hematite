from pathlib import Path
import hashlib
import json
import sys
sys.path.insert(0, str(Path('tools').resolve()))
import build_apps
out = Path('docs/evidence/launcher-second-artifact').resolve()
out.mkdir()
kernel = Path('target/aarch64/monitor/aarch64-unknown-none/release/hematite').resolve()
apps, archive = build_apps.verify(Path('crates/hm-core/fixtures/apps/manifest.json').resolve(), Path('crates/hm-core/fixtures/apps-initramfs.cpio').resolve(), out, kernel)
result = {'passed': len(apps) == 4, 'archive': archive, 'kernel_sha256': hashlib.sha256(kernel.read_bytes()).hexdigest()}
(out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
