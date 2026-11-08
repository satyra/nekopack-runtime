"""Verify frozen CLI output, unchanged BLESS prefix and the leftward chapter continuation."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser()
for name in ['pages','baseline','output']:p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();paths=sorted((a.pages/'0').glob('[0-9]*.json'),key=lambda p:int(p.stem));assert len(paths)==606
rows=[]
for i,path in enumerate(paths):
 page=json.loads(path.read_text(encoding='utf8'));assert page['sequence']==i+1
 if i<604:
  for ext in ['.json','.png']:assert path.with_suffix(ext).read_bytes()==(a.baseline/'0'/path.with_suffix(ext).name).read_bytes(),(i,ext)
 if i==604:
  assert [page['source'],page['line'],page['voice']]==['012702',70,'']
  assert page['motion']==dict(duration_ms=250,actors=[dict(slot=3,start_x=1120,end_x=670,center_x=640)])
  assert [(x['slot'],x['anchor']) for x in page['scene']['actors']]==[(1,[160,360]),(3,[670,360])]
 if i==605:
  assert [page['source'],page['line'],page['voice']]==['012702',82,'OP_012702_0003']
  assert 'motion' not in page and [x['slot'] for x in page['scene']['actors']]==[2]
  assert 'flash ffffff 1' in page['effects'] and 'se SE_024' in page['effects']
 rows.append(dict(sequence=i+1,source=page['source'],line=page['line'],png_sha256=hashlib.sha256(path.with_suffix('.png').read_bytes()).hexdigest()))
a.output.write_text(json.dumps(dict(scope='Validation of CLI-generated frozen pages; native motion checked separately',pages=rows,unchanged_prefix_pages=604,limit=606),indent=2)+'\n',encoding='utf8');print('PASS 606 BLESS pages, 604 unchanged, leftward then flash/SE/voice')
