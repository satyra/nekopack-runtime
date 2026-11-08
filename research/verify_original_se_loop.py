"""Verify original SE registration and shared audio parser against the local BLESS executable."""
import argparse,hashlib,json,struct,sys
from pathlib import Path
import pefile,capstone
p=argparse.ArgumentParser()
for name in ['games','reference-root','output']:p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();raw=next(a.games.rglob('Tiny_BoD.exe')).read_bytes();sha=hashlib.sha256(raw).hexdigest();assert sha=='fc2a82cc733ae8fd9585b9e151d7f6a2ca5f1572c2df017e20d330eeff219521'
pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
basis=Path('research/env-batch23-original.json');old=json.loads(basis.read_text(encoding='utf8'))
# Shared command parser and playback path, excluding the unrelated env restore path.
expected=[(int(f['va'],16),f['instruction']) for f in old['facts'] if int(f['va'],16)<0x447000 or int(f['va'],16)>=0x448000]
expected += [(0x444712,'push 0x550a6f'),(0x44471b,'push 0x550a6c'),(0x444725,'call 0x4447dc'),(0x445292,'mov dl, byte ptr [eax + 8]'),(0x445297,'je 0x4452a0')]
facts=[]
for va,wanted in expected:
 i=next(md.disasm(pe.get_data(va-base,16),va));actual=(i.mnemonic+' '+i.op_str).strip();assert actual==wanted,(hex(va),actual,wanted);facts.append(dict(va=hex(va),instruction=actual))
for va,text in [(0x550a6c,b'se'),(0x550a6f,b'sound/se'),(0x550a92,b'loop'),(0x550a97,b'once')]:assert pe.get_data(va-base,len(text)+1)==text+b'\0'
sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources'];packs=[(s['priority'],Archive(a.games/s['file'])) for s in sources if any(x in s['file'] for x in ['system.dat','sound.dat','patch'])]
def read(name):
 choices=[(priority,pack) for priority,pack in packs if name in pack.entries];best=max(p for p,_ in choices);selected=[pack for p,pack in choices if p==best];assert len(selected)==1;return selected[0].read(name)
script=read('script/012802.txt');lines=script.split(b'\n');commands=[]
for line,wanted in [(28,b'.se SE_028'),(42,b'.se SE_028 loop'),(54,b'.se')]:
 assert lines[line-1].strip()==wanted;commands.append(dict(source='012802',line=line,command=wanted.decode()))
data=read('sound/se/se_028.ogg');h=data.index(b'\x01vorbis');rate=struct.unpack_from('<I',data,h+12)[0];channels=data[h+11];at=0
while at<len(data):
 assert data[at:at+4]==b'OggS';samples=struct.unpack_from('<Q',data,at+6)[0];n=data[at+26];at+=27+n+sum(data[at+27:at+27+n])
i=data.index(b'\x03vorbis')+7;n=struct.unpack_from('<I',data,i)[0];i+=4+n;comments=struct.unpack_from('<I',data,i)[0];assert comments==0 and samples*channels*2<0x200000
result=dict(scope='BLESS original static code and raw DAT, not acoustic parity',exe_sha256=sha,shared_evidence_sha256=hashlib.sha256(basis.read_bytes()).hexdigest(),facts=facts,script_sha256=hashlib.sha256(script).hexdigest(),commands=commands,resource=dict(name='SE_028',sha256=hashlib.sha256(data).hexdigest(),sample_rate=rate,channels=channels,total_samples=samples,duration_seconds=samples/rate,vorbis_comments=comments),semantics=dict(channel_index=2,directory='sound/se',default_loop=False,loop_option=True,once_option=False,default_command_volume_percent=100,volume_note='descriptor type, not overridden loop bit, chooses default volume',named='replace only selected channel; start then return without waiting for completion',empty='release selected channel immediately, no implicit fade'),limits=['No original live acoustic/seamless-loop comparison','Global/category volumes and resumable audio state remain outside this batch','Other SE modifiers remain rejected'])
a.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8');print('PASS original SE loop/once/stop',len(facts),'instruction checks')
