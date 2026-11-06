"""Generate continuous desktop pages and assert known unsupported TD boundaries.
A nonzero exit alone never passes; counts, location, error and state must match.
"""
import argparse,hashlib,json,subprocess
from pathlib import Path

def main():
 p=argparse.ArgumentParser()
 for name in ['cli','games','pages','output']:p.add_argument('--'+name,type=Path,required=True)
 p.add_argument('--profile',choices=['batch9','batch10','batch11','batch12','batch13','batch14'],default='batch10')
 a=p.parse_args();manifest=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))
 expected=[(12,'story1',10,1,196,'unsupported story preview command movie'),(8,'012401',52,2,183,'unsupported full-screen image'),(41,'010901',180,2,340,'BGM fade requires explicit duration in this profile'),(16,'P01',71,2,195,'unsupported full-screen image')]
 if a.profile=='batch10':expected=[(12,'story1',10,1,196,'unsupported story preview command movie'),(8,'012401',58,2,189,'unsupported story preview command shake'),(72,'011001',137,2,484,'unsupported story preview command shake'),(16,'P01',74,2,198,'unsupported story preview command shake')]
 if a.profile=='batch11':expected=[(12,'story1',10,1,196,'unsupported story preview command movie'),(33,'012401',276,2,407,'unsupported story preview command flash'),(100,'011001',247,2,594,None),(19,'P01',90,2,214,'unsupported story preview command flash')]
 if a.profile=='batch12':expected=[(12,'story1',10,1,196,'unsupported story preview command movie'),(97,'012401',615,2,746,'preview effect requires one argument'),(120,'011002',19,2,688,None),(49,'P01',229,2,353,'preview effect requires one argument')]
 if a.profile=='batch13':expected=[(12,'story1',10,1,196,'unsupported story preview command movie'),(120,'012401',711,2,842,None),(120,'011002',19,2,688,None),(49,'story1',10,1,362,'unsupported story preview command movie')]
 if a.profile=='batch14':expected=[(12,'story1',12,1,198,'unsupported story preview command route'),(120,'012401',711,2,842,None),(120,'011002',19,2,688,None),(49,'story1',12,1,364,'unsupported story preview command route')]
 limit=120 if a.profile in ['batch12','batch13','batch14'] else 100
 results=[]
 for i,(g,(count,source,line,depth,steps,error)) in enumerate(zip(manifest['games'],expected)):
  root=a.pages/str(i)
  if root.exists() and any(root.iterdir()):raise RuntimeError('use an empty output directory; stale pages are not accepted')
  files=[str(a.games/s['file']) for s in g['sources']]
  run=subprocess.run([str(a.cli.resolve()),'preview-story',str(root),str(limit),*files],capture_output=True)
  assert run.returncode==(1 if error else 0),run.stderr
  assert len(list(root.glob('[0-9]*.json')))==count
  if error:
   checkpoint=json.loads((root/'boundary.json').read_text(encoding='utf8'))
   message=(root/'boundary.txt').read_text(encoding='utf8')
   assert message==f'story {source}:{line} depth={depth} steps={steps}: {error}'
  else:
   assert not (root/'boundary.json').exists()
   checkpoint=json.loads((root/f'{count-1}.json').read_text(encoding='utf8'))
   message='page limit reached; not a script boundary or game completion'
  assert [checkpoint[k] for k in ['source','line','depth','steps']]==[source,line,depth,steps]
  if i==0:
   assert [71,800,1] in checkpoint['slots'] and not checkpoint['scene']['actors']
  pages=[]
  for n in range(count):
   page=json.loads((root/f'{n}.json').read_text(encoding='utf8'));assert page['sequence']==n+1
   pages.append(dict(sequence=n+1,source=page['source'],line=page['line'],depth=page['depth'],steps=page['steps'],png_sha256=hashlib.sha256((root/f'{n}.png').read_bytes()).hexdigest()))
  results.append(dict(index=i,pages=pages,boundary=checkpoint if error else None,error=message,page_limit=not bool(error)))
  print('PASS boundary',i,count,flush=True)
 a.output.write_text(json.dumps(dict(scope=f'desktop raw DAT static profile; stop at first unsupported request or explicit {limit}-page limit, not game completion',cli_sha256=hashlib.sha256(a.cli.read_bytes()).hexdigest(),games=results),indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
