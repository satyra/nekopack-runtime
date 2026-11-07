"""Run BLESS raw DAT through the motion/BGM interleave, with exact prior prefix."""
import argparse,hashlib,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for n in ['cli','games','pages','baseline','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();m=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
root=a.pages/'0';assert not root.exists() or not any(root.iterdir())
r=subprocess.run([str(a.cli.resolve()),'preview-story',str(root),'400',*[str(a.games/s['file']) for s in m['sources']]],capture_output=True)
assert r.returncode==1,r.stderr.decode('utf8')
boundary=json.loads((root/'boundary.json').read_text(encoding='utf8'))
assert [boundary[k] for k in ['source','line','depth','steps']]==['012501',582,2,1455],boundary
assert (root/'boundary.txt').read_text(encoding='utf8').endswith('unsupported actor modifier [[[[')
paths=sorted(root.glob('[0-9]*.json'),key=lambda p:int(p.stem));assert len(paths)==269
rows=[];motions=[]
for n,path in enumerate(paths):
 page=json.loads(path.read_text(encoding='utf8'));assert page['sequence']==n+1
 if n<175:
  assert path.read_bytes()==(a.baseline/'0'/path.name).read_bytes()
  assert path.with_suffix('.png').read_bytes()==(a.baseline/'0'/path.with_suffix('.png').name).read_bytes()
 if 'motion' in page:motions.append(n+1)
 if n==175:
  assert [page['source'],page['line'],page['voice'],page['effects']]==['012501',125,'VE_012501_0003',['bgm BGM021']]
  assert page['motion']==dict(duration_ms=250,actors=[dict(slot=1,start_x=640,end_x=640,center_x=640,kind='stationary',start_y=360,end_y=360,center_y=360,amplitude_x=0)])
  actor=next(l for l in page['scene']['actors'] if l['slot']==1)
  assert actor['anchor']==[640,360] and actor['name']=='SGVE00Aa'
 if n>175:assert 'motion' not in page
 rows.append(dict(sequence=n+1,source=page['source'],line=page['line'],steps=page['steps'],png_sha256=hashlib.sha256(path.with_suffix('.png').read_bytes()).hexdigest()))
assert motions==[56,57,60,159,176]
a.output.write_text(json.dumps(dict(scope='BLESS raw DAT headless endpoints; audio-start ordering requires separate device evidence',cli_sha256=hashlib.sha256(a.cli.read_bytes()).hexdigest(),unchanged_prefix_pages=175,motion_pages=motions,pages=rows,boundary=boundary),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS BLESS 269 pages, first 175 identical; next boundary 012501:582 [[[[')
