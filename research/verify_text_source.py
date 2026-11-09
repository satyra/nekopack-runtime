"""BLESS raw DAT evidence for message timing and NEXT / KeepVoice branches."""
import argparse,hashlib,json,sys
import pefile,capstone
from pathlib import Path
p=argparse.ArgumentParser()
for key in ('games','reference-root','output'):p.add_argument('--'+key,type=Path,required=True)
a=p.parse_args();sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
packs=[(s['priority'],Archive(a.games/s['file'])) for s in sources if any(x in s['file'] for x in ['system.dat','patch'])]
def read(name):
    choices=[(p,pack) for p,pack in packs if name in pack.entries];best=max(p for p,_ in choices)
    chosen=[pack for p,pack in choices if p==best];assert len(chosen)==1
    return chosen[0].read(name)
message=read('system/message.nut');config=read('system/config.nut');text=message.decode('cp932')
checks=['local speed = Config.TextSpeed','t.Get() * (speed+10) / 1000','case ID_NEXT:','if(end){','switch(Config.KeepVoice)','case 0: StopVoice(); break','case 2: FadeVoice(); break','speed=SPEED_MAX','local list1  = comp.Build(m.text)']
assert all(c in text for c in checks)
assert 'TextSpeed  = SPEED_MAX / 2' in config.decode('cp932') and 'KeepVoice  = 1' in config.decode('cp932')
pinned=next(r for r in json.loads(Path('research/original-effects-batch10-evidence.json').read_text())['games'] if r['file']=='Tiny_BoD.exe')
paths=list(a.games.rglob(pinned['file']));assert len(paths)==1
exe=paths[0].read_bytes();assert hashlib.sha256(exe).hexdigest()==pinned['sha256']
pe=pefile.PE(data=exe);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
native=[]
for va,mn,op in [(0x406e37,'push','0x40b944'),(0x406e3c,'push','0x54832a'),(0x40b947,'call','0x446a34'),(0x446a34,'push','0x584598'),(0x446a39,'call','0x4c03ac'),(0x446a3f,'ret',''),(0x406e55,'push','0x40b950'),(0x406e5a,'push','0x548336'),(0x40b953,'call','0x446a40'),(0x446a40,'push','0x7d0'),(0x446a4c,'call','0x4c04e8')]:
    i=next(md.disasm(pe.get_data(va-base,16),va));assert (i.mnemonic,i.op_str)==(mn,op)
    native.append(dict(va=hex(va),instruction=(mn+' '+op).strip()))
assert pe.get_data(0x54832a-base,10)==b'StopVoice\0'
assert pe.get_data(0x548336-base,10)==b'FadeVoice\0'
result=dict(scope='BLESS static raw DAT and pinned original PE evidence, not dynamic original rendering parity',message_sha256=hashlib.sha256(message).hexdigest(),config_sha256=hashlib.sha256(config).hexdigest(),exe_sha256=pinned['sha256'],native=native,handle_release_evidence='research/original-bgm-stop-batch13-verification.json',facts=dict(default_speed=50,default_keep_voice=1,progress='floor(active_ms*(speed+10)/1000), instant at speed 100',incomplete_next='set maximum display speed without returning from message',complete_next='return from message; stop voice for 0, preserve for 1, FadeVoice for 2',stop_voice='direct call to the same verified handle-release routine 0x4c03ac used by empty-name sound stop',fade_voice='entry passes 2000 to shared fade function; full envelope/completion behavior not verified here'),limits=['Original Glyph.Build/Build2 layout is not implemented; current plain text reveals Unicode scalar values','FadeVoice full envelope/completion remains unverified; KeepVoice=2 is rejected','Auto, skip, backlog, rich text, MuteBGM and persistent settings remain outside this batch'])
a.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8');print('PASS original text/NEXT/KeepVoice source')
