"""Run BLESS raw DAT through environment start/stop and the following night dialogue."""
import argparse,hashlib,json,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
for n in ['cli','games','pages','baseline','output']:p.add_argument('--'+n,type=Path,required=True)
a=p.parse_args();m=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
root=a.pages/'0';assert not root.exists() or not any(root.iterdir())
r=subprocess.run([str(a.cli.resolve()),'preview-story',str(root),'598',*[str(a.games/s['file']) for s in m['sources']]],capture_output=True)
assert r.returncode==0,r.stderr.decode('utf8')
paths=sorted(root.glob('[0-9]*.json'),key=lambda p:int(p.stem));assert len(paths)==598 and not (root/'boundary.json').exists()
rows=[];events=[]
for index,path in enumerate(paths):
 page=json.loads(path.read_text(encoding='utf8'));assert page['sequence']==index+1
 if index<563:
  for suffix in ['.json','.png']:assert path.with_suffix(suffix).read_bytes()==(a.baseline/'0'/path.with_suffix(suffix).name).read_bytes()
 for effect in page['effects']:
  if effect.startswith('env'):events.append(dict(sequence=index+1,effect=effect))
 if index==563:assert [page['source'],page['line']]==['012701',14]
 if index==592:assert [page['source'],page['line']]==['012702',11] and 'env_stop' in page['effects']
 if index==597:assert [page['source'],page['line']]==['012702',36] and 'bgm_stop' in page['effects']
 rows.append(dict(sequence=index+1,source=page['source'],line=page['line'],png_sha256=hashlib.sha256(path.with_suffix('.png').read_bytes()).hexdigest()))
assert events==[dict(sequence=564,effect='env ENV_001'),dict(sequence=593,effect='env_stop')]
a.output.write_text(json.dumps(dict(scope='BLESS bounded raw DAT execution, not an unsupported boundary',page_limit=598,unchanged_prefix_pages=563,cli_sha256=hashlib.sha256(a.cli.read_bytes()).hexdigest(),environment_events=events,pages=rows),indent=2)+'\n',encoding='utf8');print('PASS BLESS 598 pages; prior 563 identical; environment start/stop and later BGM stop')
