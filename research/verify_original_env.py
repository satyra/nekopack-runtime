"""BLESS original environment channel: registration, loop flag, replacement and stop."""
import argparse,hashlib,json,struct,sys
from pathlib import Path
import pefile,capstone
p=argparse.ArgumentParser()
for name in ['games','reference-root','output']:p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();raw=next(a.games.rglob('Tiny_BoD.exe')).read_bytes()
assert hashlib.sha256(raw).hexdigest()=='fc2a82cc733ae8fd9585b9e151d7f6a2ca5f1572c2df017e20d330eeff219521'
pe=pefile.PE(data=raw);base=pe.OPTIONAL_HEADER.ImageBase;md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
expected=[(0x4bf6d4,'push','0xf4240'),(0x4bf7f6,'call','0x46fd50'),(0x46fd90,'fdivp','st(1)'),(0x46fd95,'call','0x504cf4'),(0x46fd9d,'fmul','dword ptr [0x46fde8]'),(0x504880,'fldlg2',''),(0x4448a7,'mov','byte ptr [ebp - 0x20e], 0'),(0x4448ae,'mov','byte ptr [ebp - 0x20f], 0'),(0x445283,'je','0x4452bd'),(0x445299,'mov','ecx, 0x32'),(0x4452a0,'mov','ecx, 0x64'),(0x4452a5,'mov','dword ptr [ebp - 0x214], ecx'),(0x4465f8,'jne','0x446847'),(0x446606,'jne','0x446847'),(0x4c00a9,'cmp','eax, 0x200000'),(0x4c00ae,'jae','0x4c00e5'),(0x4446d9,'push','0'),(0x4446db,'push','1'),(0x4446f3,'push','0x550a62'),(0x4446f8,'push','1'),(0x4446fa,'push','1'),(0x444706,'call','0x4447dc'),(0x444717,'push','2'),(0x444719,'push','0'),(0x44480f,'mov','byte ptr [edx + 8], cl'),(0x444818,'mov','byte ptr [eax + 9], dl'),(0x44489f,'mov','byte ptr [ebp - 0x20d], cl'),(0x444ad3,'mov','byte ptr [ebp - 0x20d], 1'),(0x444b12,'mov','byte ptr [ebp - 0x20d], 0'),(0x4453ab,'mov','cl, byte ptr [ebp - 0x20d]'),(0x445ccf,'mov','byte ptr [ebp - 0x18d], dl'),(0x445fca,'movsx','edx, byte ptr [eax + 9]'),(0x445fd1,'lea','ebx, [edx + edx*4]'),(0x445fd4,'lea','ebx, [edx + ebx*4]'),(0x445fd7,'shl','ebx, 3'),(0x445fda,'add','ebx, 0x584340'),(0x445fe2,'je','0x446598'),(0x44600d,'call','0x4c03ac'),(0x446082,'call','0x4492a4'),(0x4460ea,'mov','cl, byte ptr [ebp - 0x18d]'),(0x4460f6,'call','0x4c0024'),(0x4c0208,'mov','dl, byte ptr [ebp + 0x10]'),(0x4c020f,'call','dword ptr [ecx + 4]'),(0x4bf865,'call','0x46fb44'),(0x46fb78,'mov','edx, 1'),(0x46fb89,'call','dword ptr [eax + 0x30]'),(0x446610,'call','0x4c03ac'),(0x4c0405,'mov','dword ptr [ebx + 4], edx'),(0x44686e,'ret',''),(0x44717c,'mov','ebx, 0x5843e8'),(0x4471b7,'push','0x550b40'),(0x4471bd,'call','0x4492a4'),(0x4471c5,'push','1'),(0x4471cc,'call','0x4c0024')]
facts=[]
for va,mn,op in expected:
 i=next(md.disasm(pe.get_data(va-base,16),va));assert (i.mnemonic,i.op_str)==(mn,op),(hex(va),i.mnemonic,i.op_str)
 facts.append(dict(va=hex(va),instruction=(mn+' '+op).strip()))
assert struct.unpack('<f',pe.get_data(0x46fde8-base,4))[0]==2000.0
for va,text in [(0x56d5e0,b'log10'),(0x550a5e,b'env'),(0x550a62,b'sound/env'),(0x550a92,b'loop'),(0x550a97,b'once'),(0x550aa5,b'loop'),(0x556c52,b'XSoundBuf::Play')]:assert pe.get_data(va-base,len(text)+1)==text+b'\0'
assert struct.unpack('<I',pe.get_data(0x5612e0-base,4))[0]==0x4bf5fc
sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
packs=[(s['priority'],Archive(a.games/s['file'])) for s in sources if any(x in s['file'] for x in ['system.dat','sound.dat','patch'])]
def read(name):
 choices=[(priority,pack) for priority,pack in packs if name in pack.entries];best=max(p for p,_ in choices);selected=[pack for p,pack in choices if p==best];assert len(selected)==1
 return selected[0].read(name)
commands=[]
for name in sorted(set(n for _,pack in packs for n in pack.entries if n.startswith('script/') and n.endswith('.txt'))):
 for line,rawline in enumerate(read(name).split(b'\n'),1):
  text=rawline.strip().decode('cp932')
  if text=='.env' or text.startswith('.env '):commands.append(dict(source=name,line=line,command=text))
assert len(commands)==12
assert [c['line'] for c in commands if c['source']=='script/012701.txt']==[9,164]
resources=[]
for name in ['env_000','env_001','env_002']:
 data=read('sound/env/'+name+'.ogg');head=data.index(b'\x01vorbis');rate=struct.unpack_from('<I',data,head+12)[0];channels=data[head+11]
 at=0
 while at<len(data):
  assert data[at:at+4]==b'OggS';granule=struct.unpack_from('<Q',data,at+6)[0];count=data[at+26];at+=27+count+sum(data[at+27:at+27+count])
 i=data.index(b'\x03vorbis')+7;size=struct.unpack_from('<I',data,i)[0];i+=4+size;comments=struct.unpack_from('<I',data,i)[0];assert comments==0
 assert granule*channels*2<0x200000
 resources.append(dict(channels=channels,decoded_pcm_bytes=granule*channels*2,name=name,sha256=hashlib.sha256(data).hexdigest(),sample_rate=rate,total_samples=granule,duration_seconds=granule/rate,vorbis_comments=comments))
result=dict(scope='BLESS original static executable and raw DAT; no original live acoustic parity claim',exe_sha256=hashlib.sha256(raw).hexdigest(),facts=facts,commands=commands,resources=resources,semantics=dict(directory='sound/env',channel_index=1,bgm_channel_index=0,se_channel_index=2,default_loop=True,default_command_volume_percent=50,volume_mapping='command/category/fade percent factors multiplied, divided by 1000000; 2000*log10(ratio) to DirectSound attenuation',named='release selected channel handle, load resource, start looping; no wait for end of audio',empty='release selected channel immediately; no implicit fade',restore='original restores the saved env channel name with looping; runtime save/resume not added in this batch'),limits=['Only bare stop and one resource name are implemented; explicit once/loop, volume, pan and fade modifiers remain rejected','Original master/category volume, loop seam quality and complete save/resume are not verified', 'The original BGM descriptor also defaults to 50; its existing adapter gain behavior is unchanged in this env-only batch and needs separate alignment'])
a.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8');print('PASS original env evidence',len(facts),'instructions',len(commands),'raw DAT commands')
