#!/usr/bin/env python3
"""Fixed task-negative automata, separate from the unchanged ordinary acceptance gate."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import struct
import sys

import build_task_negative_cases as builder
import test_task_cases as ordinary
import test_runtime as runtime
from test_process import NUMBER, require

PROFILES = builder.PROFILES
# All ten initial raw profiles accepted in docs/reviews/0030-task-injection.md.
TRACE_GRAMMAR_REVIEWED = True
FIELDS = dict(ordinary.FIELDS, SEED='kind value', INJECT='group turn kind',
    RETRY_SETUP='result before after published', LEGACY_BEGIN='entry root before during frames',
    LEGACY_END='reason code calls written root vbar controls before after')
EXTRA_TEXT = {'SEED': {'kind'}, 'INJECT': {'kind'}, 'RETRY_SETUP': {'result'}, 'LEGACY_END': {'reason'}}


def fields(line, kind):
    if kind in ordinary.FIELDS: return ordinary.fields(line, kind)
    require(kind in EXTRA_TEXT or kind == 'LEGACY_BEGIN', 'Unknown negative record')
    names = FIELDS[kind].split(); text = EXTRA_TEXT.get(kind, set())
    pattern = 'HMTASK ' + kind + ''.join(' ' + key + '=' + ('([a-z0-9_]+)' if key in text else NUMBER) for key in names)
    match = re.fullmatch(pattern, line)
    require(match is not None, 'Malformed negative ' + kind)
    result = {key: value if key in text else int(value) for key, value in zip(names, match.groups())}
    require(all(value < 2**64 for value in result.values() if type(value) is int), 'Negative integer exceeds u64')
    return result


class Records:
    def __init__(self, raw):
        require(len(raw) <= ordinary.MAX_LOG_BYTES, 'Oversized negative serial')
        self.rows, self.positions, self.index = [], [], 0
        active = ended = legacy = False
        lines = raw.split(b'\n')
        for index, value in enumerate(lines):
            value = value.removesuffix(b'\r')
            if value.startswith(b'HMTASK '):
                require(not ended and index < len(lines)-1, 'Extra/unterminated negative record')
                kind = value.split()[1]
                if kind == b'LEGACY_BEGIN': require(not legacy, 'Nested legacy interval'); legacy = True
                elif kind == b'LEGACY_END': require(legacy, 'Legacy end without begin'); legacy = False
                else: require(not legacy, 'Task record inside legacy interval')
                self.rows.append(value.decode('ascii')); self.positions.append(index)
                ended = value == b'HMTASK END 1'; active = not ended
            elif value.startswith(b'HMPROC ') and legacy and index < len(lines)-1:
                self.rows.append(value.decode('ascii')); self.positions.append(index)
            elif active or b'HMTASK' in value or b'HMPROC' in value or b'HMRT ' in value:
                raise ValueError('Foreign output in negative profile')
        require(not legacy, 'Unclosed legacy interval')
    def kind(self):
        require(self.index < len(self.rows), 'Incomplete negative records')
        return self.rows[self.index].split()[1]
    def take(self, kind):
        require(self.kind() == kind, 'Expected negative ' + kind + ', got ' + self.kind())
        row = fields(self.rows[self.index], kind); self.index += 1
        return row
    def literal(self, line):
        require(self.index < len(self.rows) and self.rows[self.index] == line, 'Missing ' + line)
        self.index += 1


def script(case):
    spawn, wait = ordinary.spawn, ordinary.wait
    if case in ('final_budget_spawn', 'platform_reply'): return [spawn('a')]
    if case in ('final_budget_park', 'platform_park', 'child_retire_waiting'): return [spawn('a', 2), wait('a', None)]
    if case == 'abandon_dispatch': return [spawn('a', 2), spawn(None, abandon=True)]
    if case == 'wake_install_failure': return [spawn('a'), wait('a')]
    if case == 'capability_exhaustion': return [spawn(None, status=9, exhausted=True), ordinary.exit_call()]
    if case == 'slot_exhaustion':
        result=[]
        for alias in ('a','b','c'): result += [spawn(alias), wait(alias)]
        return result+[spawn(None,status=9,exhausted=True),ordinary.exit_call()]
    if case == 'child_retire_ready': return [spawn('a',2)] + [dict(ordinary.write(site='write_svc'), zero_invalid=True) for _ in range(32)]
    raise ValueError('Unknown negative fixture')


def expected(case):
    return 'budget' if case.startswith('final_budget_') else 'exit' if case.endswith('exhaustion') else 'unavailable'


class NegativeModel(ordinary.Model):
    """Only normal wire operations reuse the ordinary checker; failure transitions are explicit."""
    def __init__(self, records, fixtures, symbols, layout, info, baseline, case):
        super().__init__(records,fixtures,symbols,layout,info,baseline)
        self.case=case;self.next_group=1;self.next_task=0xffffffff if case=='capability_exhaustion' else 1
        self.next_console=1;self.last_console=0;self.injected=None;self.retired=False
        self.seeded=case in ('capability_exhaustion','slot_exhaustion')
    def new_task(self,name,slot,generation,root,console,payload,commands):
        require(root != self.symbols['arm_root'],'Admitted negative owner aliases boot translation root')
        return super().new_task(name,slot,generation,root,console,payload,commands)
    def begin(self):
        f=self.fixtures[self.case];frames=f['pages']+29+self.k
        require(self.r.take('GROUP')==dict(group=1,limit=512,metadata=self.metadata,before=self.baseline,after=self.baseline-self.metadata-frames), 'Wrong negative group charge')
        self.group=dict(id=1,slots={},caps={},next_slots=[1]+([0xffffffff]*3 if self.case=='slot_exhaustion' else [1]*3),
            cursor=0,sequence=0,closure=None,published=False,reaped=False,pressure=0,gates=[],controls=[],consumes=[],commits=[],pressures=[],case=self.case)
        row=self.r.take('ROOT')
        require(row==dict(group=1,slot=0,generation=1,launch_generation=self.next_task,console_generation=1,fixture=self.case,
            entry=f['entry'],root=row['root'],process_frames=frames-self.k,box_frames=self.k,before=self.baseline,after=self.baseline-self.metadata-frames), 'Wrong seeded/root identity')
        self.group['launch']=self.next_task<<32|2
        self.next_task=0 if self.next_task==0xffffffff else self.next_task+1
        self.next_console=2;self.last_console=1
        self.new_task(self.case,0,1,row['root'],0x100000001,None,script(self.case))
        self.free=row['after'];self.balance()
    def consume(self,cap_slot):
        task=super().consume(cap_slot)
        if self.group['next_slots'][task['slot']]==0x100000000:
            self.group['next_slots'][task['slot']]=0
            # A retired slot remains unavailable, but owns neither frames nor a cap.
            self.group['slots'][task['slot']]=dict(slot=task['slot'],generation=0,root=0,live=False,frames=0,retired=True)
        return task
    def request(self,task,step):
        spec=task['commands'][task['calls']]
        if not (spec.get('abandon') or spec.get('exhausted') or spec.get('zero_invalid')):
            return super().request(task,step)
        row=ordinary.scoped(self.r,'CALL',step);task['calls']+=1
        f=self.fixtures[task['fixture']]
        require(row['ordinal']==task['calls'] and row['pc']==task['pc']==f['symbols'][spec['site']]+4 and
                row['version']==1 and row['operation']==spec['op'], 'Wrong special negative request')
        if spec.get('zero_invalid'):
            require((row['arg1'],row['arg2'],row['arg3'])==(task['console'],2**64-1,0),'Ready probe WRITE differs')
            kind,status='reply',0
        else:
            require(row['arg1']==self.group['launch'] and row['arg3']==64 and 0x100000000<=row['arg2']<=0x100200000-64,'Wrong negative SPAWN authority/range')
            kind,status=('failure',0) if spec.get('abandon') else ('reply',9)
            if spec.get('exhausted'):
                copy=ordinary.scoped(self.r,'COPY',step)
                require(copy['part']=='descriptor' and copy['length']==64,'Missing complete exhausted request copy')
                ordinary.descriptor_bytes(spec,bytes.fromhex(copy['hex']),f,ordinary.WORKER_BYTES)
                copy=ordinary.scoped(self.r,'COPY',step)
                require(copy['part']=='input' and bytes.fromhex(copy['hex'])==ordinary.WORKER_BYTES,'Exhaustion skipped copied input')
                if self.case=='capability_exhaustion': require(self.next_task==0 and not self.group['caps'],'Capability issuer was not exhausted')
                else: require(self.group['next_slots'][1:]==[0,0,0] and not self.group['caps'],'Fourth spawn before three actual MAX slot reaps')
        require(ordinary.scoped(self.r,'SETTLE',step)==dict(group=1,turn=step['turn'],kind=kind,status=status,result=0,
            child_slot=0,child_generation=0,cap_slot=0,cap_generation=0),'Wrong checked exhaustion/failure response')
        return None,0
    def eligible(self,task,trap):
        if self.injected or self.seeded:return None
        root=self.group['slots'][0]
        if task['slot']==0:
            if self.case in ('final_budget_spawn','platform_reply') and task['calls']==1 and len(self.group['commits'])==1:
                return 'final_budget' if self.case.startswith('final') else 'retire'
            if self.case in ('final_budget_park','platform_park') and task['calls']==2 and task['waiting'] is not None:
                return 'final_budget' if self.case.startswith('final') else 'retire'
            if self.case=='abandon_dispatch' and task['calls']==2:return 'abandon'
        elif (self.case in ('child_retire_ready','child_retire_waiting') and self.group['commits'] and task is self.group['commits'][0] and
                trap is not None and trap['kind']=='irq' and task['fixture']=='loop' and
                self.fixtures['loop']['symbols']['loop_start']<=trap['elr']<self.fixtures['loop']['symbols']['loop_end']):
            if (root['waiting'] is None)==(self.case=='child_retire_ready'):return 'child_retire'
        return None
    def injection(self,step,kind):
        require(self.injected is None and not self.seeded,'Repeated/unexpected injection')
        require(ordinary.scoped(self.r,'INJECT',step)==dict(group=1,turn=step['turn'],kind=kind),'Wrong/missing first eligible injection')
        self.injected=(kind,step['slot'],step['generation'],step['turn'])
        if kind!='final_budget':self.retired=True
    def close(self,end,root,turn,trigger=None):
        require(self.group['closure'] is None,'Replaced root closure')
        if root['live']:root['live']=False;self.free+=root['frames']
        root['terminal']=end;root['code']=0;root['waiting']=None
        self.group['closure']=dict(group=1,root_slot=0,root_generation=1,end=end,code=0,
            evidence='administrative' if trigger else 'executed',turn=0 if trigger else turn,
            trigger_group=1 if trigger else 0,trigger_slot=trigger['slot'] if trigger else 0,trigger_generation=trigger['generation'] if trigger else 0)
        for child in self.group['slots'].values():
            if child['slot'] and child['live']:child['cancel']=True
    def management(self):
        g=self.group;root=g['slots'][0];reaped=getattr(self,'management_reaped',0);self.management_reaped=0;woken=0
        if g['closure']:
            for cap,value in list(g['caps'].items()):
                if not g['slots'][value['slot']]['live']:self.consume(cap);reaped+=1
        elif root['waiting'] and not root['cancel']:
            cap,outcome=root['waiting'];child=g['slots'][g['caps'][cap]['slot']]
            if not child['live']:
                require(ordinary.packed(child['terminal'],child['code'])==outcome,'Wrong genuine child completion')
                self.consume(cap);root['waiting']=None;reaped+=1;woken=1
        g['sequence']+=1
        expected_row=dict(group=1,sequence=g['sequence'],woken=woken,reaped=reaped,remaining_children=len(g['caps']),
                          closing=int(bool(g['closure'])),charged=self.charged())
        require(self.r.take('MANAGE')==expected_row,'Negative management consumed/woke wrong owner')
        return expected_row
    def step(self):
        g=self.group;root=g['slots'][0]
        selected=next((i for n in range(4) if (i:=(g['cursor']+n)%4) in g['slots'] and g['slots'][i]['live'] and
                       (g['slots'][i]['cancel'] or g['slots'][i]['waiting'] is None)),None)
        require(selected is not None and not g['published'],'Negative step without runnable owner')
        task=g['slots'][selected];f=self.fixtures[task['fixture']];before=self.free
        step=self.r.take('STEP')
        require(step==dict(group=1,turn=task['ordinal']+1,slot=selected,generation=task['generation'],
            state='cancel_pending' if task['cancel'] else 'ready',calls=task['calls'],written=task['written'],irqs=task['irqs'],
            remaining=task['remaining'],charged=task['charged'],turns=task['turns'],before=before),'Wrong selected negative owner/snapshot')
        task['ordinal']+=1;g['cursor']=(selected+1)%4;self.case_steps+=1
        require(self.case_steps<=1024 and task['turns']<256,'Unbounded negative execution')
        terminal='cancelled' if task['cancel'] else None;code=0;event='none';enter=trap=clock=None
        if not task['cancel']:
            require(not self.retired,'Entry admitted after sticky retirement')
            clock=ordinary.scoped(self.r,'CLOCK',step);start=clock['begin']
            require(self.last_clock<=start<=2**56-1 and clock['remaining']==task['remaining'] and
                    clock['whole_end']==start+task['remaining']<=2**56-1 and
                    clock['comparator']==start+min(task['remaining'],self.info['quantum_ticks']),'Wrong negative deadline/counter')
            # Every noncancel successful negative turn must really enter EL0.
            # Earlier setup-budget/failure is not the requested injected outcome.
            enter=ordinary.scoped(self.r,'ENTER',step)
            require(enter['pc']==task['pc'] and enter['root']==task['root'],'Wrong negative resume PC/root')
            trap=ordinary.scoped(self.r,'TRAP',step)
            require(trap['kind'] in ('svc','irq') and trap['spsr'] & ~0xf0000000==0x00400340,'Unexpected negative fault/PSTATE')
            task['pc']=task['elr']=trap['elr']
            if trap['kind']=='svc':
                require(trap['esr']==0x56000000 and trap['far']==0,'Wrong negative SVC syndrome')
                require(task['calls']<len(task['commands']),'Unexpected fallback/extra fixture call')
                terminal,code=self.request(task,step);event='svc'
            else:
                require(runtime.executable(f,trap['elr']),'IRQ outside negative executable')
                task['irqs']+=1;event='timer'
                if f['symbols']['loop_start']<=trap['elr']<f['symbols']['loop_end']:task['poll_irqs']+=1
            kind=self.eligible(task,trap)
            if kind:
                self.injection(step,kind)
                if kind!='final_budget':terminal='unavailable';code=0
            self.slices.append(dict(enter=enter,trap=trap))
            charge=ordinary.scoped(self.r,'CHARGE',step)
            require(start<=charge['end']<=2**56-1 and charge['accounted']==1,'Rejected finish cannot pass a negative profile')
            if trap['kind']=='irq':require(charge['end']>=clock['comparator'],'IRQ before programmed deadline')
            if kind=='final_budget':require(charge['end']>=clock['whole_end'],'Budget hook did not reach actual whole-turn deadline')
            elapsed=min(charge['end']-start,task['remaining']);task['remaining']-=elapsed;task['charged']+=elapsed;task['turns']+=1
            require((charge['remaining'],charge['charged'],charge['turns'])==(task['remaining'],task['charged'],task['turns']), 'Forged negative budget charge')
            self.last_clock=charge['end']
            if terminal is None and task['remaining']==0:terminal='budget'
            require(terminal!='budget' or kind=='final_budget','Earlier natural Budget is not injection success')
        wake_failure=False
        if terminal:
            task['live']=False;task['terminal']=terminal;task['code']=code;self.free+=task['frames'];task['waiting']=None
            if selected==0:
                require(terminal==expected(self.case) and code==0 and task['calls']==len(task['commands']), 'Wrong executed root outcome/script')
                self.close(terminal,root,task['ordinal'])
            elif terminal=='unavailable':
                require(self.injected is not None and self.injected[0]=='child_retire','Unexpected child retirement')
                require(root['calls']==2 if self.case=='child_retire_waiting' else 1<=root['calls']<=33,'Wrong root state at child retirement')
                self.close('unavailable',root,0,task)
            elif terminal=='exit':
                require(code==55 and task['calls']==6,'Worker did not execute its exact real script')
                if self.case=='wake_install_failure':
                    require(task is g['commits'][0] and root['calls']==2 and root['waiting'] is not None,'Wake failure lacked actual parked parent')
                    cap,outcome=root['waiting'];require(g['caps'][cap]['slot']==selected and outcome==55<<8,'Wrong pending child wake')
                    self.consume(cap);self.management_reaped=1
                    self.close('unavailable',root,0,task);wake_failure=True
        state='closing' if selected==0 and terminal else 'completed' if terminal else 'waiting' if task['waiting'] else 'ready'
        returned=ordinary.scoped(self.r,'RETURN',step)
        require(returned==dict(group=1,turn=task['ordinal'],event=event,terminal=terminal or 'none',state=state,entered=int(enter is not None),
            code=code,calls=task['calls'],written=task['written'],irqs=task['irqs'],elr=task['elr'],remaining=task['remaining'],
            charged=task['charged'],turns=task['turns'],root=self.symbols['arm_root'],vbar=self.symbols['arm_vectors'],controls=1,before=before,after=self.free),
            'Wrong finalized negative step/control/resource result')
        if wake_failure:self.injection(step,'wake_install')
        if g['closure'] is not None and not g.get('closure_emitted'):
            require(self.r.take('CLOSE')==g['closure'],'Wrong immutable negative root closure')
            g['closure_emitted']=True
        managed=self.management();self.balance()
        task['history'].append(dict(step=step,trap=trap,returned=returned,manage=managed))
    def finish(self):
        g=self.group
        require(g['published'] and self.charged()==self.metadata and not g['caps'],'Negative root publication before child cleanup')
        if self.seeded:require(self.injected is None,'Seed profile also injected a boundary')
        else:require(self.injected is not None,'Missing or unreached negative hook')
        require(g['closure']['end']==expected(self.case),'Wrong published negative outcome')
        require(g['closure']['evidence']==('administrative' if self.case in ('wake_install_failure','child_retire_ready','child_retire_waiting') else 'executed'), 'Wrong root evidence kind')
        require(self.r.take('REAP_ROOT')==dict(group=1,outcome=ordinary.packed(expected(self.case),0),before=self.free,after=self.baseline),'Negative root reap changed reason/metadata')
        g['reaped']=True;self.free=self.baseline
        require(self.r.take('DROP')==dict(group=1,before=self.baseline,after=self.baseline),'Negative group drop leaked')
        if self.case=='slot_exhaustion':require(g['next_slots']==[1,0,0,0] and self.next_task==5 and self.next_console==5,'Slot exhaustion conflated issuers')
        if self.case=='capability_exhaustion':require(self.next_task==0 and self.next_console==2,'Task exhaustion burned console or reset task issuer')
    def legacy(self):
        require(self.case=='capability_exhaustion' and not self.retired,'Legacy after retired Executor')
        require(self.r.take('RETRY_SETUP')==dict(result='resources',before=self.baseline,after=self.baseline,published=0),'Exhausted task setup resumed or leaked')
        f=self.fixtures['legacy_hello'];frames=f['pages']+29;row=self.r.take('LEGACY_BEGIN')
        require(row==dict(entry=f['entry'],root=row['root'],before=self.baseline,during=self.baseline-frames,frames=frames) and
                row['root']%4096==0 and 0x40000000<=row['root']<0x41000000 and row['root']!=self.symbols['arm_root'],'Legacy physical ownership differs')
        pc=f['entry'];handle=0x200000001
        for ordinal,label in ((1,'write_svc'),(2,'exit_svc')):
            successor=f['symbols'][label]+4
            args=(handle,f['symbols']['message'],15) if ordinal==1 else (0,0,0)
            self.r.literal(f'HMPROC CALL index=0 ordinal={ordinal} pc={successor} version=1 operation={ordinal} arg1={args[0]} arg2={args[1]} arg3={args[2]}')
            if ordinal==1:
                self.r.literal('HMPROC OUTPUT index=0 ordinal=1 length=15 hex='+b'hello from EL0\n'.hex())
                self.r.literal('HMPROC RESULT index=0 ordinal=1 status=0 bytes=15')
            self.slices.append(dict(enter=dict(pc=pc,root=row['root']),trap=dict(kind='svc',esr=0x56000000,far=0,elr=successor)))
            pc=successor
        require(self.r.take('LEGACY_END')==dict(reason='exit',code=0,calls=2,written=15,root=self.symbols['arm_root'],
            vbar=self.symbols['arm_vectors'],controls=1,before=self.baseline,after=self.baseline),'Legacy recovery failed exact result/restoration')


def validate_serial(raw,fixtures,symbols,witness,case):
    require(case in PROFILES,'Unknown negative case')
    r=Records(raw);require(r.take('PREFLIGHT')==dict(checks=15,irqs=8),'Negative preflight missing')
    if case=='capability_exhaustion':require(r.take('SEED')==dict(kind='capability_max',value=0xffffffff),'Missing fresh task-cap seed')
    r.literal('HMTASK BEGIN 1');info=r.take('INFO')
    fl=witness['fields'];tl=witness['task_fields']
    require(info==dict(wire=1,cpus=1,frequency=info['frequency'],total_ticks=info['frequency']//10,
        quantum_ticks=info['frequency']//1000,process_bytes=fl['process_bytes'],process_pages=fl['box_backing_pages'],
        image_bytes=tl['image_bytes'],image_pages=tl['image_pages'],authority_bytes=tl['authority_bytes'],authority_pages=tl['authority_pages']) and
        info['quantum_ticks']>0,'Wrong negative layout/timer contract')
    first=r.take('CASE');baseline=first['before']
    require(300<baseline<4096 and first==dict(index=0,**{'pass':0},name=case,before=baseline),'Wrong single negative case')
    if case=='slot_exhaustion':require(r.take('SEED')==dict(kind='child_slots_max',value=0xffffffff),'Missing prepublication child-slot seed')
    model=NegativeModel(r,fixtures,symbols,witness,info,baseline,case);model.begin()
    while not model.group['published']:
        if r.kind()=='STEP':model.step()
        elif r.kind()=='MANAGE':model.publish()
        else:raise ValueError('Unexpected negative state record: '+r.kind())
    model.finish()
    if case=='capability_exhaustion':model.legacy()
    require(r.take('PASS')==dict(index=0,**{'pass':0},name=case,before=baseline,after=baseline),'Negative PASS did not restore baseline')
    r.literal('HMTASK END 1');require(r.index==len(r.rows),'Extra negative output')
    baseline_positions=[i for i,line in enumerate(raw.split(b'\n')) if line.startswith(b'HMTEST ')]
    require(baseline_positions and min(baseline_positions)>r.positions[-1],'Baseline before negative END')
    return dict(slices=model.slices,records=len(r.rows),case=case,injection=model.injected,retired=model.retired,
                next_task=model.next_task,next_console=model.next_console)


def validate(console,trace,execution,fixtures,symbols,fixups,layout,case):
    errors=[];report=traced=None
    try:
        require(execution.get('returncode')==0 and execution.get('timed_out') is False and execution.get('launch_error') is None,'Negative guest failed or exceeded deadline')
        report=validate_serial(console,fixtures,symbols,layout,case)
        require(len(trace)<=ordinary.MAX_TRACE_BYTES,'Oversized negative hardware trace')
        traced=runtime.validate_trace(trace,report,symbols,fixups)
        require(TRACE_GRAMMAR_REVIEWED,'Actual ten-profile negative grammar awaits independent review')
    except (ValueError,KeyError,TypeError,UnicodeError,StopIteration,IndexError) as error:errors.append(str(error) or 'Incomplete negative evidence')
    baseline=runtime.validate_run(console,execution.get('returncode'),arch='aarch64',timed_out=execution.get('timed_out',True),launch_error=execution.get('launch_error'))
    return dict(passed=not errors and baseline['passed'],case=case,errors=errors,baseline=baseline,trace=traced,
        injection=report['injection'] if report else None,retired=report['retired'] if report else None,
        scope='one fixed injected/seeded task profile; not ordinary hardware malfunction or complete ten-profile acceptance')


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--case',choices=PROFILES,required=True)
    for name in ('kernel','fixtures','archive','layout','output'):parser.add_argument('--'+name,type=Path,required=True)
    args=parser.parse_args();output=args.output.resolve()
    if output.exists():parser.error('preserve failures; use fresh output')
    output.mkdir(parents=True)
    try:
        kernel=args.kernel.resolve();_,layout=runtime.verify_layout(args.layout.resolve())
        fixtures,archive=builder.verify(args.fixtures.resolve(),args.archive.resolve(),args.layout.resolve(),output,kernel)
        symbols=runtime.read_symbols(kernel,output,'kernel-symbols',runtime.SYMBOLS)
        fixups=runtime.fixup_table(kernel.read_bytes(),symbols)
        inputs=dict(selected_case=args.case,fixtures_sha256=hashlib.sha256(args.fixtures.read_bytes()).hexdigest(),
                    layout_sha256=hashlib.sha256(args.layout.read_bytes()).hexdigest(),archive=archive)
        metadata=dict(**inputs,layout=layout,symbols=symbols,sources={name:hashlib.sha256(Path(__file__).with_name(name).read_bytes()).hexdigest()
            for name in ('test_task_negative.py','test_task_cases.py','build_task_negative_cases.py','test_runtime.py','test_launcher.py','test_process.py')})
        (output/'metadata.json').write_text(json.dumps(metadata,indent=2)+'\n')
        result=run_profile(output/'guest',kernel,fixtures,symbols,fixups,layout['result'],inputs,args.case)
    except (OSError,ValueError,KeyError,TypeError,UnicodeError,struct.error) as error:
        result=dict(passed=False,case=args.case,error=str(error))
    (output/'result.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
    return 0 if result['passed'] else 1


def run_profile(output, kernel, fixtures, symbols, fixups, layout, inputs, case):
    memory = 128
    import subprocess
    import time
    from test_kernel import qemu_command
    from test_boot_profiles import write_json
    output.mkdir()
    command = qemu_command(kernel, 'tcg', 'aarch64')
    command[command.index('-m') + 1] = f'{memory}M'
    command[command.index('-semihosting-config') + 1] = 'enable=on,target=native,userspace=off'
    command += ['-d', 'int', '-D', str(output / 'exceptions.log'), '-trace', 'enable=gic_acknowledge_irq', '-trace', 'enable=gic_cpu_write']
    write_json(output / 'command.json', command)
    write_json(output / 'input.json', dict(kernel=str(kernel), kernel_sha256=hashlib.sha256(kernel.read_bytes()).hexdigest(),
        ram_mb=memory, cpus=1, timeout_seconds=60, max_serial_bytes=ordinary.MAX_LOG_BYTES, max_trace_bytes=ordinary.MAX_TRACE_BYTES, **inputs))
    execution = dict(returncode=None, timed_out=False, launch_error=None)
    started = time.monotonic()
    paths = [output / name for name in ('serial.log', 'stderr.log', 'exceptions.log')]
    with paths[0].open('wb') as stdout, paths[1].open('wb') as stderr:
        try:
            with subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr) as process:
                while process.poll() is None:
                    if any(p.exists() and p.stat().st_size > (ordinary.MAX_TRACE_BYTES if p.name == 'exceptions.log' else ordinary.MAX_LOG_BYTES) for p in paths):
                        execution['launch_error'] = 'Task artifact size exceeded'
                        process.kill(); process.wait(); break
                    remaining = 60 - (time.monotonic() - started)
                    if remaining <= 0:
                        execution['timed_out'] = True
                        process.kill(); process.wait(); break
                    try: process.wait(timeout=min(remaining, 0.25))
                    except subprocess.TimeoutExpired: pass
                execution['returncode'] = process.returncode
        except OSError as error: execution['launch_error'] = str(error)
    execution['elapsed_seconds'] = time.monotonic() - started
    logs = {p.name: p.read_bytes() if p.exists() and p.stat().st_size <= (ordinary.MAX_TRACE_BYTES if p.name == 'exceptions.log' else ordinary.MAX_LOG_BYTES) else b'' for p in paths}
    result = validate(logs['serial.log'], logs['exceptions.log'], execution, fixtures, symbols, fixups, layout, case)
    result['execution'] = execution
    result['artifacts'] = {p.name: dict(exists=p.exists(), bytes=p.stat().st_size if p.exists() else 0,
        sha256=hashlib.sha256(logs[p.name]).hexdigest(), within_size_limit=p.exists() and p.stat().st_size <= (ordinary.MAX_TRACE_BYTES if p.name == 'exceptions.log' else ordinary.MAX_LOG_BYTES)) for p in paths}
    result['passed'] &= all(v['within_size_limit'] for v in result['artifacts'].values())
    write_json(output / 'result.json', result)
    return result



if __name__ == "__main__": sys.exit(main())
