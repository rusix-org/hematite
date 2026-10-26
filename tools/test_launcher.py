#!/usr/bin/env python3
"""Strict interactive normal-launcher and source-EL0 trace verification."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import selectors
import struct
import subprocess
import sys
import time

import build_apps
from test_boot_profiles import help_body, status_body, write_json
from test_kernel import CASES, qemu_command
from test_process import parse_trace, read_symbols, require
from test_preemption_negative import MAX_LOG_BYTES, bounded_bytes, artifact

PROMPT = b'hm> '
TIMEOUT = 40
SYMBOLS = ('__kernel_end', 'arm_vectors', 'arm_process_vectors', 'arm_timer_window_begin', 'arm_timer_window_end',
           'arm_fault_fixups', '__stack_guard', '__exception_stack_guard', 'arm_probe_ro_datum', 'arm_probe_nx_target')
INFO = b'native ABI 1\nconsole: granted\nwrite limit: 256 bytes\nconsole budget: 4096 bytes\ncall limit: 64\n'
HELLO = b'hello from EL0\n'
EXIT = b'Program exited (status 0).\n'
APPS = b'Native programs:\n  bin/hello\n  bin/info\n  demo/fault\n  demo/loop\n'
SETUP = ((0, 0), (4, 255), (8, 0), (0, 1))
SLICE_SETUP = ((4, 255), (8, 0), (0, 1))
SLICE_RESTORE = ((0, 0), (4, 255), (8, 0), (0, 0))


def transport(data):
    return data.replace(b'\n', b'\r\n')


def test_body():
    return ('\n'.join([f'HMTEST BEGIN {len(CASES)}'] + [f'HMTEST PASS {case}' for case in CASES] +
                     [f'HMTEST END {len(CASES)}', 'Bootstrap tests passed.']) + '\n').encode()


def banner(ram_mb, pages):
    return transport((f'hematite: ARM64 Rust kernel entered at EL1\nBoot RAM bytes: 0x{ram_mb * 1024 * 1024:016x} (discovered RAM)\n'
                      f'Frame pool pages: 0x{pages:016x} (4 KiB each; boot reservations excluded)\n'
                      'Hematite bootstrap monitor (local working name)\n'
                      'Commands: help, status, test, apps, run <path>. This is not a Linux shell.\n').encode())


def steps(pages, fresh=False):
    result = []
    def command(name, request, body):
        request = request.encode()
        result.append((name, request + b'\n', transport(request + b'\n' + body)))
    def status(name, count, free):
        command(name, 'status', status_body('aarch64', free, count, count or None))
    if fresh:
        status('initial_status', 0, pages)
        command('hello', 'run bin/hello', HELLO + EXIT)
        status('hello_status', 1, pages)
        return result
    command('warmup', 'test', test_body())
    command('apps', 'apps', APPS)
    free = pages - 3
    status('initial_status', 0, free)
    # Rejections occur before lazy initialization. They must not issue generations.
    command('missing', 'run bin/missing', b'Program not found.\n')
    command('directory', 'run bin', b'Cannot launch a directory.\n')
    for name, request in (('bare_run', 'run'), ('empty_path', 'run '), ('extra_separator', 'run  bin/hello'),
                          ('arguments', 'run bin/hello extra')):
        command(name, request, b'Usage: run <archive-path>.\n')
    for name, request in (('relative_path', 'run ./bin/hello'), ('case_sensitive', 'run bin/Hello')):
        command(name, request, b'Program not found.\n')
    result.append(('overflow', b'x' * 65 + b'\n', transport(b'x' * 64 + b'\nRejected: use at most 64 printable ASCII bytes.\n')))
    command('help_recovery', 'help', help_body())
    status('rejected_status', 0, free)
    for count, (name, path, body) in enumerate((('info', 'bin/info', INFO + EXIT), ('hello', 'bin/hello', HELLO + EXIT),
            ('fault', 'demo/fault', b'Program faulted.\n'), ('loop', 'demo/loop', b'Program timed out.\n')), 1):
        command(name, 'run ' + path, body)
        status(name + '_status', count, free)
    command('interleaved_test', 'test', test_body())
    status('interleaved_status', 4, free)
    command('missing_after_launch', 'run bin/missing', b'Program not found.\n')
    command('hello_again', 'run bin/hello', HELLO + EXIT)
    status('final_status', 5, free)
    return result


def validate_serial(raw, ram_mb, pages, fresh=False):
    expected = banner(ram_mb, pages) + PROMPT
    for _, _, response in steps(pages, fresh):
        expected += response + PROMPT
    require(raw == expected, 'Normal monitor transcript differs from exact command/output/prompt inventory')
    require(not any(marker in raw for marker in (b'HMPROC', b'HMSPACE', b'HMEXCEPTION')), 'Internal test diagnostics leaked into normal flow')
    return {'commands': len(steps(pages, fresh)), 'completed': 1 if fresh else 5,
            'generation': 1 if fresh else 5, 'free_frames': pages if fresh else pages - 3}


def load_bytes(kernel, address, length, required_flags=None):
    require(len(kernel) >= 64, 'Truncated kernel')
    phoff = struct.unpack_from('<Q', kernel, 32)[0]
    phsize, count = struct.unpack_from('<HH', kernel, 54)
    require(phsize == 56 and 0 < count <= 128 and phoff <= len(kernel) and count * 56 <= len(kernel) - phoff, 'Invalid kernel program table')
    matches = []
    for index in range(count):
        kind, flags, offset, va, _, filesz, memsz, _ = struct.unpack_from('<IIQQQQQQ', kernel, phoff + index * 56)
        if kind == 1 and va <= address and address + length <= va + filesz:
            require(filesz <= memsz and offset <= len(kernel) and filesz <= len(kernel) - offset, 'Invalid kernel LOAD bounds')
            if required_flags is not None:
                require(flags == required_flags, 'Kernel evidence bytes have wrong LOAD permissions')
            start = offset + address - va
            matches.append(kernel[start:start + length])
    require(len(matches) == 1, 'Kernel evidence address not uniquely file-backed')
    return matches[0]


def fixup_table(kernel, symbols):
    data = load_bytes(kernel, symbols['arm_fault_fixups'], 6 * 32, 4)
    rows = [struct.unpack_from('<QQQQ', data, index * 32) for index in range(6)]
    fars = (0, symbols['__stack_guard'], symbols['__exception_stack_guard'], symbols['arm_probe_ro_datum'], symbols['arm_probe_nx_target'], 0)
    syndromes = (0x96000006, 0x96000007, 0x96000007, 0x9600004f, 0x8600000f, 0x96000006)
    require(len({row[0] for row in rows}) == 6 and len({row[1] for row in rows}) == 6, 'Duplicate kernel fixup')
    for (fault, resume, far, esr), expected_far, expected_esr in zip(rows, fars, syndromes):
        require(far == expected_far and esr == expected_esr and fault % 4 == resume % 4 == 0, 'Wrong registered fault inventory')
        load_bytes(kernel, fault, 4, 4 if esr == 0x8600000f else 5)
        load_bytes(kernel, resume, 4, 5)
    return rows


def validate_trace(raw, apps, symbols, fixups, fresh=False):
    events, _ = parse_trace(raw)
    cursor = iter(events)
    def take():
        value = next(cursor, None)
        require(value is not None, 'Incomplete launcher trace')
        return value
    def writes(sequence):
        for offset, value in sequence:
            event = take()
            require(event['kind'] == 'write' and (event['offset'], event['value']) == (offset, value), 'Wrong exact GIC write sequence')
    def returned(el, pc):
        event = take()
        require(event['kind'] == 'return' and event['el'] == el and event['pc'] == pc, 'Missing exact exception return')
    def timer_group():
        writes(SETUP)
        for _ in range(8):
            event = take()
            require(event['kind'] == 'exception' and event['number'] == 5 and event['source'] == event['target'] == 1 and
                    event['vector'] == symbols['arm_vectors'] + 0x280 and
                    symbols['arm_timer_window_begin'] <= event['elr'] < symbols['arm_timer_window_end'], 'Unexpected preflight/test timer exception')
            ack = take()
            require(ack['kind'] == 'ack' and ack['id'] == 30, 'Wrong timer acknowledge')
            writes(((16, 30),))
            returned(1, event['elr'])
        writes(((0, 0),) + SETUP + ((0, 0),))
    def baseline():
        timer_group()
        timer_group()
        for _ in range(4):
            for pc, resume, far, esr in fixups:
                event = take()
                require(event['kind'] == 'exception' and event['source'] == event['target'] == 1 and
                        event['number'] == (3 if esr == 0x8600000f else 4) and
                        event['esr'] == esr and event.get('far') == far and event['elr'] == pc and
                        event['vector'] == symbols['arm_vectors'] + 0x200, 'Unexpected baseline fault transaction')
                returned(1, resume)
        writes((SETUP + ((0, 0),)) * 2)
    def launch(name):
        app = apps[name]
        writes(SLICE_SETUP)
        returned(0, app['entry'])
        if name in ('hello', 'info'):
            for index, label in enumerate(('write_svc', 'exit_svc')):
                event = take()
                require(event['kind'] == 'exception' and event['number'] == 2 and event['source'] == 0 and
                        event['target'] == 1 and event['esr'] == 0x56000000 and event.get('far') is None and
                        event['elr'] == app['symbols'][label] + 4 and
                        event['vector'] == symbols['arm_process_vectors'] + 0x400, 'Wrong normal app SVC')
                writes(SLICE_RESTORE)
                if index == 0:
                    writes(SLICE_SETUP)
                    returned(0, event['elr'])
        else:
            event = take()
            require(event['kind'] == 'exception' and event['source'] == 0 and event['target'] == 1, 'Missing source-EL0 terminal exception')
            if name == 'fault':
                require(event['number'] == 4 and event['esr'] == 0x9200004f and event.get('far') == app['entry'] and
                        event['elr'] == app['symbols']['fault'] and event['vector'] == symbols['arm_process_vectors'] + 0x400,
                        'Fault was not exact own-entry RX write abort')
            else:
                require(event['number'] == 5 and app['symbols']['loop_start'] <= event['elr'] < app['symbols']['loop_end'] and
                        event['vector'] == symbols['arm_process_vectors'] + 0x480, 'Loop did not stop on lower-EL timer IRQ')
                ack = take()
                require(ack['kind'] == 'ack' and ack['id'] == 30, 'Wrong process timer acknowledge')
                writes(((16, 30),))
            writes(SLICE_RESTORE)
    if not fresh:
        baseline()
    timer_group()
    if fresh:
        launch('hello')
    else:
        for name in ('info', 'hello', 'fault', 'loop'):
            launch(name)
        baseline()
        launch('hello')
    require(next(cursor, None) is None, 'Extra transitions after completed monitor interactions')
    return {'runs': 1 if fresh else 5, 'el0_svcs': 2 if fresh else 6, 'el0_faults': 0 if fresh else 1,
            'el0_timer_irqs': 0 if fresh else 1, 'el1_timer_irqs': 8 if fresh else 40,
            'scope': 'pinned QEMU exact registered EL1 diagnostics and actual normal-app EL0 transitions'}


def validate(raw, trace, execution, apps, symbols, fixups, ram_mb, pages, fresh=False):
    errors, serial_report, trace_report = [], None, None
    try:
        require(execution.get('launch_error') is None and execution.get('timed_out') is False and
                execution.get('host_terminated') is True and execution.get('host_killed') is False and
                execution.get('alive_after_last_prompt') is True and execution.get('returncode') in (-15, 0),
                'Monitor did not remain alive until deliberate bounded host shutdown')
        serial_report = validate_serial(raw, ram_mb, pages, fresh)
        trace_report = validate_trace(trace, apps, symbols, fixups, fresh)
    except (ValueError, KeyError, TypeError, UnicodeError, StopIteration) as error:
        errors.append(str(error))
    return {'passed': not errors, 'errors': errors, 'execution': execution, 'serial': serial_report, 'trace': trace_report}


def run_profile(output, kernel, apps, symbols, fixups, ram_mb=128, fresh=False):
    output.mkdir()
    pages = (0x41000000 - symbols['__kernel_end']) // 4096
    require(300 < pages < 4096, 'Invalid normal kernel frame baseline')
    command = qemu_command(kernel, 'tcg', 'aarch64')
    command[command.index('-m') + 1] = f'{ram_mb}M'
    command[command.index('-semihosting-config') + 1] = 'enable=on,target=native,userspace=off'
    command += ['-d', 'int', '-D', str(output / 'exceptions.log'), '-trace', 'enable=gic_acknowledge_irq', '-trace', 'enable=gic_cpu_write']
    write_json(output / 'command.json', command)
    write_json(output / 'input.json', {'kernel': str(kernel), 'kernel_sha256': hashlib.sha256(kernel.read_bytes()).hexdigest(),
        'ram_mb': ram_mb, 'fresh': fresh, 'pages': pages, 'timeout_seconds': TIMEOUT})
    execution = {'returncode': None, 'timed_out': False, 'launch_error': None, 'host_terminated': False,
                 'host_killed': False, 'alive_after_last_prompt': False}
    pending, interactions = bytearray(), []
    started = time.monotonic()
    deadline = started + TIMEOUT
    proc = None
    with (output / 'serial.log').open('wb') as log, (output / 'stderr.log').open('wb') as stderr, selectors.DefaultSelector() as selector:
        def read_prompt():
            while True:
                position = pending.find(PROMPT)
                if position >= 0:
                    response = bytes(pending[:position])
                    del pending[:position + len(PROMPT)]
                    return response
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise TimeoutError('No complete monitor prompt before deadline')
                for path in (output / 'exceptions.log', output / 'stderr.log'):
                    require(not path.exists() or path.stat().st_size <= MAX_LOG_BYTES, 'Oversized runtime artifact')
                if not selector.select(min(remaining, 0.25)):
                    continue
                chunk = os.read(proc.stdout.fileno(), 4096)
                require(chunk, 'Guest exited before prompt')
                log.write(chunk)
                log.flush()
                pending.extend(chunk)
                require(log.tell() <= MAX_LOG_BYTES and len(pending) <= 65536, 'Oversized serial response')
                for path in (output / 'exceptions.log', output / 'stderr.log'):
                    require(not path.exists() or path.stat().st_size <= MAX_LOG_BYTES, 'Oversized runtime artifact')
        try:
            proc = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr, bufsize=0)
            selector.register(proc.stdout, selectors.EVENT_READ)
            require(read_prompt() == banner(ram_mb, pages), 'Wrong boot banner')
            for name, request, expected in steps(pages, fresh):
                require(proc.stdin.write(request) == len(request), 'Short monitor input write')
                response = read_prompt()
                interactions.append({'name': name, 'input_hex': request.hex(), 'expected_hex': expected.hex(), 'response_hex': response.hex()})
                require(response == expected, 'Wrong interaction: ' + name)
            execution['alive_after_last_prompt'] = proc.poll() is None
        except TimeoutError as error:
            execution['timed_out'] = True
            execution['error'] = str(error)
        except (OSError, ValueError) as error:
            execution['launch_error'] = str(error)
        finally:
            if proc is not None:
                if proc.poll() is None:
                    execution['host_terminated'] = True
                    proc.terminate()
                try:
                    tail, _ = proc.communicate(timeout=2)
                except subprocess.TimeoutExpired:
                    execution['host_killed'] = True
                    proc.kill()
                    tail, _ = proc.communicate()
                if tail:
                    log.write(tail)
                execution['returncode'] = proc.returncode
    execution['elapsed_seconds'] = time.monotonic() - started
    write_json(output / 'interactions.json', interactions)
    result = validate(bounded_bytes(output / 'serial.log'), bounded_bytes(output / 'exceptions.log'), execution,
                      apps, symbols, fixups, ram_mb, pages, fresh)
    result['artifacts'] = {name: artifact(output / name) for name in ('serial.log', 'exceptions.log', 'stderr.log')}
    result['passed'] &= all(item['within_size_limit'] for item in result['artifacts'].values())
    write_json(output / 'result.json', result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--kernel', type=Path, required=True)
    parser.add_argument('--apps', type=Path, required=True)
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    output = args.output.resolve()
    if output.exists():
        parser.error('preserve evidence; use a fresh directory')
    output.mkdir(parents=True)
    profiles, error = {}, None
    try:
        kernel = args.kernel.resolve()
        apps, archive = build_apps.verify(args.apps.resolve(), args.archive.resolve(), output, kernel)
        symbols = read_symbols(kernel, output, 'kernel-symbols', SYMBOLS)
        fixups = fixup_table(kernel.read_bytes(), symbols)
        sources = ('test_launcher.py', 'build_apps.py', 'build_initramfs.py', 'test_process.py', 'test_boot_profiles.py', 'test_kernel.py')
        write_json(output / 'metadata.json', {'apps': apps, 'archive': archive, 'symbols': symbols, 'fixups': fixups,
            'sources': {name: hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest() for name in sources}})
        for ram_mb, fresh in ((64, False), (128, False), (128, True)):
            name = f'launcher-{ram_mb}m' + ('-fresh' if fresh else '')
            profiles[name] = run_profile(output / name, kernel, apps, symbols, fixups, ram_mb, fresh)['passed']
    except (OSError, ValueError, UnicodeError, KeyError, TypeError, struct.error) as caught:
        error = str(caught)
    result = {'passed': error is None and len(profiles) == 3 and all(profiles.values()), 'profiles': profiles, 'error': error,
              'scope': 'ordinary ARM64 one-CPU native launcher, four real apps, containment, generations and reclamation; no Linux ABI/service claim'}
    write_json(output / 'result.json', result)
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
