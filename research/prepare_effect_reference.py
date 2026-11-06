"""Prepare local, non-distributable effect oracles from original DATs.
The prior page PNG must already have passed verify_story_layers.py.
"""
import argparse,hashlib,json,shutil,sys
from pathlib import Path

def main():
 p=argparse.ArgumentParser()
 for name in ['games','reference-root','pages','output-dir','report']:p.add_argument('--'+name,type=Path,required=True)
 a=p.parse_args();sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
 manifest=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'));rows=[]
 for i in [1,3]:
  packs=[(s['priority'],Archive(a.games/s['file'])) for s in manifest['games'][i]['sources'] if any(n in s['file'] for n in ['image.dat','script.dat','patch'])]
  def read(name):
   found=[(priority,pack) for priority,pack in packs if name.lower() in pack.entries]
   top=max(priority for priority,_ in found);selected=[pack for priority,pack in found if priority==top];assert len(selected)==1
   return selected[0].read(name)
  if i==1:
   lines=read('script/012401.txt').decode('cp932').splitlines()
   assert lines[51].strip()=='.img EF04' and lines[52].strip()=='.wipe EF04 100'
   root=a.output_dir/'1/EF04';root.mkdir(parents=True,exist_ok=True)
   shutil.copyfile(a.pages/'1/7.png',root/'old.png')
   files=[('image/visual/EF04.png',root/'target.png'),('image/mask/EF04.png',root/'mask.png')]
  else:
   lines=read('script/P01.txt').decode('cp932').splitlines()
   assert lines[70].strip()=='.img VGVE_00d' and lines[71].strip()=='.fade'
   root=a.output_dir/'3';root.mkdir(parents=True,exist_ok=True)
   files=[('image/visual/VGVE_00d.png',root/'visual.png')]
  for logical,dest in files:
   raw=read(logical);dest.write_bytes(raw);rows.append(dict(game=i,logical=logical,sha256=hashlib.sha256(raw).hexdigest()))
 a.report.write_text(json.dumps(dict(scope='independent raw DAT effect image references; original script commands asserted; device DATs reuse previous batches',records=rows),indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
