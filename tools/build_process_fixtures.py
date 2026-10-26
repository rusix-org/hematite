#!/usr/bin/env python3
"""Build and independently inspect native EL0 inputs; never run kernel code."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys

CASES = 'hello badptr_null badptr_overflow badptr_crossing oversize badhandle badversion badopcode rxwrite rwexecute stackexecute kernelread kernelwrite uartread gicread cntpdisable daifmask stackguard loop zerowrite maxwrite bytebudget calllimit invalidstorm exit64 invalidexit stalehandle nogrant svcimm registers badsp startupwrite semihost'.split()
SYMBOLS = ('native_start','write_svc','exit_svc','fault','loop_start','loop_end','data','message','register_svc_1','register_svc_2','register_svc_3','kernel_text','kernel_data')
ROOT = Path(__file__).resolve().parents[1]
CANONICAL = ROOT / 'crates/hm-core/fixtures/process'


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest()


def run(command, output, name):
    (output / (name + '.command.json')).write_text(json.dumps(command, indent=2)+'\n')
    result = subprocess.run(command, cwd=ROOT, capture_output=True, timeout=60, check=False)
    (output / (name + '.stdout.log')).write_bytes(result.stdout)
    (output / (name + '.stderr.log')).write_bytes(result.stderr)
    if result.returncode:
        raise ValueError(f'{name} exited {result.returncode}')
    return result.stdout.decode('utf8')


def inspect(path, symbols_text):
    data = path.read_bytes()
    if data[:7] != b'\x7fELF\x02\x01\x01' or struct.unpack_from('<HH',data,16) != (2,183):
        raise ValueError('Not a static AArch64 ELF64 input')
    entry,phoff = struct.unpack_from('<QQ',data,24)
    phsize,phcount = struct.unpack_from('<HH',data,54)
    if phsize != 56 or not 1 <= phcount <= 16:
        raise ValueError('Unexpected program header layout')
    segments=[]
    for i in range(phcount):
        kind,flags,offset,va,_,filesz,memsz,align=struct.unpack_from('<IIQQQQQQ',data,phoff+i*phsize)
        if kind != 1:
            raise ValueError('Unexpected non-LOAD header')
        if flags not in (4,5,6) or not memsz or filesz>memsz or offset+filesz>len(data):
            raise ValueError('Invalid LOAD')
        start,end=va&~4095,(va+memsz+4095)&~4095
        if not 0x100000000<=start<end<=0x100100000 or (segments and start<segments[-1]['end']):
            raise ValueError('Overlapping or out-of-range LOAD')
        segments.append(dict(flags=flags,offset=offset,va=va,filesz=filesz,memsz=memsz,align=align,start=start,end=end))
    symbols={}
    for line in symbols_text.splitlines():
        match=re.fullmatch(r'\s*\d+:\s+([0-9a-fA-F]+)\s+\d+\s+\S+\s+\S+\s+\S+\s+\S+\s+(\S+)\s*',line)
        if match and match[2] in SYMBOLS:
            symbols[match[2]]=int(match[1],16)
    if symbols.get('native_start')!=entry or any(s not in symbols for s in ('write_svc','exit_svc','data')):
        raise ValueError('Missing required independent symbols')
    def instruction(name):
        va=symbols[name]
        for segment in segments:
            if segment['flags']==5 and segment['va']<=va and va+4<=segment['va']+segment['filesz']:
                return struct.unpack_from('<I',data,segment['offset']+va-segment['va'])[0]
        raise ValueError(f'{name} not in file-backed executable storage')
    for name in ('write_svc','exit_svc','register_svc_1','register_svc_2','register_svc_3'):
        if instruction(name)!=0xd4000001:
            raise ValueError('Wrong SVC opcode')
    if instruction('loop_start')!=0x14000000 or symbols['loop_end']!=symbols['loop_start']+4:
        raise ValueError('Wrong infinite loop interval')
    return dict(entry=entry,pages=sum((s['end']-s['start'])//4096 for s in segments),symbols=symbols,segments=segments)


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--update',action='store_true')
    parser.add_argument('--kernel-data',type=lambda s:int(s,0))
    args=parser.parse_args()
    output=args.output.resolve()
    if output.exists():parser.error('use a fresh output directory')
    output.mkdir(parents=True)
    old=json.loads((CANONICAL/'manifest.json').read_text()) if (CANONICAL/'manifest.json').exists() else None
    kernel_data=args.kernel_data if args.kernel_data is not None else (old or {}).get('kernel_data')
    if kernel_data is None or not 0x40200000<=kernel_data<0x41000000:
        parser.error('require actual kernel BSS address')
    manifest=dict(version=1,kernel_text=0x40200000,kernel_data=kernel_data,fixtures={})
    sources=['crates/hm-core/fixtures/'+n for n in ('process.rs','process.S','process-case.rs','native-elf.ld')]+['tools/build_process_fixtures.py']
    (output/'input.json').write_text(json.dumps({'sources':{n:digest(ROOT/n) for n in sources},'kernel_data':kernel_data,'update':args.update},indent=2)+'\n')
    results={}
    try:
        for case in CASES:
            directory=output/case;directory.mkdir()
            elf=output/(case+'.elf')
            command=['rustc','--edition=2024','--crate-name','native_process','--crate-type=bin','--target','aarch64-unknown-none',
                     '--cfg',f'fixture="{case}"','-C','panic=abort','-C','opt-level=2','-C','codegen-units=1','-C','relocation-model=static',
                     '-C',f'link-arg=-T{ROOT / "crates/hm-core/fixtures/native-elf.ld"}',
                     '-C','link-arg=--build-id=none','-C','link-arg=-z','-C','link-arg=max-page-size=4096','-C','link-arg=--no-dynamic-linker',
                     '-C','link-arg=--defsym=kernel_text=0x40200000','-C',f'link-arg=--defsym=kernel_data={kernel_data}',
                     str(ROOT/'crates/hm-core/fixtures/process.rs'),'-o',str(elf)]
            run(command,directory,'build')
            run(['readelf','-hW',str(elf)],directory,'header')
            run(['readelf','-lW',str(elf)],directory,'program')
            symbols=run(['readelf','-sW',str(elf)],directory,'symbols')
            run(['aarch64-linux-gnu-objdump','-d',str(elf)],directory,'disassembly')
            info=inspect(elf,symbols)
            info.update(file=case+'.elf',sha256=digest(elf))
            if case=='hello' and 'message' not in info['symbols']:
                raise ValueError('Hello message missing')
            if case in ('cntpdisable','daifmask','svcimm','semihost'):
                opcode={'cntpdisable':0xd51be23f,'daifmask':0xd50342df,'svcimm':0xd4000021,'semihost':0xd45e0000}[case]
                va=info['symbols']['fault']
                seg=next(s for s in info['segments'] if s['flags']==5 and s['va']<=va<va+4<=s['va']+s['filesz'])
                actual=struct.unpack_from('<I',elf.read_bytes(),seg['offset']+va-seg['va'])[0]
                if actual!=opcode:raise ValueError(f'{case} opcode {actual:#x} != {opcode:#x}')
            manifest['fixtures'][case]=info
            matches=args.update or ((CANONICAL/(case+'.elf')).is_file() and elf.read_bytes()==(CANONICAL/(case+'.elf')).read_bytes())
            results[case]={'passed':matches,'sha256':info['sha256'],'pages':info['pages']}
        if not args.update and manifest!=old:raise ValueError('Canonical manifest does not reproduce')
        if not all(x['passed'] for x in results.values()):raise ValueError('Canonical bytes do not reproduce')
        if args.update:
            CANONICAL.mkdir(exist_ok=True)
            for case in CASES:shutil.copy2(output/(case+'.elf'),CANONICAL/(case+'.elf'))
            (CANONICAL/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
        (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
        result={'passed':True,'fixtures':results,'scope':'reproducible ELF bytes, symbols and instruction inspection; not execution'}
    except (ValueError,OSError,subprocess.TimeoutExpired,struct.error,StopIteration) as error:
        result={'passed':False,'fixtures':results,'error':str(error)}
    (output/'result.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))
    return 0 if result['passed'] else 1

if __name__=='__main__':sys.exit(main())
