#!/usr/bin/env python3
"""Synthetic idle grammar mutations only: these are not execution evidence."""
import struct
import unittest
from pathlib import Path
import tempfile
from unittest.mock import patch
import build_idle as b
import test_idle as h
from test_launcher import SETUP, SLICE_SETUP, SLICE_RESTORE
from test_kernel import CASES
from test_test_rw_copy import trace as baseline_trace
from test_test_rw_copy import SYMBOLS as OLD_SYMBOLS, FIXUPS

SYMBOLS = dict(OLD_SYMBOLS, arm_idle_wfi=0x40300000,arm_idle_resume=0x40300004,arm_idle_end=0x40300010,__exception_stack_top=0x40410000)
HELLO = dict(entry=0x100000000,symbols=dict(write_svc=0x100000100,exit_svc=0x100000110))
EXECUTION = dict(returncode=0,timed_out=False,log_limit_exceeded=False,launch_error=None,host_killed=False,elapsed_seconds=5)
INPUTS = [dict(case='pending_byte',hex='50',trigger='ready',time=1),dict(case='uart_wait',hex='55',trigger='wfi_attempt',time=2)]


def serial(mode='short'):
    cases = h.SHORT if mode == 'short' else ('production_wall',)
    lines = ['HMIDLE PREFLIGHT checks=15 irqs=8',f'HMIDLE BEGIN profile={mode} cases={len(cases)}']
    def fields(prefix, names, values, hexadecimal=()):
        lines.append(prefix + ' ' + ' '.join(k+'='+ (hex(values[k]) if k in hexadecimal else str(values[k])) for k in names))
    for i,name in enumerate(cases):
        expired = name == 'expired_limit'; entered = name not in ('expired_limit','pending_byte')
        deadline = name in ('expired_limit','timer_wait','production_wall')
        pre = 110 + i * 100
        limit = 100 if expired else 300100 if mode == 'wall' else pre + (5000 if name == 'uart_wait' else 1000)
        wake = (limit if deadline else pre+5) if entered else 0
        final = max(pre,wake) + 10
        # Preserve cross-case monotonicity after the timer case (which is last).
        counts = dict(frequency=1000,start=100,wall_end=300100,limit=limit,pre_arm=pre,precheck=pre,wake=wake,final=final,samples=2 if expired else 10 if name=='pending_byte' else 9,poisoned=0)
        lines.append(f'HMIDLE CASE index={i} name={name}')
        if name in ('pending_byte','uart_wait'): lines.append('HMIDLE READY name='+name)
        fields(f'HMIDLE COUNTS index={i}',h.COUNTS,counts)
        entry = dict.fromkeys(h.SNAP,0); entry.update(root=SYMBOLS['arm_root'],ctl=2,currentel=4,spsel=1,daif=0x3c0,pmr=255,tcr=0x80993519,mair=0xff,sctlr=0x3058180d,vbar=SYMBOLS['arm_vectors'],sp0=SYMBOLS['__exception_stack_top'])
        fields(f'HMIDLE SNAPSHOT index={i} phase=entry',h.SNAP,entry,h.SNAP)
        setup = {k:entry[k] for k in h.HARDWARE}; setup['changed']=0
        if not expired:
            for key,mask,value in h.MASKS: setup[key]=(entry[key]&~mask)|value
            setup.update(cval=limit,ctl=1,enable0=1<<30,enable1=2,imsc=0x7d0,dctl=1,cctl=1,changed=(1<<20)|(1<<21)|sum(1<<j for j in range(26,40) if j != 32))
        fields(f'HMIDLE SETUP index={i}',['changed']+h.HARDWARE,setup,['changed']+h.HARDWARE)
        for phase in ('precheck','post'):
            cause = dict.fromkeys(h.CAUSE,0)
            if not expired and (phase=='precheck' or entered):
                data = name=='pending_byte' or name=='uart_wait' and phase=='post'
                cause.update(sampled=1,fr=0 if data else 0x10,ris=0x10 if data else 0,mis=0x10 if data else 0,ctl=5 if deadline and phase=='post' else 1,count=wake if phase=='post' else pre)
            fields(f'HMIDLE CAUSE index={i} phase={phase}',h.CAUSE,cause,('fr','ris','mis','rsr','ctl'))
        wait=dict(entered=int(entered),expired=int(deadline),wfi=SYMBOLS['arm_idle_wfi'],resume=SYMBOLS['arm_idle_resume'],acks=0 if expired else 1,spurious=0 if expired else 1)
        fields(f'HMIDLE WAIT index={i}',list(wait),wait,('wfi','resume'))
        if not expired:
            ack=dict(iar=30 if deadline else 33,enable=0,source=2 if deadline else 0,pending_before=0,active_before=1,pending_after=0,active_after=0,counter_before=max(pre,wake)+1,counter_after=max(pre,wake)+2,polls_before=1,polls_after=1)
            fields(f'HMIDLE ACK index={i} ordinal=0',h.ACK,ack,('iar','source'))
        for irq in (30,33):
            clean=dict(id=irq,enable=0,pending=0,active=0,counter=0 if expired else max(pre,wake)+3,polls=0 if expired else 1)
            fields(f'HMIDLE CLEANUP index={i}',h.CLEAN,clean)
        fields(f'HMIDLE SNAPSHOT index={i} phase=restored',h.SNAP,entry,h.SNAP)
        lines.append(f'HMIDLE RESTORED index={i} controls=1')
        byte='50' if name=='pending_byte' else '55' if name=='uart_wait' else 'none'
        lines += [f'HMIDLE PUMP index={i} count={int(byte!="none")} dr={hex(int(byte,16)) if byte!="none" else "0x0"} hex={byte} empty=1',f'HMIDLE PASS index={i}']
    lines += [f'HMIDLE END cases={len(cases)}','HMIDLE HELLO BEGIN','hello from EL0','HMIDLE HELLO END status=0 generation=1 restored=1','HMTEST BEGIN 41']
    lines += ['HMTEST PASS '+name for name in CASES]+['HMTEST END 41','Bootstrap tests passed.']
    return ('\n'.join(lines)+'\n').encode()


def trace(mode='short'):
    lines=[]
    def writes(seq): lines.extend(f'gic_cpu_write cpu 0 iface write at {o:#x} {v:#x}' for o,v in seq)
    def returned(el,pc): lines.append(f'Exception return from AArch64 EL1 to AArch64 EL{el} PC {pc:#x}')
    def exception(number,name,el,pc,esr,vector):
        lines.extend([f'Taking exception {number} [{name}] on CPU 0',f'...from EL{el} to EL1',f'...with ESR {esr>>26:#x}/{esr:#x}',f'...with ELR {pc:#x}',f'...to EL1 PC {vector:#x} PSTATE 0x3c5'])
    def timer():
        writes(SETUP)
        for _ in range(8):
            exception(5,'IRQ',1,SYMBOLS['arm_timer_window_begin'],0,SYMBOLS['arm_vectors']+0x280)
            lines.append('gic_acknowledge_irq cpu 0 acknowledged irq 30'); writes(((16,30),)); returned(1,SYMBOLS['arm_timer_window_begin'])
        writes(((0,0),)+SETUP+((0,0),))
    timer()
    translated=False
    for case in h.validate_serial(serial(mode),SYMBOLS,mode):
        if case['name']=='expired_limit': continue
        writes(((4,255),(8,0),(0,1)))
        if case['wait']['entered']:
            for pc, offsets in ((SYMBOLS['arm_idle_wfi'],(0,)),(SYMBOLS['arm_idle_resume'],(4,)),(SYMBOLS['arm_idle_resume']+4,(8,12))):
                if not translated:
                    lines.extend(['----------------','IN:'])
                    lines.extend(f'{SYMBOLS["arm_idle_wfi"]+o:#x}:  {b.LEAF[o//4]:08x}  instruction' for o in offsets)
                    lines.append('')
                lines.append(f'Trace 0: 0x1234 [0000000000000000/{pc:016x}/00000000/00000000]')
            translated=True
        for ack in case['acks']:
            lines.append(f'gic_acknowledge_irq cpu 0 acknowledged irq {ack["iar"]}'); writes(((16,ack['iar']),))
        lines.append('gic_acknowledge_irq cpu 0 acknowledged irq 1023')
        writes(((0,0),(4,255),(8,0)))
    timer(); writes(SLICE_SETUP); returned(0,HELLO['entry'])
    for i,label in enumerate(('write_svc','exit_svc')):
        pc=HELLO['symbols'][label]+4
        exception(2,'SVC',0,pc,0x56000000,SYMBOLS['arm_process_vectors']+0x400)
        writes(SLICE_RESTORE)
        if i==0: writes(SLICE_SETUP); returned(0,pc)
    return ('\n'.join(lines)+'\n').encode()+baseline_trace()


def kernel():
    raw=bytearray(256)
    raw[:7]=b'\x7fELF\x02\x01\x01'; struct.pack_into('<HHI',raw,16,2,183,1)
    struct.pack_into('<Q',raw,32,64); struct.pack_into('<HH',raw,54,56,1)
    struct.pack_into('<IIQQQQQQ',raw,64,1,5,128,SYMBOLS['arm_idle_wfi']-4,0,20,20,4)
    struct.pack_into('<5I',raw,128,0x94000001,*b.LEAF)
    return raw


class IdleTests(unittest.TestCase):
    def test_full_short_and_real_duration_synthetic_contract(self):
        for mode in ('short','wall'):
            with self.subTest(mode=mode):
                cases=h.validate_serial(serial(mode),SYMBOLS,mode)
                result=h.validate_trace(trace(mode),SYMBOLS,FIXUPS,HELLO,cases)
                self.assertEqual(result['wfi'],2 if mode=='short' else 1)

    def test_all_records_required_once(self):
        raw=serial()
        for line in raw.splitlines(keepends=True):
            if line.startswith(b'HMIDLE'):
                for bad in (raw.replace(line,b'',1),raw.replace(line,line+line,1)):
                    with self.subTest(line=line),self.assertRaises(ValueError): h.validate_serial(bad,SYMBOLS,'short')

    def test_causal_clock_and_restore_mutations(self):
        raw=serial()
        for old,new in ((b'pending_before=0',b'pending_before=1'),(b'active_before=1',b'active_before=0'),(b'active_after=0',b'active_after=1'),(b'polls_before=1',b'polls_before=1025'),(b'iar=0x21',b'iar=0x3ff'),(b'entered=1',b'entered=0'),(b'poisoned=0',b'poisoned=1'),(b'dr=0x50',b'dr=0x150'),(b'hex=50',b'hex=55'),(b'wall_end=300100',b'wall_end=300101'),(b'controls=1',b'controls=0'),(b'spurious=1',b'spurious=0')):
            with self.subTest(old=old),self.assertRaises(ValueError): h.validate_serial(raw.replace(old,new,1),SYMBOLS,'short')
        with self.assertRaises(ValueError): h.validate_serial(serial('wall').replace(b'limit=300100',b'limit=1100'),SYMBOLS,'wall')

    def test_stopped_attempts_and_wrong_execution_order(self):
        raw=trace(); cases=h.validate_serial(serial(),SYMBOLS,'short')
        for old,new in ((b'Trace 0:',b'Trace 1:'),(b'0000000040300004',b'0000000040300000'),(b'd503207f',b'd503201f'),(b'acknowledged irq 33',b'acknowledged irq 30'),(b'write at 0x10 0x21',b'write at 0x10 0x3ff')):
            with self.subTest(old=old),self.assertRaises(ValueError): h.validate_trace(raw.replace(old,new,1),SYMBOLS,FIXUPS,HELLO,cases)
        for suffix in (b'Stopped execution of TB chain before 0x1234\n',b'Exception return from AArch64 EL1 to AArch64 EL0 PC 0x100000000\n'):
            with self.assertRaises(ValueError): h.validate_trace(raw+suffix,SYMBOLS,FIXUPS,HELLO,cases)
        for line in raw.splitlines(keepends=True):
            if line.startswith((b'Trace ',b'gic_',b'Exception return')):
                with self.assertRaises(ValueError): h.validate_trace(raw.replace(line,b'',1),SYMBOLS,FIXUPS,HELLO,cases)

    def test_execution_and_actual_input_gate(self):
        with patch.object(h,'TRACE_GRAMMAR_REVIEWED',True):
            self.assertTrue(h.validate(serial(),trace(),EXECUTION,SYMBOLS,FIXUPS,HELLO,'short',INPUTS)['passed'])
            for change in (dict(returncode=1),dict(returncode=None),dict(timed_out=True),dict(host_killed=True),dict(log_limit_exceeded=True),dict(launch_error='missing')):
                self.assertFalse(h.validate(serial(),trace(),{**EXECUTION,**change},SYMBOLS,FIXUPS,HELLO,'short',INPUTS)['passed'])
            for inputs in ([],INPUTS+INPUTS, [INPUTS[0],dict(INPUTS[1],trigger='ready')]):
                self.assertFalse(h.validate(serial(),trace(),EXECUTION,SYMBOLS,FIXUPS,HELLO,'short',inputs)['passed'])
        with patch.object(h,'TRACE_GRAMMAR_REVIEWED',False):
            self.assertFalse(h.validate(serial(),trace(),EXECUTION,SYMBOLS,FIXUPS,HELLO,'short',INPUTS)['passed'])

    def test_translation_block_must_finish_before_execution(self):
        raw=trace(); cases=h.validate_serial(serial(),SYMBOLS,'short')
        # Removing the terminator must not let the Trace branch discard an
        # unfinished block; merging WFI and resume also violates TB geometry.
        bad=raw.replace(b'instruction\n\nTrace ',b'instruction\nTrace ',1)
        with self.assertRaises(ValueError): h.validate_trace(bad,SYMBOLS,FIXUPS,HELLO,cases)
        extra=f'{SYMBOLS["arm_idle_resume"]:#x}:  {b.LEAF[1]:08x}  instruction\n'.encode()
        bad=raw.replace(b'instruction\n\nTrace ',b'instruction\n'+extra+b'\nTrace ',1)
        with self.assertRaises(ValueError): h.validate_trace(bad,SYMBOLS,FIXUPS,HELLO,cases)

    def test_exact_three_block_counter_continuation(self):
        raw=trace(); cases=h.validate_serial(serial(),SYMBOLS,'short')
        counter=next(line for line in raw.splitlines(keepends=True) if line.startswith(b'Trace ') and f'/{SYMBOLS["arm_idle_resume"]+4:016x}/'.encode() in line)
        isb=next(line for line in raw.splitlines(keepends=True) if line.startswith(b'Trace ') and f'/{SYMBOLS["arm_idle_resume"]:016x}/'.encode() in line)
        for bad in (raw.replace(counter,b'',1),raw.replace(counter,counter+counter,1),
                    raw.replace(isb,b'SWAP\n',1).replace(counter,isb,1).replace(b'SWAP\n',counter,1)):
            with self.assertRaises(ValueError): h.validate_trace(bad,SYMBOLS,FIXUPS,HELLO,cases)
        # Exact bytes are insufficient if an ISB translation is merged with
        # the counter block instead of the pinned three-block split.
        instruction=f'{SYMBOLS["arm_idle_resume"]+4:#x}:  {b.LEAF[2]:08x}  instruction\n'.encode()
        marker=f'{SYMBOLS["arm_idle_resume"]:#x}:  {b.LEAF[1]:08x}  instruction\n'.encode()
        bad=raw.replace(marker,marker+instruction,1)
        with self.assertRaises(ValueError): h.validate_trace(bad,SYMBOLS,FIXUPS,HELLO,cases)

    def test_final_log_sizes_include_stderr_after_exit(self):
        with tempfile.TemporaryDirectory(prefix='hematite-idle-size-') as directory:
            root=Path(directory)
            with patch.object(h,'SERIAL_LIMIT',8),patch.object(h,'TRACE_LIMIT',16):
                for name in ('serial.log','trace.log','stderr.log'): (root/name).write_bytes(b'x')
                self.assertEqual(h.log_sizes(root),{})
                for name,limit in (('serial.log',8),('trace.log',16),('stderr.log',16)):
                    (root/name).write_bytes(b'x'*(limit+1))
                    expected={name:limit+1}
                    if name!='serial.log': expected['combined_trace']=limit+2
                    self.assertEqual(h.log_sizes(root),expected)
                    (root/name).write_bytes(b'x')
                (root/'trace.log').write_bytes(b'x'*8)
                (root/'stderr.log').write_bytes(b'x'*8)
                self.assertEqual(h.log_sizes(root),{})
                (root/'stderr.log').write_bytes(b'x'*9)
                self.assertEqual(h.log_sizes(root),{'combined_trace':17})

    def test_source_closure_and_coherent_impossible_admission(self):
        paths=b.source_paths()
        self.assertTrue({'crates/hm-core/src/main.rs','crates/hm-core/src/cpu_local.rs',
            'crates/hm-core/src/threads.rs','crates/hm-kernel/src/native.rs',
            'Cargo.toml','Cargo.lock','.cargo/config.toml'} <= paths)
        raw=serial()
        for old,new in ((b'tcr=0x80993519',b'tcr=0x0'),(b'mair=0xff',b'mair=0x0'),
                        (b'sctlr=0x3058180d',b'sctlr=0x0'),(b'ttbr1=0x0',b'ttbr1=0x1000'),
                        (b'pmr=0xff',b'pmr=0x1ff'),(b'bpr=0x0',b'bpr=0x8')):
            # Change both entry and restored so equality alone cannot catch it.
            with self.subTest(old=old),self.assertRaises(ValueError): h.validate_serial(raw.replace(old,new),SYMBOLS,'short')

    def test_exact_admitted_frequency_boundaries(self):
        raw=serial()
        for frequency in (999,1000,10**9,10**9+1):
            changed=raw.replace(b'frequency=1000 ',f'frequency={frequency} '.encode()).replace(
                b'wall_end=300100 ',f'wall_end={100+frequency*300} '.encode())
            if frequency in (1000,10**9):
                self.assertEqual(len(h.validate_serial(changed,SYMBOLS,'short')),4)
            else:
                with self.assertRaises(ValueError): h.validate_serial(changed,SYMBOLS,'short')

    def test_single_cpu_target_is_read_only_zero(self):
        raw=serial()
        cases=h.validate_serial(raw,SYMBOLS,'short')
        self.assertTrue(all(case['entry']['target8']==0 for case in cases))
        for change in (raw.replace(b'target8=0x0',b'target8=0x100'),
                       raw.replace(b'target8=0x0',b'target8=0x1')):
            with self.assertRaises(ValueError): h.validate_serial(change,SYMBOLS,'short')
        dirty=(1<<20)|(1<<21)|sum(1<<j for j in range(26,40) if j!=32)
        changed=raw.replace(f'changed={dirty:#x}'.encode(),f'changed={dirty|(1<<32):#x}'.encode(),1)
        with self.assertRaises(ValueError): h.validate_serial(changed,SYMBOLS,'short')
        # UP correction must not waive the other programmed fields.
        for key,mask,value in h.MASKS:
            old=f'{key}={value:#x}'.encode(); new=f'{key}={value^mask:#x}'.encode()
            start=raw.index(b'HMIDLE SETUP index=0 '); end=raw.index(b'\n',start)
            line=raw[start:end]
            self.assertIn(old,line)
            changed=raw[:start]+line.replace(old,new,1)+raw[end:]
            with self.subTest(field=key),self.assertRaises(ValueError): h.validate_serial(changed,SYMBOLS,'short')

    def test_linked_leaf_and_incoming_control_flow(self):
        raw=kernel(); self.assertEqual(b.inspect(raw,SYMBOLS)['entry'],SYMBOLS['arm_idle_wfi'])
        for offset,value in ((128,0x94000002),(132,0xd503201f),(140,0xd53be040)):
            bad=bytearray(raw); struct.pack_into('<I',bad,offset,value)
            with self.assertRaises(ValueError): b.inspect(bad,SYMBOLS)
        with self.assertRaises(ValueError): b.inspect(raw,dict(SYMBOLS,arm_idle_resume=SYMBOLS['arm_idle_resume']+4))


if __name__=='__main__': unittest.main()
