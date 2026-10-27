#!/usr/bin/env python3
"""Strict fixed masked-WFI profile; no interactive READ or halted-residency claim."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import selectors
import signal
import subprocess
import sys
import time
import build_idle as builder
from test_boot_profiles import write_json
from test_kernel import qemu_command, validate_run
from test_launcher import TraceReader, SLICE_SETUP, SLICE_RESTORE
from test_process import require, parse_trace

# All three diagnostic profiles independently reviewed; see0031-idle-transaction.
TRACE_GRAMMAR_REVIEWED = True
MAX_COUNT = (1 << 56) - 1
SERIAL_LIMIT, TRACE_LIMIT, BUFFER_LIMIT = 1024 * 1024, 16 * 1024 * 1024, 65536
PROFILES = (('short64', 'short', 64, 60), ('short128', 'short', 128, 60), ('wall128', 'wall', 128, 360))
SHORT = ('pending_byte', 'expired_limit', 'uart_wait', 'timer_wait')
SNAP = 'root vbar sp0 daif cntkctl cpacr tpidr0 tpidrro par sctlr tcr mair ttbr1 elr spsr esr far fpcr fpsr tpidr1 cval ctl currentel spsel dit ssbs group0 group1 cfg1 cfg2 prio7 prio8 target8 pmr bpr enable0 enable1 imsc dctl cctl'.split()
HARDWARE = 'cval ctl group0 group1 cfg1 cfg2 prio7 prio8 target8 pmr bpr enable0 enable1 imsc dctl cctl'.split()
COUNTS = 'frequency start wall_end limit pre_arm precheck wake final samples poisoned'.split()
CAUSE = 'sampled fr ris mis rsr ctl count'.split()
ACK = 'iar enable source pending_before active_before pending_after active_after counter_before counter_after polls_before polls_after'.split()
CLEAN = 'id enable pending active counter polls'.split()
MASKS = (( 'group0', 1 << 30, 0), ('group1', 2, 0), ('cfg1', 1 << 29, 0), ('cfg2', 8, 0),
         ('prio7', 0xff0000, 0x800000), ('prio8', 0xff00, 0x8000),
         ('pmr', 255, 255), ('bpr', 7, 0))
DEC = r'(?:0|[1-9][0-9]*)'
HEX = r'0x(?:0|[1-9a-f][0-9a-f]*)'
TB = re.compile(r'Trace 0: 0x[0-9a-f]+ \[([0-9a-f]+)/([0-9a-f]+)/([0-9a-f]+)/([0-9a-f]+)\](?: [A-Za-z0-9_.$]+)?')


class Records:
    def __init__(self, lines): self.lines, self.pos = lines, 0
    def exact(self, text):
        require(self.pos < len(self.lines) and self.lines[self.pos] == text, 'Missing/reordered record: ' + text)
        self.pos += 1
    def fields(self, prefix, fields, hexadecimal=()):
        require(self.pos < len(self.lines), 'Truncated ' + prefix)
        line = self.lines[self.pos]
        parts = line.split(' ')
        head = prefix.split(' ')
        require(parts[:len(head)] == head and len(parts) == len(head) + len(fields), 'Wrong fields/order: ' + prefix)
        result = {}
        for name, word in zip(fields, parts[len(head):]):
            pattern = HEX if name in hexadecimal else DEC
            match = re.fullmatch(re.escape(name) + '=(' + pattern + ')', word)
            require(match is not None, 'Malformed field: ' + prefix + ' ' + name)
            value = int(match[1], 16 if name in hexadecimal else 10)
            require(value <= (1 << 64) - 1, 'Integer exceeds u64')
            result[name] = value
        self.pos += 1
        return result


def validate_serial(raw, symbols, profile):
    require(raw and len(raw) <= SERIAL_LIMIT and raw.endswith(b'\n'), 'Missing/oversized/truncated serial')
    lines = raw.decode('ascii').replace('\r\n', '\n').splitlines()
    require(not any(marker in raw for marker in (b'HMPROC', b'HMRT', b'HMTASK', b'HMSPACE', b'HMRWCOPY')), 'Foreign profile records')
    indices = [i for i, line in enumerate(lines) if 'HMIDLE' in line]
    require(indices and all(lines[i].startswith('HMIDLE ') for i in indices), 'Malformed idle prefix')
    first = indices[0]
    require(not any('HMTEST' in line for line in lines[:first]), 'Premature baseline')
    r = Records(lines[first:])
    r.exact('HMIDLE PREFLIGHT checks=15 irqs=8')
    cases = SHORT if profile == 'short' else ('production_wall',)
    r.exact(f'HMIDLE BEGIN profile={profile} cases={len(cases)}')
    reports, identity, prior_final = [], None, None
    for i, name in enumerate(cases):
        r.exact(f'HMIDLE CASE index={i} name={name}')
        if name in ('pending_byte', 'uart_wait'): r.exact('HMIDLE READY name=' + name)
        c = r.fields(f'HMIDLE COUNTS index={i}', COUNTS)
        require(1000 <= c['frequency'] <= 10**9 and c['poisoned'] == 0 and 1 <= c['samples'] <= (16_777_216 + 8192 if name == 'pending_byte' else 8192), 'Bad counter metadata')
        require(all(c[k] <= MAX_COUNT for k in ('start', 'wall_end', 'limit', 'pre_arm', 'precheck', 'wake', 'final')), 'Counter exceeds admitted range')
        key = (c['frequency'], c['start'], c['wall_end'])
        require(identity is None or identity == key, 'Rebased session wall deadline')
        identity = key
        require(c['wall_end'] == c['start'] + c['frequency'] * 300 and c['start'] <= c['limit'] <= c['wall_end'], 'Wrong immutable production bound')
        require(c['start'] <= c['pre_arm'] <= c['precheck'] <= c['final'] and (prior_final is None or prior_final <= c['pre_arm']), 'Regressing boundary counter')
        prior_final = c['final']
        entry = r.fields(f'HMIDLE SNAPSHOT index={i} phase=entry', SNAP, SNAP)
        require(entry['root'] == symbols['arm_root'] and entry['ctl'] == 2 and entry['currentel'] == 4 and entry['spsel'] == 1 and entry['daif'] == 0x3c0, 'Wrong entry context')
        require(entry['tcr'] == 0x80993519 and entry['mair'] == 0xff and entry['sctlr'] == 0x3058180d and entry['ttbr1'] == 0 and entry['vbar'] == symbols['arm_vectors'] and entry['sp0'] == symbols['__exception_stack_top'] and entry['pmr'] <= 255 and entry['bpr'] <= 7 and entry['target8'] == 0, 'Impossible admitted CPU/GIC controls')
        require(entry['imsc'] == entry['dctl'] == entry['cctl'] == entry['enable1'] == 0 and entry['enable0'] & 0xffff0000 == 0, 'Entry device ownership differs')
        setup = r.fields(f'HMIDLE SETUP index={i}', ['changed'] + HARDWARE, ['changed'] + HARDWARE)
        causes = [r.fields(f'HMIDLE CAUSE index={i} phase={phase}', CAUSE, ('fr','ris','mis','rsr','ctl')) for phase in ('precheck', 'post')]
        wait = r.fields(f'HMIDLE WAIT index={i}', ['entered','expired','wfi','resume','acks','spurious'], ('wfi','resume'))
        require(wait['wfi'] == symbols['arm_idle_wfi'] and wait['resume'] == symbols['arm_idle_resume'] and wait['spurious'] in (0,1) and wait['acks'] <= 2, 'Wrong wait metadata')
        expired = name == 'expired_limit'
        entered = name not in ('pending_byte', 'expired_limit')
        deadline = name in ('timer_wait', 'production_wall', 'expired_limit')
        require(wait['entered'] == int(entered) and wait['expired'] == int(deadline), 'Wrong causal wait outcome')
        if expired:
            require(c['start'] <= c['limit'] <= c['pre_arm'] and c['wake'] == 0 and setup == dict(changed=0, **{k: entry[k] for k in HARDWARE}), 'Expired case changed hardware')
            require(wait['acks'] == wait['spurious'] == 0 and c['pre_arm'] == c['precheck'], 'Expired case armed/acknowledged')
        else:
            require(c['precheck'] < c['limit'], 'Case deadline expired before wait')
            expected = {k: entry[k] for k in HARDWARE}
            for key, mask, value in MASKS: expected[key] = (entry[key] & ~mask) | value
            expected.update(cval=c['limit'], ctl=1, enable0=entry['enable0'] | (1 << 30), enable1=entry['enable1'] | 2, imsc=0x7d0, dctl=1, cctl=1)
            changed = (1 << 20) | (1 << 21) | sum(1 << j for j in range(26,40) if j != 32)
            require(setup == dict(changed=changed, **expected), 'Wrong setup readbacks/dirty fields')
            if name == 'production_wall': require(c['limit'] == c['wall_end'], 'Shortened production duration')
            else:
                duration = 5 if name == 'uart_wait' else 1
                require(0 < c['limit'] - c['pre_arm'] <= duration * c['frequency'], 'Wrong short deadline')
        for phase, cause in enumerate(causes):
            sampled = not expired and (phase == 0 or entered)
            require(cause['sampled'] == int(sampled), 'Invented/missing cause sample')
            if not sampled:
                require(all(v == 0 for v in cause.values()), 'Unsampled cause carries data')
                continue
            require(c['pre_arm'] <= cause['count'] <= c['final'] and cause['rsr'] == 0 and cause['fr'] <= 0x1ff and cause['ris'] & ~0x7ff == 0 and cause['mis'] == cause['ris'] & 0x7d0 and cause['ctl'] & 3 == 1, 'Bad sampled device state')
            require(not cause['ris'] & 0x780, 'Unexpected UART error')
            data = name == 'pending_byte' or name == 'uart_wait' and phase == 1
            require(bool(cause['fr'] & 0x10) != data, 'UART FIFO cause differs')
            require(bool(cause['ctl'] & 4) == (deadline and phase == 1), 'Timer cause differs')
            require(cause['ctl'] <= 7 and cause['count'] == c['precheck' if phase == 0 else 'wake'], 'Unexpected CTL/sample boundary')
        if entered:
            require(c['precheck'] <= c['wake'] <= c['final'] and causes[1]['count'] >= c['wake'], 'Missing completed WFI counter')
            require((c['wake'] >= c['limit']) if deadline else (c['wake'] < c['limit']), 'Wrong wake deadline')
        else: require(c['wake'] == 0, 'Skipped WFI has wake counter')
        acks = []
        previous = c['wake'] if entered else c['precheck']
        for ordinal in range(wait['acks']):
            a = r.fields(f'HMIDLE ACK index={i} ordinal={ordinal}', ACK, ('iar','source'))
            require(a['iar'] in (30,33) and all(old['iar'] != a['iar'] for old in acks), 'Foreign/duplicate ACK')
            require(a['enable'] == a['pending_before'] == a['pending_after'] == a['active_after'] == 0 and a['active_before'] == 1 and a['source'] == (2 if a['iar'] == 30 else 0), 'Unsafe EOI observations')
            require(previous <= a['counter_before'] <= a['counter_after'] <= c['final'] and 1 <= a['polls_before'] <= 1024 and 1 <= a['polls_after'] <= 1024, 'Bad ACK clock/poll bounds')
            previous = a['counter_after']; acks.append(a)
        expected_ids = [] if expired else [30] if deadline else [33]
        actual_ids = [a['iar'] for a in acks]
        require(actual_ids == expected_ids or name == 'pending_byte' and actual_ids == [], 'Missing/wrong causal acknowledgement')
        require(expired or wait['spurious'] == 1, 'Missing terminal spurious IAR observation')
        poll_samples = sum(a['polls_before'] + a['polls_after'] for a in acks)
        for irq in (30,33):
            clean = r.fields(f'HMIDLE CLEANUP index={i}', CLEAN)
            poll_samples += clean['polls']
            require(clean['id'] == irq and clean['enable'] == clean['pending'] == clean['active'] == 0, 'Cleanup not quiet')
            if expired: require(clean['polls'] == clean['counter'] == 0, 'Invented expired cleanup')
            else:
                require(1 <= clean['polls'] <= 1024 and previous <= clean['counter'] <= c['final'], 'Invalid cleanup poll/counter')
                previous = clean['counter']
        if expired: require(c['samples'] == 2, 'Wrong expired actual sample count')
        elif name == 'pending_byte': require(1 <= c['samples'] - 5 - poll_samples <= 16_777_216, 'Wrong bounded readiness sample count')
        else: require(c['samples'] == 5 + poll_samples, 'Wrong actual transaction sample count')
        restored = r.fields(f'HMIDLE SNAPSHOT index={i} phase=restored', SNAP, SNAP)
        require(restored == entry, 'Changed restored controls')
        r.exact(f'HMIDLE RESTORED index={i} controls=1')
        byte = '50' if name == 'pending_byte' else '55' if name == 'uart_wait' else 'none'
        count = int(byte != 'none'); dr = int(byte,16) if count else 0
        r.exact(f'HMIDLE PUMP index={i} count={count} dr=0x{dr:x} hex={byte} empty=1')
        r.exact(f'HMIDLE PASS index={i}')
        reports.append(dict(name=name, counts=c, entry=entry, wait=wait, acks=acks))
    r.exact(f'HMIDLE END cases={len(cases)}')
    r.exact('HMIDLE HELLO BEGIN')
    r.exact('hello from EL0')
    r.exact('HMIDLE HELLO END status=0 generation=1 restored=1')
    tail = '\n'.join(r.lines[r.pos:]) + '\n'
    require('HMIDLE' not in tail and 'HMTEST BEGIN 41' in tail, 'Extra idle or missing final baseline')
    require(validate_run(tail.encode(), 0)['passed'], 'Wrong final baseline inventory')
    return reports


def trace_events(raw, symbols):
    require(raw and len(raw) <= TRACE_LIMIT and raw.endswith(b'\n'), 'Missing/incomplete/bounded trace')
    events, pending, assembly, disassembled = [], [], None, set()
    def flush():
        if pending:
            parsed, _ = parse_trace(('\n'.join(pending) + '\n').encode())
            events.extend(parsed); pending.clear()
    for line in raw.decode('ascii').splitlines():
        match = TB.fullmatch(line)
        if match:
            require(assembly is None, 'Execution attempt before completed disassembly block')
            flush()
            pc = int(match[2],16)
            require(pc in (symbols['arm_idle_wfi'], symbols['arm_idle_resume'], symbols['arm_idle_resume'] + 4), 'Unexpected filtered TB execution')
            events.append(dict(kind='tb', pc=pc))
            assembly = None
        elif line == '----------------':
            require(assembly is None, 'Nested translation block')
            assembly = []
        elif line.startswith('IN:'):
            require(assembly == [] and len(line) <= 128, 'Unexpected translation header')
        elif re.match(r'0x[0-9a-f]+:', line):
            require(assembly is not None, 'Instruction outside translation block')
            instruction = re.fullmatch(r'(0x[0-9a-f]+):\s+([0-9a-f]{8})\s+([^\r\n]{1,100})', line)
            require(instruction is not None, 'Malformed translated instruction')
            pc, opcode = int(instruction[1],16), int(instruction[2],16)
            offset = pc - symbols['arm_idle_wfi']
            require(offset in (0,4,8,12) and opcode == builder.LEAF[offset//4], 'Translated idle bytes differ')
            require(pc not in assembly, 'Duplicate translation instruction')
            assembly.append(pc); disassembled.add(pc)
        elif not line and assembly is not None:
            require(assembly in ([symbols['arm_idle_wfi']], [symbols['arm_idle_resume']], [symbols['arm_idle_resume'] + j for j in (4,8)]), 'Wrong translated block boundaries')
            assembly = None
        else:
            require(not line.startswith(('Trace ', 'Stopped ', 'Linking ', 'IN:')) and assembly is None, 'Stopped/unknown TB attempt or translation')
            pending.append(line)
    require(assembly is None, 'Unfinished translation block')
    flush()
    require(disassembled == {symbols['arm_idle_wfi'] + j for j in (0,4,8,12)}, 'Missing real leaf translation')
    return events


def validate_trace(raw, symbols, fixups, hello, cases):
    reader = object.__new__(TraceReader)
    reader.cursor = iter(trace_events(raw, symbols)); reader.symbols, reader.fixups = symbols, fixups
    reader.timer_group()
    for case in cases:
        if case['name'] == 'expired_limit': continue
        # Exact CPU-interface writes from Transaction::configure/service/restore.
        # Distributor writes are independently bound by artifact/source review.
        reader.writes(((4,255),(8,0),(0,1)))
        if case['wait']['entered']:
            # Pinned QEMU ends a TB at ISB as well as WFI. Counter/RET
            # must execute in the distinct third block, never be discarded.
            for pc in (symbols['arm_idle_wfi'], symbols['arm_idle_resume'], symbols['arm_idle_resume'] + 4):
                event = reader.take()
                require(event == dict(kind='tb', pc=pc), 'Missing/reordered real WFI/ISB/counter attempt')
        for ack in case['acks']:
            event = reader.take()
            require(event['kind'] == 'ack' and event['id'] == ack['iar'], 'ACK differs from observation')
            reader.writes(((16,ack['iar']),))
        # QEMU v7.2.22 arm_gic.c:607-612 traces the pending ID before
        # the >= GIC_MAXIRQ early return, including real spurious reads.
        if case['wait']['spurious']:
            event = reader.take()
            require(event['kind'] == 'ack' and event['id'] == 1023, 'Missing real spurious IAR read')
        reader.writes(((0,0),(4,case['entry']['pmr']),(8,case['entry']['bpr'])))
    reader.timer_group()
    reader.writes(SLICE_SETUP); reader.returned(0, hello['entry'])
    for index, label in enumerate(('write_svc','exit_svc')):
        event = reader.take()
        require(event['kind'] == 'exception' and event['number'] == 2 and event['source'] == 0 and event['target'] == 1 and event['esr'] == 0x56000000 and event.get('far') is None and event['elr'] == hello['symbols'][label] + 4 and event['vector'] == symbols['arm_process_vectors'] + 0x400, 'Wrong subsequent hello SVC')
        reader.writes(SLICE_RESTORE)
        if index == 0:
            reader.writes(SLICE_SETUP); reader.returned(0,event['elr'])
    reader.baseline()
    event = reader.take()
    require(event['kind'] == 'exception' and event['number'] == 16 and event['source'] == event['target'] == 1, 'Missing sole privileged exit')
    reader.done()
    return dict(wfi=sum(c['wait']['entered'] for c in cases), el0_svcs=2, timer_irqs=32, baseline_faults=24)


def validate(raw, trace, execution, symbols, fixups, hello, profile, inputs):
    errors, serial_report, trace_report = [], None, None
    try:
        require(execution.get('returncode') == 0 and execution.get('timed_out') is False and execution.get('log_limit_exceeded') is False and execution.get('launch_error') is None and execution.get('host_killed') is False, 'Execution failed/incomplete')
        expected = [] if profile == 'wall' else [('pending_byte','50','ready'),('uart_wait','55','wfi_attempt')]
        require([(i['case'],i['hex'],i['trigger']) for i in inputs] == expected, 'Wrong actual host inputs')
        require(all(type(i['time']) in (int,float) and 0 <= i['time'] <= execution['elapsed_seconds'] for i in inputs), 'Invalid input timestamps')
        serial_report = validate_serial(raw,symbols,profile)
        trace_report = validate_trace(trace,symbols,fixups,hello,serial_report)
        require(TRACE_GRAMMAR_REVIEWED, 'Actual idle trace grammar awaits independent review')
    except (ValueError, KeyError, TypeError, UnicodeError, StopIteration, IndexError) as error:
        errors.append(str(error))
    return dict(passed=not errors, errors=errors, execution=execution, serial=serial_report, trace=trace_report)


def log_sizes(output):
    limits = {'serial.log': SERIAL_LIMIT, 'trace.log': TRACE_LIMIT, 'stderr.log': TRACE_LIMIT}
    sizes = {name: path.stat().st_size for name in limits if (path := output / name).exists()}
    oversized = {name: size for name, size in sizes.items() if size > limits[name]}
    combined = sizes.get('trace.log', 0) + sizes.get('stderr.log', 0)
    if combined > TRACE_LIMIT: oversized['combined_trace'] = combined
    return oversized


def file_hash(path):
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(65536), b''): digest.update(chunk)
    return digest.hexdigest()


def run_profile(output, kernel, symbols, fixups, hello, binding, profile):
    name, mode, ram, timeout = profile
    output.mkdir()
    command = qemu_command(kernel,'tcg','aarch64')
    command[command.index('-smp')+1] = '1'
    command[command.index('-m')+1] = str(ram) + 'M'
    machine_index = command.index('-machine')+1
    # Preserve the existing explicitly admitted machine properties.
    parts = command[machine_index].split(',')
    parts = [p for p in parts if not p.startswith(('secure=','gic-version='))]
    command[machine_index] = ','.join(parts + ['secure=off','gic-version=2'])
    # Each ELF has a fixed compile-time profile; UART carries only real input.
    command += ['-d','exec,nochain,in_asm,int',
                '-dfilter', f"0x{symbols['arm_idle_wfi']:x}..0x{symbols['arm_idle_end']-1:x}",
                '-D',str(output/'trace.log'),'-trace','enable=gic_acknowledge_irq','-trace','enable=gic_cpu_write']
    write_json(output/'command.json',command)
    write_json(output/'input.json',dict(binding=binding, profile=name, ram_mb=ram, cpus=1, timeout=timeout,
               serial_limit=SERIAL_LIMIT,trace_limit=TRACE_LIMIT,pending_limit=BUFFER_LIMIT,poll_seconds=0.05))
    execution = dict(returncode=None,timed_out=False,log_limit_exceeded=False,host_killed=False,launch_error=None)
    inputs, pending, trace_pending = [], bytearray(), bytearray()
    trace_offset, current, proc = 0, None, None
    start = time.monotonic(); deadline = start + timeout
    with (output/'serial.log').open('wb') as serial, (output/'stderr.log').open('wb') as stderr, selectors.DefaultSelector() as selector:
        def send(case, byte, trigger):
            require(not any(item['case'] == case for item in inputs), 'Repeated host injection')
            require(os.write(proc.stdin.fileno(),byte) == 1, 'Incomplete UART input write')
            inputs.append(dict(case=case,hex=byte.hex(),trigger=trigger,time=time.monotonic()-start))
        try:
            proc = subprocess.Popen(command,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=stderr,bufsize=0,start_new_session=True)
            os.set_blocking(proc.stdout.fileno(),False); os.set_blocking(proc.stdin.fileno(),False)
            selector.register(proc.stdout,selectors.EVENT_READ)
            while True:
                require(time.monotonic() < deadline, 'Host deadline exceeded')
                oversized = log_sizes(output)
                if oversized:
                    execution['log_limit_exceeded'] = True
                    execution['oversized_logs'] = oversized
                    raise ValueError('Live log size exceeded')
                for _key, _mask in selector.select(0.05):
                    chunk = os.read(proc.stdout.fileno(),4096)
                    if chunk:
                        serial.write(chunk); serial.flush(); pending.extend(chunk)
                        require(len(pending) <= BUFFER_LIMIT, 'Oversized pending serial')
                        while b'\n' in pending:
                            line, _, rest = pending.partition(b'\n'); pending[:] = rest
                            line = line.rstrip(b'\r')
                            if line == b'HMIDLE READY name=pending_byte':
                                require(mode == 'short', 'Input requested in quiet wall profile')
                                send('pending_byte',b'P','ready')
                            elif line == b'HMIDLE READY name=uart_wait':
                                require(mode == 'short', 'Input requested in quiet wall profile')
                                current = 'uart_wait'
                    else: selector.unregister(proc.stdout)
                trace_path = output/'trace.log'
                if trace_path.exists():
                    with trace_path.open('rb') as reader:
                        reader.seek(trace_offset); chunk = reader.read(65536); trace_offset += len(chunk)
                    trace_pending.extend(chunk)
                    while b'\n' in trace_pending:
                        line, _, rest = trace_pending.partition(b'\n'); trace_pending[:] = rest
                        match = TB.fullmatch(line.decode('ascii'))
                        if match and int(match[2],16) == symbols['arm_idle_wfi'] and current == 'uart_wait':
                            send('uart_wait',b'U','wfi_attempt'); current = None
                    require(len(trace_pending) <= BUFFER_LIMIT, 'Oversized pending trace line')
                if proc.poll() is not None and not selector.get_map(): break
        except (OSError,ValueError,UnicodeError) as error:
            if time.monotonic() >= deadline: execution['timed_out'] = True
            execution['launch_error'] = str(error)
        finally:
            if proc is not None:
                if proc.poll() is None:
                    execution['host_killed'] = True
                    os.killpg(proc.pid,signal.SIGKILL)
                proc.wait(timeout=5)
                execution['returncode'] = proc.returncode
            execution['elapsed_seconds'] = time.monotonic()-start
    oversized = log_sizes(output)
    if oversized:
        execution['log_limit_exceeded'] = True
        execution['oversized_logs'] = oversized
    write_json(output/'execution.json',execution); write_json(output/'inputs.json',inputs)
    with (output/'serial.log').open('rb') as stream: raw = stream.read(SERIAL_LIMIT + 1)
    trace = b''
    if (output/'trace.log').exists():
        with (output/'trace.log').open('rb') as stream: trace = stream.read(TRACE_LIMIT + 1)
    result = validate(raw,trace,execution,symbols,fixups,hello,mode,inputs)
    result['artifacts'] = {p.name:dict(bytes=p.stat().st_size,sha256=file_hash(p)) for p in (output/'serial.log',output/'trace.log',output/'stderr.log') if p.exists()}
    write_json(output/'result.json',result)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('kernel','wall-kernel','apps','archive','output'): parser.add_argument('--'+name,type=Path,required=True)
    args = parser.parse_args()
    if args.output.exists(): parser.error('use a fresh output directory')
    args.output.mkdir(parents=True)
    try:
        bindings = {}
        for mode, kernel in (('short',args.kernel),('wall',args.wall_kernel)):
            evidence = args.output / ('artifact-' + mode); evidence.mkdir()
            bindings[mode] = builder.verify(kernel.resolve(),args.apps.resolve(),args.archive.resolve(),evidence)
        require(bindings['short'][3]['kernel_sha256'] != bindings['wall'][3]['kernel_sha256'], 'Profile kernels must be distinct fixed artifacts')
        reports = []
        for profile in PROFILES:
            symbols,fixups,apps,binding = bindings[profile[1]]
            kernel = args.kernel if profile[1] == 'short' else args.wall_kernel
            reports.append(run_profile(args.output/profile[0],kernel.resolve(),symbols,fixups,apps['hello'],binding,profile))
        result = dict(passed=all(r['passed'] for r in reports),profiles=reports,scope='fixed boot-only idle transaction; no interactive READ')
    except (OSError,ValueError,KeyError,TypeError,subprocess.TimeoutExpired) as error:
        result = dict(passed=False,error=str(error))
    write_json(args.output/'result.json',result)
    print(json.dumps(result,indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__': sys.exit(main())
