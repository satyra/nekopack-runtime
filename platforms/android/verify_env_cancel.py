"""BLESS-only regression to an ADB Home-triggered background while the real looping audio is playing."""
import argparse,hashlib,json,re,subprocess,time,uuid,xml.etree.ElementTree as ET
from pathlib import Path
p=argparse.ArgumentParser()
for n in ['adb','desktop','output']:p.add_argument('--'+n,type=Path,required=True)
p.add_argument('--channel',choices=['env','se'],default='env');p.add_argument('--serial',required=True);p.add_argument('--page',type=int,default=564);a=p.parse_args();assert a.page>0
prefix=[str(a.adb),'-s',a.serial];pkg='org.nekopack.probe';run=a.channel+'_cancel_'+uuid.uuid4().hex

def adb(*args):return subprocess.check_output(prefix+list(args),timeout=90)
def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
continue_point=None
def tap(text):
 global continue_point
 # The cancellation run verifies every resulting page. Reuse the observed button
 # within the fixed landscape story layout; movie UI transitions invalidate it.
 if text=='继续' and continue_point is not None:
  adb('shell','input','tap',*continue_point);return
 continue_point=None
 adb('shell','uiautomator','dump','/sdcard/neko-motion-cancel.xml');xml=adb('shell','cat','/sdcard/neko-motion-cancel.xml')
 node=next(n for n in ET.fromstring(xml).iter('node') if n.get('text')==text and n.get('enabled')=='true')
 x,y,r,b=map(int,re.findall(r'\d+',node.get('bounds')));point=(str((x+r)//2),str((y+b)//2))
 if text=='继续':continue_point=point
 adb('shell','input','tap',*point)
build=json.loads(Path('.local/android-probe/build.json').read_text());apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:')
assert adb('shell','sha256sum',apk).decode().split()[0]==build['sha256']['nekopack-arm64-probe.apk']
device={k:adb('shell','getprop',k).decode().strip() for k in ['ro.product.cpu.abilist','ro.dalvik.vm.native.bridge','ro.build.version.sdk']};device['kernel_machine']=adb('shell','uname','-m').decode().strip()
assert device['kernel_machine']=='aarch64' and device['ro.dalvik.vm.native.bridge'] in ('','0')
adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','story_game','0','--es','run_id',run)
skipped=False;pages=[]
for n in range(1,a.page+1):
 deadline=time.monotonic()+60
 while time.monotonic()<deadline:
  raw=read(f'page-{n}.json')
  if raw.startswith(b'{'):break
  if n==13 and not skipped:
   movie=read('movie-8-events.jsonl')
   if movie.startswith(b'{') and any(e['event']=='progress' and e['head']>0 for e in map(json.loads,movie.splitlines())):tap('返回剧情');skipped=True
  error=read('error.txt');assert b'No such file' in error,error
  time.sleep(.1)
 else:raise AssertionError('page deadline')
 page=json.loads(raw);assert page==json.loads((a.desktop/'0'/f'{n-1}.json').read_text(encoding='utf8'))
 pages.append(dict(sequence=n,source=page['source'],line=page['line'],metadata_sha256=hashlib.sha256(raw).hexdigest()))
 if n<a.page:tap('继续')
before=[json.loads(line) for line in read('host-events.jsonl').splitlines()]
env=next(e for e in reversed(before) if e['event']=='playing' and e['kind']==a.channel);token=env['token']
assert env['value']>0
assert any(e['event']=='gain' and e['token']==token and e['value']==(50 if a.channel=='env' else 100) for e in before)
assert any(e['event']=='looping' and e['token']==token and e['value']==1 for e in before)
assert any(e['event']=='progress' and e['token']==token and e['value']>0 for e in before)
assert not any(e['event']=='completed' and e['token']==token for e in before)
adb('shell','input','keyevent','KEYCODE_HOME')
deadline=time.monotonic()+15
while time.monotonic()<deadline:
 closed=read('lifecycle-closed.json')
 if closed.startswith(b'{'):break
 time.sleep(.1)
else:raise AssertionError('VM cleanup timeout')
assert json.loads(closed)=={'closed':True}
pause=json.loads(read('lifecycle-pause.json'));snapshot=read('host-events.jsonl')
time.sleep(1);assert read('host-events.jsonl')==snapshot
for name in [f'page-{a.page+1}.json','error.txt','boundary.json']:assert b'No such file' in read(name)
host=[json.loads(line) for line in read('host-events.jsonl').splitlines()];assert host[-1]['event']=='released' and host[-1]['value']==0
assert b'.ogg' not in adb('shell','run-as',pkg,'ls',f'files/{run}')
movie=[json.loads(line) for line in read('movie-8-events.jsonl').splitlines()];assert movie[-1]['event']=='closed' and movie[-1]['reason']==2 and movie[-1]['error'] is None
result=dict(scope=f'BLESS real title-to-page{a.page}; actual Home key while {a.channel} audio is playing; onPause ends session, not resumable save',execution='native ARM64',device=device,run_id=run,build=build,pages=pages,target_page=a.page,channel=a.channel,**({"environment_token":token,"environment_before_pause":[e for e in before if e['token']==token]} if a.channel=='env' else {"sound_effect_token":token,"sound_effect_before_pause":[e for e in before if e['token']==token]}),pause=pause,host_events=host,movie_end=movie[-1],vm_live_bytes=0,no_late_audio_after_close=True)
a.output.write_text(json.dumps(result,indent=2)+'\n');print('PASS BLESS active',a.channel,'cancellation and VM cleanup',flush=True)
