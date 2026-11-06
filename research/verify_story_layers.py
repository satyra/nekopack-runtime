"""Independent raw-DAT/Pillow oracle for the observed static TD layer profile.
Not original-engine screenshots, motion, MNG playback, or a script VM oracle.
"""
import argparse,hashlib,io,json,re,struct,sys
from pathlib import Path
from PIL import Image

def main():
 p=argparse.ArgumentParser()
 for name in ['games','reference-root','pages','output']:p.add_argument('--'+name,type=Path,required=True)
 p.add_argument('--effects',action='store_true')
 p.add_argument('--actor-filter',action='store_true')
 p.add_argument('--rightward-motion',action='store_true')
 p.add_argument('--games-selected',type=int,nargs='+',choices=range(4),default=list(range(4)))
 a=p.parse_args();sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive,png_geometry
 manifest=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'));results=[]
 for i,g in enumerate(manifest['games']):
  if i not in a.games_selected:continue
  packs=[(s['priority'],Archive(a.games/s['file'])) for s in g['sources'] if any(n in s['file'] for n in ['image.dat','system.dat','script.dat','patch'])]
  def read(name):
   choices=[(priority,pack) for priority,pack in packs if name.lower() in pack.entries]
   best=max(priority for priority,_ in choices);selected=[pack for priority,pack in choices if priority==best]
   assert len(selected)==1
   return selected[0].read(name)
  size=list(struct.unpack_from('>II',read('system/title.mng'),16))
  macros={}
  for line in read('system/imagemap.txt').decode('cp932').splitlines():
   if '=' in line:
    key,value=line.split('=',1);macros[key.strip()]=value.split()
  for path in sorted((a.pages/str(i)).glob('[0-9]*.json'),key=lambda p:int(p.stem)):
   page=json.loads(path.read_text(encoding='utf8'))
   # These four tested spans each remain inside one story source. Replay from its start.
   lines=read('script/'+page['source']+'.txt').decode('cp932').splitlines()
   bg=None;bgcolor=None;overlay=None;visual=None;layers=[];active_filter=None
   colors={'白':[255]*4,'黒':[0,0,0,255]}
   for line in lines[:page['line']]:
    words=line.strip().split()
    if not words:continue
    command,*args=words
    if command=='.filter' and a.actor_filter:
     assert args in [[],['夜']]
     active_filter='night' if args else None
    elif command=='.img0':
     bg=args[0] if args and args[0] not in colors else None
     bgcolor=colors.get(args[0]) if args else None
     if args:overlay=None;visual=None
    elif re.fullmatch(r'\.img[1-9]',command):
     slot=int(command[-1]);old=next((l for l in layers if l['slot']==slot),None)
     if not args or args==['clear']:
      layers=[l for l in layers if l['slot']!=slot];continue
     tokens=[token for word in args for token in macros.get(word,[word])]
     names=[t for t in tokens if t not in ['front','back','keep','center','/','/big'] and not re.fullmatch(r'(left|right)[1-4]?',t) and not (a.rightward_motion and re.fullmatch(r'>+',t))]
     assert len(names)<=1
     name=names[0] if names else old['name']
     folder=('image/actor' if names else old['folder'])
     anchor=list(old['anchor']) if old else [size[0]//2,size[1]//2]
     order='keep' if a.rightward_motion and any(re.fullmatch(r'>+',t) for t in tokens) and not names else 'front'
     for t in tokens:
      if t in ['front','back','keep']:order=t
      elif t in ['/','/big']:folder='image/actor'+t.rstrip('/')
      elif a.rightward_motion and re.fullmatch(r'>+',t):anchor[0]+=len(t)*50
      elif t=='center':anchor[0]=size[0]//2
      elif re.fullmatch(r'(left|right)[1-4]?',t):anchor[0]=size[0]//2+(-1 if t.startswith('left') else 1)*160*(int(t[-1]) if t[-1].isdigit() else 1)
     raw=read(folder+'/'+name+'.png');geo=png_geometry(raw)
     layer=dict(slot=slot,name=name,folder=folder,anchor=anchor,offset=[geo['offset_x'],geo['offset_y']])
     if a.actor_filter:
      layer['filter']=active_filter if names or any(t in ['/','/big'] for t in tokens) else old['filter']
      layer['filter_pending']=False
     index=layers.index(old) if old else len(layers)
     layers=[l for l in layers if l['slot']!=slot]
     layers.insert(0 if order=='back' else index if order=='keep' else len(layers),layer)
    elif command=='.imgx':layers=[next(l for l in layers if l['slot']==int(slot)) for slot in args if any(l['slot']==int(slot) for l in layers)]
    elif command in ['.img','.page']:
     if not args:bg=None;bgcolor=None;overlay=None;visual=None;layers=[]
     else:
      assert command=='.img' and len(args)==1
      overlay=colors.get(args[0]);visual=None if overlay else args[0]
   expected=dict(size=size,background=bg,background_color=bgcolor,overlay=overlay,visual=visual,actors=layers)
   if a.actor_filter:expected['filter']=active_filter
   assert page['scene']==expected,f'layer metadata mismatch game {i} page {path.stem}'
   canvas=Image.new('RGBA',tuple(size),tuple(overlay or bgcolor or [0,0,0,255]))
   if not overlay:
    if bg:canvas.alpha_composite(Image.open(io.BytesIO(read('image/back/'+bg+'.png'))).convert('RGBA'))
    for layer in layers:
     raw=read(layer['folder']+'/'+layer['name']+'.png');position=tuple(x+y for x,y in zip(layer['anchor'],layer['offset']))
     actor=Image.open(io.BytesIO(raw)).convert('RGBA')
     if a.actor_filter and layer['filter']=='night':
      actor=actor.point([max(0,v-17) for v in range(256)]+[max(0,v-22) for v in range(256)]+[min(255,v+17) for v in range(256)]+list(range(256)))
     canvas.alpha_composite(actor,position)
   if visual and not overlay:canvas.alpha_composite(Image.open(io.BytesIO(read('image/visual/'+visual+'.png'))).convert('RGBA'))
   actual=Image.open(path.with_suffix('.png')).convert('RGBA')
   assert actual.size==canvas.size and actual.tobytes()==canvas.tobytes(),f'pixel mismatch game {i} page {path.stem}'
   record=dict(game=i,source=page['source'],line=page['line'],actor_slots=[l['slot'] for l in layers],rgba_sha256=hashlib.sha256(actual.tobytes()).hexdigest(),independent_pixels_equal=True,independent_layers_equal=True)
   if a.actor_filter:record['actor_filters']=[l['filter'] for l in layers]
   record['effect_token' if a.effects else 'page']=int(path.stem)+(0 if a.effects else 1)
   results.append(record)
  print('PASS independent layers/pixels',i,flush=True)
 a.output.write_text(json.dumps(dict(scope='independent archive decoder, source replay of static layer commands and Pillow; no original-engine parity claim',actor_filter_profile=a.actor_filter,rightward_motion_endpoint_profile=a.rightward_motion,**{'snapshots' if a.effects else 'pages':results}),indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
