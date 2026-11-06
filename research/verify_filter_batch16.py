"""BLESS-only raw DAT progression through Night actors to the next explicit boundary."""
import argparse,hashlib,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for n in ['cli','games','pages','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();m=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
root=a.pages/'0';assert not root.exists() or not any(root.iterdir())
r=subprocess.run([str(a.cli.resolve()),'preview-story',str(root),'90',*[str(a.games/s['file']) for s in m['sources']]],capture_output=True)
assert r.returncode==1
assert (root/'boundary.txt').read_text(encoding='utf8')=='story 012402:72 depth=2 steps=408: unsupported actor modifier >>>>>>>'
boundary=json.loads((root/'boundary.json').read_text(encoding='utf8'));assert [boundary[k] for k in ['source','line','depth','steps']]==['012402',72,2,408]
paths=sorted(root.glob('[0-9]*.json'),key=lambda p:int(p.stem));assert len(paths)==55
rows=[]
for n,path in enumerate(paths):
 page=json.loads(path.read_text(encoding='utf8'));assert page['sequence']==n+1
 if n<12:
  old=json.loads((Path('.local/batch15-story-verified/0')/path.name).read_text(encoding='utf8'))
  stripped=json.loads(json.dumps(page));stripped['scene'].pop('filter')
  for actor in stripped['scene']['actors']:actor.pop('filter');actor.pop('filter_pending')
  assert stripped==old and path.with_suffix('.png').read_bytes()==(Path('.local/batch15-story-verified/0')/path.with_suffix('.png').name).read_bytes()
 if n==12:assert page['source']=='012401' and page['line']==11 and page['labels']['route']=='第１章'
 actors=page['scene']['actors'];assert all(not l['filter_pending'] for l in actors)
 rows.append(dict(sequence=n+1,source=page['source'],line=page['line'],depth=page['depth'],steps=page['steps'],active_filter=page['scene']['filter'],actors=[dict(slot=l['slot'],filter=l['filter']) for l in actors],png_sha256=hashlib.sha256(path.with_suffix('.png').read_bytes()).hexdigest()))
assert any(row['actors'] and all(l['filter']=='night' for l in row['actors']) for row in rows[12:])
a.output.write_text(json.dumps(dict(scope='BLESS only; headless movie control-flow continuation is not video playback; 12 unchanged prologue pages and 43 post-movie pages',cli_sha256=hashlib.sha256(a.cli.read_bytes()).hexdigest(),pages=rows,boundary=boundary),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS BLESS 55 pages, movie-to-dialogue and explicit movement boundary')
