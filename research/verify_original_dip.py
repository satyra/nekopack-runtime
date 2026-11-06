"""Pin BLESS `_` parser sign/amplitude, sine phase, no permanent displacement and draw axis."""
import argparse,hashlib,json,struct
from pathlib import Path
import pefile,capstone
p=argparse.ArgumentParser();p.add_argument('--games',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
raw=next(a.games.rglob('Tiny_BoD.exe')).read_bytes();pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
prior=json.loads(Path('research/motion-batch17-original.json').read_text());assert hashlib.sha256(raw).hexdigest()==prior['exe_sha256']
expected=[(0x43cefe,'mov','dword ptr [ebp - 0x64], 1'),(0x43cfb3,'sub','eax, 0x5e'),(0x43cfb8,'dec','eax'),(0x43cfb9,'je','0x43d007'),(0x43d00a,'add','dword ptr [ebp - 0x5c], ecx'),(0x43d060,'mov','eax, dword ptr [ebp - 0x5c]'),(0x43d066,'add','eax, eax'),(0x43d068,'lea','eax, [eax + eax*4]'),(0x43d06b,'lea','eax, [eax + eax*4]'),(0x43d06e,'mov','dword ptr [edx + 0x10], eax'),
 (0x4389a2,'mov','dword ptr [ebp - 0x1c4], 0xfa'),(0x4386be,'mov','ecx, dword ptr [eax + 0x10]'),(0x4386c1,'mov','dword ptr [ebp - 0x2c], ecx'),(0x438797,'fmul','qword ptr [0x438834]'),(0x4387a3,'call','0x5061f8'),(0x50620d,'fsin',''),(0x4387c6,'fild','dword ptr [ebp - 0x50]'),(0x4387c9,'fild','dword ptr [ebp - 0x2c]'),(0x4387cc,'fmul','qword ptr [ebp - 0x20]'),(0x4387cf,'faddp','st(1)'),(0x4387d1,'call','0x504990'),(0x50499f,'or','dword ptr [ebp - 4], 0xc01'),(0x438821,'mov','dword ptr [ecx + 0xa4], edi'),
 (0x437c20,'add','dword ptr [edi + 0x98], eax'),(0x437c26,'mov','edx, dword ptr [esi + 0xc]'),(0x437c29,'add','dword ptr [edi + 0x9c], edx'),(0x437c67,'mov','eax, dword ptr [edi + 0x9c]'),(0x437c6d,'mov','dword ptr [edi + 0xa4], eax'),(0x437a1c,'mov','edx, dword ptr [ebx + 0xa4]'),(0x437a22,'add','edx, 0x168')]
facts=[]
for va,mnemonic,operands in expected:
 ins=next(md.disasm(pe.get_data(va-base,16),va));assert (ins.mnemonic,ins.op_str)==(mnemonic,operands),(hex(va),ins.mnemonic,ins.op_str)
 facts.append(dict(va=hex(va),instruction=(mnemonic+' '+operands).strip()))
assert struct.unpack('<d',pe.get_data(0x438834-base,8))[0]==3.141592653589793
spans=[(0x43cea4,0x1ee),(0x438640,0x204),(0x437c0c,0x964),(0x4379fb,0x59)]
a.output.write_text(json.dumps(dict(scope='BLESS static original machine code; inherited concurrent dialogue schedule from batch17 original system-script evidence; no dynamic original screen parity',exe_sha256=hashlib.sha256(raw).hexdigest(),prior_evidence_sha256=hashlib.sha256(Path('research/motion-batch17-original.json').read_bytes()).hexdigest(),facts=facts,spans=[dict(va=hex(va),length=n,sha256=hashlib.sha256(pe.get_data(va-base,n)).hexdigest()) for va,n in spans],semantics=dict(symbol='_',default_duration_ms=250,temporary_y_amplitude=50,curve='sin(pi*elapsed/250)',direction='positive screen Y (down)',permanent_displacement=[0,0],rounding='truncate centered Y toward zero before adding canvas center'),limits=['Single terminal _ only; repeated/numeric/combined motion modifiers are not implemented','No dynamic original screen comparison']),indent=2)+'\n');print('PASS original BLESS dip evidence')
