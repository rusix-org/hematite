#!/usr/bin/env python3
"""Failure-sensitive task wire, fixed scripts, and a complete parked-child lifecycle."""
import copy
import struct
import unittest

import test_task_cases as h

SYMBOLS = {'arm_root': 0x40001000, 'arm_vectors': 0x40200000, 'arm_process_vectors': 0x40210000}
INFO = dict(total_ticks=100, quantum_ticks=1)
LAYOUT = {'fields': {'box_backing_pages': 3}, 'task_fields': {'metadata_pages': 3}}
FIXTURES = {name: dict(entry=0x100000000, pages=3,
    segments=[dict(flags=5, va=0x100000000, filesz=4096)],
    symbols={'spawn_svc': 0x100000040, 'wait_svc': 0x100000050, 'exit_svc': 0x100000060,
             'loop_start': 0x100000080, 'worker_message': 0x100001000}) for name in ('wait_budget_child', 'loop', 'worker')}


def parked_lifecycle():
    lines = []
    def emit(record, **values):
        lines.append('HMTASK ' + record + ''.join(f' {key}={values[key]}' for key in h.FIELDS[record].split()))
    group, launch, console = 7, 5, 4
    emit('GROUP', group=group, limit=512, metadata=3, before=1000, after=962)
    emit('ROOT', group=group, slot=0, generation=1, launch_generation=launch, console_generation=console,
         fixture='wait_budget_child', entry=0x100000000, root=0x40010000, process_frames=32, box_frames=3, before=1000, after=962)
    def begin(slot, turn, calls, irqs, remaining, charged, turns, free, start, pc, trap_pc, kind):
        emit('STEP', group=group, turn=turn, slot=slot, generation=1, state='ready', calls=calls, written=0, irqs=irqs,
             remaining=remaining, charged=charged, turns=turns, before=free)
        emit('CLOCK', group=group, turn=turn, begin=start, remaining=remaining, comparator=start+1, whole_end=start+remaining)
        emit('ENTER', group=group, turn=turn, pc=pc, root=0x40010000+slot*4096)
        emit('TRAP', group=group, turn=turn, kind=kind, esr=0x56000000 if kind=='svc' else 0, far=0, elr=trap_pc, spsr=0x00400340)
    def finish(slot, turn, calls, irqs, remaining, charged, turns, before, after, end, pc, event, terminal='none', state='ready', code=0):
        emit('CHARGE', group=group, turn=turn, end=end, accounted=1, remaining=remaining, charged=charged, turns=turns)
        emit('RETURN', group=group, turn=turn, event=event, terminal=terminal, state=state, entered=1, code=code,
             calls=calls, written=0, irqs=irqs, elr=pc, remaining=remaining, charged=charged, turns=turns,
             root=SYMBOLS['arm_root'], vbar=SYMBOLS['arm_vectors'], controls=1, before=before, after=after)
    def manage(sequence, charged, children, woken=0, reaped=0, closing=0):
        emit('MANAGE', group=group, sequence=sequence, woken=woken, reaped=reaped, remaining_children=children, closing=closing, charged=charged)
    def call(turn, ordinal, pc, op, a, b, c):
        emit('CALL', group=group, turn=turn, ordinal=ordinal, pc=pc, version=1, operation=op, arg1=a, arg2=b, arg3=c)
    def settle(turn, kind='reply', result=0, child=0, cap=0, gen=0):
        emit('SETTLE', group=group, turn=turn, kind=kind, status=0, result=result,
             child_slot=child, child_generation=int(child!=0), cap_slot=cap, cap_generation=gen)
    begin(0,1,0,0,100,0,0,962,10,0x100000000,0x100000044,'svc')
    call(1,1,0x100000044,3,launch<<32|2,0x1001fff00,64)
    descriptor = struct.pack('<IIIIQQQI',1,64,2,2,0,0,128,3)+bytes(20)
    emit('COPY',group=group,turn=1,part='descriptor',length=64,hex=descriptor.hex())
    emit('COMMIT',group=group,turn=1,cap_slot=3,cap_generation=6,child_slot=1,child_generation=1,program=2,
         console_generation=5,entry=0x100000000,root=0x40011000,process_frames=32,box_frames=3,before=962,after=927)
    settle(1,result=6<<32|3)
    finish(0,1,1,0,99,1,1,962,927,11,0x100000044,'svc');manage(1,73,1)
    begin(1,1,0,0,100,0,0,927,20,0x100000000,0x100000080,'irq')
    finish(1,1,0,1,99,1,1,927,927,21,0x100000080,'timer');manage(2,73,1)
    begin(0,2,1,0,99,1,1,927,30,0x100000044,0x100000054,'svc')
    call(2,2,0x100000054,4,6<<32|3,0,0);settle(2,'park',child=1,cap=3,gen=6)
    finish(0,2,2,0,98,2,2,927,927,31,0x100000054,'svc',state='waiting');manage(3,73,1)
    for op,result in (('task_completion','none'),('task_reap','pending')):
        emit('GATE',group=group,phase='waiting',op=op,slot=0,generation=1,result=result,before=927,after=927,
             charged_before=73,charged_after=73,task_next_before=7,task_next_after=7,console_before=5,console_after=5)
    begin(1,2,0,1,99,1,1,927,40,0x100000080,0x100000080,'irq')
    finish(1,2,0,2,0,100,2,927,962,139,0x100000080,'timer',terminal='budget',state='completed');manage(4,38,0,woken=1,reaped=1)
    begin(0,3,2,0,98,2,2,962,150,0x100000054,0x100000064,'svc')
    call(3,3,0x100000064,2,0,0,0);settle(3,'terminate')
    finish(0,3,3,0,97,3,3,962,997,151,0x100000064,'svc',terminal='exit',state='closing')
    close = dict(group=group,root_slot=0,root_generation=1,end='exit',code=0,evidence='executed',turn=3,
                 trigger_group=0,trigger_slot=0,trigger_generation=0)
    emit('CLOSE',**close);manage(5,3,0,closing=1)
    manage(6,3,0,closing=1);emit('PUBLISH',**close,charged=3)
    emit('REAP_ROOT',group=group,outcome=0,before=997,after=1000)
    emit('DROP',group=group,before=1000,after=1000)
    return ('\n'.join(lines)+'\nHMTASK END 50\n').encode()


def check_lifecycle(raw):
    records=h.Records(raw)
    model=h.Model(records,FIXTURES,SYMBOLS,LAYOUT,INFO,1000)
    model.begin('wait_budget_child');model.group['case']='wait_budget_child';model.group['recovery']=False
    methods={'STEP':model.step,'GATE':model.gate,'MANAGE':model.publish,'REAP_ROOT':model.reap_root,'DROP':model.finish_group}
    while model.group is not None: methods[records.kind()]()
    records.literal('HMTASK END 50')
    h.require(records.index==len(records.rows),'Extra lifecycle records')
    return model


class TaskCaseTests(unittest.TestCase):
    def test_parked_budget_child_complete_lifecycle(self):
        model=check_lifecycle(parked_lifecycle())
        self.assertEqual(len(model.slices),5)
        self.assertEqual(model.free,1000)
        root=model.history[0]['root']
        self.assertEqual((root['calls'],root['charged'],root['wakes']),(3,3,1))

    def test_no_metadata_substitute_for_calls_and_timer(self):
        raw=parked_lifecycle()
        for line in raw.splitlines(keepends=True):
            if line.startswith((b'HMTASK ENTER',b'HMTASK TRAP',b'HMTASK CALL',b'HMTASK COMMIT')):
                with self.assertRaises((ValueError,KeyError)): check_lifecycle(raw.replace(line,b'',1))

    def test_wait_resume_ownership_and_counter_mutations(self):
        raw=parked_lifecycle()
        mutations=((b'pc=4294967380 root=1073807360',b'pc=4294967296 root=1073807360'),
                   (b'woken=1 reaped=1',b'woken=0 reaped=1'),
                   (b'end=151 accounted=1 remaining=97 charged=3',b'end=151 accounted=1 remaining=0 charged=100'),
                   (b'cap_generation=6 child_slot=1',b'cap_generation=5 child_slot=1'),
                   (b'evidence=executed',b'evidence=administrative'),
                   (b'after=1000',b'after=999'))
        for before,after in mutations:
            self.assertIn(before,raw)
            with self.assertRaises(ValueError): check_lifecycle(raw.replace(before,after,1))

    def test_publication_requires_separate_management_and_metadata_reap(self):
        raw=parked_lifecycle()
        for prefix in (b'HMTASK CLOSE',b'HMTASK MANAGE group=7 sequence=6',b'HMTASK PUBLISH',b'HMTASK REAP_ROOT'):
            line=next(line for line in raw.splitlines(keepends=True) if line.startswith(prefix))
            with self.assertRaises((ValueError,KeyError)): check_lifecycle(raw.replace(line,b'',1))

    def test_global_clock_and_per_owner_deadlines(self):
        raw=parked_lifecycle()
        for before,after in ((b'begin=150',b'begin=138'),(b'comparator=151',b'comparator=152'),
                             (b'whole_end=248',b'whole_end=249'),(b'accounted=1',b'accounted=0')):
            with self.assertRaises(ValueError): check_lifecycle(raw.replace(before,after,1))

    def test_timer_cannot_finish_before_actual_comparator(self):
        raw=parked_lifecycle().replace(b'end=21 accounted=1 remaining=99 charged=1 turns=1',
                                     b'end=20 accounted=1 remaining=100 charged=0 turns=1',1)
        raw=raw.replace(b'irqs=1 elr=4294967424 remaining=99 charged=1 turns=1',
                        b'irqs=1 elr=4294967424 remaining=100 charged=0 turns=1',1)
        records=h.Records(raw);model=h.Model(records,FIXTURES,SYMBOLS,LAYOUT,INFO,1000)
        model.begin('wait_budget_child');model.group['case']='wait_budget_child';model.group['recovery']=False
        model.step()
        with self.assertRaisesRegex(ValueError,'before programmed comparator'):model.step()

    def test_exact_schema_and_no_injection_profiles(self):
        row='HMTASK CHARGE group=7 turn=1 end=10 accounted=1 remaining=99 charged=1 turns=1'
        self.assertIsInstance(h.fields(row,'CHARGE')['end'],int)
        for changed in (row+' extra=1',row.replace('end=10','end=010'),row.replace('end=10','end=18446744073709551616')):
            with self.assertRaises(ValueError): h.fields(changed,'CHARGE')
        for kind in ('ADMIN','INJECT','SEED'):
            with self.assertRaises(ValueError): h.fields('HMTASK '+kind,kind)
        for raw in (b'HMTASK BEGIN 50\nnoise\n',b'HMTASK END 50\nHMTASK BEGIN 50\n',b'HMTASK BEGIN 50'):
            with self.assertRaises(ValueError): h.Records(raw)

    def test_fixed_case_call_inventory(self):
        names=list(h.CASES);names.remove('rights_console');names+=['rights_console_policy','rights_console_wait_only']
        expected=[9,11,8,3,6,7,12,20,9,5,11,4,6,6,3,2,2,64,2,2,64,64,64,2,4,4]
        self.assertEqual([len(h.script(name,35)) for name in names],expected)
        for name in ('parent_call_limit','limit_spawn','limit_wait','limit_cancel'):
            commands=h.script(name,35)
            self.assertEqual(len(commands),64);self.assertTrue(commands[-1]['limit'])
        self.assertEqual([c['op'] for c in h.script('held_capacity',35)],[3,3,5,5,3,4,3,4,4,4,2])

    def test_child_modes_are_exact_and_consoleless_has_no_write(self):
        self.assertEqual(h.child_script(None,0),[h.exit_call(10)])
        self.assertEqual(h.child_script(b'',0),[h.exit_call(11)])
        self.assertEqual(h.child_script(b'HMFAULT\0',1),[])
        self.assertEqual(len(h.child_script(b'HMAUTH\0\0'+struct.pack('<3Q',2,3,1),1)),9)
        self.assertEqual(len(h.child_script(bytes(range(256)),1)),6)

    def test_consoleless_prepare_burns_generation_without_last_prepared(self):
        lines = parked_lifecycle().decode().splitlines()
        end = next(i for i, line in enumerate(lines) if line.startswith('HMTASK SETTLE'))
        lines = lines[:end+1]
        descriptor = struct.pack('<IIIIQQQI',1,64,1,0,0,0,128,3)+bytes(20)
        lines = [line.replace('program=2 console_generation=5','program=1 console_generation=0') for line in lines]
        lines = [line.split('hex=')[0] + 'hex=' + descriptor.hex() if line.startswith('HMTASK COPY') else line for line in lines]
        records=h.Records(('\n'.join(lines)+'\nHMTASK END 50\n').encode())
        model=h.Model(records,FIXTURES,SYMBOLS,LAYOUT,INFO,1000)
        model.begin('wait_budget_child');model.group['case']='rights_console_policy'
        task=model.group['slots'][0]
        task['commands']=[h.spawn('a',payload=None,console=False)]
        step=records.take('STEP');records.take('CLOCK');records.take('ENTER');records.take('TRAP')
        task['pc']=0x100000044
        model.request(task,step)
        self.assertEqual((model.next_console,model.last_console),(6,4))
        self.assertEqual(model.group['slots'][1]['console'],0)

    def test_fault_is_lower_el_and_exact_own_rx_site(self):
        fixture={'entry':0x100000000,'symbols':{'fault':0x100000080}}
        trap={'esr':0x9200004f,'far':0x100000000,'elr':0x100000080,'spsr':0x00400340}
        task={'fixture':'worker','payload':b'HMFAULT\0'}
        h.check_fault(task,fixture,trap)
        for change in ({'esr':0x9600004f},{'esr':0x9200000f},{'far':0x40000000},
                       {'elr':0x100000084},{'spsr':0x10400340}):
            with self.assertRaises(ValueError):h.check_fault(task,fixture,{**trap,**change})

    def test_descriptor_mutation_requires_all_exact_bytes(self):
        spec=h.spawn('a',2)
        raw=struct.pack('<IIIIQQQI',1,64,2,2,0,0,128,3)+bytes(20)
        h.descriptor_bytes(spec,raw,FIXTURES['worker'],None)
        for index in range(64):
            changed=bytearray(raw);changed[index]^=0x80
            with self.assertRaises(ValueError):h.descriptor_bytes(spec,bytes(changed),FIXTURES['worker'],None)



# Synthetic protocol generator: no VM evidence. It shares the immutable request
# inventory, but independently executes ownership/scheduling and formats records.
class SyntheticMatrix:
    def __init__(self):
        self.lines=[];self.clock=0;self.namespace=7;self.task_next=5;self.console_next=4;self.last_console=3
        self.fixtures={}
        labels=('spawn_svc','wait_svc','cancel_svc','write_svc','exit_svc','raw_svc','overwrite_svc',
                'worker_yield_1','worker_yield_2','worker_yield_3','worker_yield_4','fault','loop_start')
        for name in h.builder.APPS:
            self.fixtures[name]=dict(entry=0x100000000,pages=228 if name=='large_root' else 96 if name=='large_worker' else 3,
                segments=[dict(flags=5,va=0x100000000,filesz=4096)],
                symbols={label:0x100000040+i*8 for i,label in enumerate(labels)} | {
                    'worker_message':0x100001000,'descriptor_storage':0x100002000,'input_storage':0x100002100})
        self.layout={'fields':{'process_bytes':9832,'box_backing_pages':3},
                     'task_fields':{'image_bytes':472,'image_pages':1,'authority_bytes':928,'authority_pages':1,'metadata_pages':3}}
        self.symbols=SYMBOLS
    def emit(self,record,**values):
        self.lines.append('HMTASK '+record+''.join(f' {key}={values[key]}' for key in h.FIELDS[record].split()))
    def f(self,name):return self.fixtures['large_root' if name=='group_frame_cap' else name]
    def owner(self,name,slot,generation,console,payload,commands):
        return dict(name=name,slot=slot,generation=generation,console=console,payload=payload,commands=commands,
            root=0x40010000+slot*4096,pc=self.f(name)['entry'],calls=0,written=0,irqs=0,charged=0,remaining=100,turns=0,
            ordinal=0,alive=True,cancel=False,pending=None,end=None,code=0,aliases={},poll=0,frames=self.f(name)['pages']+32)
    def charge(self):return (0 if self.reaped else 3)+sum(t['frames'] for t in self.slots.values() if t['alive'])
    def free(self):return 1000-self.held-self.charge()
    def gate(self,phase,ops,target=0):
        for op in ops:
            result='none' if phase in ('waiting','closing') and op=='task_completion' else 'pending' if phase in ('waiting','closing') else 'idle' if op=='turn' else 'mode'
            self.emit('GATE',group=self.g,phase=phase,op=op,slot=target,generation=self.slots[target]['generation'],result=result,
                before=self.free(),after=self.free(),charged_before=self.charge(),charged_after=self.charge(),
                task_next_before=self.task_next,task_next_after=self.task_next,console_before=self.last_console,console_after=self.last_console)
    def reap_child(self,cap):
        slot=self.caps.pop(cap)['slot'];self.slots.pop(slot);self.next_slots[slot]+=1
    def manage(self):
        reaped=woken=0;root=self.slots[0]
        if self.closure:
            for cap,child in list(self.caps.items()):
                if not self.slots[child['slot']]['alive']:self.reap_child(cap);reaped+=1
        elif root['pending'] is not None and not root['cancel']:
            cap=root['pending'][0];child=self.slots[self.caps[cap]['slot']]
            if not child['alive']:self.reap_child(cap);root['pending']=None;reaped=woken=1
        self.sequence+=1
        self.emit('MANAGE',group=self.g,sequence=self.sequence,woken=woken,reaped=reaped,remaining_children=len(self.caps),
                  closing=int(bool(self.closure)),charged=self.charge())
    def descriptor(self,spec,payload):
        pointer=0 if not payload else 0x100001000
        if spec.get('unaligned'):pointer=self.f(self.name)['symbols']['input_storage']+1
        length=0 if payload is None else len(payload)
        if 'bad_input' in spec:pointer,length=spec['bad_input']
        data=bytearray(struct.pack('<IIIIQQQI',1,64,spec['program'],int(payload is not None)|int(spec['console'])<<1,
                                   pointer,length,spec['frames'],spec['rights'])+bytes(20))
        n=spec.get('mutation')
        if n is not None:
            offsets={0:(0,2),1:(4,63),2:(12,7),3:(44,1),4:(48,1),5:(56,1),6:(40,0),7:(40,2),13:(8,3)}
            if n in offsets:offset,value=offsets[n];data[offset]=value
            elif n in (8,9):data[32:40]=struct.pack('<Q',0 if n==8 else 129)
            elif n==10:data[12]=2;data[24:32]=bytes(8)
            elif n==11:data[24:32]=bytes(8)
            elif n==12:data[8:12]=bytes(4)
            elif n==14:data[24:32]=struct.pack('<Q',257)
            elif n==15:data[16:24]=struct.pack('<Q',2**64-1)
            elif n==16:data[0]=2;data[24:32]=struct.pack('<Q',257)
        return bytes(data)
    def syscall(self,t,spec):
        t['calls']+=1;ordinal=t['calls'];op=spec['op'];turn=t['ordinal'];scope=dict(group=self.g,turn=turn)
        payload=spec.get('payload');status=spec.get('status',0);result=spec.get('result',0)
        if payload=='foreign':payload=b'HMAUTH\0\0'+struct.pack('<3Q',self.launch,t['aliases']['a'],t['console'])
        if spec.get('outer'):
            authority={'zero':0,'console':t['console'],'launch':self.launch,'unknown':0xffffffff00000002}
            a=authority.get(spec['authority'],spec['authority']);b=spec['address'];c=spec['length']
        elif op==1:a=t['console'];b=2**64-1 if spec['site']!='write_svc' else 0x1001ff000;c=len(payload)
        elif op==2:a=spec['code'];b=c=0
        elif op==3:a=self.launch;b=self.f(t['name'])['symbols']['descriptor_storage']+1 if spec.get('unaligned') else 0x1001ffe00;c=64
        else:a=t['aliases'][spec['alias']];b=spec.get('mode',0);c=0
        self.emit('CALL',**scope,ordinal=ordinal,pc=t['pc'],version=spec.get('version',1),operation=op,arg1=a,arg2=b,arg3=c)
        if spec.get('limit'):
            self.emit('SETTLE',**scope,kind='terminate',status=0,result=0,child_slot=0,child_generation=0,cap_slot=0,cap_generation=0)
            return 'call_limit',0
        if op==3 and not spec.get('outer'):
            data=self.descriptor(spec,payload);self.emit('COPY',**scope,part='descriptor',length=64,hex=data.hex())
            if status in (0,9) and payload:self.emit('COPY',**scope,part='input',length=len(payload),hex=payload.hex())
            if status==0:
                slot=next(i for i in range(1,4) if i not in self.slots);cap=next(i for i in (3,4) if i not in self.caps)
                name=('large_worker' if self.name=='group_frame_cap' else 'worker') if spec['program']==1 else ('large_loop' if self.name=='group_frame_cap' else 'loop')
                console=self.console_next if spec['console'] else 0
                child=self.owner(name,slot,self.next_slots[slot],console<<32|1 if console else 0,payload,
                                 h.child_script(payload,console<<32|1 if console else 0) if spec['program']==1 else [])
                before=self.free();self.slots[slot]=child
                self.emit('COMMIT',**scope,cap_slot=cap,cap_generation=self.task_next,child_slot=slot,child_generation=child['generation'],
                    program=spec['program'],console_generation=console,entry=self.f(name)['entry'],root=child['root'],
                    process_frames=child['frames']-3,box_frames=3,before=before,after=self.free())
                result=self.task_next<<32|cap;self.task_next+=1
                if console:self.last_console=self.console_next
                self.console_next+=1;self.caps[cap]=dict(slot=slot,generation=result>>32);t['aliases'][spec['alias']]=result
            elif 'pressure' in spec:
                self.task_next+=1
                if spec['pressure']:
                    if spec['pressure']==2:self.last_console=self.console_next
                    self.console_next+=1
        if op==1 and not spec.get('outer'):
            if payload:self.emit('OUTPUT',**scope,ordinal=ordinal,length=len(payload),hex=payload.hex())
            t['written']+=len(payload)
        if op==4 and not spec.get('outer') and status==0:
            token=t['aliases'][spec['alias']];cap=token&0xffffffff;target=self.slots[self.caps[cap]['slot']]
            if target['alive']:t['pending']=(cap,result)
            else:
                self.emit('CONSUME',**scope,child_slot=target['slot'],child_generation=target['generation'],cap_slot=cap,
                          cap_generation=token>>32,outcome=result)
                self.reap_child(cap)
        if t['pending'] and op==4 and status==0:
            cap=t['pending'][0];target=self.slots[self.caps[cap]['slot']]
            self.emit('SETTLE',**scope,kind='park',status=0,result=0,child_slot=target['slot'],child_generation=target['generation'],
                      cap_slot=cap,cap_generation=self.caps[cap]['generation'])
        else:self.emit('SETTLE',**scope,kind='terminate' if op==2 else 'reply',status=status,result=result,
                       child_slot=0,child_generation=0,cap_slot=0,cap_generation=0)
        if op==5 and not spec.get('outer') and status==0:
            target=self.slots[self.caps[t['aliases'][spec['alias']]&0xffffffff]['slot']]
            if target['alive']:target['cancel']=True
        return ('exit',spec['code']) if op==2 else (None,0)
    def step(self):
        slot=next(i for n in range(4) if (i:=(self.cursor+n)%4) in self.slots and self.slots[i]['alive'] and
                  (self.slots[i]['cancel'] or self.slots[i]['pending'] is None))
        self.cursor=(slot+1)%4;t=self.slots[slot];t['ordinal']+=1;scope=dict(group=self.g,turn=t['ordinal'])
        before=self.free()
        self.emit('STEP',**scope,slot=slot,generation=t['generation'],state='cancel_pending' if t['cancel'] else 'ready',calls=t['calls'],
            written=t['written'],irqs=t['irqs'],remaining=t['remaining'],charged=t['charged'],turns=t['turns'],before=before)
        terminal='cancelled' if t['cancel'] else None;code=0;event='none';entered=not t['cancel']
        if entered:
            start=self.clock+10;self.emit('CLOCK',**scope,begin=start,remaining=t['remaining'],comparator=start+1,whole_end=start+t['remaining'])
            self.emit('ENTER',**scope,pc=t['pc'],root=t['root'])
            if t['calls']<len(t['commands']):
                spec=t['commands'][t['calls']];t['pc']=self.f(t['name'])['symbols'][spec['site']]+4;kind='svc';esr=0x56000000;far=0;event='svc'
            elif t['name']=='parent_fault' or t['payload']==b'HMFAULT\0':
                kind='fault';t['pc']=self.f(t['name'])['symbols']['fault'];esr=0x9200004f;far=self.f(t['name'])['entry'];event='fault'
            else:kind='irq';t['pc']=self.f(t['name'])['symbols']['loop_start'];esr=far=0;event='timer'
            self.emit('TRAP',**scope,kind=kind,esr=esr,far=far,elr=t['pc'],spsr=0x00400340)
            elapsed=1
            if kind=='svc':terminal,code=self.syscall(t,spec)
            elif kind=='fault':terminal='fault'
            else:
                t['irqs']+=1;t['poll']+=1
                if t['poll']>=2 and (t['name']=='parent_budget' or (self.name=='wait_budget_child' and slot!=0)):
                    elapsed=t['remaining'];terminal='budget'
            t['remaining']-=elapsed;t['charged']+=elapsed;t['turns']+=1;self.clock=start+elapsed
            self.emit('CHARGE',**scope,end=self.clock,accounted=1,remaining=t['remaining'],charged=t['charged'],turns=t['turns'])
        if terminal:
            t['alive']=False;t['end']=terminal;t['code']=code;t['pending']=None
            if slot==0:
                self.closure=dict(group=self.g,root_slot=0,root_generation=1,end=terminal,code=code,evidence='executed',turn=t['ordinal'],
                                  trigger_group=0,trigger_slot=0,trigger_generation=0)
                for child in self.slots.values():
                    if child['slot'] and child['alive']:child['cancel']=True
        state='closing' if terminal and slot==0 else 'completed' if terminal else 'waiting' if t['pending'] else 'ready'
        self.emit('RETURN',**scope,event=event,terminal=terminal or 'none',state=state,entered=int(entered),code=code,
            calls=t['calls'],written=t['written'],irqs=t['irqs'],elr=t['pc'],remaining=t['remaining'],charged=t['charged'],turns=t['turns'],
            root=SYMBOLS['arm_root'],vbar=SYMBOLS['arm_vectors'],controls=1,before=before,after=self.free())
        if terminal and slot==0:self.emit('CLOSE',**self.closure)
        self.manage();return t,event,state
    def group(self,name,gates=False):
        self.name=name;self.g=self.namespace;self.namespace+=1;self.launch=self.task_next<<32|2;self.task_next+=1
        self.next_slots=[1]*4;self.cursor=0;self.sequence=0;self.closure=None;self.caps={};self.held=0;self.reaped=False
        root=self.owner(name,0,1,self.console_next<<32|1,None,h.script(name,35));self.last_console=self.console_next;self.console_next+=1
        self.slots={0:root}
        self.emit('GROUP',group=self.g,limit=512,metadata=3,before=1000,after=self.free())
        self.emit('ROOT',group=self.g,slot=0,generation=1,launch_generation=self.launch>>32,console_generation=root['console']>>32,
            fixture=name,entry=self.f(name)['entry'],root=root['root'],process_frames=root['frames']-3,box_frames=3,before=1000,after=self.free())
        if gates:self.gate('active',('admit','completion','reap'))
        child_gate=closing_gate=waiting_gate=cancelled=False;pressure_stage=0
        for _ in range(1024):
            if self.closure and not self.caps:break
            if name=='allocation_rollback' and root['calls']<3 and self.held==0:
                leave=(0,31,32)[pressure_stage];before=self.free();self.held=before-leave
                self.emit('PRESSURE',group=self.g,phase='hold',stage=('load','startup','box')[pressure_stage],before=before,after=leave,held=self.held)
            selected,event,state=self.step()
            if name=='allocation_rollback' and self.held and selected['slot']==0 and event=='svc':
                before=self.free();held=self.held;self.held=0
                self.emit('PRESSURE',group=self.g,phase='release',stage=('load','startup','box')[pressure_stage],before=before,after=self.free(),held=held)
                pressure_stage+=1
            if gates and selected['slot'] and not child_gate:self.gate('child',('info','cancel','task_completion','task_reap'),selected['slot']);child_gate=True
            if selected['slot']==0:
                if gates and state=='closing' and not closing_gate:self.gate('closing',('task_completion','task_reap'));closing_gate=True
                if name=='wait_budget_child' and state=='waiting' and not waiting_gate:self.gate('waiting',('task_completion','task_reap'));waiting_gate=True
                if name in ('parent_cancel_ready','parent_cancel_waiting') and root['calls']==2 and not cancelled:
                    self.emit('CONTROL',group=self.g,operation='cancel_root',state='ready' if name.endswith('ready') else 'waiting',before=self.free(),after=self.free())
                    root['cancel']=True;cancelled=True
                if name=='drop_waiting' and state=='waiting':
                    before=self.free();self.emit('CONTROL',group=self.g,operation='drop',state='waiting',before=before,after=1000)
                    self.emit('DROP',group=self.g,before=before,after=1000);return
        else:raise AssertionError('Synthetic scheduler exceeded bound')
        self.manage();self.emit('PUBLISH',**self.closure,charged=3)
        self.emit('REAP_ROOT',group=self.g,outcome=h.packed(self.closure['end'],self.closure['code']),before=self.free(),after=1000)
        self.reaped=True
        if gates:self.gate('closed',('admit','info','completion','reap','cancel','task_completion','task_reap','task_root','turn'))
        self.emit('DROP',group=self.g,before=1000,after=1000)
    def generate(self):
        self.emit('PREFLIGHT',checks=15,irqs=8)
        for stage,leave in zip(('image1','image2','root_load','root_startup','root_box','authority'),(0,1,2,33,34,37)):
            self.emit('SETUP',stage=stage,free=leave,before=1000,after=1000,published=0)
        self.lines.append('HMTASK BEGIN 50')
        self.emit('INFO',wire=1,cpus=1,frequency=1000,total_ticks=100,quantum_ticks=1,process_bytes=9832,process_pages=3,
                  image_bytes=472,image_pages=1,authority_bytes=928,authority_pages=1)
        for index in range(50):
            case=h.CASES[index%25];self.emit('CASE',index=index,**{'pass':index//25},name=case,before=1000)
            names=('rights_console_policy','rights_console_wait_only') if case=='rights_console' else ('drop_waiting','supervisor') if case=='drop_waiting' else (case,)
            for name in names:self.group(name,case=='supervisor')
            self.emit('PASS',index=index,**{'pass':index//25},name=case,before=1000,after=1000)
        self.lines+=['HMTASK END 50','HMTEST BEGIN 41']
        return ('\n'.join(self.lines)+'\n').encode()


class FullMatrixTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.synthetic=SyntheticMatrix();cls.raw=cls.synthetic.generate()
    def check(self,raw):return h.validate_serial(raw,self.synthetic.fixtures,SYMBOLS,self.synthetic.layout)
    def test_full_fifty_case_positive(self):
        report=self.check(self.raw)
        self.assertEqual(len(report['cases']),50)
        self.assertEqual(len(report['groups']),54)
    def test_missing_duplicate_and_reordered_envelope_actions(self):
        self.check(self.raw)
        for prefix in (b'HMTASK SETUP',b'HMTASK CASE',b'HMTASK GROUP',b'HMTASK ROOT',b'HMTASK PRESSURE',b'HMTASK GATE',
                       b'HMTASK CONTROL',b'HMTASK CONSUME',b'HMTASK REAP_ROOT',b'HMTASK DROP',b'HMTASK PASS'):
            line=next(line for line in self.raw.splitlines(keepends=True) if line.startswith(prefix))
            for replacement in (b'',line+line):
                with self.assertRaises((ValueError,KeyError,IndexError)):self.check(self.raw.replace(line,replacement,1))
        a=b'HMTASK SETUP stage=image1 free=0 before=1000 after=1000 published=0\n'
        b=b'HMTASK SETUP stage=image2 free=1 before=1000 after=1000 published=0\n'
        with self.assertRaises(ValueError):self.check(self.raw.replace(a+b,b+a,1))

    def test_permuted_state_and_ownership_actions(self):
        for prefix in (b'HMTASK GROUP',b'HMTASK ROOT',b'HMTASK GATE',b'HMTASK PRESSURE',
                       b'HMTASK COMMIT',b'HMTASK CONSUME',b'HMTASK CONTROL',b'HMTASK PASS'):
            lines=self.raw.splitlines(keepends=True)
            positions=[i for i,line in enumerate(lines) if line.startswith(prefix)][:2]
            self.assertEqual(len(positions),2)
            a,b=positions;lines[a],lines[b]=lines[b],lines[a]
            with self.assertRaises((ValueError,KeyError,IndexError)):self.check(b''.join(lines))


if __name__=='__main__':unittest.main()
