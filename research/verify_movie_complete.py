"""Decode whole original movies with FFmpeg and compare EOF counts and PCM.
The numeric PCM tolerance is this regression profile, not original-engine parity.
"""
import argparse,hashlib,json,subprocess
from pathlib import Path
import numpy as np
p=argparse.ArgumentParser()
for name in ['ffmpeg','games','decoder','references','output']:p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();rows=[]
for i,name in enumerate(['Tiny_BoD.exe','Tiny_BfY.exe','Tiny_BoS.exe','Tiny_BaW.exe']):
 source=next(a.games.rglob(name)).parent/'op.mpg';out=a.references/str(i);out.mkdir(parents=True,exist_ok=True)
 command=[str(a.ffmpeg),'-hide_banner','-loglevel','error','-y','-i',str(source)]
 result=subprocess.run([*command,'-map','0:v:0','-fps_mode','passthrough','-progress','pipe:1','-f','null','-','-map','0:a:0','-acodec','pcm_s16le','-f','s16le',str(out/'complete.pcm')],capture_output=True,check=True)
 assert not result.stderr,result.stderr
 frames=[int(x.split('=')[1]) for x in result.stdout.decode().splitlines() if x.startswith('frame=')][-1]
 decoder=a.decoder/str(i);info=json.loads((decoder/'report.json').read_text())
 assert frames==info['frames'],(i,frames,info['frames'])
 x=np.frombuffer((out/'complete.pcm').read_bytes(),'<i2').astype(np.float64);y=np.frombuffer((decoder/'complete.pcm').read_bytes(),'<i2').astype(np.float64)
 assert len(x)==len(y)==info['samples_per_channel']*2
 delta=x-y;pcm=dict(rms_error=float(np.sqrt(np.mean(delta*delta))),max_error=float(np.max(np.abs(delta))),relative_gain=float(np.sum(x*y)/np.sum(x*x)),reference_rms=float(np.sqrt(np.mean(x*x))))
 assert pcm['rms_error']<=8 and pcm['max_error']<=64 and .999<=pcm['relative_gain']<=1.001,(i,pcm)
 result=subprocess.run([*command,'-vf',f'select=eq(n\\,{frames-1})','-fps_mode','passthrough','-frames:v','1','-pix_fmt','rgba','-f','rawvideo',str(out/'last.rgba')],capture_output=True,check=True)
 assert not result.stderr,result.stderr
 x=np.frombuffer((out/'last.rgba').read_bytes(),np.uint8).astype(np.int16);y=np.frombuffer((decoder/'last.rgba').read_bytes(),np.uint8).astype(np.int16);assert x.shape==y.shape
 last=dict(mae=float(np.abs(x-y).mean()),max_error=int(np.abs(x-y).max()))
 rows.append(dict(game=i,source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),frames=frames,samples_per_channel=len(delta)//2,pcm=pcm,last_frame=last,decoder_peak_bytes=info['peak_bytes']))
 print('PASS independent whole movie',i,frames,pcm,flush=True)
a.output.write_text(json.dumps(dict(scope='independent whole-file frame/PCM counts and PCM regression tolerance; last-frame RGB diagnostics; not original DirectShow or physical AV parity',pcm_tolerance=dict(rms=8,max=64,relative_gain=[.999,1.001]),ffmpeg_sha256=hashlib.sha256(a.ffmpeg.read_bytes()).hexdigest(),vendor_sha256_lf=hashlib.sha256(Path('vendor/pl_mpeg/pl_mpeg.h').read_text(encoding='utf8').encode()).hexdigest(),games=rows),indent=2)+'\n')
