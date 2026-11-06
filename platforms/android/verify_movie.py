"""Play original loose MPEG files through the persisted SAF grant. No transcoding.
Host-only tests do not claim execution of a game's movie command.
"""
import argparse,hashlib,json,subprocess,time,uuid,struct,math
from pathlib import Path

def main():
 p=argparse.ArgumentParser()
 p.add_argument('--adb',required=True);p.add_argument('--serial',required=True)
 p.add_argument('--output',type=Path,required=True);p.add_argument('--captures',type=Path,required=True)
 p.add_argument('--desktop',type=Path,required=True);p.add_argument('--first-run')
 p.add_argument('--runs',nargs=4,help='Previously observed run IDs for rechecking saved evidence; - starts a new run')
 a=p.parse_args();prefix=[a.adb,'-s',a.serial];package='org.nekopack.probe'
 def adb(*args):return subprocess.check_output(prefix+list(args),timeout=90)
 def private(run,name):return adb('exec-out','run-as',package,'cat',f'files/{run}/{name}')
 build=json.loads(Path('.local/android-probe/build.json').read_text())
 apk=adb('shell','pm','path',package).decode().strip().removeprefix('package:')
 assert adb('shell','sha256sum',apk).decode().split()[0]==build['sha256']['nekopack-arm64-probe.apk']
 uploads=next(x for x in json.loads(Path('research/movie-batch14-upload.json').read_text()) if x['serial']==a.serial)
 reports=[]
 for game in range(4):
  reuse=(a.runs[game] if a.runs and a.runs[game]!='-' else None) or (a.first_run if game==0 else None)
  run=reuse or 'movie_'+uuid.uuid4().hex
  if not reuse:adb('shell','am','start','-W','-S','-n',package+'/.MainActivity','--ei','movie_probe_game',str(game),'--es','run_id',run)
  deadline=time.monotonic()+700
  out=a.captures/str(game);out.mkdir(parents=True,exist_ok=True)
  captured=bool(reuse and (out/'display.png').exists())
  while time.monotonic()<deadline:
   events_raw=private(run,'movie-1-events.jsonl')
   events=[] if events_raw.startswith(b'cat:') else [json.loads(x) for x in events_raw.splitlines()]
   if any(x['event']=='progress' and x['head']>0 for x in events) and not captured:
    (out/'display.png').write_bytes(adb('exec-out','screencap','-p'));captured=True
   if events and events[-1]['event']=='closed':break
   error=private(run,'error.txt');assert b'No such file' in error,error
   # pidof can race Android process publication even after am -W.
   # Success requires a complete, independently checked decode/close trace below.
   time.sleep(2)
  else:raise AssertionError('movie deadline')
  end=events[-1];assert end['event']=='closed' and end['error'] is None and end['reason']==0,end
  assert json.loads(private(run,'movie-complete.json'))['reason']==0
  info=json.loads(private(run,'movie-1-info.json'));ref=json.loads((a.desktop/str(game)/'report.json').read_text())
  assert info['source_sha256']==uploads['files'][game]['sha256']
  assert end['frames']==ref['frames'] and end['samples']==ref['samples_per_channel']
  assert end['head']>=end['samples']>0 and end['peak_bytes']<=64*1024*1024
  assert end['elapsed_ms']>=end['samples']/info['sample_rate']*1000-200
  samples=[]
  for n in [0,24,240]:
   data=private(run,f'movie-1-frame-{n}.rgba');expected=(a.desktop/str(game)/f'frame-{n}.rgba').read_bytes()
   assert data==expected,('decoder RGB platform mismatch',game,n)
   (out/f'frame-{n}.rgba').write_bytes(data);samples.append(dict(frame=n,sha256=hashlib.sha256(data).hexdigest(),desktop_equal=True))
  pcm=private(run,'movie-1-sample.pcm');(out/'sample.pcm').write_bytes(pcm)
  pcm_reference=(a.desktop/str(game)/'audio.pcm').read_bytes();assert len(pcm)==len(pcm_reference)>0
  actual_samples=struct.unpack('<'+str(len(pcm)//2)+'h',pcm);expected_samples=struct.unpack('<'+str(len(pcm)//2)+'h',pcm_reference)
  differences=[abs(x-y) for x,y in zip(actual_samples,expected_samples)]
  pcm_error=dict(max_error=max(differences),rms_error=math.sqrt(sum(d*d for d in differences)/len(differences)))
  assert pcm_error['max_error']<=4 and pcm_error['rms_error']<=2,('platform PCM mismatch',game,pcm_error)
  (out/'events.jsonl').write_bytes(events_raw);(out/'info.json').write_text(json.dumps(info))
  reports.append(dict(game=game,run_id=run,rechecked_observed_run=bool(reuse),info=info,closed=end,events=events,frames=samples,pcm_sha256=hashlib.sha256(pcm).hexdigest(),desktop_pcm_error=pcm_error,display_captured=captured))
  print('PASS movie natural',a.serial,game,end['frames'],end['elapsed_ms'],flush=True)
 kernel=adb('shell','uname','-m').decode().strip();bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip()
 result=dict(scope='four original MPEG files through SAF; standalone Android host, not four script-chain runs',serial=a.serial,execution='native ARM64' if kernel=='aarch64' and bridge in ('','0') else 'translated ARM64',kernel=kernel,native_bridge=bridge,build=build,games=reports)
 a.output.write_text(json.dumps(result,indent=2)+'\n')
if __name__=='__main__':main()
