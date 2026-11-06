"""BLESS-only raw DAT progression through `_`; exact prior prefix and next boundary."""
import argparse,hashlib,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for n in ['cli','games','pages','baseline','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();m=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
root=a.pages/'0';assert not root.exists() or not any(root.iterdir())
r=subprocess.run([str(a.cli.resolve()),'preview-story',str(root),'90',*[str(a.games/s['file']) for s in m['sources']]],capture_output=True)
assert r.returncode==1
boundary=json.loads((root/'boundary.json').read_text(encoding='utf8'));assert [boundary[k] for k in ['source','line','depth']]==['012402',94,2],boundary
assert (root/'boundary.txt').read_text(encoding='utf8').endswith('unsupported actor modifier []')
paths=sorted(root.glob('[0-9]*.json'),key=lambda p:int(p.stem));assert len(paths)==59
rows=[]
for n,path in enumerate(paths):
 page=json.loads(path.read_text(encoding='utf8'));assert page['sequence']==n+1
 if n<56:
  assert path.read_bytes()==(a.baseline/'0'/path.name).read_bytes()
  assert path.with_suffix('.png').read_bytes()==(a.baseline/'0'/path.with_suffix('.png').name).read_bytes()
 else:
  assert page['source']=='012402' and page['line']==[84,87,92][n-56]
  assert next(l for l in page['scene']['actors'] if l['slot']==6)['anchor']==[990,360]
  if n==56:
   assert page['motion']==dict(duration_ms=250,actors=[dict(slot=6,start_x=990,end_x=990,center_x=640,kind='dip',start_y=360,end_y=360,center_y=360,amplitude_y=50)])
  else:assert 'motion' not in page
 rows.append(dict(sequence=n+1,source=page['source'],line=page['line'],steps=page['steps'],png_sha256=hashlib.sha256(path.with_suffix('.png').read_bytes()).hexdigest()))
a.output.write_text(json.dumps(dict(scope='BLESS headless endpoints; no device animation claim',cli_sha256=hashlib.sha256(a.cli.read_bytes()).hexdigest(),unchanged_prefix_pages=56,pages=rows,boundary=boundary),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS BLESS 59 pages, first 56 identical; next boundary 012402:94 []')
