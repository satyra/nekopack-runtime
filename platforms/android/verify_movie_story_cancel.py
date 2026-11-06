"""Background a real BLESS story movie and require both decoder and VM cleanup."""
import argparse,json,re,subprocess,time,uuid,xml.etree.ElementTree as ET
from pathlib import Path

def main():
 p=argparse.ArgumentParser();p.add_argument('--adb',required=True);p.add_argument('--serial',required=True);p.add_argument('--desktop',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 prefix=[a.adb,'-s',a.serial];pkg='org.nekopack.probe';run='movie_story_cancel_'+uuid.uuid4().hex
 def adb(*args):return subprocess.check_output(prefix+list(args),timeout=90)
 def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
 build=json.loads(Path('.local/android-probe/build.json').read_text())
 apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:')
 assert adb('shell','sha256sum',apk).decode().split()[0]==build['sha256']['nekopack-arm64-probe.apk']
 def until(f):
  deadline=time.monotonic()+45
  while time.monotonic()<deadline:
   value=f()
   if value:return value
   time.sleep(.2)
  raise AssertionError('movie story cancel deadline')
 def page(n):
  raw=read(f'page-{n}.json');return json.loads(raw) if raw.startswith(b'{') else None
 def tap_next():
  adb('shell','uiautomator','dump','/sdcard/neko-movie-cancel.xml');xml=adb('shell','cat','/sdcard/neko-movie-cancel.xml')
  node=next(n for n in ET.fromstring(xml).iter('node') if n.get('text')=='继续' and n.get('enabled')=='true')
  x,y,r,b=map(int,re.findall(r'\d+',node.get('bounds')));adb('shell','input','tap',str((x+r)//2),str((y+b)//2))
 adb('shell','am','start','-S','-n',pkg+'/.MainActivity','--ei','story_game','0','--es','run_id',run)
 for n in range(1,13):
  actual=until(lambda:page(n));assert actual==json.loads((a.desktop/'0'/f'{n-1}.json').read_text(encoding='utf8'));tap_next()
 def movie_token():
  names=adb('shell','run-as',pkg,'ls','-1',f'files/{run}').decode().splitlines()
  return next((int(n.split('-')[1]) for n in names if re.fullmatch(r'movie-\d+-info.json',n)),None)
 token=until(movie_token)
 def events():return [json.loads(x) for x in read(f'movie-{token}-events.jsonl').splitlines()]
 until(lambda:any(e['event']=='progress' and e['head']>0 for e in events()))
 pending=json.loads(read(f'effect-{token}.json'));assert pending['host_effect']=='movie' and pending['name']=='op.mpg' and pending['token']==token
 adb('shell','input','keyevent','3')
 until(lambda:read('lifecycle-closed.json').startswith(b'{'))
 assert json.loads(read('lifecycle-closed.json'))=={'closed':True}
 closed=until(lambda:next((e for e in events() if e['event']=='closed'),None))
 assert closed['error'] is None and closed['reason']==3 and closed['frames']>0 and closed['samples']>0
 time.sleep(1);assert events()[-1]==closed
 for name in ['boundary.json','page-13.json','error.txt']:assert b'No such file' in read(name)
 host=[json.loads(x) for x in read('host-events.jsonl').splitlines()];assert host[-1]['event']=='released' and host[-1]['value']==0
 assert not any(e['token']>token for e in host)
 a.output.write_text(json.dumps(dict(scope='background during real story1:10 movie; no movie ACK, no following route execution; both native owners closed',serial=a.serial,run_id=run,pending=pending,movie_events=events(),host_events=host,vm_live_bytes=0,build=build),indent=2)+'\n')
 print('PASS movie story background',a.serial,flush=True)
if __name__=='__main__':main()
