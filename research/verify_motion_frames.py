"""Independent raw DAT/Pillow composition at every captured native actor-motion frame."""
import argparse,hashlib,io,json,math,sys
from pathlib import Path
from PIL import Image
p=argparse.ArgumentParser()
for n in ['games','reference-root','captures','device-report','output']:p.add_argument('--'+n,type=Path,required=True)
p.add_argument('--include-dip',action='store_true')
p.add_argument('--include-stationary',action='store_true')
p.add_argument('--include-bgm-interleave',action='store_true')
p.add_argument('--include-left-excursion',action='store_true')
p.add_argument('--include-queue',action='store_true')
a=p.parse_args();sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
manifest=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]
packs=[(s['priority'],Archive(a.games/s['file'])) for s in manifest['sources'] if any(x in s['file'] for x in ['image.dat','system.dat','script.dat','patch'])]
def read(name):
 choices=[(p,pack) for p,pack in packs if name.lower() in pack.entries];best=max(p for p,_ in choices);selected=[pack for p,pack in choices if p==best];assert len(selected)==1
 return selected[0].read(name)
report=json.loads(a.device_report.read_text(encoding='utf8'));assert report['execution']=='native ARM64';assert len(report['games'])==1 and report['games'][0]['index']==0
game=report['games'][0];expected_pages=[56,57,60,159,176,270,501] if a.include_queue else [56,57,60,159,176,270] if a.include_left_excursion else [56,57,60,159,176] if a.include_bgm_interleave else [56,57,60,159] if a.include_stationary else [56,57] if a.include_dip else [56];assert [m['sequence'] for m in game['motions']]==expected_pages and game['closed_vm_live_bytes']==0
assert sorted(int(d.name) for d in (a.captures/'0').iterdir() if d.is_dir())==expected_pages
rows=[];interleave=[]
for capture in sorted((a.captures/'0').iterdir()):
 page=json.loads((capture/'page.json').read_text(encoding='utf8'));sequence=page['sequence'];dip=sequence==57;excursion=sequence==270;stationary=sequence in [60,159,176];slot=2 if sequence==501 else 1 if sequence in [159,176,270] else 6
 assert hashlib.sha256(json.dumps(page,sort_keys=True,ensure_ascii=False).encode()).hexdigest()==next(p for p in game['pages'] if p['sequence']==sequence)['metadata_sha256']
 source,line,command_line,command,background={56:('012402',76,72,'.img6 SGVE00Aa >>>>>>>','BG201d'),57:('012402',84,81,'.img6 _','BG201d'),60:('012402',97,94,'.img6 SGVE201a []','BG201d'),159:('012501',31,27,'.img1 SGVE101a []','BG005a'),176:('012501',125,120,'.img1 SGVE00Aa []','BG005a'),270:('012501',585,582,'.img1 [[[[','BG005a'),501:('012603',215,211,'.img2 _ >>>>','BG005a')}[sequence]
 assert [page['source'],page['line']]==[source,line]
 lines=read('script/'+source+'.txt').decode('cp932').splitlines();assert lines[command_line-1].strip()==command;assert lines[line-1].strip()==page['text']
 wanted=dict(slot=slot,start_x=640 if sequence in [56,159,176] else 990,end_x=640 if sequence in [159,176] else 990,center_x=640)
 if dip:wanted.update(kind='dip',start_y=360,end_y=360,center_y=360,amplitude_y=50)
 if stationary:wanted.update(kind='stationary',start_y=360,end_y=360,center_y=360,amplitude_x=0)
 if excursion:wanted.update(start_x=1120,end_x=1120,kind='horizontal_excursion',start_y=360,end_y=360,center_y=360,amplitude_x=-200)
 if sequence==501:
  wanted=dict(slot=2,kind='queue',segments=[dict(slot=2,start_x=640,end_x=640,center_x=640,kind='dip',start_y=360,end_y=360,center_y=360,amplitude_y=50),dict(slot=2,start_x=640,end_x=840,center_x=640)])
 assert page['motion']==dict(duration_ms=500 if sequence==501 else 250,actors=[wanted])
 events=json.loads((capture/'events.json').read_text());frames=[e for e in events if e['event']=='frame'];assert any(0<e['elapsed_ms']<250 for e in frames)
 assert events==next(m for m in game['motions'] if m['sequence']==sequence)['events']
 host=game['host_events'];voice=[e for e in host if e['event']=='playing' and e['kind']=='voice' and e['time_ms']<=events[0]['time_ms']];assert voice
 if sequence!=159:
  token=voice[-1]['token'];audio_begins=[e for e in host if e['event']=='begin' and e['kind'] in ['bgm','se','voice']];assert len(audio_begins)==len(game['audio']);assert next(meta for begin,meta in zip(audio_begins,game['audio']) if begin['token']==token)['name']=={56:'UR_012402_0001',57:'VE_012402_0002',60:'VE_012402_0003',176:'VE_012501_0003',270:'VE_012501_0011',501:'NO_012603_0004'}[sequence]
  if sequence==176:
   assert lines[120].strip()=='.bgm BGM021' and page['effects']==['bgm BGM021']
   bgm=next(e for e in reversed(host[:host.index(voice[-1])]) if e['event']=='playing' and e['kind']=='bgm')
   assert next(meta for begin,meta in zip(audio_begins,game['audio']) if begin['token']==bgm['token'])['name']=='BGM021'
   bgm_ack=next(e for e in host if e['event']=='ack' and e['token']==bgm['token'])
   bgm_begin=next(e for e in audio_begins if e['token']==bgm['token'])
   previous_bgm=[e for e in host[:host.index(bgm_begin)] if e['event']=='playing' and e['kind']=='bgm']
   assert previous_bgm
   assert not any(e['token']==previous_bgm[-1]['token'] and e['event'] in ['progress','completed'] for e in host[host.index(bgm_begin)+1:]), 'replaced BGM produced a late callback'
   voice_begin=next(e for e in audio_begins if e['token']==token)
   voice_ack=next(e for e in host if e['event']=='ack' and e['token']==token)
   assert host.index(bgm)<host.index(bgm_ack)<host.index(voice_begin)<host.index(voice[-1])<host.index(voice_ack)
   assert bgm['token']+1==token and bgm_ack['time_ms']<=voice_begin['time_ms']<=voice_ack['time_ms']<=events[0]['time_ms']
   assert events[0]['time_ms']-bgm['time_ms']<bgm['value'], 'dialogue waited for music completion'
   interleave.append(dict(sequence=sequence,bgm_token=bgm['token'],voice_token=token,replaced_bgm_token=previous_bgm[-1]['token'],no_late_replaced_bgm_callbacks=True,bgm_started_ms=bgm['time_ms'],bgm_ack_ms=bgm_ack['time_ms'],voice_started_ms=voice[-1]['time_ms'],voice_ack_ms=voice_ack['time_ms'],motion_started_ms=events[0]['time_ms'],music_duration_ms=bgm['value'],ordered_start_acks_before_motion=True))
 else:assert page['voice']==''
 scene=page['scene'];assert scene['size']==[1280,720] and scene['background']==background and scene['visual'] is None and scene['overlay'] is None
 images={}
 for layer in scene['actors']:
  actor=Image.open(io.BytesIO(read(layer['folder']+'/'+layer['name']+'.png'))).convert('RGBA');assert layer['filter']==(None if sequence in [159,176,270,501] else 'night')
  images[layer['slot']]=actor if layer['filter'] is None else actor.point([max(0,v-17) for v in range(256)]+[max(0,v-22) for v in range(256)]+[min(255,v+17) for v in range(256)]+list(range(256)))
 segment=0;started=0
 for e in frames:
  requested=e['elapsed_ms'];t=min(requested,250);x=wanted['start_x'] if stationary else 990 if dip or t==250 else 640+int(350*(math.sin((t/250-.5)*math.pi)+1)*.5)
  if excursion:x=1120 if t in [0,250] else 640+int(480-200*math.sin(t/250*math.pi))
  y=360+(int(50*math.sin(t/250*math.pi)) if dip and 0<t<250 else 0)
  if sequence==501:
   if segment<2 and requested-started>=250:segment+=1;started=requested
   local=requested-started
   x=840 if segment==2 else 640 if segment==0 else 640+int(200*(math.sin((local/250-.5)*math.pi)+1)*.5)
   y=360+int(50*math.sin(local/250*math.pi)) if segment==0 else 360
   assert e['finished']==(segment==2)
  elif 'finished' in e:assert e['finished']==(requested>=250)
  canvas=Image.open(io.BytesIO(read('image/back/'+background+'.png'))).convert('RGBA')
  for layer in scene['actors']:
   anchor=[x,y] if layer['slot']==slot else layer['anchor'];position=tuple(v+o for v,o in zip(anchor,layer['offset']))
   canvas.alpha_composite(images[layer['slot']],position)
  actual=(capture/f'{requested}.rgba').read_bytes();assert actual==canvas.tobytes(),f'frame mismatch at {t}ms'
  rows.append(dict(sequence=page['sequence'],elapsed_ms=requested,**{f'actor{slot}_x':x,f'actor{slot}_y':y},rgba_sha256=hashlib.sha256(actual).hexdigest(),independent_pixels_equal=True))
for sequence in expected_pages:
 group=[r for r in rows if r['sequence']==sequence];assert group[0]['elapsed_ms']==0 and group[-1]['elapsed_ms']>=(500 if sequence==501 else 250)
 if sequence==501:
  assert group[0]['actor2_x']==640 and group[-1]['actor2_x']==840 and group[-1]['actor2_y']==360
  assert any(r['actor2_y']>360 for r in group) and any(640<r['actor2_x']<840 for r in group)
  assert all(r['actor2_x']==640 or r['actor2_y']==360 for r in group)
  continue
 if sequence in [60,159,176]:
  assert len(set(r['rgba_sha256'] for r in group))==1
  continue
 assert len(set(r['rgba_sha256'] for r in group))>=3
 if sequence==57:
  assert group[0]['rgba_sha256']==group[-1]['rgba_sha256']
  assert all(r['actor6_x']==990 and 360<=r['actor6_y']<=410 for r in group)
  assert any(r['actor6_y']>360 for r in group)
 elif sequence==270:
  assert group[0]['rgba_sha256']==group[-1]['rgba_sha256']
  assert all(r['actor1_y']==360 and 920<=r['actor1_x']<=1120 for r in group)
  assert any(r['actor1_x']<1120 for r in group)
 else:assert group[0]['actor6_x']==640 and group[-1]['actor6_x']==990
a.output.write_text(json.dumps(dict(scope='BLESS native ARM64; every captured motion frame compared with independent raw DAT/Pillow and pinned original curve, not original screen parity',device_report_sha256=hashlib.sha256(a.device_report.read_bytes()).hexdigest(),build=report['build'],voice_started_before_motion_pages=[n for n in expected_pages if n!=159],bgm_interleave=interleave,frames=rows),indent=2)+'\n')
print('PASS independent motion frames',len(rows))
