#!/usr/bin/env python3
"""Mutations of ordinary group transcript and saved-PC transition evidence."""
import unittest

import test_group_launcher as h
import test_launcher as legacy
import test_test_launcher as existing


def serial(fresh=True, choice=1):
    raw = legacy.banner(128, existing.PAGES) + legacy.PROMPT
    for name, _, choices in h.steps(existing.PAGES, fresh):
        raw += choices[choice if name == 'echo_group' else 0] + legacy.PROMPT
    return raw


def trace(fresh=True, busy_irqs=3, peer=True, failed_second_enters=False):
    def prefix(fresh):
        original = existing.trace(fresh=fresh).decode().splitlines()
        first = next(i for i, line in enumerate(original) if 'to AArch64 EL0 PC' in line)
        return original[:first - 3]
    lines = prefix(fresh)
    pcs = [existing.APPS['echo']['entry']] * 2
    def writes(sequence):
        for offset, value in sequence:
            lines.append(f'gic_cpu_write cpu 0 iface write at {offset:#010x} {value:#010x}')
    for label in ('write_svc', 'newline_svc', 'exit_svc'):
        for index in range(2):
            writes(legacy.SLICE_SETUP)
            lines.append(f'Exception return from AArch64 EL1 to AArch64 EL0 PC {pcs[index]:#x}')
            pc = existing.APPS['echo']['symbols'][label] + 4
            lines += ['Taking exception 2 [SVC] on CPU 0', '...from EL0 to EL1',
                      '...with ESR 0x15/0x56000000', f'...with ELR {pc:#x}',
                      f'...to EL1 PC {existing.SYMBOLS["arm_process_vectors"] + 0x400:#x} PSTATE 0x3c5']
            writes(legacy.SLICE_RESTORE)
            pcs[index] = pc
    def hello_slice(entry, label):
        writes(legacy.SLICE_SETUP)
        lines.append(f'Exception return from AArch64 EL1 to AArch64 EL0 PC {entry:#x}')
        pc = existing.APPS['hello']['symbols'][label] + 4
        lines.extend(['Taking exception 2 [SVC] on CPU 0', '...from EL0 to EL1',
                      '...with ESR 0x15/0x56000000', f'...with ELR {pc:#x}',
                      f'...to EL1 PC {existing.SYMBOLS["arm_process_vectors"] + 0x400:#x} PSTATE 0x3c5'])
        writes(legacy.SLICE_RESTORE)
    if not fresh:
        if failed_second_enters:
            hello_slice(existing.APPS['hello']['entry'], 'write_svc')
        for index in range(max(busy_irqs, 2)):
            if index < busy_irqs:
                writes(legacy.SLICE_SETUP)
                entry = existing.APPS['loop']['entry'] if index == 0 else existing.APPS['loop']['symbols']['loop_start']
                lines.append(f'Exception return from AArch64 EL1 to AArch64 EL0 PC {entry:#x}')
                lines.extend(['Taking exception 5 [IRQ] on CPU 0', '...from EL0 to EL1',
                              '...with ESR 0x0/0x0', f'...with ELR {existing.APPS["loop"]["symbols"]["loop_start"]:#x}',
                              f'...to EL1 PC {existing.SYMBOLS["arm_process_vectors"] + 0x480:#x} PSTATE 0x3c5',
                              'gic_acknowledge_irq cpu 0 acknowledged irq 30'])
                writes(((16, 30),))
                writes(legacy.SLICE_RESTORE)
            if peer and index < 2:
                entry = existing.APPS['hello']['entry'] if index == 0 else existing.APPS['hello']['symbols']['write_svc'] + 4
                hello_slice(entry, 'write_svc' if index == 0 else 'exit_svc')
        lines += prefix(False)[:-len(prefix(True))]
        hello_slice(existing.APPS['hello']['entry'], 'write_svc')
        hello_slice(existing.APPS['hello']['symbols']['write_svc'] + 4, 'exit_svc')
    return ('\n'.join(lines) + '\n').encode()


def validate(raw=None, traced=None, execution=None, fresh=True):
    apps = {name: {**app, 'segments': [{'flags': 5, 'start': app['entry'], 'end': app['entry'] + 4096}]}
            for name, app in existing.APPS.items()}
    return h.validate(serial(fresh) if raw is None else raw, trace(fresh) if traced is None else traced,
                      existing.EXECUTION if execution is None else execution,
                      apps, existing.SYMBOLS, existing.FIXUPS, 128, existing.PAGES, fresh)


class GroupLauncherTests(unittest.TestCase):
    def test_complete_main_trace_and_busy_peer(self):
        result = validate(fresh=False)
        self.assertTrue(result['passed'], result)
        self.assertEqual(result['trace']['busy_peer']['irqs'], [3, 0])
        self.assertEqual(result['trace']['busy_peer']['calls'], [0, 2])

    def test_busy_peer_missing_progress_or_irq_and_failed_admission_entry(self):
        for options in ({'busy_irqs': 0}, {'busy_irqs': 1}, {'peer': False}, {'failed_second_enters': True}):
            self.assertFalse(validate(traced=trace(False, **options), fresh=False)['passed'])
        raw = trace(False)
        where = raw.index(b'Taking exception 5 [IRQ] on CPU 0\n...from EL0')
        prefix, busy = raw[:where], raw[where:]
        self.assertFalse(validate(traced=prefix + busy.replace(b'gic_acknowledge_irq cpu 0 acknowledged irq 30\n', b'', 1), fresh=False)['passed'])
        restore = b'gic_cpu_write cpu 0 iface write at 0x00000000 0x00000000\n'
        self.assertFalse(validate(traced=prefix + busy.replace(restore, b'', 1), fresh=False)['passed'])

    def test_exact_interleaved_fresh_profile(self):
        result = validate()
        self.assertTrue(result['passed'], result)
        self.assertEqual(result['trace']['echo']['calls'], [3, 3])
        self.assertEqual(result['trace']['echo']['slices'], 6)

    def test_all_serial_interleavings_but_trace_must_agree(self):
        for choice in range(4):
            self.assertEqual(h.validate_serial(serial(choice=choice), 128, existing.PAGES, True)['completed'], 2)
            self.assertEqual(validate(raw=serial(choice=choice))['passed'], choice == 1)

    def test_exact_completed_generation_and_frame_accounting(self):
        raw = serial(fresh=False)
        h.validate_serial(raw, 128, existing.PAGES)
        for old, new in ((b'Native launches: 5', b'Native launches: 4'),
                         (b'Last prepared generation: 6', b'Last prepared generation: 5'),
                         (b'Free frames: 997', b'Free frames: 996')):
            with self.assertRaises(ValueError):
                h.validate_serial(raw.replace(old, new, 1), 128, existing.PAGES)

    def test_every_interaction_and_prompt_required(self):
        raw = serial()
        for name, _, choices in h.steps(existing.PAGES, True):
            selected = choices[1 if name == 'echo_group' else 0]
            self.assertFalse(validate(raw=raw.replace(selected + legacy.PROMPT, b'', 1))['passed'])
            self.assertFalse(validate(raw=raw.replace(selected, selected + selected, 1))['passed'])
        self.assertFalse(validate(raw=raw + b'extra')['passed'])

    def test_resume_cannot_restart_or_switch_to_peer_pc(self):
        raw = trace()
        for new in (b'0x100000000', b'0x10000005c'):
            self.assertFalse(validate(traced=raw.replace(
                b'to AArch64 EL0 PC 0x100000044', b'to AArch64 EL0 PC ' + new, 1))['passed'])

    def test_missing_extra_and_wrong_exception(self):
        raw = trace()
        lines = raw.splitlines(keepends=True)
        for index, line in enumerate(lines):
            if line.startswith(b'Exception return') and b'EL0' in line:
                self.assertFalse(validate(traced=b''.join(lines[:index] + lines[index + 1:]))['passed'])
        self.assertFalse(validate(traced=raw + raw)['passed'])
        self.assertFalse(validate(traced=raw.replace(b'...from EL0 to EL1', b'...from EL1 to EL1', 1))['passed'])
        self.assertFalse(validate(traced=raw.replace(b'0x15/0x56000000', b'0x15/0x56000001', 1))['passed'])

    def test_execution_failure_cannot_hide_behind_valid_bytes(self):
        for changes in ({'timed_out': True}, {'returncode': 1}, {'host_killed': True},
                        {'launch_error': 'missing'}, {'alive_after_last_prompt': False}):
            self.assertFalse(validate(execution={**existing.EXECUTION, **changes})['passed'])


if __name__ == '__main__':
    unittest.main()
