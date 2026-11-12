"""Rebuild isolated audio fixtures from original UI archives and selected voice bytes.
No original story text or images are decoded. Synthetic script has test labels only.
"""
from pathlib import Path
import argparse,hashlib,json,re,shutil,struct,subprocess
ROOT=Path(__file__).resolve().parents[3]
BASE=ROOT/'.local/android-probe'
CLI=ROOT/'target/x86_64-pc-windows-gnu/release/nekopack-cli.exe'
ADB='D:/codex/tools/android/sdk/platform-tools/adb.exe'
shift=bytes(int(s,16) for s in re.findall(r'0x([0-9A-F]{2})',(ROOT/'crates/nekopack-archive/src/names.rs').read_text()))
def key(seed,s):
 for c in s.encode('ascii'):seed=81*(seed^shift[c])&0xffffffff
 return seed

def sha(path):
 with path.open('rb') as f:return hashlib.file_digest(f,'sha256').hexdigest()

def extract(packs,directory,name):
 rows=subprocess.check_output([str(CLI),'lookup-name',directory.encode().hex(),name.encode().hex(),*packs]).decode().splitlines()
 selected=rows[0].split('\t');assert selected[0]=='selected',(directory,name,rows)
 source=packs[int(selected[1])];ordinal=int(selected[2])
 data=subprocess.check_output([str(CLI),'read-entry',source,str(ordinal)])
 return data,dict(archive=source,ordinal=ordinal,name=directory+'/'+name,sha256=hashlib.sha256(data).hexdigest(),bytes=len(data))

def pack(path,entries):
 groups={}
 for directory,name,data in entries:groups.setdefault(directory,[]).append((name,data))
 index=b'';payload=b''
 for directory,items in groups.items():
  index+=struct.pack('<II',key(0,directory),len(items))
  for name,data in items:
   index+=struct.pack('<II',key(0,name),len(data))
   payload+=struct.pack('<II',0,len(data))+data
 path.write_bytes(b'NEKOPACK'+struct.pack('<IIII',0,1000000,0,len(index))+index+payload)

def main():
 p=argparse.ArgumentParser()
 p.add_argument('--deploy',action='store_true')
 p.add_argument('--serial',default='emulator-5572')
 p.add_argument('--games',nargs='+',choices=['BIRTH','BRAVE','BLACK'],default=['BIRTH','BRAVE','BLACK'])
 p.add_argument('--output',type=Path,default=BASE/'isolated-fixtures')
 p.add_argument('--birth-ka-original',action='store_true',help='Use independently verified BIRTH KA_EP02_0000 resource')
 a=p.parse_args()
 index=json.loads((BASE/'remaining-route-index.json').read_text());assets=json.loads((BASE/'remaining-extra-voice-assets.json').read_text())
 if a.birth_ka_original:
  assets['BIRTH']['KA']='KA_EP02_0000'
 out=a.output;out.mkdir(parents=True,exist_ok=True);report=[]
 for game in a.games:
  name=game+('-ENV-TEST' if game=='BIRTH' else '-AUDIO-TEST');dest=out/name
  assert not dest.exists(),f'Refusing to overwrite fixture {dest}'
  dest.mkdir();sources=index[game]['dat'];record=dict(game=game,directory=name,files=[],voice_resources=[])
  wanted={'system.dat','sound.dat'}|({'script.dat'} if game=='BLACK' else set())|({'patch101.dat'} if game=='BIRTH' else set())
  for source in sources:
   path=Path(source)
   if path.name in wanted:
    shutil.copy2(path,dest/path.name)
    record['files'].append(dict(name=path.name,source=source,bytes=path.stat().st_size,sha256=sha(path)))
  entries=[]
  lines=['.env ENV_002' if game=='BIRTH' else '.img']
  for tag,cue in assets[game].items():
   source_game='BLESS' if game=='BIRTH' and tag=='KA' and cue=='KA_P01_0001' else game
   data,meta=extract(index[source_game]['dat'],'voice',cue+'.ogg');assert data.startswith(b'OggS'),meta
   meta.update(tag=tag,cue=cue,original_game=source_game,borrowed=source_game!=game)
   if game=='BIRTH' and tag=='KA' and cue=='KA_EP02_0000':
    evidence=BASE/'birth-ka-original-resource-evidence.json'
    original=json.loads(evidence.read_text(encoding='utf-8'))['voice_resource']
    assert meta['sha256']==original['sha256'] and meta['ordinal']==original['ordinal'],meta
    meta['evidence']=str(evidence)
   record['voice_resources'].append(meta);entries.append(('voice',cue+'.ogg',data))
   lines.extend([f':Probe:{cue}:{tag}','probe'])
  lines+=['.load title','']
  entries.append(('script','story1.txt','\n'.join(lines).encode('ascii')))
  entries.append(('script','titlelogo.txt',b'.load title\n'))
  pack(dest/'voice_matrix.dat',entries)
  record['files'].append(dict(name='voice_matrix.dat',source='synthetic script plus selected original voice bytes',bytes=(dest/'voice_matrix.dat').stat().st_size,sha256=sha(dest/'voice_matrix.dat')))
  # Validate the archive and title -> synthetic story path without viewing generated images.
  subprocess.run([str(CLI),'inspect-archive',str(dest/'voice_matrix.dat')],stdout=subprocess.DEVNULL,check=True)
  validation=dest.parent/(name+'-validation');validation.mkdir()
  result=subprocess.run([str(CLI),'preview-title-story',str(validation),str(len(assets[game])),*[str(f) for f in dest.glob('*.dat')]],capture_output=True)
  (validation/'run.log').write_bytes(result.stdout+result.stderr)
  record['offline_validation_exit']=result.returncode
  if result.returncode==0:
   record['offline_pages']=[]
   for page,(tag,cue) in enumerate(assets[game].items()):
    state=json.loads((validation/f'{page}.json').read_text(encoding='utf-8'))
    assert state['source']=='story1' and state['line']==3+2*page,(game,tag,state['source'],state['line'])
    assert state['voice']==cue,(game,tag,state['voice'],cue)
    record['offline_pages'].append(dict(page=page+1,tag=tag,source=state['source'],line=state['line'],voice=state['voice']))
  record['offline_validation_log']=str(validation/'run.log')
  (dest/'manifest.json').write_text(json.dumps(record,indent=2),encoding='utf-8')
  report.append(record)
  print(game,'BUILD',result.returncode,[(f['name'],f['bytes']) for f in record['files']],flush=True)
  if result.returncode:print(result.stderr.decode(errors='replace'),flush=True)
 (out/'manifest.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
 assert all(r['offline_validation_exit']==0 for r in report),'Offline fixture validation failed; not deploying'
 if a.deploy:
  for record in report:
   remote='/storage/emulated/0/Download/NekoPack-BLESS/'+record['directory']
   check=subprocess.run([ADB,'-s',a.serial,'shell','test','-e',remote])
   assert check.returncode==1,f'Remote path already exists: {remote}'
   subprocess.run([ADB,'-s',a.serial,'push',str(out/record['directory']),remote],check=True)
   for f in record['files']:
    line=subprocess.check_output([ADB,'-s',a.serial,'shell','sha256sum',remote+'/'+f['name']]).decode()
    assert line.split()[0]==f['sha256'],line
   print('DEPLOYED',record['game'],remote,flush=True)
if __name__=='__main__':main()
