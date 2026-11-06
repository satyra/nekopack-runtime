"""Exercise real UI clicks and compare Android story pages with desktop and source text.
Requires a prior SAF grant; never creates or grants URI permissions.
"""
import argparse,hashlib,json,re,subprocess,time,uuid,xml.etree.ElementTree as ET
from pathlib import Path

def completed_audio(events,token):
 playing=[e for e in events if e['event']=='playing' and e['token']==token]
 return len(playing)==1 and playing[0]['value']>0 and any(e['event']=='completed' and e['token']==token and e['time_ms']-playing[0]['time_ms']>=playing[0]['value'] for e in events)

def replacement_audio_token(events,token):
 playing=[e for e in events if e['event']=='playing' and e['token']==token]
 if len(playing)!=1 or playing[0]['value']<=0:return None
 if any(e['event']=='completed' and e['token']==token for e in events):return None
 start=playing[0];start_index=events.index(start)
 for index,event in enumerate(events):
  if index<=start_index:continue
  if event['event']!='begin' or event['kind']!=start['kind'] or event['token']<=token:continue
  if not 0<=event['time_ms']-start['time_ms']<start['value']:continue
  if any(e['token']==token and e['event'] in ['progress','completed'] for e in events[index+1:]):continue
  replacements=[e for e in events[index+1:] if e['event']=='playing' and e['kind']==start['kind'] and e['token']==event['token'] and e['value']>0 and e['time_ms']>=event['time_ms']]
  if len(replacements)==1:return event['token']
 return None

def verify_bgm_stop(host,begin,ack):
 assert begin['value']==0
 stopped=[e for e in host if e['event']=='stopped' and e['kind']=='bgm_stop' and e['token']==begin['token']]
 assert len(stopped)==1 and stopped[0]['value']==0
 assert host.index(begin)<host.index(stopped[0])<host.index(ack), 'BGM stop acknowledged before release'
 prior=[e for e in host[:host.index(begin)] if e['event']=='playing' and e['kind']=='bgm']
 if prior:
  assert not any(e['token']==prior[-1]['token'] and e['event'] in ['progress','completed'] for e in host[host.index(stopped[0])+1:]), 'released BGM produced late playback events'

def main():
 p=argparse.ArgumentParser()
 for name in ['adb','desktop','fixtures','output']:p.add_argument('--'+name,type=Path,required=True)
 p.add_argument('--movie-completion',choices=['natural','secondary'])
 p.add_argument('--audio-reference',type=Path)
 p.add_argument('--until-boundary',action='store_true')
 p.add_argument('--allow-page-limit',action='store_true')
 p.add_argument('--wipe-reference',type=Path)
 p.add_argument('--effect-captures',type=Path)
 p.add_argument('--motion-captures',type=Path)
 p.add_argument('--screen-captures',type=Path)
 p.add_argument('--screen-pages',type=int,nargs='+',default=[])
 p.add_argument('--games',type=int,nargs='+',choices=range(4),default=list(range(4)))
 p.add_argument('--serial',required=True);a=p.parse_args()
 if a.screen_pages and not a.screen_captures:p.error('--screen-pages requires --screen-captures')
 prefix=[str(a.adb),'-s',a.serial];package='org.nekopack.probe'
 def adb(*args):return subprocess.check_output(prefix+list(args),timeout=90)
 def private(run,name):return adb('exec-out','run-as',package,'cat',f'files/{run}/{name}')
 skipped_movies=set()
 def skip_movie(run):
  if a.movie_completion!='secondary':return
  names=adb('shell','run-as',package,'ls','-1',f'files/{run}').decode().splitlines()
  for name in names:
   if not re.fullmatch(r'movie-\d+-events.jsonl',name):continue
   key=(run,name)
   events=[json.loads(x) for x in private(run,name).splitlines()]
   if key not in skipped_movies and not any(e['event']=='closed' for e in events) and any(e['event']=='progress' and e['head']>0 for e in events):
    tap('返回剧情');skipped_movies.add(key)
 def wait_page(run,n):
  deadline=time.monotonic()+(700 if a.movie_completion else 45)
  while time.monotonic()<deadline:
   raw=private(run,f'page-{n}.json')
   if raw.startswith(b'{'):return json.loads(raw)
   skip_movie(run)
   error=private(run,'error.txt')
   if b'No such file' not in error:raise RuntimeError(error.decode())
   time.sleep(.3)
  raise RuntimeError('page timeout')
 def wait_audio(run):
  deadline=time.monotonic()+10
  while time.monotonic()<deadline:
   events=[json.loads(line) for line in private(run,'host-events.jsonl').decode().splitlines()]
   tokens={e['token'] for e in events if e['event']=='begin' and e['kind'] in ['bgm','se','voice']}
   advanced={e['token'] for e in events if e['event'] in ['progress','completed'] and e['value']>0}
   if all(token in advanced or completed_audio(events,token) or replacement_audio_token(events,token) is not None for token in tokens):return
   time.sleep(.2)
  raise RuntimeError('audio did not advance within the observation window')
 def tap(text):
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   adb('shell','uiautomator','dump','/sdcard/neko-story-ui.xml')
   xml=adb('shell','cat','/sdcard/neko-story-ui.xml')
   for node in ET.fromstring(xml).iter('node'):
    if node.get('text')==text and node.get('enabled')=='true':
     x,y,r,b=map(int,re.findall(r'\d+',node.get('bounds')));adb('shell','input','tap',str((x+r)//2),str((y+b)//2));return
   time.sleep(.3)
  raise RuntimeError('enabled control not found: '+text)
 build=json.loads(Path('.local/android-probe/build.json').read_text())
 apkpath=adb('shell','pm','path',package).decode().strip().removeprefix('package:')
 assert adb('shell','sha256sum',apkpath).decode().split()[0]==build['sha256']['nekopack-arm64-probe.apk']
 games=[]
 for i in a.games:
  run='story_'+uuid.uuid4().hex
  adb('shell','am','start','-W','-S','-n',package+'/.MainActivity','--ei','story_game',str(i),'--es','run_id',run)
  pages=[];motions=[]
  to_boundary=a.until_boundary and (a.desktop/str(i)/'boundary.json').exists()
  if a.until_boundary and not to_boundary:assert a.allow_page_limit, 'missing expected boundary'
  count=len(list((a.desktop/str(i)).glob('[0-9]*.json'))) if a.until_boundary else 2
  assert count>0
  last_page_audio=set()
  for n in range(1,count+1):
   actual=wait_page(run,n)
   expected=json.loads((a.desktop/str(i)/f'{n-1}.json').read_text(encoding='utf8'))
   if actual!=expected:raise RuntimeError('Android/desktop page metadata mismatch')
   # The file must be stable while the user has not acknowledged this page.
   assert private(run,f'page-{n}.json')==private(run,f'page-{n}.json')
   png=private(run,f'page-{n}.png');assert png==(a.desktop/str(i)/f'{n-1}.png').read_bytes()
   source=next(f for f in (a.fixtures/str(i)/'script').iterdir() if f.name.lower()==(actual['source']+'.txt').lower())
   lines=source.read_bytes().split(b'\n');assert lines[actual['line']-1].strip().decode('utf8')==actual['text']
   pages.append(dict(sequence=n,source=actual['source'],line=actual['line'],text_sha256=hashlib.sha256(actual['text'].encode()).hexdigest(),png_sha256=hashlib.sha256(png).hexdigest(),metadata_sha256=hashlib.sha256(json.dumps(actual,sort_keys=True,ensure_ascii=False).encode()).hexdigest(),effects=actual['effects']))
   if 'motion' in actual:
    assert a.motion_captures, 'motion pages require frame capture and verification'
    deadline=time.monotonic()+20
    while time.monotonic()<deadline:
     raw=private(run,'motion-events.jsonl')
     events=[json.loads(line) for line in raw.splitlines()] if raw.startswith(b'{') else []
     selected=[e for e in events if e['sequence']==n]
     if any(e['event']=='finished' for e in selected):break
     time.sleep(.1)
    else:raise RuntimeError('dialogue motion never completed')
    assert selected[0]['event']=='begin' and selected[-1]['event']=='finished'
    assert not any(e['event']=='cancelled' for e in selected)
    frames=[e for e in selected if e['event']=='frame'];duration=actual['motion']['duration_ms']
    assert frames[0]['elapsed_ms']==0 and frames[-1]['elapsed_ms']==duration
    assert any(0<e['elapsed_ms']<duration for e in frames)
    assert [e['elapsed_ms'] for e in frames]==sorted(set(e['elapsed_ms'] for e in frames))
    assert selected[-1]['time_ms']-selected[0]['time_ms']>=duration
    folder=a.motion_captures/str(i)/str(n);folder.mkdir(parents=True,exist_ok=True)
    (folder/'page.json').write_text(json.dumps(actual,ensure_ascii=False),encoding='utf8')
    (folder/'events.json').write_text(json.dumps(selected),encoding='utf8')
    for event in frames:
     elapsed=event['elapsed_ms'];rgba=private(run,f'motion-{n}-{elapsed}.rgba')
     assert len(rgba)==actual['scene']['size'][0]*actual['scene']['size'][1]*4
     (folder/f'{elapsed}.rgba').write_bytes(rgba)
    motions.append(dict(sequence=n,events=selected,captured_frames=len(frames)))
   if n in a.screen_pages:
    import io
    from PIL import Image
    deadline=time.monotonic()+30
    wanted=(actual['speaker']+'\n'+actual['text']).strip()
    while time.monotonic()<deadline:
     adb('shell','uiautomator','dump','/sdcard/neko-story-screen.xml')
     xml=adb('shell','cat','/sdcard/neko-story-screen.xml')
     if any(node.get('text','').strip()==wanted for node in ET.fromstring(xml).iter('node')):break
     time.sleep(.2)
    else:raise RuntimeError('story dialogue not visible on screen')
    capture=adb('exec-out','screencap','-p');screen=Image.open(io.BytesIO(capture));assert screen.width>screen.height
    folder=a.screen_captures/str(i);folder.mkdir(parents=True,exist_ok=True);(folder/f'{n}.png').write_bytes(capture)
    pages[-1]['screen']=dict(dialogue_visible=True,landscape=True,sha256=hashlib.sha256(capture).hexdigest())
   if a.audio_reference:wait_audio(run)
   if a.audio_reference:last_page_audio={e['token'] for e in [json.loads(line) for line in private(run,'host-events.jsonl').decode().splitlines()] if e['event']=='begin' and e['kind'] in ['bgm','se','voice']}
   if n<count or to_boundary:tap('继续')
  boundary=None
  if to_boundary:
   deadline=time.monotonic()+(700 if a.movie_completion else 45)
   while time.monotonic()<deadline:
    skip_movie(run)
    raw=private(run,'boundary.json')
    if raw.startswith(b'{'):break
    time.sleep(.3)
   else:raise RuntimeError('unsupported boundary timeout')
   boundary=json.loads(raw)
   assert boundary['checkpoint']==json.loads((a.desktop/str(i)/'boundary.json').read_text(encoding='utf8'))
   assert boundary['error']==(a.desktop/str(i)/'boundary.txt').read_text(encoding='utf8')
   assert boundary['vm_live_bytes']==0
   assert b'No such file' in private(run,f'page-{count+1}.json')
  movies=[]
  if a.movie_completion:
   names=adb('shell','run-as',package,'ls','-1',f'files/{run}').decode().splitlines()
   for name in names:
    if not re.fullmatch(r'movie-\d+-info.json',name):continue
    token=int(name.split('-')[1]);meta=json.loads(private(run,f'effect-{token}.json'))
    info=json.loads(private(run,name));events=[json.loads(x) for x in private(run,f'movie-{token}-events.jsonl').splitlines()]
    end=events[-1];assert end['event']=='closed' and end['error'] is None and end['frames']>0 and end['head']>0
    assert end['reason']==(0 if a.movie_completion=='natural' else 2)
    assert meta['host_effect']=='movie' and meta['name']=='op.mpg' and not meta['movie_left_skip']
    upload=next(r for r in json.loads(Path('research/movie-batch14-upload.json').read_text()) if r['serial']==a.serial)
    assert info['source_sha256']==upload['files'][i]['sha256']
    if a.movie_completion=='natural':assert end['samples']/info['sample_rate']>=info['duration']-.1 and end['head']>=end['samples']
    else:assert end['samples']/info['sample_rate']<info['duration']
    movies.append(dict(token=token,metadata=meta,info=info,events=events))
   wanted=sum(e.startswith('movie ') for page in pages+([boundary['checkpoint']] if boundary else []) for e in page['effects'])
   assert len(movies)==wanted
  tap('结束运行')
  deadline=time.monotonic()+15
  while time.monotonic()<deadline:
   if private(run,'closed.txt')==b'closed; VM live bytes=0':break
   time.sleep(.2)
  else:raise RuntimeError('session did not close cleanly')
  if not to_boundary and b'No such file' not in private(run,'error.txt'):raise RuntimeError('host reported a story error')
  host=[];audio=[]
  if a.audio_reference:
   host=[json.loads(line) for line in private(run,'host-events.jsonl').decode().splitlines()]
   movie_stops=[e for e in host if e['event']=='stopped_all' and e['kind']=='movie']
   if a.movie_completion:
    assert len(movie_stops)==len(movies)
    for movie in movies:
     stop=next(e for e in movie_stops if e['token']==movie['token']);started=next(e for e in movie['events'] if e['event']=='started')
     assert stop['value']==0 and stop['time_ms']<=started['time_ms']
   stop_count=sum(effect=='bgm_stop' for page in pages+([boundary['checkpoint']] if boundary else []) for effect in page['effects'])
   assert sum(e['event']=='begin' and e['kind']=='bgm_stop' for e in host)==stop_count, 'missing or duplicated BGM stop request'
   begins=[e for e in host if e['event']=='begin'];acks=[e for e in host if e['event']=='ack']
   if [e['token'] for e in begins]!=[e['token'] for e in acks]:raise RuntimeError('missing/duplicate host completion')
   reference=json.loads(a.audio_reference.read_text(encoding='utf8'))['games'][i]
   for begin,ack in zip(begins,acks):
    if begin['kind'] in ['fade','wait','wipe','shake','flash']:
     if ack['value']<begin['value']:raise RuntimeError('premature timed completion')
     if begin['kind']=='fade' and not any(e['event']=='frame' and e['token']==begin['token'] and 0<e['value']<1000 for e in host):raise RuntimeError('fade had no intermediate frame')
    elif begin['kind']=='bgm_fade':
     finished=[e for e in host if e['event']=='finished' and e['token']==begin['token']]
     cancelled=[e for e in host if e['event']=='cancelled' and e['token']==begin['token']]
     assert len(finished)+len(cancelled)==1
     if finished:assert finished[0]['value']>=begin['value']
     assert ack['value']<begin['value'], 'BGM fade blocked the script'
     assert any(e['event']=='begin' and e['token']>begin['token'] and e['time_ms']< (finished or cancelled)[0]['time_ms'] for e in host), 'no concurrent script progress'
    elif begin['kind']=='bgm_stop':
     verify_bgm_stop(host,begin,ack)
    elif begin['kind'] in ['bgm','se','voice']:
     token=begin['token'];playing=[e for e in host if e['event']=='playing' and e['token']==token]
     progress=[e for e in host if e['event'] in ['progress','completed'] and e['token']==token and e['value']>0]
     completed=completed_audio(host,token)
     replacement=replacement_audio_token(host,token)
     if len(playing)!=1 or (not progress and not completed and replacement is None and token in last_page_audio):raise RuntimeError('audio never started, advanced, completed or demonstrably replaced')
     audio_meta=json.loads(private(run,f'audio-{token}.json'))
     key=audio_meta['kind']+'/'+audio_meta['name'].lower()
     if audio_meta['sha256']!=reference[key]:raise RuntimeError('raw DAT audio mismatch')
     audio_meta['progress_observed']=bool(progress)
     audio_meta['completed_after_duration']=completed
     audio_meta['replaced_by_token']=replacement
     audio_meta['ended_in_terminal_span']=not progress and not completed and replacement is None and token not in last_page_audio
     audio.append(audio_meta)
    else:raise RuntimeError('unrecognized host effect in verification')
    if begin['kind']=='bgm_fade' and not cancelled and not any(e['event']=='stopped' and e['kind']=='bgm' and e['token']==begin['token'] for e in host):raise RuntimeError('BGM fade did not release player')
   if set(reference)!=set(e['kind']+'/'+e['name'].lower() for e in audio):raise RuntimeError('missing expected audio resource')
   if not host or host[-1]['event']!='released' or host[-1]['value']!=0:raise RuntimeError('audio not released')
   if b'.ogg' in adb('shell','run-as',package,'ls',f'files/{run}'):raise RuntimeError('audio cache was not removed')
  layout=json.loads(private(run,'layout.json'));assert layout['width']>layout['height'] and layout['orientation']==2 and layout['stage_height']>0
  if a.effect_captures:
   folder=a.effect_captures/str(i);folder.mkdir(parents=True,exist_ok=True)
   for begin in [e for e in host if e['event']=='begin' and e['kind'] in ['fade','wipe','shake','flash']]:
    token=begin['token'];meta=json.loads(private(run,f'effect-{token}.json'))
    (folder/f'{token}.json').write_text(json.dumps(meta['checkpoint']),encoding='utf8')
    (folder/f'{token}.png').write_bytes(private(run,f'effect-{token}.png'))
  wipes=[];shakes=[]
  for begin in [e for e in host if e['event']=='begin' and e['kind'] in ['wipe','shake']]:
   import io,math,numpy as np
   from PIL import Image
   kind=begin['kind'];token=begin['token']
   meta=json.loads(private(run,f"effect-{token}.json"))
   if a.effect_captures:
    folder=a.effect_captures/str(i);folder.mkdir(parents=True,exist_ok=True)
    (folder/f'{token}.json').write_text(json.dumps(meta['checkpoint']),encoding='utf8')
    (folder/f'{token}.png').write_bytes(private(run,f'effect-{token}.png'))
    old=np.array(Image.open(io.BytesIO(private(run,f'{kind}-{token}-old.png'))).convert('RGBA')).astype('int32')
    target=np.array(Image.open(io.BytesIO(private(run,f'effect-{token}.png'))).convert('RGBA')).astype('int32')
   else:
    assert kind=='wipe' and a.wipe_reference is not None
    folder=a.wipe_reference/str(i)/meta['name']
    old=np.array(Image.open(folder/'old.png').convert('RGBA')).astype('int32')
    target=np.array(Image.open(folder/'target.png').convert('RGBA')).astype('int32')
   if kind=='wipe':
    mask=np.array(Image.open(a.wipe_reference/str(i)/meta['name']/'mask.png').convert('RGBA')).astype('int32')
   else:
    ax=ay=8;span=250;once=False
    for option in meta['name'].split():
     if option=='once':once=True
     elif option=='short':span//=2
     elif option=='long':span*=2
     elif option=='honly':ay=0
     elif option=='vonly':ax=0
     elif option=='big':ax*=2;ay*=2
     elif option=='small':ax//=2;ay//=2
     else:raise RuntimeError('unknown shake option')
    if once:span=50
    assert span==begin['value']
   frames=[e for e in host if e['event']=='frame' and e['kind']==kind and e['token']==token]
   assert any(0<e['value']<begin['value'] for e in frames), kind+' never displayed an intermediate frame'
   assert frames[-1]['value']==begin['value']
   hashes=[];expected=old.copy();moved=False
   for frame in frames:
    elapsed=frame['value']
    if kind=='wipe':
     q=min(256,elapsed*256//begin['value'])
     # Metadata currently names the mask; softness is recorded by the source effect.
     soft=meta.get('checkpoint',{}).get('effects',[''])[-1].endswith(' fade')
     alpha=np.clip(mask+2*q-255,0,255)
     expected=old if q==0 else target if q>=255 else old+((target-old)*alpha)//256 if soft else np.where(mask+q>=255,target,old)
    elif elapsed<span:
     dx=int(math.sin(elapsed/7)*ax);dy=int(math.sin(elapsed/8)*ay);moved|=bool(dx or dy)
     h,w=expected.shape[:2];left=max(0,dx);right=min(w,w+dx);top=max(0,dy);bottom=min(h,h+dy)
     if left<right and top<bottom:expected[top:bottom,left:right]=target[top-dy:bottom-dy,left-dx:right-dx]
    raw=private(run,f"{kind}-{token}-{elapsed}.rgba");actual=np.frombuffer(raw,dtype=np.uint8).reshape(old.shape)
    assert np.array_equal(actual,expected), kind+' kernel/reference mismatch'
    hashes.append(dict(elapsed_ms=elapsed,rgba_sha256=hashlib.sha256(actual.tobytes()).hexdigest()))
   if kind=='shake':assert moved, 'shake never produced a nonzero displacement'
   (wipes if kind=='wipe' else shakes).append(dict(token=token,name=meta['name'],frames=hashes,independent_kernel_pixels_equal=True))
  flashes=[]
  for begin in [e for e in host if e['event']=='begin' and e['kind']=='flash']:
   import io,numpy as np
   from PIL import Image
   token=begin['token'];meta=json.loads(private(run,f'effect-{token}.json'))
   color,count=meta['name'].split();count=int(count);assert begin['value']==count*60
   cp=meta['checkpoint'];source=next(f for f in (a.fixtures/str(i)/'script').iterdir() if f.name.lower()==(cp['source']+'.txt').lower())
   command=source.read_bytes().decode('utf8').split('\n')[cp['line']-1].strip().split();assert command[0]=='.flash'
   reference_color='ffffff';reference_count=1
   palette={'white':'ffffff','白':'ffffff','赤':'ff0000','青':'0000ff','黄':'ffff00','緑':'00ff00','紫':'ff00ff','水':'00ffff'}
   for word in command[1:]:
    if word.isascii() and word.isdigit():reference_count=int(word)
    else:reference_color=palette[word]
   assert (color,count)==(reference_color,reference_count), 'flash parameters differ from source command'

   target=np.array(Image.open(io.BytesIO(private(run,f'effect-{token}.png'))).convert('RGBA'))
   solid=np.empty_like(target);solid[:]=[int(color[j:j+2],16) for j in [0,2,4]]+[255]
   frames=[e for e in host if e['event']=='frame' and e['token']==token]
   assert [e['value'] for e in frames]==list(range(count*2)), 'flash phase skipped or duplicated'
   previous=begin['time_ms'];hashes=[]
   for frame in frames:
    assert frame['time_ms']-previous>=30, 'flash phase wait shortened'
    phase=frame['value'];actual=np.frombuffer(private(run,f'flash-{token}-{phase}.rgba'),dtype=np.uint8).reshape(target.shape)
    assert np.array_equal(actual,solid if phase%2==0 else target), 'flash color/restoration mismatch'
    hashes.append(dict(phase=phase,rgba_sha256=hashlib.sha256(actual.tobytes()).hexdigest()));previous=frame['time_ms']
   flashes.append(dict(token=token,color=color,count=count,phases=hashes,independent_phase_pixels_equal=True))
  visual=False
  if boundary and boundary['checkpoint']['scene'].get('visual') and not a.effect_captures:
   from PIL import Image
   import io
   assert a.wipe_reference is not None
   last=max(e['token'] for e in host if e['event']=='begin' and e['kind']=='fade')
   actual=Image.open(io.BytesIO(private(run,f'effect-{last}.png'))).convert('RGBA')
   expected=Image.open(a.wipe_reference/str(i)/'visual.png').convert('RGBA')
   assert actual.size==expected.size and actual.tobytes()==expected.tobytes()
   visual=True
  games.append(dict(index=i,run_id=run,movies=movies,layout=layout,motions=motions,wipes=wipes,shakes=shakes,flashes=flashes,final_visual_independent_pixels_equal=visual,boundary=boundary,page_limit_reached=bool(a.until_boundary and not to_boundary),pages=pages,actual_ui_next=True,closed_vm_live_bytes=0,host_events=host,audio=audio))
  print('PASS game',i,flush=True)
 device={k:adb('shell','getprop',k).decode().strip() for k in ['ro.product.cpu.abilist','ro.dalvik.vm.native.bridge','ro.build.version.sdk']}
 device['kernel_machine']=adb('shell','uname','-m').decode().strip()
 result=dict(scope=('bounded continuous pages; unsupported boundaries or explicit reference page limit' if a.until_boundary else 'two text pages per game; host-driven audio and timed fade') if a.audio_reference else 'host-driven story pages; audio resource and timing verification omitted',execution='translated ARM64' if device['kernel_machine']=='x86_64' else ('native ARM64' if device['kernel_machine']=='aarch64' and device['ro.dalvik.vm.native.bridge'] in ('','0') else 'unclassified; inspect properties'),device=device,build=build,games=games)
 a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
if __name__=='__main__':main()
