"""BLESS `[]`: cancelling temporary X components, retained timer, and actor-clear coordinates."""
import argparse,hashlib,json
from pathlib import Path
import pefile,capstone
p=argparse.ArgumentParser();p.add_argument('--games',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
raw=next(a.games.rglob('Tiny_BoD.exe')).read_bytes();pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
prior=json.loads(Path('research/motion-batch17-original.json').read_text());assert hashlib.sha256(raw).hexdigest()==prior['exe_sha256']
expected=[(0x43ceec,'mov','dword ptr [ebp - 0x58], edx'),(0x43cefe,'mov','dword ptr [ebp - 0x64], 1'),(0x43cfa0,'cmp','eax, 0x5d'),(0x43cfa5,'je','0x43cff7'),(0x43cfac,'sub','eax, 0x1b'),(0x43cfaf,'je','0x43cfef'),(0x43cff2,'sub','dword ptr [ebp - 0x58], ecx'),(0x43cffa,'add','dword ptr [ebp - 0x58], eax'),(0x43d022,'mov','bl, byte ptr [edi]'),(0x43d024,'inc','edi'),(0x43d027,'jne','0x43cefe'),(0x43d04f,'mov','edx, dword ptr [ebp - 0x58]'),(0x43d055,'add','edx, edx'),(0x43d057,'lea','edx, [edx + edx*4]'),(0x43d05a,'lea','edx, [edx + edx*4]'),(0x43d05d,'mov','dword ptr [ecx + 0xc], edx'),(0x43d07f,'mov','al, 1'),
 (0x4389a2,'mov','dword ptr [ebp - 0x1c4], 0xfa'),(0x43971e,'push','ebx'),(0x439726,'call','0x43cea4'),(0x43972e,'test','al, al'),(0x439730,'je','0x43975a'),(0x43974d,'call','0x43ab98'),(0x438555,'test','edx, edx'),(0x438557,'je','0x437c1d'),(0x438561,'call','0x4b5558'),
 (0x438651,'mov','edx, dword ptr [esi + 4]'),(0x438669,'call','0x4b556c'),(0x438674,'cmp','edx, eax'),(0x438676,'jl','0x438692'),(0x438679,'call','0x437c0c'),(0x4386e9,'jne','0x4386f6'),(0x4386f0,'je','0x438786'),(0x438792,'je','0x4387d8'),(0x4387dd,'je','0x438813'),(0x438816,'mov','al, 1'),
 (0x437174,'xor','eax, eax'),(0x437176,'mov','dword ptr [edi + 0xa4], eax'),(0x43717c,'mov','dword ptr [edi + 0xa0], eax'),(0x437182,'mov','dword ptr [edi + 0x9c], eax'),(0x437188,'mov','dword ptr [edi + 0x98], eax')]
facts=[]
for va,mnemonic,operands in expected:
 ins=next(md.disasm(pe.get_data(va-base,16),va));assert (ins.mnemonic,ins.op_str)==(mnemonic,operands),(hex(va),ins.mnemonic,ins.op_str)
 facts.append(dict(va=hex(va),instruction=(mnemonic+' '+operands).strip()))
spans=[(0x43cea4,0x1ee),(0x438640,0x204),(0x437c0c,0x964),(0x43971e,0x3c),(0x437174,0x1f)]
a.output.write_text(json.dumps(dict(scope='BLESS original static disassembly, same EXE as prior original evidence; no dynamic original screen parity',exe_sha256=hashlib.sha256(raw).hexdigest(),facts=facts,spans=[dict(va=hex(va),length=n,sha256=hashlib.sha256(pe.get_data(va-base,n)).hexdigest()) for va,n in spans],semantics=dict(symbol='[]',temporary_x_count_steps=[0,-1,0],temporary_x_amplitude=0,permanent_displacement=[0,0],default_duration_ms=250,scheduling='active timer while dialogue runs, including zero displacement; not an additional blocking wait',image_change='preceding image resource operation still executes',actor_clear='clears current and base XY; reload without position uses canvas center'),limits=['Only terminal literal [] added; other bracket forms and multi-operation queues remain unsupported','Headless final PNG cannot alone prove stationary timer dispatch']),indent=2)+'\n');print('PASS original BLESS bracket cancellation and timer evidence')
