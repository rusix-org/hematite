#!/usr/bin/env python3
"""Ordinary two-process monitor scenarios and exact resumed-EL0 transitions."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

import build_apps
import test_launcher as legacy
from test_boot_profiles import status_body, write_json
from test_process import require, read_symbols

ECHO_END = b'bin/echo: exited (status 0).\n' * 2
ECHO_BYTES = (b'worker-a\nworker-b\n', b'worker-aworker-b\n\n',
              b'worker-b\nworker-a\n', b'worker-bworker-a\n\n')


def steps(pages, fresh=False, input_first=False):
    require(not input_first, 'Group profile has no input-first variant')
    result = []
    def command(name, text, bodies):
        request = text.encode()
        bodies = bodies if isinstance(bodies, tuple) else (bodies,)
        choices = tuple(legacy.transport(request + b'\n' + body) for body in bodies)
        result.append((name, request + b'\n', choices))
    free = pages if fresh else pages - 3
    if not fresh:
        command('warmup', 'test', legacy.test_body())
    command('initial_status', 'status', status_body('aarch64', free, 0, None))
    command('echo_group', 'run-group bin/echo bin/echo', tuple(body + ECHO_END for body in ECHO_BYTES))
    command('echo_status', 'status', status_body('aarch64', free, 2, 2))
    if fresh:
        return result
    command('failed_second', 'run-group bin/hello bin/missing', b'Program not found.\n')
    command('rollback_status', 'status', status_body('aarch64', free, 2, 3))
    command('busy_peer', 'run-group demo/loop bin/hello', legacy.HELLO +
            b'bin/hello: exited (status 0).\ndemo/loop: exhausted its active-time budget.\n')
    command('busy_status', 'status', status_body('aarch64', free, 4, 5))
    command('interleaved_test', 'test', legacy.test_body())
    command('legacy_hello', 'run bin/hello', legacy.HELLO + legacy.EXIT)
    command('final_status', 'status', status_body('aarch64', free, 5, 6))
    return result


def validate_serial(raw, ram_mb, pages, fresh=False, input_first=False):
    start = legacy.banner(ram_mb, pages) + legacy.PROMPT
    require(raw.startswith(start), 'Wrong group monitor banner')
    remaining = raw[len(start):]
    echo = None
    for name, _, choices in steps(pages, fresh, input_first):
        matches = [choice for choice in choices if remaining.startswith(choice + legacy.PROMPT)]
        require(len(matches) == 1, 'Missing/incorrect group command response: ' + name)
        response = matches[0]
        remaining = remaining[len(response) + len(legacy.PROMPT):]
        if name == 'echo_group':
            prefix = legacy.transport(b'run-group bin/echo bin/echo\n')
            echo = response[len(prefix):-len(legacy.transport(ECHO_END))].replace(b'\r\n', b'\n')
    require(not remaining, 'Extra bytes after final group prompt')
    return {'commands': len(steps(pages, fresh)), 'completed': 2 if fresh else 5,
            'generation': 2 if fresh else 6, 'echo_hex': echo.hex(),
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
        require(self.look is None, 'Unconsumed transition after group')
        super().done()


def validate_trace(raw, apps, symbols, fixups, fresh=False):
    reader = Reader(raw, symbols, fixups)
    if not fresh:
        reader.baseline()
    reader.timer_group()
    def pair(names, looping=False):
        pcs = [apps[name]['entry'] for name in names]
        calls, irqs, done = [0, 0], [0, 0], [False, False]
        cursor, slices, poll_irqs = 0, 0, 0
        output = bytearray()
        while not all(done):
            if looping and done[1] and reader.peek()['kind'] == 'write' and reader.peek()['offset'] == 0:
                break
            require(slices < 512, 'Unbounded group transitions')
            if done[cursor]:
                cursor = 1 - cursor
            index, app = cursor, apps[names[cursor]]
            reader.writes(legacy.SLICE_SETUP)
            reader.returned(0, pcs[index])
            trap = reader.take()
            require(trap['kind'] == 'exception' and trap['source'] == 0 and trap['target'] == 1,
                    'Missing source-EL0 group trap')
            if trap['number'] == 5:
                require(trap['vector'] == symbols['arm_process_vectors'] + 0x480 and any(
                    segment['flags'] == 5 and segment['start'] <= trap['elr'] < segment['end']
                    for segment in app['segments']), 'IRQ outside admitted executable mapping')
                ack = reader.take()
                require(ack['kind'] == 'ack' and ack['id'] == 30, 'Wrong group IRQ ACK')
                reader.writes(((16, 30),))
                irqs[index] += 1
                if names[index] == 'loop' and app['symbols']['loop_start'] <= trap['elr'] < app['symbols']['loop_end']:
                    poll_irqs += 1
            else:
                labels = ('write_svc', 'newline_svc', 'exit_svc') if names[index] == 'echo' else ('write_svc', 'exit_svc')
                require(names[index] != 'loop' and calls[index] < len(labels), 'Unexpected group call')
                label = labels[calls[index]]
                require(trap['number'] == 2 and trap['esr'] == 0x56000000 and trap.get('far') is None and
                        trap['vector'] == symbols['arm_process_vectors'] + 0x400 and
                        trap['elr'] == app['symbols'][label] + 4, 'Wrong group SVC site')
                if names[index] == 'echo' and label != 'exit_svc':
                    output.extend((b'worker-a' if index == 0 else b'worker-b') if label == 'write_svc' else b'\n')
                calls[index] += 1
                done[index] = label == 'exit_svc'
            reader.writes(legacy.SLICE_RESTORE)
            pcs[index] = trap['elr']
            slices += 1
            cursor = 1 - cursor
        if looping:
            require(done == [False, True] and calls == [0, 2] and poll_irqs >= 2,
                    'Busy child was not repeatedly preempted while hello completed')
        else:
            require(calls == [3, 3] and all(done), 'Both echo processes must exit')
        return {'slices': slices, 'calls': calls, 'irqs': irqs, 'output_hex': output.hex()}
    echo = pair(['echo', 'echo'])
    busy = None
    if not fresh:
        # Failed second admission must create no user transition.
        busy = pair(['loop', 'hello'], looping=True)
        reader.baseline()
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
    return {'echo': echo, 'busy_peer': busy,
            'scope': 'exact saved-PC resumption and bounded interleaving; private-root identity covered by runtime profile'}


def validate(raw, trace, execution, apps, symbols, fixups, ram_mb, pages, fresh=False, input_first=False):
    errors, serial, transitions = [], None, None
    try:
        require(execution.get('launch_error') is None and execution.get('timed_out') is False and
                execution.get('host_terminated') is True and execution.get('host_killed') is False and
                execution.get('alive_after_last_prompt') is True and execution.get('returncode') in (-15, 0),
                'Group monitor did not remain alive until bounded host shutdown')
        serial = validate_serial(raw, ram_mb, pages, fresh, input_first)
        transitions = validate_trace(trace, apps, symbols, fixups, fresh)
        require(serial['echo_hex'] == transitions['echo']['output_hex'], 'Echo bytes disagree with actual call interleaving')
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
                        for name in ('test_group_launcher.py', 'test_launcher.py', 'test_process.py')}})
        for ram, fresh in ((64, False), (128, False), (128, True)):
            name = f'group-{ram}m' + ('-fresh' if fresh else '')
            profiles[name] = legacy.run_profile(output / name, kernel, apps, symbols, fixups, ram, fresh,
                                                scenario=sys.modules[__name__])['passed']
    except (OSError, ValueError, KeyError, TypeError) as caught:
        error = str(caught)
    result = {'passed': error is None and len(profiles) == 3 and all(profiles.values()),
              'profiles': profiles, 'error': error, 'scope': 'ordinary ARM groups; no userspace spawn or Linux claim'}
    write_json(output / 'result.json', result)
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
