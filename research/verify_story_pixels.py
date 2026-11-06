"""Independent Pillow/reference-reader check for the two-page static preview profile.
Not a comparison with original-engine screenshots or timing.
"""
import argparse,hashlib,io,json,struct,sys
from pathlib import Path
from PIL import Image

def main():
 p=argparse.ArgumentParser()
 for name in ['games','reference-root','fixtures','pages','output']:p.add_argument('--'+name,type=Path,required=True)
 a=p.parse_args();sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive,png_geometry
 manifest=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'));results=[]
 for i,g in enumerate(manifest['games']):
  packs=[(s['priority'],Archive(a.games/s['file'])) for s in g['sources'] if any(n in s['file'] for n in ['image.dat','system.dat','script.dat','patch'])]
  def read(name):
   choices=[(priority,pack) for priority,pack in packs if name.lower() in pack.entries]
   best=max(priority for priority,_ in choices);selected=[pack for priority,pack in choices if priority==best]
   assert len(selected)==1
   return selected[0].read(name)
  dimensions=struct.unpack_from('>II',read('system/title.mng'),16)
  for n in range(2):
   page=json.loads((a.pages/str(i)/f'{n}.json').read_text(encoding='utf8'))
   source=next(f for f in (a.fixtures/str(i)/'script').iterdir() if f.name.lower()==page['source'].lower()+'.txt')
   bg=actor=solid=None;size=dimensions
   for raw in source.read_bytes().split(b'\n')[:page['line']]:
    words=raw.strip().decode('utf8').split()
    if not words:continue
    if words[0]=='.img0':bg=read('image/back/'+words[1]+'.png');solid=None;size=Image.open(io.BytesIO(bg)).size
    elif words[0]=='.img1':actor=read('image/actor/'+words[1]+'.png')
    elif words[0]=='.img':solid={'白':'white','黒':'black'}[words[1]]
   canvas=Image.new('RGBA',size,solid or 'black')
   if not solid:
    if bg:canvas.alpha_composite(Image.open(io.BytesIO(bg)).convert('RGBA'))
    if actor:
     geometry=png_geometry(actor);position=(size[0]//2+geometry['offset_x'],size[1]//2+geometry['offset_y'])
     canvas.alpha_composite(Image.open(io.BytesIO(actor)).convert('RGBA'),position)
   actual=Image.open(a.pages/str(i)/f'{n}.png').convert('RGBA')
   assert actual.size==canvas.size and actual.tobytes()==canvas.tobytes(),f'pixel mismatch game {i} page {n}'
   results.append(dict(game=i,page=n+1,size=size,rgba_sha256=hashlib.sha256(actual.tobytes()).hexdigest(),independent_pixels_equal=True))
  print('PASS independent pixels',i,flush=True)
 a.output.write_text(json.dumps(dict(scope='reference archive decoder + Pillow composition; static centered-actor preview policy, not original-engine parity',pages=results),indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
