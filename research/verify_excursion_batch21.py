"""Run BLESS raw DAT through left excursion, preserving the prior prefix."""
import argparse,hashlib,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for n in ['cli','games','pages','baseline','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();m=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
root=a.pages/'0';assert not root.exists() or not any(root.iterdir())
r=subprocess.run([str(a.cli.resolve()),'preview-story',str(root),'600',*[str(a.games/s['file']) for s in m['sources']]],capture_output=True)
assert r.returncode==1,r.stderr.decode('utf8')
boundary=json.loads((root/'boundary.json').read_text(encoding='utf8'))
assert [boundary[k] for k in ['source','line','depth','steps']]==['012603',211,2,2650],boundary
assert (root/'boundary.txt').read_text(encoding='utf8').endswith('motion must be the sole terminal position modifier')
paths=sorted(root.glob('[0-9]*.json'),key=lambda p:int(p.stem));assert len(paths)==500
rows=[];motions=[]
for n,path in enumerate(paths):
 page=json.loads(path.read_text(encoding='utf8'));assert page['sequence']==n+1
 if n<269:
  assert path.read_bytes()==(a.baseline/'0'/path.name).read_bytes()
  assert path.with_suffix('.png').read_bytes()==(a.baseline/'0'/path.with_suffix('.png').name).read_bytes()
 if 'motion' in page:motions.append(n+1)
 if n==269:
  assert [page['source'],page['line'],page['voice'],page['effects']]==['012501',585,'VE_012501_0011',[]]
  assert page['motion']==dict(duration_ms=250,actors=[dict(slot=1,start_x=1120,end_x=1120,center_x=640,kind='horizontal_excursion',start_y=360,end_y=360,center_y=360,amplitude_x=-200)])
  actor=next(l for l in page['scene']['actors'] if l['slot']==1)
  assert actor['anchor']==[1120,360] and actor['name']=='SGVE00Aa'
 if n>269:assert 'motion' not in page
 rows.append(dict(sequence=n+1,source=page['source'],line=page['line'],steps=page['steps'],png_sha256=hashlib.sha256(path.with_suffix('.png').read_bytes()).hexdigest()))
assert motions==[56,57,60,159,176,270]
a.output.write_text(json.dumps(dict(scope='BLESS raw DAT headless endpoints; intermediate excursion frames require separate device evidence',cli_sha256=hashlib.sha256(a.cli.read_bytes()).hexdigest(),unchanged_prefix_pages=269,motion_pages=motions,pages=rows,boundary=boundary),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS BLESS 500 pages, first 269 identical; next boundary 012603:211 combined motion')
