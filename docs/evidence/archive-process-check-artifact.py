from pathlib import Path
import json
import sys
sys.path.insert(0, str(Path('tools').resolve()))
import test_process as h
output = Path('docs/evidence/archive-process-first-artifact').resolve()
output.mkdir()
kernel = Path('target/aarch64/process-test/aarch64-unknown-none/release/hematite').resolve()
manifest = Path('crates/hm-core/fixtures/process/manifest.json').resolve()
archive = Path('crates/hm-core/fixtures/process-initramfs.cpio').resolve()
symbols = h.read_symbols(kernel, output, 'kernel-symbols', h.SYMBOLS)
fixtures = h.check_fixture_manifest(manifest, output, symbols)
result = h.check_archive_artifact(archive, manifest, kernel, output)
result['passed'] = len(fixtures) == 33
(output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
print(json.dumps(result, indent=2))
