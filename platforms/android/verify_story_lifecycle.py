"""Interrupt actual host effects with HOME; ensure cancellation and VM cleanup."""
import argparse,json,subprocess,time,uuid
from pathlib import Path

def main():
 p=argparse.ArgumentParser();p.add_argument('--adb',type=Path,required=True);p.add_argument('--serial',required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
 prefix=[str(a.adb),'-s',a.serial];package='org.nekopack.probe'
 def adb(*args):return subprocess.check_output(prefix+list(args),timeout=45)
 def read(run,file):return adb('exec-out','run-as',package,'cat',f'files/{run}/{file}')
 results=[]
 for game,kind,event in [(0,'fade','begin'),(2,'bgm','playing')]:
  run='cancel_'+uuid.uuid4().hex
  adb('shell','am','start','-S','-n',package+'/.MainActivity','--ei','story_game',str(game),'--es','run_id',run)
  deadline=time.monotonic()+45;observed=None
  while time.monotonic()<deadline:
   text=read(run,'host-events.jsonl')
   if text.startswith(b'{'):
    for line in text.decode().splitlines():
     row=json.loads(line)
     if row['kind']==kind and row['event']==event:observed=row;break
   if observed:break
   time.sleep(.1)
  if not observed:raise RuntimeError('effect never started')
  adb('shell','input','keyevent','3')
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   closed=read(run,'lifecycle-closed.json')
   if closed.startswith(b'{'):
    if json.loads(closed)!={'closed':True}:raise RuntimeError('native session cleanup failed')
    break
   time.sleep(.1)
  else:raise RuntimeError('onDestroy cleanup timeout')
  host=[json.loads(line) for line in read(run,'host-events.jsonl').decode().splitlines()]
  released=[e for e in host if e['event']=='released']
  if len(released)!=1 or released[0]['value']!=0:raise RuntimeError('players not released')
  if any(e['event']=='ack' and e['time_ms']>released[0]['time_ms'] for e in host):raise RuntimeError('late completion after close')
  if kind=='fade' and any(e['event']=='ack' and e['token']==observed['token'] for e in host):raise RuntimeError('cancelled fade acknowledged')
  if b'.ogg' in adb('shell','run-as',package,'ls',f'files/{run}'):raise RuntimeError('audio cache leaked')
  if b'No such file' not in read(run,'error.txt'):raise RuntimeError('host failure during cancellation')
  results.append(dict(game=game,interrupted=kind,run_id=run,host_events=host,closed= True));print('PASS cancelled',kind,flush=True)
 build=json.loads(Path('.local/android-probe/build.json').read_text())
 apk=adb('shell','pm','path',package).decode().strip().removeprefix('package:')
 if adb('shell','sha256sum',apk).decode().split()[0]!=build['sha256']['nekopack-arm64-probe.apk']:raise RuntimeError('APK mismatch')
 a.output.write_text(json.dumps(dict(scope='HOME during fade or BGM; onStop ends session, no resume/save claim',build=build,tests=results),indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
