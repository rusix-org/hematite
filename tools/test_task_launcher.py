#!/usr/bin/env python3
"""Ordinary EL0 supervisor spawn/wait/cancel and exact resumed transitions."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

import build_apps
import test_launcher as legacy
from test_boot_profiles import status_body, write_json
from test_process import require, read_symbols

TASK_OUTPUT = b'worker complete\nsupervisor complete\nbin/supervisor: exited (status 0).\n'
# Review 0029 accepts ordinary-first raw traces; authority negatives are separate.
TRACE_GRAMMAR_REVIEWED = True


def steps(pages, fresh=False, input_first=False):
    require(not input_first, 'No task input-first variant')
    result = []
    def command(name, request, body):
        request = request.encode()
        result.append((name, request + b'\n', legacy.transport(request + b'\n' + body)))
    free = pages if fresh else pages - 3
    if not fresh:
        command('warmup', 'test', legacy.test_body())
    command('initial_status', 'status', status_body('aarch64', free))
    for index in (1, 2):
        command('supervisor_' + str(index), 'run-task bin/supervisor', TASK_OUTPUT)
        command('status_' + str(index), 'status', status_body('aarch64', free, index * 3, index * 3))
    command('missing', 'run-task bin/missing', b'Program not found.\n')
    command('missing_arity', 'run-task', b'Usage: run-task <archive-path>.\n')
    command('extra_arity', 'run-task bin/supervisor extra', b'Usage: run-task <archive-path>.\n')
    command('rejected_status', 'status', status_body('aarch64', free, 6, 6))
    command('legacy_hello', 'run bin/hello', legacy.HELLO + legacy.EXIT)
    command('final_status', 'status', status_body('aarch64', free, 7, 7))
    return result


def validate_serial(raw, ram_mb, pages, fresh=False, input_first=False):
    expected = legacy.banner(ram_mb, pages) + legacy.PROMPT
    expected += b''.join(response + legacy.PROMPT for _, _, response in steps(pages, fresh, input_first))
    require(raw == expected, 'Task transcript, counters, reclamation or prompts differ')
    return {'commands': len(steps(pages, fresh)), 'completed': 7, 'generation': 7,
            'free_frames': pages if fresh else pages - 3}


class Reader(legacy.TraceReader):
    def __init__(self, *args):
        super().__init__(*args)
        self.look = None
    def peek(self):
        if self.look is None:
            self.look = super().take()
        return self.look
    def take(self):
        if self.look is not None:
            value, self.look = self.look, None
            return value
        return super().take()
    def done(self):
        require(self.look is None, 'Unconsumed transition after task group')
        super().done()


def validate_trace(raw, apps, symbols, fixups, fresh=False):
    reader = Reader(raw, symbols, fixups)
    if not fresh:
        reader.baseline()
    reader.timer_group()
    def supervisor():
        names = ('supervisor', 'worker', 'loop')
        labels = (('spawn_svc', 'spawn_svc', 'spawn_svc', 'wait_svc',
                   'cancel_svc', 'cancel_svc', 'wait_svc', 'write_svc', 'exit_svc'),
                  ('worker_yield_1', 'worker_yield_2', 'worker_yield_3', 'worker_yield_4',
                   'write_svc', 'exit_svc'), ())
        pcs = [apps[name]['entry'] for name in names]
        states, calls, irqs = ['ready', 'absent', 'absent'], [0]*3, [0]*3
        cursor, slices = 0, 0
        while states[0] != 'done':
            require(slices < 768, 'Unbounded task transition stream')
            selected = None
            for _ in range(3):
                index, cursor = cursor, (cursor + 1) % 3
                if states[index] == 'cancel':
                    states[index] = 'done'  # Quiescent cancellation performs no ERET.
                if states[index] == 'ready':
                    selected = index
                    break
            require(selected is not None, 'No runnable owner before supervisor exit')
            index = selected
            app = apps[names[index]]
            reader.writes(legacy.SLICE_SETUP)
            reader.returned(0, pcs[index])
            trap = reader.take()
            require(trap['kind'] == 'exception' and trap['source'] == 0 and trap['target'] == 1,
                    'Task transition is not a lower-EL trap')
            if trap['number'] == 5:
                require(trap['vector'] == symbols['arm_process_vectors'] + 0x480 and any(
                    segment['flags'] == 5 and segment['start'] <= trap['elr'] < segment['end']
                    for segment in app['segments']), 'Task IRQ outside actual executable image')
                ack = reader.take()
                require(ack['kind'] == 'ack' and ack['id'] == 30, 'Wrong task timer ACK')
                reader.writes(((16, 30),))
                irqs[index] += 1
            else:
                require(calls[index] < len(labels[index]), 'Unexpected task SVC')
                label = labels[index][calls[index]]
                require(trap['number'] == 2 and trap['esr'] == 0x56000000 and trap.get('far') is None and
                        trap['vector'] == symbols['arm_process_vectors'] + 0x400 and
                        trap['elr'] == app['symbols'][label] + 4, 'Wrong task SVC site or order')
                calls[index] += 1
                if index == 0:
                    if calls[0] == 1:
                        states[1] = 'ready'
                    elif calls[0] == 2:
                        states[2] = 'ready'
                    elif calls[0] == 4 and states[1] != 'done':
                        states[0] = 'waiting'
                    elif calls[0] == 5:
                        require(states[2] == 'ready', 'Loop completed before cancellation')
                        states[2] = 'cancel'
                    elif calls[0] == 6:
                        require(states[2] == 'done', 'Repeated cancel before child teardown')
                    elif calls[0] == 9:
                        require(states[1:] == ['done', 'done'], 'Root exit before child reclamation')
                        states[0] = 'done'
                elif label == 'exit_svc':
                    states[1] = 'done'
                    if states[0] == 'waiting':
                        states[0] = 'ready'
            reader.writes(legacy.SLICE_RESTORE)
            pcs[index] = trap['elr']
            slices += 1
        require(calls == [9, 6, 0], 'Missing actual parent or worker calls')
        return {'calls': calls, 'irqs': irqs, 'slices': slices}
    runs = [supervisor(), supervisor()]
    app = apps['hello']
    pc = app['entry']
    for label in ('write_svc', 'exit_svc'):
        reader.writes(legacy.SLICE_SETUP)
        reader.returned(0, pc)
        trap = reader.take()
        require(trap['kind'] == 'exception' and trap['number'] == 2 and trap['source'] == 0 and
                trap['target'] == 1 and trap['esr'] == 0x56000000 and trap.get('far') is None and
                trap['elr'] == app['symbols'][label] + 4 and
                trap['vector'] == symbols['arm_process_vectors'] + 0x400, 'Legacy hello regression')
        reader.writes(legacy.SLICE_RESTORE)
        pc = trap['elr']
    reader.done()
    return {'supervisors': runs, 'legacy_calls': 2,
            'scope': 'artifact-bound task SVC order and saved-PC resumption; private authority negatives use dedicated gate'}


def validate(raw, trace, execution, apps, symbols, fixups, ram_mb, pages, fresh=False, input_first=False):
    errors, serial, transitions = [], None, None
    try:
        require(execution.get('launch_error') is None and execution.get('timed_out') is False and
                execution.get('host_terminated') is True and execution.get('host_killed') is False and
                execution.get('alive_after_last_prompt') is True and execution.get('returncode') in (-15, 0),
                'Task monitor did not remain alive until bounded host shutdown')
        serial = validate_serial(raw, ram_mb, pages, fresh, input_first)
        transitions = validate_trace(trace, apps, symbols, fixups, fresh)
    except (ValueError, KeyError, TypeError, UnicodeError, StopIteration) as error:
        errors.append(str(error))
    return {'passed': not errors, 'errors': errors, 'execution': execution, 'serial': serial, 'trace': transitions}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('kernel', 'apps', 'archive', 'output'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error('use a fresh evidence directory')
    output.mkdir(parents=True)
    profiles, error = {}, None
    try:
        kernel = args.kernel.resolve()
        apps, archive = build_apps.verify(args.apps.resolve(), args.archive.resolve(), output, kernel)
        symbols = read_symbols(kernel, output, 'kernel-symbols', legacy.SYMBOLS)
        fixups = legacy.fixup_table(kernel.read_bytes(), symbols)
        write_json(output / 'metadata.json', {'apps': apps, 'archive': archive, 'symbols': symbols,
            'sources': {name: hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest()
                        for name in ('test_task_launcher.py', 'test_launcher.py', 'test_process.py', 'build_tasks.py')}})
        for ram, fresh in ((64, False), (128, False), (128, True)):
            name = f'task-{ram}m' + ('-fresh' if fresh else '')
            profiles[name] = legacy.run_profile(output / name, kernel, apps, symbols, fixups, ram, fresh,
                                                scenario=sys.modules[__name__])['passed']
    except (OSError, ValueError, KeyError, TypeError) as caught:
        error = str(caught)
    result = {'passed': TRACE_GRAMMAR_REVIEWED and error is None and len(profiles) == 3 and all(profiles.values()),
              'profiles': profiles, 'error': error, 'trace_grammar_reviewed': TRACE_GRAMMAR_REVIEWED, 'scope': 'ordinary native task creation; detailed negative authority profiles separate'}
    write_json(output / 'result.json', result)
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
