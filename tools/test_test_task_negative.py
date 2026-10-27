#!/usr/bin/env python3
"""Synthetic ten-profile transitions and mutations; never substitute for raw guest evidence."""
import copy
import unittest

import test_task_negative as h
import test_task_cases as ordinary
from test_test_task_cases import SyntheticMatrix, SYMBOLS


class SyntheticNegative(SyntheticMatrix):
    def __init__(self,case):
        super().__init__();self.case=case;self.name=case;self.injected=False;self.retired=False;self.extra_reaped=0
        template=copy.deepcopy(self.fixtures['supervisor'])
        for name in h.PROFILES:self.fixtures[name]=copy.deepcopy(template)
        self.fixtures['legacy_hello']=copy.deepcopy(template);self.fixtures['legacy_hello']['symbols']['message']=0x100001100
        for fixture in self.fixtures.values():fixture['symbols']['loop_end']=fixture['symbols']['loop_start']+4
        self.layout['feature']='task-negative-tests'
        self.namespace=1;self.task_next=0xffffffff if case=='capability_exhaustion' else 1;self.console_next=1;self.last_console=0
    def emit(self,record,**values):
        self.lines.append('HMTASK '+record+''.join(f' {key}={values[key]}' for key in h.FIELDS[record].split()))
    def reap_child(self,cap):
        slot=self.caps[cap]['slot'];super().reap_child(cap)
        if self.next_slots[slot]==0x100000000:
            self.next_slots[slot]=0;self.slots[slot]=dict(slot=slot,generation=0,alive=False,frames=0)
    def syscall(self,t,spec):
        if spec.get('abandon'):
            t['calls']+=1
            self.emit('CALL',group=1,turn=t['ordinal'],ordinal=t['calls'],pc=t['pc'],version=1,operation=3,
                arg1=self.launch,arg2=0x1001ffe00,arg3=64)
            self.emit('SETTLE',group=1,turn=t['ordinal'],kind='failure',status=0,result=0,child_slot=0,child_generation=0,cap_slot=0,cap_generation=0)
            return None,0
        start=len(self.lines);result=super().syscall(t,spec)
        if spec.get('zero_invalid'):
            for index in range(start,len(self.lines)):
                if self.lines[index].startswith('HMTASK CALL '):
                    self.lines[index]=self.lines[index].replace('arg2=4297060352','arg2=18446744073709551615')
        return result
    def inject(self,t,kind):
        if self.injected:raise AssertionError('duplicate synthetic hook')
        self.injected=True;self.retired=kind!='final_budget'
        self.emit('INJECT',group=1,turn=t['ordinal'],kind=kind)
    def close_root(self,end,trigger=None):
        root=self.slots[0];root['alive']=False;root['end']=end;root['code']=0;root['pending']=None
        self.closure=dict(group=1,root_slot=0,root_generation=1,end=end,code=0,evidence='administrative' if trigger else 'executed',
            turn=0 if trigger else root['ordinal'],trigger_group=1 if trigger else 0,
            trigger_slot=trigger['slot'] if trigger else 0,trigger_generation=trigger['generation'] if trigger else 0)
        for child in self.slots.values():
            if child['slot'] and child['alive']:child['cancel']=True
    def manage(self):
        reaped=self.extra_reaped;self.extra_reaped=0;woken=0;root=self.slots[0]
        if self.closure:
            for cap,child in list(self.caps.items()):
                if not self.slots[child['slot']]['alive']:self.reap_child(cap);reaped+=1
        elif root['pending'] is not None:
            cap=root['pending'][0];child=self.slots[self.caps[cap]['slot']]
            if not child['alive']:self.reap_child(cap);root['pending']=None;reaped+=1;woken=1
        self.sequence+=1
        self.emit('MANAGE',group=1,sequence=self.sequence,woken=woken,reaped=reaped,remaining_children=len(self.caps),
                  closing=int(bool(self.closure)),charged=self.charge())
    def step(self):
        slot=next(i for n in range(4) if (i:=(self.cursor+n)%4) in self.slots and self.slots[i]['alive'] and
                  (self.slots[i]['cancel'] or self.slots[i]['pending'] is None))
        self.cursor=(slot+1)%4;t=self.slots[slot];t['ordinal']+=1;scope=dict(group=1,turn=t['ordinal']);before=self.free()
        self.emit('STEP',**scope,slot=slot,generation=t['generation'],state='cancel_pending' if t['cancel'] else 'ready',
            calls=t['calls'],written=t['written'],irqs=t['irqs'],remaining=t['remaining'],charged=t['charged'],turns=t['turns'],before=before)
        terminal='cancelled' if t['cancel'] else None;code=0;event='none';entered=not t['cancel'];injected_kind=None
        if entered:
            start=self.clock+10;whole=start+t['remaining']
            self.emit('CLOCK',**scope,begin=start,remaining=t['remaining'],comparator=start+1,whole_end=whole)
            self.emit('ENTER',**scope,pc=t['pc'],root=t['root'])
            if t['calls']<len(t['commands']):
                spec=t['commands'][t['calls']];t['pc']=self.f(t['name'])['symbols'][spec['site']]+4;kind='svc';event='svc';esr=0x56000000
            else:kind='irq';event='timer';esr=0;t['pc']=self.f(t['name'])['symbols']['loop_start']
            self.emit('TRAP',**scope,kind=kind,esr=esr,far=0,elr=t['pc'],spsr=0x00400340)
            if kind=='svc':terminal,code=self.syscall(t,spec)
            else:t['irqs']+=1;t['poll']+=1
            if not self.injected:
                if slot==0 and t['calls']==1 and self.case in ('final_budget_spawn','platform_reply'):
                    injected_kind='final_budget' if self.case.startswith('final') else 'retire'
                elif slot==0 and t['calls']==2 and self.case in ('final_budget_park','platform_park'):
                    injected_kind='final_budget' if self.case.startswith('final') else 'retire'
                elif slot==0 and t['calls']==2 and self.case=='abandon_dispatch':injected_kind='abandon'
                elif slot and kind=='irq' and self.case in ('child_retire_ready','child_retire_waiting'):
                    if (self.slots[0]['pending'] is None)==(self.case=='child_retire_ready'):injected_kind='child_retire'
            elapsed=1
            if injected_kind:
                self.inject(t,injected_kind)
                if injected_kind=='final_budget':elapsed=t['remaining'];terminal='budget'
                else:terminal='unavailable'
            t['remaining']-=elapsed;t['charged']+=elapsed;t['turns']+=1;self.clock=start+elapsed
            self.emit('CHARGE',**scope,end=self.clock,accounted=1,remaining=t['remaining'],charged=t['charged'],turns=t['turns'])
        wake=False;closed=False
        if terminal:
            t['alive']=False;t['end']=terminal;t['code']=code;t['pending']=None
            if slot==0:self.close_root(terminal);closed=True
            elif terminal=='unavailable':self.close_root('unavailable',t);closed=True
            elif terminal=='exit' and self.case=='wake_install_failure':
                cap=self.slots[0]['pending'][0];self.reap_child(cap);self.extra_reaped=1
                self.close_root('unavailable',t);closed=True;wake=True
        state='closing' if terminal and slot==0 else 'completed' if terminal else 'waiting' if t['pending'] else 'ready'
        self.emit('RETURN',**scope,event=event,terminal=terminal or 'none',state=state,entered=int(entered),code=code,
            calls=t['calls'],written=t['written'],irqs=t['irqs'],elr=t['pc'],remaining=t['remaining'],charged=t['charged'],turns=t['turns'],
            root=SYMBOLS['arm_root'],vbar=SYMBOLS['arm_vectors'],controls=1,before=before,after=self.free())
        if wake:self.inject(t,'wake_install')
        if closed:self.emit('CLOSE',**self.closure)
        self.manage()
    def generate(self):
        self.emit('PREFLIGHT',checks=15,irqs=8)
        if self.case=='capability_exhaustion':self.emit('SEED',kind='capability_max',value=0xffffffff)
        self.lines.append('HMTASK BEGIN 1')
        self.emit('INFO',wire=1,cpus=1,frequency=1000,total_ticks=100,quantum_ticks=1,process_bytes=9832,process_pages=3,
                  image_bytes=472,image_pages=1,authority_bytes=928,authority_pages=1)
        self.emit('CASE',index=0,**{'pass':0},name=self.case,before=1000)
        if self.case=='slot_exhaustion':self.emit('SEED',kind='child_slots_max',value=0xffffffff)
        self.g=1;self.next_slots=[1]+([0xffffffff]*3 if self.case=='slot_exhaustion' else [1]*3)
        self.cursor=0;self.sequence=0;self.closure=None;self.caps={};self.held=0;self.reaped=False
        self.launch=self.task_next<<32|2;self.task_next=0 if self.task_next==0xffffffff else 2
        root=self.owner(self.case,0,1,0x100000001,None,h.script(self.case));self.slots={0:root};self.console_next=2;self.last_console=1
        self.emit('GROUP',group=1,limit=512,metadata=3,before=1000,after=self.free())
        self.emit('ROOT',group=1,slot=0,generation=1,launch_generation=self.launch>>32,console_generation=1,fixture=self.case,
            entry=self.f(self.case)['entry'],root=root['root'],process_frames=root['frames']-3,box_frames=3,before=1000,after=self.free())
        for _ in range(1024):
            if self.closure and not self.caps:break
            self.step()
        else:raise AssertionError('Synthetic negative exceeded bound')
        self.manage();self.emit('PUBLISH',**self.closure,charged=3)
        self.emit('REAP_ROOT',group=1,outcome=ordinary.packed(self.closure['end'],self.closure['code']),before=self.free(),after=1000)
        self.emit('DROP',group=1,before=1000,after=1000)
        if self.case=='capability_exhaustion':
            self.emit('RETRY_SETUP',result='resources',before=1000,after=1000,published=0)
            f=self.fixtures['legacy_hello'];frames=f['pages']+29
            self.emit('LEGACY_BEGIN',entry=f['entry'],root=0x40010000,before=1000,during=1000-frames,frames=frames)
            self.lines += [f'HMPROC CALL index=0 ordinal=1 pc={f["symbols"]["write_svc"]+4} version=1 operation=1 arg1=8589934593 arg2={f["symbols"]["message"]} arg3=15',
                'HMPROC OUTPUT index=0 ordinal=1 length=15 hex='+b'hello from EL0\n'.hex(),
                'HMPROC RESULT index=0 ordinal=1 status=0 bytes=15',
                f'HMPROC CALL index=0 ordinal=2 pc={f["symbols"]["exit_svc"]+4} version=1 operation=2 arg1=0 arg2=0 arg3=0']
            self.emit('LEGACY_END',reason='exit',code=0,calls=2,written=15,root=SYMBOLS['arm_root'],vbar=SYMBOLS['arm_vectors'],controls=1,before=1000,after=1000)
        self.emit('PASS',index=0,**{'pass':0},name=self.case,before=1000,after=1000)
        self.lines += ['HMTASK END 1','HMTEST BEGIN 41']
        return ('\n'.join(self.lines)+'\n').encode()


class NegativeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.examples={}
        for case in h.PROFILES:
            generator=SyntheticNegative(case);cls.examples[case]=(generator,generator.generate())
    def check(self,case,raw=None):
        generator,original=self.examples[case]
        return h.validate_serial(original if raw is None else raw,generator.fixtures,SYMBOLS,generator.layout,case)
    def test_all_ten_complete_automata(self):
        for case in h.PROFILES:
            with self.subTest(case=case):
                report=self.check(case)
                self.assertEqual(report['retired'],case not in ('final_budget_spawn','final_budget_park','capability_exhaustion','slot_exhaustion'))
                self.assertEqual(report['injection'] is None,case.endswith('exhaustion'))
    def test_required_injection_seed_and_publication_records(self):
        for case,(_,raw) in self.examples.items():
            for prefix in (b'HMTASK SEED ' if case.endswith('exhaustion') else b'HMTASK INJECT ',b'HMTASK CLOSE ',b'HMTASK PUBLISH ',b'HMTASK REAP_ROOT '):
                line=next(v for v in raw.splitlines(keepends=True) if v.startswith(prefix))
                for replacement in (b'',line+line):
                    with self.subTest(case=case,prefix=prefix,replacement=bool(replacement)):
                        with self.assertRaises((ValueError,KeyError,IndexError)):self.check(case,raw.replace(line,replacement,1))
    def test_exhaustion_never_resets_or_forges_generation(self):
        for case in ('capability_exhaustion','slot_exhaustion'):
            raw=self.examples[case][1]
            for old,new in ((b'value=4294967295',b'value=4294967294'),(b'launch_generation=1 ',b'launch_generation=2 '),
                            (b'child_generation=4294967295',b'child_generation=1'),(b'status=9 result=0',b'status=0 result=0')):
                if old in raw:
                    with self.assertRaises(ValueError):self.check(case,raw.replace(old,new,1))
    def test_provisional_park_never_becomes_waiting_on_terminal_hook(self):
        for case in ('final_budget_park','platform_park'):
            raw=self.examples[case][1]
            with self.assertRaises(ValueError):self.check(case,raw.replace(b'state=closing entered=1',b'state=waiting entered=1',1))
    def test_actual_budget_threshold_and_accounting_required(self):
        raw=self.examples['final_budget_spawn'][1]
        for before,after in ((b'end=110 accounted=1',b'end=109 accounted=1'),(b'accounted=1',b'accounted=0'),
                             (b'kind=final_budget',b'kind=retire')):
            with self.assertRaises(ValueError):self.check('final_budget_spawn',raw.replace(before,after,1))
    def test_wake_failure_preserves_child_and_administrative_trigger(self):
        raw=self.examples['wake_install_failure'][1]
        for before,after in ((b'evidence=administrative turn=0',b'evidence=executed turn=2'),
                            (b'trigger_slot=1',b'trigger_slot=0'),(b'woken=0 reaped=1',b'woken=1 reaped=1'),
                            (b'terminal=exit state=completed',b'terminal=unavailable state=completed')):
            with self.assertRaises(ValueError):self.check('wake_install_failure',raw.replace(before,after,1))
    def test_legacy_is_exact_nested_same_executor_generation(self):
        raw=self.examples['capability_exhaustion'][1]
        for before,after in ((b'arg1=8589934593',b'arg1=4294967297'),(b'HMPROC RESULT index=0',b'HMPROC RESULT index=1'),
                            (b'result=resources',b'result=ok'),(b'written=15',b'written=14')):
            with self.assertRaises(ValueError):self.check('capability_exhaustion',raw.replace(before,after,1))
        line=next(v for v in raw.splitlines(keepends=True) if v.startswith(b'HMPROC OUTPUT'))
        with self.assertRaises(ValueError):self.check('capability_exhaustion',line+raw)
    def test_no_extra_entry_after_retirement(self):
        for case in ('platform_reply','platform_park','abandon_dispatch','child_retire_ready','child_retire_waiting'):
            raw=self.examples[case][1]
            line=b'HMTASK ENTER group=1 turn=999 pc=4294967296 root=1073807360\n'
            position=raw.index(b'HMTASK CLOSE')
            with self.assertRaises(ValueError):self.check(case,raw[:position]+line+raw[position:])
    def test_coordinated_owner_and_legacy_boot_root_aliases(self):
        raw=self.examples['platform_park'][1]
        boot=f'root={SYMBOLS["arm_root"]}'.encode()
        for address in (0x40010000,0x40011000):
            original=f'root={address}'.encode()
            self.assertGreater(raw.count(original),1)
            # Change both admission and every corresponding ENTER consistently.
            with self.assertRaises(ValueError):self.check('platform_park',raw.replace(original,boot))
        raw=self.examples['capability_exhaustion'][1]
        boundary=raw.index(b'HMTASK LEGACY_BEGIN')
        changed=raw[:boundary]+raw[boundary:].replace(b'root=1073807360',boot)
        self.assertNotEqual(raw,changed)
        with self.assertRaises(ValueError):self.check('capability_exhaustion',changed)

    def test_wrong_profile_and_unknown_records_rejected(self):
        raw=self.examples['platform_reply'][1]
        with self.assertRaises(ValueError):self.check('final_budget_spawn',raw)
        with self.assertRaises(ValueError):self.check('platform_reply',raw.replace(b'HMTASK CLOSE',b'HMTASK ADMIN',1))


if __name__=='__main__':unittest.main()
