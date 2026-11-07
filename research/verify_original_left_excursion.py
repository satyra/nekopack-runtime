"""Pin BLESS repeated '[' transient X accumulation and sine excursion; no game bytes copied."""
import argparse,hashlib,json,struct,sys
from pathlib import Path
import pefile,capstone
p=argparse.ArgumentParser()
for n in ['games','reference-root','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();raw=next(a.games.rglob('Tiny_BoD.exe')).read_bytes()
prior=json.loads(Path('research/brackets-batch19-original.json').read_text())
assert hashlib.sha256(raw).hexdigest()==prior['exe_sha256']
pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
expected=[(0x43ceec,'mov','dword ptr [ebp - 0x58], edx'),(0x43cefe,'mov','dword ptr [ebp - 0x64], 1'),
 (0x43cfaf,'je','0x43cfef'),(0x43cff2,'sub','dword ptr [ebp - 0x58], ecx'),
 (0x43d022,'mov','bl, byte ptr [edi]'),(0x43d024,'inc','edi'),(0x43d027,'jne','0x43cefe'),
 (0x43d04f,'mov','edx, dword ptr [ebp - 0x58]'),(0x43d055,'add','edx, edx'),
 (0x43d057,'lea','edx, [edx + edx*4]'),(0x43d05a,'lea','edx, [edx + edx*4]'),(0x43d05d,'mov','dword ptr [ecx + 0xc], edx'),
 (0x4389a2,'mov','dword ptr [ebp - 0x1c4], 0xfa'),(0x4386b8,'mov','ecx, dword ptr [eax + 0xc]'),(0x4386bb,'mov','dword ptr [ebp - 0x30], ecx'),
 (0x438794,'fld','qword ptr [ebp - 0x10]'),(0x438797,'fmul','qword ptr [0x438834]'),(0x4387a3,'call','0x5061f8'),(0x50620d,'fsin',''),
 (0x4387b1,'fild','dword ptr [ebp - 0x4c]'),(0x4387b4,'fild','dword ptr [ebp - 0x30]'),(0x4387b7,'fmul','qword ptr [ebp - 0x20]'),(0x4387ba,'faddp','st(1)'),(0x4387bc,'call','0x504990'),
 (0x50499f,'or','dword ptr [ebp - 4], 0xc01'),(0x437c1d,'mov','eax, dword ptr [esi + 8]'),(0x437c20,'add','dword ptr [edi + 0x98], eax'),(0x437c26,'mov','edx, dword ptr [esi + 0xc]'),(0x437c29,'add','dword ptr [edi + 0x9c], edx')]
facts=[]
for va,mn,op in expected:
 i=next(md.disasm(pe.get_data(va-base,16),va));assert (i.mnemonic,i.op_str)==(mn,op)
 facts.append(dict(va=hex(va),instruction=(mn+' '+op).strip()))
assert struct.unpack('<d',pe.get_data(0x438834-base,8))[0]==3.141592653589793
sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
packs=[(s['priority'],Archive(a.games/s['file'])) for s in sources if any(x in s['file'] for x in ['system.dat','patch'])]
choices=[(p,pack) for p,pack in packs if 'script/012501.txt' in pack.entries];best=max(p for p,_ in choices);selected=[pack for p,pack in choices if p==best];assert len(selected)==1
script=selected[0].read('script/012501.txt');lines=script.decode('cp932').splitlines()
assert lines[581].strip()=='.img1 [[[[' and lines[583].strip()==':ヴェル:VE_012501_0011:VE'
spans=[(0x43cea4,0x1ee),(0x438640,0x204),(0x437c0c,0x964)]
a.output.write_text(json.dumps(dict(scope='BLESS original static disassembly and source command; not live original screen parity',exe_sha256=prior['exe_sha256'],script_sha256=hashlib.sha256(script).hexdigest(),facts=facts,spans=[dict(va=hex(va),length=n,sha256=hashlib.sha256(pe.get_data(va-base,n)).hexdigest()) for va,n in spans],semantics=dict(symbol='[[[[',count_steps=[0,-1,-2,-3,-4],amplitude_x=-200,permanent_displacement=[0,0],duration_ms=250,curve='base centered X + amplitude_x * sin(pi * elapsed / duration), truncate toward zero, then add canvas center',completion='only permanent dx/dy are committed; transient X returns to base',source='012501',command_line=582),limits=['Repeated terminal [ only; counts, speed modifiers, mixed symbols and multi-operation queues remain unsupported','No dynamic original timing or full-game compatibility claim']),indent=2)+'\n')
print('PASS original BLESS left excursion evidence')
