"""Pin original BLESS left-arrow parser, queued motion, clock and interpolation."""
import argparse,hashlib,json,struct,sys
from pathlib import Path
import pefile,capstone
p=argparse.ArgumentParser()
for n in ['games','reference-root','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();path=next(a.games.rglob('Tiny_BoD.exe'));raw=path.read_bytes();assert hashlib.sha256(raw).hexdigest()=='fc2a82cc733ae8fd9585b9e151d7f6a2ca5f1572c2df017e20d330eeff219521';pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase
md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
expected=[(0x43835f,'mov','cl, byte ptr [eax + 0x18]'),(0x438372,'sub','edx, ecx'),(0x438374,'mov','dword ptr [esi + 8], edx'),(0x4389b1,'mov','dword ptr [ebp - 0x1c8], edx'),(0x439762,'call','0x43d7cc'),(0x439776,'mov','dword ptr [ebp - 0x198], eax'),(0x43979d,'call','0x43ab98'),(0x43d842,'shl','edx, 5'),(0x43d845,'lea','edx, [edx + edx*4]'),(0x43d848,'mov','dword ptr [esi + 4], edx'),(0x43d88e,'mov','byte ptr [esi + 0x18], 1'),(0x43cee2,'mov','dword ptr [ebp - 0x50], ecx'),(0x43cefe,'mov','dword ptr [ebp - 0x64], 1'),(0x43cf94,'sub','eax, 0x2d'),(0x43cf99,'sub','eax, 0xf'),(0x43cf9c,'je','0x43cfcf'),(0x43cfcf,'mov','edx, dword ptr [ebp - 0x64]'),(0x43cfd2,'sub','dword ptr [ebp - 0x50], edx'),(0x43cf73,'cmp','eax, 0x3e'),(0x43cf78,'je','0x43cfd7'),(0x43cfda,'add','dword ptr [ebp - 0x50], ecx'),
 (0x43d02d,'mov','eax, dword ptr [ebp - 0x50]'),(0x43d033,'add','eax, eax'),(0x43d035,'lea','eax, [eax + eax*4]'),(0x43d038,'lea','eax, [eax + eax*4]'),(0x43d03b,'mov','dword ptr [edx + 4], eax'),
 (0x4389a2,'mov','dword ptr [ebp - 0x1c4], 0xfa'),(0x4389b7,'mov','byte ptr [ebp - 0x1c9], 0'),(0x438f34,'mov','dword ptr [ebp - 0x198], ecx'),(0x439726,'call','0x43cea4'),(0x43974d,'call','0x43ab98'),
 (0x437c20,'add','dword ptr [edi + 0x98], eax'),(0x438555,'test','edx, edx'),(0x438557,'je','0x437c1d'),(0x438561,'call','0x4b5558'),
 (0x438669,'call','0x4b556c'),(0x438674,'cmp','edx, eax'),(0x438679,'call','0x437c0c'),(0x4386df,'fdivp','st(1)'),(0x438708,'jb','0x438714'),
 (0x438717,'fsub','dword ptr [0x438830]'),(0x43871d,'fmul','qword ptr [0x438834]'),(0x438729,'call','0x5061f8'),(0x50620d,'fsin',''),(0x438731,'fadd','dword ptr [0x43883c]'),(0x438737,'fmul','dword ptr [0x438830]'),
 (0x43876a,'call','0x504990'),(0x50499f,'or','dword ptr [ebp - 4], 0xc01'),(0x438818,'mov','dword ptr [edx + 0xa0], ebx'),(0x44100b,'call','0x43e610'),(0x40c83b,'call','0x440fec')]
facts=[]
for va,mnemonic,operands in expected:
 ins=next(md.disasm(pe.get_data(va-base,16),va));assert (ins.mnemonic,ins.op_str)==(mnemonic,operands),(hex(va),ins.mnemonic,ins.op_str)
 facts.append(dict(va=hex(va),instruction=(mnemonic+' '+operands).strip()))
for va,f,value in [(0x438830,'f',.5),(0x438834,'d',3.141592653589793),(0x43883c,'f',1.)]:assert struct.unpack('<'+f,pe.get_data(va-base,struct.calcsize(f)))[0]==value
assert pe.get_data(0x54ef3a-base,6)==b'right\0'
assert pe.get_data(0x54ef40-base,5)==b'left\0'
sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
packs=[(s['priority'],Archive(a.games/s['file'])) for s in sources if any(x in s['file'] for x in ['system.dat','patch'])]
choices=[(priority,pack) for priority,pack in packs if 'system/message.nut' in pack.entries];best=max(p for p,_ in choices);selected=[pack for p,pack in choices if p==best];assert len(selected)==1
message=selected[0].read('system/message.nut');lines=message.decode('cp932').splitlines()
assert lines[285].strip()=='if(voice)PlayVoice(m.voice)'
assert lines[290].strip()=='while(wait(delay)){'
assert lines[312].strip()=='if(s.Render())delay=0'
script_choices=[(priority,pack) for priority,pack in packs if 'script/012702.txt' in pack.entries];priority,pack=max(script_choices,key=lambda item:item[0]);script=pack.read('script/012702.txt');script_lines=script.split(b'\n');assert script_lines[66].strip()==b'.img3 SGMY011a right3 <<<<<<<<<'
spans=[(0x43cea4,0x1ee),(0x438640,0x204),(0x437c0c,0x964),(0x440f80,0x17f)]
a.output.write_text(json.dumps(dict(scope='BLESS original static disassembly and original DAT system script; not dynamic original screen parity',exe_sha256=hashlib.sha256(raw).hexdigest(),message_sha256=hashlib.sha256(message).hexdigest(),facts=facts,spans=[dict(va=hex(va),length=n,sha256=hashlib.sha256(pe.get_data(va-base,n)).hexdigest()) for va,n in spans],semantics=dict(position_then_motion='right3 enqueues zero-duration absolute +480 centered coordinate before the relative -450 movement',left_arrow_pixels=-50,nine_arrows_pixels=-450,command_source='012702:67',script_sha256=hashlib.sha256(script).hexdigest(),default_duration_ms=250,default_curve='(sin((elapsed/duration - 0.5)*pi)+1)*0.5',rounding='truncate toward zero in centered coordinates, then add canvas center',scheduling='image operations enqueue; Screen.Render updates while dialogue/voice run; no dedicated movement wait',source_lines=[286,291,313]),limits=['No dynamic original screen comparison','Repeated < and > use signed horizontal motion with existing actor queues; numeric counts, mixed-symbol tokens and explicit transition interactions remain unsupported']),indent=2)+'\n')
print('PASS original BLESS leftward motion evidence')
