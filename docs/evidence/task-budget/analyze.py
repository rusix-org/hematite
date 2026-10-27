"""Extract charged spawn intervals from preserved traces; guest verdicts remain separate."""
import json
from pathlib import Path
import statistics
import sys
out = Path(sys.argv[1])
results = {}
for path in sorted(out.glob('trial-*/serial.log')):
    intervals = []
    current = None
    case = None
    case_index = None
    frequency = None
    for line in path.read_text().splitlines():
        if not line.startswith('HMTASK '): continue
        kind = line.split()[1]
        if kind not in ('INFO', 'CASE', 'STEP', 'CLOCK', 'CALL', 'COMMIT', 'CHARGE', 'RETURN'): continue
        row = dict(part.split('=', 1) for part in line.split()[2:])
        if kind == 'INFO': frequency = int(row['frequency'])
        elif kind == 'CASE': case, case_index = row['name'], int(row['index'])
        elif kind == 'STEP': current = dict(case=case, case_index=case_index, **row)
        elif kind in ('CLOCK', 'CALL', 'COMMIT', 'CHARGE', 'RETURN') and current is not None:
            assert row['group'] == current['group'] and row['turn'] == current['turn']
            current[kind] = row
            if kind == 'RETURN' and 'CLOCK' in current and 'CHARGE' in current:
                current['elapsed_ms'] = (int(current['CHARGE']['end'])-int(current['CLOCK']['begin']))*1000/frequency
                if current['slot'] == '0' and 'COMMIT' in current:
                    intervals.append(current)
    selected = [r['elapsed_ms'] for r in intervals if r['case'] == 'copy_inputs']
    results[path.parent.name] = dict(copy_inputs_spawn_ms=selected,
        copy_inputs_first_pass_spawn_ms=[r['elapsed_ms'] for r in intervals if r['case_index'] == 1],
        copy_inputs_spawn_median_ms=statistics.median(selected) if selected else None,
        root_commits=intervals)
out.with_name(out.name + '-timing-analysis.json').write_text(json.dumps(results, indent=2)+'\n')
for trial, result in results.items():
    print(trial, 'copy_inputs spawn ms:', result['copy_inputs_spawn_ms'])
