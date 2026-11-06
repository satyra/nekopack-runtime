"""UI input and lifecycle checks for the original movie host; no script shortcuts."""
import argparse,json,re,subprocess,time,uuid,xml.etree.ElementTree as ET
from pathlib import Path

def main():
 p=argparse.ArgumentParser();p.add_argument('--adb',required=True);p.add_argument('--serial',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 prefix=[a.adb,'-s',a.serial];pkg='org.nekopack.probe';rows=[]
 def adb(*args):return subprocess.check_output(prefix+list(args),timeout=90)
 def read(run,name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
 def events(run):
  raw=read(run,'movie-1-events.jsonl');return [] if raw.startswith(b'cat:') else [json.loads(x) for x in raw.splitlines()]
 build=json.loads(Path('.local/android-probe/build.json').read_text())
 apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:')
 assert adb('shell','sha256sum',apk).decode().split()[0]==build['sha256']['nekopack-arm64-probe.apk']
 def until(f):
  deadline=time.monotonic()+45
  while time.monotonic()<deadline:
   result=f()
   if result:return result
   time.sleep(.3)
  raise AssertionError('movie control deadline')
 def tap_button():
  adb('shell','uiautomator','dump','/sdcard/neko-movie-ui.xml');xml=adb('shell','cat','/sdcard/neko-movie-ui.xml')
  node=next(n for n in ET.fromstring(xml).iter('node') if n.get('text')=='返回剧情')
  x,y,r,b=map(int,re.findall(r'\d+',node.get('bounds')));adb('shell','input','tap',str((x+r)//2),str((y+b)//2))
 for mode in ['default-primary-ignored-secondary-exit','explicit-primary-exit','background']:
  run='movie_control_'+uuid.uuid4().hex
  adb('shell','am','start','-S','-n',pkg+'/.MainActivity','--ei','movie_probe_game','0','--es','run_id',run,'--ez','movie_left_skip','true' if mode=='explicit-primary-exit' else 'false')
  until(lambda:any(e['event']=='progress' and e['head']>0 for e in events(run)))
  if mode=='background':adb('shell','input','keyevent','3')
  else:
   size=adb('shell','wm','size').decode();w,h=map(int,re.findall(r'(\d+)x(\d+)',size)[-1]);x,y=max(w,h)//2,min(w,h)//2
   adb('shell','input','tap',str(x),str(y))
   if mode=='default-primary-ignored-secondary-exit':
    time.sleep(2);assert not any(e['event']=='closed' for e in events(run))
    assert b'No such file' in read(run,'movie-complete.json');tap_button()
  end=until(lambda:next((e for e in events(run) if e['event']=='closed'),None))
  assert end['error'] is None and end['frames']>0 and end['head']>0,end
  expected={'default-primary-ignored-secondary-exit':2,'explicit-primary-exit':1,'background':3}[mode];assert end['reason']==expected
  if mode=='background':assert b'No such file' in read(run,'movie-complete.json')
  else:assert json.loads(read(run,'movie-complete.json'))['reason']==expected
  before=read(run,'movie-1-events.jsonl');time.sleep(1);assert read(run,'movie-1-events.jsonl')==before
  rows.append(dict(mode=mode,run_id=run,events=events(run),late_events=False));print('PASS controls',a.serial,mode,flush=True)
 a.output.write_text(json.dumps(dict(scope='original BLESS MPG host UI and background cleanup; explicit skip variant is host probe, not modified game script',serial=a.serial,build=build,cases=rows),indent=2)+'\n')
if __name__=='__main__':main()
