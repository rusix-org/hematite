"""Run exactly one unchanged fixed profile for first raw review, not full acceptance."""
import argparse
import json
from pathlib import Path
import sys

sys.path.insert(0, str(Path('tools').resolve()))
import build_idle
import test_idle

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--kernel', type=Path, required=True)
parser.add_argument('--profile', choices=[profile[0] for profile in test_idle.PROFILES], required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
if args.output.exists():
    parser.error('use a fresh output directory')
args.output.mkdir(parents=True)
artifact = args.output / 'artifact'
artifact.mkdir()
symbols, fixups, apps, binding = build_idle.verify(args.kernel.resolve(),
    Path('crates/hm-core/fixtures/apps/manifest.json').resolve(),
    Path('crates/hm-core/fixtures/apps-initramfs.cpio').resolve(), artifact)
profile = next(profile for profile in test_idle.PROFILES if profile[0] == args.profile)
result = test_idle.run_profile(args.output / args.profile, args.kernel.resolve(),
    symbols, fixups, apps['hello'], binding, profile)
report = {'scope': 'one fixed diagnostic profile only; not nine-case aggregate acceptance',
    'profile': args.profile, 'result': result}
(args.output / 'result.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps(report, indent=2))
sys.exit(0 if result['passed'] else 1)
