"""Bind every compiled negative profile to its exact canonical archive/witness; no VM."""
import json
from pathlib import Path
import sys
sys.path.insert(0, 'tools')
import build_task_negative_cases as builder

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=False)
inputs = json.loads(Path(sys.argv[2]).read_text())
layout = Path(sys.argv[3])
results = {}
for case in builder.PROFILES:
    target = out / case
    target.mkdir()
    kernel = Path(inputs['aarch64-negative-' + case]['elf'])
    try:
        _, report = builder.verify(builder.CANONICAL / 'manifest.json', builder.ARCHIVE,
                                   layout, target, kernel=kernel)
        results[case] = {'passed': True, 'report': report}
    except (OSError, ValueError, RuntimeError, AssertionError) as error:
        results[case] = {'passed': False, 'error': str(error)}
    print(case, results[case]['passed'], flush=True)
    (out / 'result.json').write_text(json.dumps(results, indent=2) + '\n')
raise SystemExit(not all(item['passed'] for item in results.values()))
