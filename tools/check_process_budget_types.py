#!/usr/bin/env python3
"""Compile actual process-budget clients and reject forged or duplicated accounting owners."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

from check_frame_types import run
from check_mapping_types import rejected_only_for

PREAMBLE = """#![no_std]
#![forbid(unsafe_code)]
use hm_kernel::process_budget::{Budget, Error, Finish, Snapshot};
"""
PROBES = {
    'move_begin_finish_stop': ("""pub fn client() -> Result<Snapshot, Error> {
        let original = Budget::new(100, 10)?; let mut budget = original;
        let _ = budget.begin(5)?; let _ = budget.finish(8)?;
        budget.stop()?; Ok(budget.snapshot())
    }""", None),
    'snapshot_mutation_is_detached': ("""pub fn client(budget: &Budget) -> Snapshot {
        let before = budget.snapshot(); let mut detached = before;
        detached.remaining = 0; detached.charged = u64::MAX; detached.turns = u64::MAX;
        assert_eq!(budget.snapshot(), before); let _ = detached; budget.snapshot()
    }""", None),
    'copied_errors_and_snapshots': ("""pub fn client(snapshot: Snapshot, outcome: Finish, error: Error) -> bool {
        let copied = (snapshot, outcome, error);
        let _ = [Finish::Ready, Finish::Exhausted]; let _ = [Error::Configuration, Error::State, Error::Clock];
        copied == (snapshot, outcome, error)
    }""", None),
    'forge_budget': ('pub fn client(budget: Budget) { let _ = Budget { ..budget }; }', 'E0451'),
    'clone_budget': ('pub fn client(budget: Budget) { let _ = budget.clone(); }', 'E0599'),
    'copy_budget': ('pub fn client(budget: Budget) { let moved = budget; let _ = budget.snapshot(); drop(moved); }', 'E0382'),
    'no_reset_method': ('pub fn client(mut budget: Budget) { budget.reset(); }', 'E0599'),
    'no_production_test_seam': ('pub fn client() -> bool { hm_kernel::process_budget::check_turn_counter_overflow() }', 'E0425'),
    'snapshot_cannot_restore_owner': ('pub fn client(snapshot: Snapshot) -> Budget { snapshot }', 'E0308'),
    'shared_budget_cannot_begin': ('pub fn client(budget: &Budget) { let _ = budget.begin(0); }', 'E0596'),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--library', type=Path, required=True)
    parser.add_argument('--target', choices=['aarch64-unknown-none', 'x86_64-unknown-none'], required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    library, output = args.library.resolve(), args.output.resolve()
    if not library.is_file() or output.exists():
        parser.error('require built hm-kernel library and fresh evidence directory')
    output.mkdir(parents=True)
    root = Path(__file__).resolve().parents[1]
    sources = [root / ('crates/hm-kernel/src/' + name + '.rs') for name in ('process_budget',)] + [Path(__file__).resolve()]
    (output / 'input.json').write_text(json.dumps({'library': str(library),
        'library_sha256': hashlib.sha256(library.read_bytes()).hexdigest(), 'target': args.target,
        'sources': {str(source.relative_to(root)): hashlib.sha256(source.read_bytes()).hexdigest() for source in sources}, 'expected_probes': list(PROBES)}, indent=2) + '\n')
    results = {}
    for name, (body, rejection) in PROBES.items():
        client, metadata = output / f'{name}.rs', output / f'{name}.rmeta'
        client.write_text(PREAMBLE + body + '\n')
        command = ['rustc', '--edition=2024', '--crate-type=lib', '--emit=metadata', '--error-format=json',
                   '--target', args.target, '-C', 'panic=abort', '--extern', f'hm_kernel={library}',
                   '-L', f'dependency={library.parent / "deps"}', '-o', str(metadata), str(client)]
        code, diagnostics = run(command, output, name)
        passed = code == 0 and metadata.is_file() if rejection is None else (
            code is not None and code != 0 and rejected_only_for(diagnostics, rejection))
        results[name] = {'passed': passed, 'returncode': code, 'expected_rejection': rejection}
    result = {'passed': len(results) == len(PROBES) and all(r['passed'] for r in results.values()),
              'probes': results, 'target': args.target,
              'scope': 'actual affine budget ownership and detached snapshots; no resumable runtime'}
    (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
