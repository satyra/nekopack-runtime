"""BLESS named BGM between an enqueued image operation and dialogue: static evidence."""
import argparse,hashlib,json,sys
from pathlib import Path
import pefile,capstone
p=argparse.ArgumentParser()
for n in ['games','reference-root','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();raw=next(a.games.rglob('Tiny_BoD.exe')).read_bytes()
prior=json.loads(Path('research/motion-batch17-original.json').read_text())
assert hashlib.sha256(raw).hexdigest()==prior['exe_sha256']
pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
expected=[(0x445fe2,'je','0x446598'),(0x44600d,'call','0x4c03ac'),
 (0x446082,'call','0x4492a4'),(0x4460f6,'call','0x4c0024'),
 (0x446102,'call','0x4c04e8'),(0x446114,'call','0x4c0558'),
 (0x44611c,'push','0xa'),(0x44611e,'call','0x5470a4'),
 (0x5470a4,'jmp','dword ptr [0x5af320]'),(0x446576,'je','0x446847'),
 (0x44686e,'ret',''),(0x43974d,'call','0x43ab98'),(0x438669,'call','0x4b556c'),
 (0x44100b,'call','0x43e610'),(0x40c83b,'call','0x440fec')]
facts=[]
for va,mn,op in expected:
 i=next(md.disasm(pe.get_data(va-base,16),va));assert (i.mnemonic,i.op_str)==(mn,op)
 facts.append(dict(va=hex(va),instruction=(mn+' '+op).strip()))
assert [(d.dll,x.name) for d in pe.DIRECTORY_ENTRY_IMPORT for x in d.imports if x.address==0x5af320]==[(b'KERNEL32.DLL',b'Sleep')]
sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
packs=[(s['priority'],Archive(a.games/s['file'])) for s in sources if any(x in s['file'] for x in ['system.dat','patch'])]
def read(name):
 choices=[(p,pack) for p,pack in packs if name in pack.entries];best=max(p for p,_ in choices);selected=[pack for p,pack in choices if p==best];assert len(selected)==1
 return selected[0].read(name)
script=read('script/012501.txt');lines=script.decode('cp932').splitlines()
assert lines[119].strip()=='.img1 SGVE00Aa []' and lines[120].strip()=='.bgm BGM021'
assert lines[123].strip()==':ヴェル:VE_012501_0003:VE' and lines[124].strip()
message=read('system/message.nut');assert hashlib.sha256(message).hexdigest()==prior['message_sha256']
ml=message.decode('cp932').splitlines();assert ml[285].strip()=='if(voice)PlayVoice(m.voice)' and ml[312].strip()=='if(s.Render())delay=0'
spans=[(0x445fc4,0x15f),(0x44656e,0x2b),(0x446847,0x28)]
a.output.write_text(json.dumps(dict(scope='BLESS original DAT and pinned native sound/image paths; static evidence, not dynamic original timing parity',exe_sha256=prior['exe_sha256'],script_sha256=hashlib.sha256(script).hexdigest(),message_sha256=prior['message_sha256'],facts=facts,spans=[dict(va=hex(va),length=n,sha256=hashlib.sha256(pe.get_data(va-base,n)).hexdigest()) for va,n in spans],semantics=dict(source='012501',command_lines=[120,121,124,125],order=['enqueue actor operation','replace BGM channel','start dialogue voice','render dialogue and update actor motion'],sound_path='release previous handle, load/start named resource, apply volume/pan, Sleep(10), return; not a wait for BGM completion',runtime_contract='retain motion through BGM and voice start acknowledgements; no additional motion ACK'),limits=['Only the named single-argument BGM interleave is enabled','No claim of matching original device latency or sample-accurate music timing','Bare BGM, fades, waits and other command interleaves remain separately guarded']),indent=2)+'\n')
print('PASS original BLESS motion/BGM/dialogue ordering evidence')
