"""Run raw BLESS DAT through the queued movement and following dialogue (502 pages)."""
import argparse,hashlib,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for n in ['cli','games','pages','baseline','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();m=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
root=a.pages/'0';assert not root.exists() or not any(root.iterdir())
r=subprocess.run([str(a.cli.resolve()),'preview-story',str(root),'502',*[str(a.games/s['file']) for s in m['sources']]],capture_output=True)
assert r.returncode==0,r.stderr.decode('utf8')
paths=sorted(root.glob('[0-9]*.json'),key=lambda p:int(p.stem));assert len(paths)==502
assert not (root/'boundary.json').exists()
rows=[];motions=[]
for n,path in enumerate(paths):
 page=json.loads(path.read_text(encoding='utf8'));assert page['sequence']==n+1
 if n<500:
  for suffix in ['.json','.png']:assert path.with_suffix(suffix).read_bytes()==(a.baseline/'0'/path.with_suffix(suffix).name).read_bytes()
 if 'motion' in page:motions.append(n+1)
 if n==500:
  assert [page['source'],page['line'],page['voice']]==['012603',215,'NO_012603_0004']
  assert page['motion']==dict(duration_ms=500,actors=[dict(slot=2,kind='queue',segments=[dict(slot=2,start_x=640,end_x=640,center_x=640,kind='dip',start_y=360,end_y=360,center_y=360,amplitude_y=50),dict(slot=2,start_x=640,end_x=840,center_x=640)])])
 if n in [500,501]:assert next(l for l in page['scene']['actors'] if l['slot']==2)['anchor']==[840,360]
 if n==501:assert 'motion' not in page
 rows.append(dict(sequence=n+1,source=page['source'],line=page['line'],png_sha256=hashlib.sha256(path.with_suffix('.png').read_bytes()).hexdigest()))
assert motions==[56,57,60,159,176,270,501]
a.output.write_text(json.dumps(dict(scope='BLESS raw DAT CLI bounded regression; not an unsupported boundary',page_limit=502,unchanged_prefix_pages=500,cli_sha256=hashlib.sha256(a.cli.read_bytes()).hexdigest(),motion_pages=motions,pages=rows),indent=2)+'\n',encoding='utf8')
print('PASS BLESS 502 pages; first 500 identical; queue endpoint and following dialogue')
