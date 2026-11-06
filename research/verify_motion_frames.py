"""Independent raw DAT/Pillow composition at every captured native actor-motion frame."""
import argparse,hashlib,io,json,math,sys
from pathlib import Path
from PIL import Image
p=argparse.ArgumentParser()
for n in ['games','reference-root','captures','device-report','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
manifest=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
packs=[(s['priority'],Archive(a.games/s['file'])) for s in manifest['sources'] if any(x in s['file'] for x in ['image.dat','system.dat','script.dat','patch'])]
def read(name):
 choices=[(p,pack) for p,pack in packs if name.lower() in pack.entries];best=max(p for p,_ in choices);selected=[pack for p,pack in choices if p==best];assert len(selected)==1
 return selected[0].read(name)
report=json.loads(a.device_report.read_text(encoding='utf8'));assert report['execution']=='native ARM64';assert len(report['games'])==1 and report['games'][0]['index']==0
game=report['games'][0];assert len(game['motions'])==1 and game['closed_vm_live_bytes']==0
rows=[]
for capture in sorted((a.captures/'0').iterdir()):
 page=json.loads((capture/'page.json').read_text(encoding='utf8'));assert [page[k] for k in ['sequence','source','line']]==[56,'012402',76]
 lines=read('script/012402.txt').decode('cp932').splitlines();assert lines[71].strip()=='.img6 SGVE00Aa >>>>>>>';assert lines[72].strip()=='.img4 SGUR000a';assert lines[75].strip()==page['text']
 assert page['motion']==dict(duration_ms=250,actors=[dict(slot=6,start_x=640,end_x=990,center_x=640)])
 events=json.loads((capture/'events.json').read_text());frames=[e for e in events if e['event']=='frame'];assert any(0<e['elapsed_ms']<250 for e in frames)
 host=game['host_events'];voice=[e for e in host if e['event']=='playing' and e['kind']=='voice' and e['time_ms']<=events[0]['time_ms']];assert voice
 token=voice[-1]['token'];audio_begins=[e for e in host if e['event']=='begin' and e['kind'] in ['bgm','se','voice']];assert len(audio_begins)==len(game['audio']);assert next(meta for begin,meta in zip(audio_begins,game['audio']) if begin['token']==token)['name']=='UR_012402_0001'
 scene=page['scene'];assert scene['size']==[1280,720] and scene['background']=='BG201d' and scene['visual'] is None and scene['overlay'] is None
 images={}
 for layer in scene['actors']:
  actor=Image.open(io.BytesIO(read(layer['folder']+'/'+layer['name']+'.png'))).convert('RGBA');assert layer['filter']=='night'
  images[layer['slot']]=actor.point([max(0,v-17) for v in range(256)]+[max(0,v-22) for v in range(256)]+[min(255,v+17) for v in range(256)]+list(range(256)))
 for e in frames:
  t=e['elapsed_ms'];x=990 if t==250 else 640+int(350*(math.sin((t/250-.5)*math.pi)+1)*.5)
  canvas=Image.open(io.BytesIO(read('image/back/BG201d.png'))).convert('RGBA')
  for layer in scene['actors']:
   anchor=[x,360] if layer['slot']==6 else layer['anchor'];position=tuple(v+o for v,o in zip(anchor,layer['offset']))
   canvas.alpha_composite(images[layer['slot']],position)
  actual=(capture/f'{t}.rgba').read_bytes();assert actual==canvas.tobytes(),f'frame mismatch at {t}ms'
  rows.append(dict(sequence=page['sequence'],elapsed_ms=t,actor6_x=x,rgba_sha256=hashlib.sha256(actual).hexdigest(),independent_pixels_equal=True))
assert rows and rows[0]['actor6_x']==640 and rows[-1]['actor6_x']==990
assert len(set(r['rgba_sha256'] for r in rows))>=3
a.output.write_text(json.dumps(dict(scope='BLESS native ARM64; every captured motion frame compared with independent raw DAT/Pillow and pinned original curve, not original screen parity',device_report_sha256=hashlib.sha256(a.device_report.read_bytes()).hexdigest(),build=report['build'],voice_started_before_motion=True,frames=rows),indent=2)+'\n')
print('PASS independent motion frames',len(rows))
