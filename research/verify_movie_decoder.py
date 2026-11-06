"""Diagnostic FFmpeg comparison; measured differences are NOT a fidelity pass.
Private decoded data stays under --references; the report contains metrics only.
"""
from pathlib import Path
import subprocess,json,hashlib,argparse,numpy as np
p=argparse.ArgumentParser()
for name in ["ffmpeg","games","decoder","references","output"]:p.add_argument("--"+name,type=Path,required=True)
args=p.parse_args()
ff=str(args.ffmpeg)
root=args.games;reports=[]
for i,name in enumerate(['Tiny_BoD.exe','Tiny_BfY.exe','Tiny_BoS.exe','Tiny_BaW.exe']):
 movie=next(root.rglob(name)).parent/'op.mpg';out=args.references/str(i);out.mkdir(parents=True,exist_ok=True)
 base=['-hide_banner','-loglevel','error','-y','-i',str(movie)]
 subprocess.run([ff,*base,'-vf','select=eq(n\\,0)+eq(n\\,24)+eq(n\\,240)','-fps_mode','passthrough','-frames:v','3','-pix_fmt','rgba','-f','rawvideo',str(out/'frames.rgba')],check=True)
 subprocess.run([ff,*base,'-t','3','-vn','-acodec','pcm_s16le','-f','s16le',str(out/'audio.pcm')],check=True)
 ref=args.decoder/str(i);info=json.loads((ref/'report.json').read_text());pixels=info['width']*info['height']*4
 data=(out/'frames.rgba').read_bytes();assert len(data)==pixels*3
 frames=[]
 for j,n in enumerate([0,24,240]):
  x=np.frombuffer(data[j*pixels:(j+1)*pixels],np.uint8).reshape(-1,4)[:,:3].astype(np.int16);y=np.frombuffer((ref/f'frame-{n}.rgba').read_bytes(),np.uint8).reshape(-1,4)[:,:3].astype(np.int16)
  d=np.abs(x-y);frames.append(dict(frame=n,mae=float(d.mean()),max_error=int(d.max()),p99=float(np.percentile(d,99))))
 a=np.frombuffer((out/'audio.pcm').read_bytes(),'<i2').astype(np.float64);b=np.frombuffer((ref/'audio.pcm').read_bytes(),'<i2').astype(np.float64);assert a.shape==b.shape
 d=a-b;pcm=dict(rms_error=float(np.sqrt(np.mean(d*d))),max_error=float(np.max(np.abs(d))),reference_rms=float(np.sqrt(np.mean(a*a))),correlation=float(np.corrcoef(a,b)[0,1]),relative_gain=float(np.sum(a*b)/np.sum(a*a)))
 reports.append(dict(game=i,frames=frames,pcm=pcm,decoder=info));print(i,frames,pcm,flush=True)
args.output.write_text(json.dumps(dict(scope='FFmpeg independent first/24/240 decoded RGB frames and first 3 seconds PCM; not original DirectShow renderer parity',ffmpeg_sha256=hashlib.sha256(Path(ff).read_bytes()).hexdigest(),games=reports),indent=2)+'\n')
