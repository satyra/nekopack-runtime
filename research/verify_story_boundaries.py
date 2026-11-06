"""Generate continuous desktop pages and assert known unsupported TD boundaries.
A nonzero exit alone never passes; counts, location, error and state must match.
"""
import argparse,hashlib,json,subprocess
from pathlib import Path

def main():
 p=argparse.ArgumentParser()
 for name in ['cli','games','pages','output']:p.add_argument('--'+name,type=Path,required=True)
 a=p.parse_args();manifest=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))
 expected=[(12,'story1',10,1,196,'unsupported story preview command movie'),(8,'012401',52,2,183,'unsupported full-screen image'),(41,'010901',180,2,340,'BGM fade requires explicit duration in this profile'),(16,'P01',71,2,195,'unsupported full-screen image')]
 results=[]
 for i,(g,(count,source,line,depth,steps,error)) in enumerate(zip(manifest['games'],expected)):
  root=a.pages/str(i)
  if root.exists() and any(root.iterdir()):raise RuntimeError('use an empty output directory; stale pages are not accepted')
  files=[str(a.games/s['file']) for s in g['sources']]
  run=subprocess.run([str(a.cli.resolve()),'preview-story',str(root),'100',*files],capture_output=True)
  assert run.returncode==1,run.stderr
  assert len(list(root.glob('[0-9]*.json')))==count
  checkpoint=json.loads((root/'boundary.json').read_text(encoding='utf8'))
  message=(root/'boundary.txt').read_text(encoding='utf8')
  assert message==f'story {source}:{line} depth={depth} steps={steps}: {error}'
  assert [checkpoint[k] for k in ['source','line','depth','steps']]==[source,line,depth,steps]
  if i==0:
   assert [71,800,1] in checkpoint['slots'] and not checkpoint['scene']['actors']
  pages=[]
  for n in range(count):
   page=json.loads((root/f'{n}.json').read_text(encoding='utf8'));assert page['sequence']==n+1
   pages.append(dict(sequence=n+1,source=page['source'],line=page['line'],depth=page['depth'],steps=page['steps'],png_sha256=hashlib.sha256((root/f'{n}.png').read_bytes()).hexdigest()))
  results.append(dict(index=i,pages=pages,boundary=checkpoint,error=message))
  print('PASS boundary',i,count,flush=True)
 a.output.write_text(json.dumps(dict(scope='desktop raw DAT static profile; stop at first unsupported request, not game completion',cli_sha256=hashlib.sha256(a.cli.read_bytes()).hexdigest(),games=results),indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
