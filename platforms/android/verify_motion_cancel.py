"""BLESS-only regression to a probe-triggered background during the real dialogue motion."""
import argparse,hashlib,json,re,subprocess,time,uuid,xml.etree.ElementTree as ET
from pathlib import Path
p=argparse.ArgumentParser()
for n in ['adb','desktop','output']:p.add_argument('--'+n,type=Path,required=True)
p.add_argument('--serial',required=True);p.add_argument('--page',type=int,default=56);a=p.parse_args();assert a.page>0
prefix=[str(a.adb),'-s',a.serial];pkg='org.nekopack.probe';run='motion_cancel_'+uuid.uuid4().hex

def adb(*args):return subprocess.check_output(prefix+list(args),timeout=90)
def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
def tap(text):
 adb('shell','uiautomator','dump','/sdcard/neko-motion-cancel.xml');xml=adb('shell','cat','/sdcard/neko-motion-cancel.xml')
 node=next(n for n in ET.fromstring(xml).iter('node') if n.get('text')==text and n.get('enabled')=='true')
 x,y,r,b=map(int,re.findall(r'\d+',node.get('bounds')));adb('shell','input','tap',str((x+r)//2),str((y+b)//2))
build=json.loads(Path('.local/android-probe/build.json').read_text());apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:')
assert adb('shell','sha256sum',apk).decode().split()[0]==build['sha256']['nekopack-arm64-probe.apk']
device={k:adb('shell','getprop',k).decode().strip() for k in ['ro.product.cpu.abilist','ro.dalvik.vm.native.bridge','ro.build.version.sdk']};device['kernel_machine']=adb('shell','uname','-m').decode().strip()
assert device['kernel_machine']=='aarch64' and device['ro.dalvik.vm.native.bridge'] in ('','0')
adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','story_game','0','--es','run_id',run,'--ez','close_on_motion','true','--ei','close_on_motion_sequence',str(a.page))
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
deadline=time.monotonic()+15
while time.monotonic()<deadline:
 closed=read('lifecycle-closed.json')
 if closed.startswith(b'{'):break
 time.sleep(.1)
else:raise AssertionError('VM cleanup timeout')
assert json.loads(closed)=={'closed':True}
pause=json.loads(read('lifecycle-pause.json'));motion=read('motion-events.jsonl');all_events=[json.loads(line) for line in motion.splitlines()];events=[e for e in all_events if e['sequence']==a.page]
assert events[0]['event']=='begin' and events[0]['sequence']==a.page
assert any(e['event']=='frame' and e['elapsed_ms']==0 for e in events), 'cancellation never reached an actual rendered motion frame'
assert len([e for e in events if e['event']=='cancelled' and e['sequence']==a.page])==1
assert not any(e['event']=='finished' for e in events)
assert events[-1]['event']=='cancelled' and events[-1]['time_ms']>=pause['time_ms']
time.sleep(1);assert read('motion-events.jsonl')==motion
for name in [f'page-{a.page+1}.json','error.txt','boundary.json']:assert b'No such file' in read(name)
host=[json.loads(line) for line in read('host-events.jsonl').splitlines()];assert host[-1]['event']=='released' and host[-1]['value']==0
assert b'.ogg' not in adb('shell','run-as',pkg,'ls',f'files/{run}')
movie=[json.loads(line) for line in read('movie-8-events.jsonl').splitlines()];assert movie[-1]['event']=='closed' and movie[-1]['reason']==2 and movie[-1]['error'] is None
result=dict(scope=f'BLESS real title-to-page{a.page} through explicit movie skip; probe requests background 80ms after scheduling target page motion; onPause ends session, not resumable save',execution='native ARM64',device=device,run_id=run,build=build,pages=pages,target_page=a.page,motion_events=events,all_motion_events=all_events,pause=pause,host_events=host,movie_end=movie[-1],vm_live_bytes=0,no_late_motion_after_close=True)
a.output.write_text(json.dumps(result,indent=2)+'\n');print('PASS BLESS motion cancellation and VM cleanup',flush=True)
