"""Pin BLESS filter parser, actor-only load path and saturated pixel operations."""
import argparse,hashlib,json
from pathlib import Path
import pefile,capstone
p=argparse.ArgumentParser();p.add_argument('--games',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
path=next(a.games.rglob('Tiny_BoD.exe'));raw=path.read_bytes();pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase
md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
def one(va):return next(md.disasm(pe.get_data(va-base,16),va))
expected=[(0x43f998,'mov','edx, 0x54f019'),(0x43f99f,'mov','esi, 0x54f01a'),
 (0x43f9c8,'mov','ebx, 0x11'),(0x43f9cd,'mov','esi, 0x111600'),
 (0x43f9d4,'mov','esi, 0x54f01d'),(0x43f9fb,'je','0x43fa09'),
 (0x43f9fd,'mov','ebx, 0x220000'),(0x43fa02,'mov','esi, 0x1022'),
 (0x43fa3b,'xor','eax, eax'),(0x43fa3d,'mov','esi, eax'),(0x43fa3f,'mov','ebx, eax'),
 (0x43fb45,'mov','dword ptr [eax], ebx'),(0x43fb61,'mov','ebx, esi'),(0x43fbac,'mov','dword ptr [eax], ebx'),
 (0x43fcb1,'mov','dword ptr [0x54e074], eax'),(0x43fcff,'mov','dword ptr [0x54e078], ecx'),
 (0x435f4f,'push','0'),(0x435f67,'push','0'),(0x435f7f,'push','1'),
 (0x4363ab,'mov','byte ptr [eax + 0x28], dl'),
 (0x43774d,'mov','edx, dword ptr [ebx + 0x34]'),(0x437752,'jne','0x4379fb'),
 (0x437987,'call','0x449c34'),(0x43798f,'mov','cl, byte ptr [ebx + 0x28]'),
 (0x437994,'je','0x4379a0'),(0x43799a,'call','0x435e18'),
 (0x435e37,'mov','eax, dword ptr [0x54e074]'),(0x435e3c,'mov','edx, dword ptr [0x54e078]'),
 (0x435ea2,'paddusb','mm1, mm0'),(0x435eae,'psubusb','mm1, mm7'),
 (0x435eda,'paddusb','mm1, mm0'),(0x435edd,'psubusb','mm1, mm7')]
facts=[]
for va,mnemonic,operands in expected:
 ins=one(va);assert (ins.mnemonic,ins.op_str)==(mnemonic,operands),(hex(va),ins.mnemonic,ins.op_str)
 facts.append(dict(va=hex(va),instruction=mnemonic+' '+operands))
for va,text in [(0x54f01a,'夜'),(0x54f01d,'夕'),(0x54f023,'add'),(0x54f027,'sub'),(0x54edcc,'image/visual'),(0x54edd9,'image/back'),(0x54ede4,'image/actor')]:assert pe.get_data(va-base,40).split(b'\0')[0].decode('cp932')==text
assert pe.get_data(0x54f019-base,1)==b'\0'
spans=[(0x43f8bc,0x3a2),(0x43fc60,0xc7),(0x435e18,0xda),(0x437714,0x33e)]
a.output.write_text(json.dumps(dict(scope='BLESS static original machine-code evidence, not original-game screenshot parity',exe_sha256=hashlib.sha256(raw).hexdigest(),facts=facts,spans=[dict(va=hex(va),length=n,sha256=hashlib.sha256(pe.get_data(va-base,n)).hexdigest()) for va,n in spans],semantics=dict(night_add_bgra=[17,0,0,0],night_subtract_bgra=[0,22,17,0],sunset_add_bgra=[0,0,34,0],sunset_subtract_bgra=[34,16,0,0],operation='unsigned byte saturating add, then saturating subtract; alpha unchanged',target='actor bitmap at resource load; visual/background layer flags are zero',lifetime='global parameters affect subsequent actor loads; cached bitmap bypasses load/filter path',empty_argument='zero add/subtract; no recoloring of retained bitmap'),limits=['Dark and custom forms outside this check','No dynamic original screen or full native save-state test']),indent=2)+'\n')
print('PASS original BLESS filter path')
