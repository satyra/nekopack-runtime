"""Exercise real UI clicks and compare Android story pages with desktop and source text.
Requires a prior SAF grant; never creates or grants URI permissions.
"""
import argparse,hashlib,json,re,subprocess,time,uuid,xml.etree.ElementTree as ET
from pathlib import Path

def completed_audio(events,token):
 playing=[e for e in events if e['event']=='playing' and e['token']==token]
 return len(playing)==1 and playing[0]['value']>0 and any(e['event']=='completed' and e['token']==token and e['time_ms']-playing[0]['time_ms']>=playing[0]['value'] for e in events)

def main():
 p=argparse.ArgumentParser()
 for name in ['adb','desktop','fixtures','output']:p.add_argument('--'+name,type=Path,required=True)
 p.add_argument('--audio-reference',type=Path)
 p.add_argument('--until-boundary',action='store_true')
 p.add_argument('--serial',required=True);a=p.parse_args()
 prefix=[str(a.adb),'-s',a.serial];package='org.nekopack.probe'
 def adb(*args):return subprocess.check_output(prefix+list(args),timeout=90)
 def private(run,name):return adb('exec-out','run-as',package,'cat',f'files/{run}/{name}')
 def wait_page(run,n):
  deadline=time.monotonic()+45
  while time.monotonic()<deadline:
   raw=private(run,f'page-{n}.json')
   if raw.startswith(b'{'):return json.loads(raw)
   error=private(run,'error.txt')
   if b'No such file' not in error:raise RuntimeError(error.decode())
   time.sleep(.3)
  raise RuntimeError('page timeout')
 def wait_audio(run):
  deadline=time.monotonic()+10
  while time.monotonic()<deadline:
   events=[json.loads(line) for line in private(run,'host-events.jsonl').decode().splitlines()]
   tokens={e['token'] for e in events if e['event']=='begin' and e['kind'] in ['bgm','se','voice']}
   advanced={e['token'] for e in events if e['event'] in ['progress','completed'] and e['value']>0}
   if all(token in advanced or completed_audio(events,token) for token in tokens):return
   time.sleep(.2)
  raise RuntimeError('audio did not advance within the observation window')
 def tap(text):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   adb('shell','uiautomator','dump','/sdcard/neko-story-ui.xml')
   xml=adb('shell','cat','/sdcard/neko-story-ui.xml')
   for node in ET.fromstring(xml).iter('node'):
    if node.get('text')==text and node.get('enabled')=='true':
     x,y,r,b=map(int,re.findall(r'\d+',node.get('bounds')));adb('shell','input','tap',str((x+r)//2),str((y+b)//2));return
   time.sleep(.3)
  raise RuntimeError('enabled control not found: '+text)
 build=json.loads(Path('.local/android-probe/build.json').read_text())
 apkpath=adb('shell','pm','path',package).decode().strip().removeprefix('package:')
 assert adb('shell','sha256sum',apkpath).decode().split()[0]==build['sha256']['nekopack-arm64-probe.apk']
 games=[]
 for i in range(4):
  run='story_'+uuid.uuid4().hex
  adb('shell','am','start','-S','-n',package+'/.MainActivity','--ei','story_game',str(i),'--es','run_id',run)
  pages=[]
  count=len(list((a.desktop/str(i)).glob('[0-9]*.json'))) if a.until_boundary else 2
  assert count>0
  last_page_audio=set()
  for n in range(1,count+1):
   actual=wait_page(run,n)
   expected=json.loads((a.desktop/str(i)/f'{n-1}.json').read_text(encoding='utf8'))
   if actual!=expected:raise RuntimeError('Android/desktop page metadata mismatch')
   # The file must be stable while the user has not acknowledged this page.
   assert private(run,f'page-{n}.json')==private(run,f'page-{n}.json')
   png=private(run,f'page-{n}.png');assert png==(a.desktop/str(i)/f'{n-1}.png').read_bytes()
   source=next(f for f in (a.fixtures/str(i)/'script').iterdir() if f.name.lower()==(actual['source']+'.txt').lower())
   lines=source.read_bytes().split(b'\n');assert lines[actual['line']-1].strip().decode('utf8')==actual['text']
   pages.append(dict(sequence=n,source=actual['source'],line=actual['line'],text_sha256=hashlib.sha256(actual['text'].encode()).hexdigest(),png_sha256=hashlib.sha256(png).hexdigest(),metadata_sha256=hashlib.sha256(json.dumps(actual,sort_keys=True,ensure_ascii=False).encode()).hexdigest(),effects=actual['effects']))
   if a.audio_reference:wait_audio(run)
   if a.audio_reference:last_page_audio={e['token'] for e in [json.loads(line) for line in private(run,'host-events.jsonl').decode().splitlines()] if e['event']=='begin' and e['kind'] in ['bgm','se','voice']}
   if n<count or a.until_boundary:tap('继续')
  boundary=None
  if a.until_boundary:
   deadline=time.monotonic()+45
   while time.monotonic()<deadline:
    raw=private(run,'boundary.json')
    if raw.startswith(b'{'):break
    time.sleep(.3)
   else:raise RuntimeError('unsupported boundary timeout')
   boundary=json.loads(raw)
   assert boundary['checkpoint']==json.loads((a.desktop/str(i)/'boundary.json').read_text(encoding='utf8'))
   assert boundary['error']==(a.desktop/str(i)/'boundary.txt').read_text(encoding='utf8')
   assert boundary['vm_live_bytes']==0
   assert b'No such file' in private(run,f'page-{count+1}.json')
  tap('结束运行' if a.audio_reference else '结束预览（无音频／转场）')
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   if private(run,'closed.txt')==b'closed; VM live bytes=0':break
   time.sleep(.2)
  else:raise RuntimeError('session did not close cleanly')
  if not a.until_boundary and b'No such file' not in private(run,'error.txt'):raise RuntimeError('host reported a story error')
  host=[];audio=[]
  if a.audio_reference:
   host=[json.loads(line) for line in private(run,'host-events.jsonl').decode().splitlines()]
   begins=[e for e in host if e['event']=='begin'];acks=[e for e in host if e['event']=='ack']
   if [e['token'] for e in begins]!=[e['token'] for e in acks]:raise RuntimeError('missing/duplicate host completion')
   reference=json.loads(a.audio_reference.read_text(encoding='utf8'))['games'][i]
   for begin,ack in zip(begins,acks):
    if begin['kind'] in ['fade','wait','bgm_fade']:
     if ack['value']<begin['value']:raise RuntimeError('premature timed completion')
     if begin['kind'] in ['fade','bgm_fade'] and not any(e['event']=='frame' and e['token']==begin['token'] and 0<e['value']<1000 for e in host):raise RuntimeError('fade had no intermediate frame')
    elif begin['kind'] in ['bgm','se','voice']:
     token=begin['token'];playing=[e for e in host if e['event']=='playing' and e['token']==token]
     progress=[e for e in host if e['event'] in ['progress','completed'] and e['token']==token and e['value']>0]
     completed=completed_audio(host,token)
     if len(playing)!=1 or (not progress and not completed and token in last_page_audio):raise RuntimeError('audio never started or advanced')
     audio_meta=json.loads(private(run,f'audio-{token}.json'))
     key=audio_meta['kind']+'/'+audio_meta['name'].lower()
     if audio_meta['sha256']!=reference[key]:raise RuntimeError('raw DAT audio mismatch')
     audio_meta['progress_observed']=bool(progress)
     audio_meta['completed_after_duration']=completed
     audio_meta['terminated_at_boundary']=not progress and not completed and token not in last_page_audio
     audio.append(audio_meta)
    else:raise RuntimeError('unrecognized host effect in verification')
    if begin['kind']=='bgm_fade' and not any(e['event']=='stopped' and e['kind']=='bgm' and e['token']==begin['token'] for e in host):raise RuntimeError('BGM fade did not release player')
   if set(reference)!=set(e['kind']+'/'+e['name'].lower() for e in audio):raise RuntimeError('missing expected audio resource')
   if not host or host[-1]['event']!='released' or host[-1]['value']!=0:raise RuntimeError('audio not released')
   if b'.ogg' in adb('shell','run-as',package,'ls',f'files/{run}'):raise RuntimeError('audio cache was not removed')
  games.append(dict(index=i,run_id=run,boundary=boundary,pages=pages,actual_ui_next=True,closed_vm_live_bytes=0,host_events=host,audio=audio))
  print('PASS game',i,flush=True)
 device={k:adb('shell','getprop',k).decode().strip() for k in ['ro.product.cpu.abilist','ro.dalvik.vm.native.bridge','ro.build.version.sdk']}
 device['kernel_machine']=adb('shell','uname','-m').decode().strip()
 result=dict(scope=('continuous pages to explicit unsupported boundary' if a.until_boundary else 'two text pages per game; host-driven audio and timed fade') if a.audio_reference else 'two text pages per game; static scene preview; audio and transitions recorded only',execution='translated ARM64' if device['kernel_machine']=='x86_64' else ('native ARM64' if device['kernel_machine']=='aarch64' and device['ro.dalvik.vm.native.bridge'] in ('','0') else 'unclassified; inspect properties'),device=device,build=build,games=games)
 a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
