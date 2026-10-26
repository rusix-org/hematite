from pathlib import Path
import hashlib
import json
import re
import struct
import sys
sys.path.insert(0,str(Path('tools').resolve()))
from build_initramfs import run
source=Path('docs/evidence/process-budget-stage-a/layout-second/sizes.s').resolve()
out=Path('docs/evidence/process-budget-stage-a/layout-extracted').resolve()
out.mkdir()
sections=re.findall(r'^\s*\.section\s+(\.rodata\.[^,\s]*LAYOUT),',source.read_text(),re.M)
assert len(sections)==1
obj=out/'sizes.o'; data=out/'layout.bin'
run(['aarch64-linux-gnu-as',str(source),'-o',str(obj)],out,'assemble')
run(['aarch64-linux-gnu-objcopy','--dump-section',sections[0]+'='+str(data),str(obj)],out,'extract')
raw=data.read_bytes(); assert len(raw)==40
values=struct.unpack('<5Q',raw)
result=dict(passed=True,fields=dict(zip(('process_bytes','space_bytes','box_bytes','optional_box_bytes','box_backing_pages'),values)),
    assembly_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),bytes_sha256=hashlib.sha256(raw).hexdigest(),
    scope='current ARM compiler layout constants only; not stack usage or runtime allocation proof')
(out/'result.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(result,indent=2))
