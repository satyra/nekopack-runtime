"""Execute BLESS raw DAT through literal []; pin prior prefix and honest next boundary."""
import argparse,hashlib,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for n in ['cli','games','pages','baseline','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();m=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
root=a.pages/'0';assert not root.exists() or not any(root.iterdir())
r=subprocess.run([str(a.cli.resolve()),'preview-story',str(root),'300',*[str(a.games/s['file']) for s in m['sources']]],capture_output=True)
assert r.returncode==1,r.stderr.decode('utf8')
boundary=json.loads((root/'boundary.json').read_text(encoding='utf8'))
assert [boundary[k] for k in ['source','line','depth','steps']]==['012501',121,2,994],boundary
assert (root/'boundary.txt').read_text(encoding='utf8').endswith('unsupported command bgm between actor motion and dialogue')
paths=sorted(root.glob('[0-9]*.json'),key=lambda p:int(p.stem));assert len(paths)==175
rows=[];motions=[];sources=set()
for n,path in enumerate(paths):
 page=json.loads(path.read_text(encoding='utf8'));assert page['sequence']==n+1;sources.add(page['source'])
 if n<59:
  assert path.read_bytes()==(a.baseline/'0'/path.name).read_bytes()
  assert path.with_suffix('.png').read_bytes()==(a.baseline/'0'/path.with_suffix('.png').name).read_bytes()
 if 'motion' in page:motions.append(n+1)
 if n+1 in [60,159]:
  slot,x,source,line,name=(6,990,'012402',97,'SGVE201a') if n+1==60 else (1,640,'012501',31,'SGVE101a')
  assert [page['source'],page['line']]==[source,line]
  assert page['motion']==dict(duration_ms=250,actors=[dict(slot=slot,start_x=x,end_x=x,center_x=640,kind='stationary',start_y=360,end_y=360,center_y=360,amplitude_x=0)])
  actor=next(l for l in page['scene']['actors'] if l['slot']==slot)
  assert actor['anchor']==[x,360] and actor['name']==name
 # Clear slot 6 then reload without a position modifier resets its stable anchor.
 if page['source']=='012402' and page['line']==139:
  actor=next(l for l in page['scene']['actors'] if l['slot']==6)
  assert actor['anchor']==[640,360] and actor['name']=='SGVE202a'
 rows.append(dict(sequence=n+1,source=page['source'],line=page['line'],steps=page['steps'],png_sha256=hashlib.sha256(path.with_suffix('.png').read_bytes()).hexdigest()))
assert motions==[56,57,60,159]
assert {'012402','012403','012404','012501'}<=sources
assert any(r['source']=='012402' and r['line']==139 for r in rows)
a.output.write_text(json.dumps(dict(scope='BLESS raw DAT headless endpoints; no device animation claim',cli_sha256=hashlib.sha256(a.cli.read_bytes()).hexdigest(),unchanged_prefix_pages=59,motion_pages=motions,pages=rows,boundary=boundary),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS BLESS 175 pages, first 59 identical; next boundary 012501:121 motion/BGM scheduling')
