"""Validate raw DAT CLI output through looping SE, stop and subsequent voiced dialogue."""
import argparse,hashlib,json
from pathlib import Path
p=argparse.ArgumentParser()
for name in ['pages','baseline','generation','before','output']:p.add_argument('--'+name,type=Path,required=True)
a=p.parse_args();generation=json.loads(a.generation.read_text(encoding='utf8'));before=json.loads(a.before.read_text(encoding='utf8'));assert generation['returncode']==0 and before['returncode']==1 and '012802:42' in before['stderr']
paths=sorted((a.pages/'0').glob('[0-9]*.json'),key=lambda p:int(p.stem));assert len(paths)==793 and not (a.pages/'0'/'boundary.json').exists();rows=[]
for i,path in enumerate(paths):
 page=json.loads(path.read_text(encoding='utf8'));assert page['sequence']==i+1
 if i<785:
  for ext in ['.json','.png']:assert path.with_suffix(ext).read_bytes()==(a.baseline/'0'/path.with_suffix(ext).name).read_bytes(),(i,ext)
 if i in [782,785,788,792]:
  line,effect,voice={782:(32,'se SE_028',''),785:(44,'se SE_028 loop',''),788:(57,'se_stop',''),792:(71,None,'XXA_012802_0000')}[i]
  assert [page['source'],page['line'],page['voice']]==['012802',line,voice]
  if effect:assert effect in page['effects']
 rows.append(dict(sequence=i+1,source=page['source'],line=page['line'],png_sha256=hashlib.sha256(path.with_suffix('.png').read_bytes()).hexdigest()))
a.output.write_text(json.dumps(dict(scope='BLESS raw DAT bounded 793-page execution; old build stops at 012802:42',generation=generation,before=before,unchanged_prefix_pages=785,pages=rows),indent=2)+'\n',encoding='utf8');print('PASS BLESS 793 pages, 785 unchanged; SE once/loop/stop then voiced dialogue')
