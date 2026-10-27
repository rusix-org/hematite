#!/usr/bin/env python3
"""Independent fixed supervisor scripts and mutations of real-transition grammar."""
import copy
import unittest

import test_task_launcher as h
import test_launcher as legacy
import test_test_launcher as old

APPS = copy.deepcopy(old.APPS)
for index, name in enumerate(('supervisor', 'worker')):
    base = 0x100000000 + index * 0x1000
    labels = ('spawn_svc', 'wait_svc', 'cancel_svc', 'write_svc', 'exit_svc',
              'worker_yield_1', 'worker_yield_2', 'worker_yield_3', 'worker_yield_4')
    APPS[name] = {'entry': base, 'symbols': {label: base + 0x40 + i * 8 for i, label in enumerate(labels)}}
for app in APPS.values():
    app['segments'] = [{'flags': 5, 'start': app['entry'], 'end': app['entry'] + 4096}]


def serial(fresh=False):
    return legacy.banner(128, old.PAGES) + legacy.PROMPT + b''.join(
        response + legacy.PROMPT for _, _, response in h.steps(old.PAGES, fresh))


def trace(fresh=False, immediate=False):
    original = old.trace(fresh=fresh).decode().splitlines()
    first = next(i for i, line in enumerate(original) if 'to AArch64 EL0 PC' in line)
    lines = original[:first - 3]
    def writes(items):
        lines.extend(f'gic_cpu_write cpu 0 iface write at {offset:#010x} {value:#010x}' for offset, value in items)
    pcs = {}
    def step(owner, label):
        app = APPS[owner]
        writes(legacy.SLICE_SETUP)
        lines.append(f'Exception return from AArch64 EL1 to AArch64 EL0 PC {pcs.get(owner, app["entry"]):#x}')
        irq = label == 'irq'
        pc = app['entry'] + 0x100 if irq else app['symbols'][label] + 4
        lines.extend([f'Taking exception {5 if irq else 2} [{"IRQ" if irq else "SVC"}] on CPU 0',
                      '...from EL0 to EL1', '...with ESR 0x0/0x0' if irq else '...with ESR 0x15/0x56000000',
                      f'...with ELR {pc:#x}',
                      f'...to EL1 PC {old.SYMBOLS["arm_process_vectors"] + (0x480 if irq else 0x400):#x} PSTATE 0x3c5'])
        if irq:
            lines.append('gic_acknowledge_irq cpu 0 acknowledged irq 30')
            writes(((16, 30),))
        writes(legacy.SLICE_RESTORE)
        pcs[owner] = pc
    for _ in range(2):
        pcs.clear()
        script = [('supervisor', 'spawn_svc'), ('worker', 'worker_yield_1'),
                  ('supervisor', 'spawn_svc'), ('worker', 'worker_yield_2'), ('loop', 'irq'),
                  ('supervisor', 'spawn_svc'), ('worker', 'worker_yield_3'), ('loop', 'irq')]
        if immediate:
            for label in ('worker_yield_4', 'write_svc', 'exit_svc'):
                script += [('supervisor', 'irq'), ('worker', label), ('loop', 'irq')]
            script += [('supervisor', 'wait_svc'), ('loop', 'irq')]
        else:
            script += [('supervisor', 'wait_svc'), ('worker', 'worker_yield_4'), ('loop', 'irq'),
                       ('worker', 'write_svc'), ('loop', 'irq'), ('worker', 'exit_svc'), ('loop', 'irq')]
        script += [('supervisor', label) for label in ('cancel_svc', 'cancel_svc', 'wait_svc', 'write_svc', 'exit_svc')]
        for owner, label in script:
            step(owner, label)
    pcs.clear()
    step('hello', 'write_svc')
    step('hello', 'exit_svc')
    return ('\n'.join(lines) + '\n').encode()


def validate(raw=None, traced=None, execution=None, fresh=False):
    return h.validate(serial(fresh) if raw is None else raw, trace(fresh) if traced is None else traced,
                      old.EXECUTION if execution is None else execution, APPS, old.SYMBOLS, old.FIXUPS,
                      128, old.PAGES, fresh)


class TaskLauncherTests(unittest.TestCase):
    def test_main_and_fresh_real_call_inventory(self):
        for fresh in (False, True):
            result = validate(fresh=fresh)
            self.assertTrue(result['passed'], result)
            self.assertEqual([r['calls'] for r in result['trace']['supervisors']], [[9, 6, 0]] * 2)

    def test_wait_can_complete_immediately_after_real_child_progress(self):
        result = validate(traced=trace(immediate=True))
        self.assertTrue(result['passed'], result)

    def test_exact_resources_generations_and_outputs(self):
        for before, after in ((b'Native launches: 6', b'Native launches: 5'),
                              (b'Last prepared generation: 6', b'Last prepared generation: 3'),
                              (b'Free frames: 997', b'Free frames: 996'),
                              (b'worker complete', b'worker skipped'),
                              (b'Program not found.', b'bin/missing: exited (status 0).')):
            self.assertFalse(validate(raw=serial().replace(before, after, 1))['passed'])

    def test_every_command_and_prompt_required(self):
        raw = serial()
        for _, _, response in h.steps(old.PAGES):
            self.assertFalse(validate(raw=raw.replace(response + legacy.PROMPT, b'', 1))['passed'])
        self.assertFalse(validate(raw=raw + b'extra')['passed'])

    def test_real_svc_order_not_serial_claims(self):
        raw = trace()
        for owner, label in (('worker', 'worker_yield_4'), ('supervisor', 'spawn_svc'), ('supervisor', 'cancel_svc')):
            pc = APPS[owner]['symbols'][label] + 4
            changed = raw.replace(f'...with ELR {pc:#x}'.encode(), b'...with ELR 0x10000ffff', 1)
            self.assertFalse(validate(traced=changed)['passed'])

    def test_resume_and_source_el_cannot_be_forged(self):
        raw = trace()
        for before, after in ((b'...from EL0 to EL1', b'...from EL1 to EL1'),
                              (b'0x15/0x56000000', b'0x15/0x56000001'),
                              (b'to AArch64 EL0 PC 0x100000044', b'to AArch64 EL0 PC 0x100000000')):
            self.assertNotEqual(raw.replace(before, after, 1), raw)
            self.assertFalse(validate(traced=raw.replace(before, after, 1))['passed'])

    def test_timer_and_restore_are_mandatory(self):
        raw = trace()
        begin = raw.index(b'Taking exception 5 [IRQ] on CPU 0\n...from EL0')
        for needle in (b'gic_acknowledge_irq cpu 0 acknowledged irq 30\n',
                       b'gic_cpu_write cpu 0 iface write at 0x00000010 0x0000001e\n',
                       b'gic_cpu_write cpu 0 iface write at 0x00000000 0x00000000\n'):
            self.assertFalse(validate(traced=raw[:begin] + raw[begin:].replace(needle, b'', 1))['passed'])

    def test_missing_extra_transitions_and_host_failure(self):
        raw = trace()
        self.assertFalse(validate(traced=raw + raw)['passed'])
        self.assertFalse(validate(traced=raw[:raw.rfind(b'Exception return')])['passed'])
        for changes in ({'timed_out': True}, {'returncode': 1}, {'host_killed': True},
                        {'launch_error': 'missing'}, {'alive_after_last_prompt': False}):
            self.assertFalse(validate(execution={**old.EXECUTION, **changes})['passed'])


if __name__ == '__main__':
    unittest.main()
