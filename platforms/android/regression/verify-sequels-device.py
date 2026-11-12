import base64,hashlib,json,re,struct,sys,time,uuid,xml.etree.ElementTree as ET
from pathlib import Path
from urllib.parse import quote
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_menu_sounds import Probe, ACTIVITY, PREFERENCES, PACKAGE
ADB='D:/codex/tools/android/sdk/platform-tools/adb.exe'
SERIAL='emulator-5572'
TITLE='\u6e38\u620f\u6807\u9898\u83dc\u5355'
SAVE='\u539f\u7248\u5b58\u8bfb\u6863\u9875'
STORY_MENU='\u539f\u7248\u5267\u60c5\u83dc\u5355'
ROOMS={'cg':'\u539f\u7248 CG \u9274\u8d4f','scene':'\u539f\u7248\u56de\u60f3\u9274\u8d4f','actor':'\u539f\u7248\u7acb\u7ed8\u9274\u8d4f'}
def setup(game):
 base=Probe(ADB,SERIAL,'unused',{})
 prefs=ET.fromstring(base.private(PREFERENCES))
 tree=prefs.find("./string[@name='dat_tree']").text
 directory=f'raw:/storage/emulated/0/Download/NekoPack-BLESS/{game}'
 return tree,directory,f'{tree}/document/{quote(directory,safe="")}'
def launch(game,index,profile,tag):
 tree,directory,_=setup(game)
 p=Probe(ADB,SERIAL,profile+'-'+tag,{})
 p.adb('shell','am','start','-W','-S','-n',ACTIVITY,'--es','run_id',p.run,
       '--ei','title_game',str(index),'--es','library_tree',tree,'--es','library_directory',directory,
       '--es','save_probe',profile,'--es','read_history_probe',profile,'--ez','lifecycle_probe','true')
 t=p.wait('title presented',lambda:p.obj('title-presented.json'),timeout=35)
 return p,t
def press(p,label,meta,button):
 assert button['enabled'],(label,button)
 l,t,r,b=p.bounds(label)
 w,h=meta.get('width',1280),meta.get('height',720)
 x,y,bw,bh=button['bounds']
 scale=min((r-l)/w,(b-t)/h)
 p.tap(round((l+r)/2+(x+bw/2-w/2)*scale),round((t+b)/2+(y+bh/2-h/2)*scale))
def btn(meta,id):
 return next(b for b in meta['buttons'] if b['id']==id)
def room(p,name,visual=False):
 return p.wait(f'{name} room',lambda: (m if (m:=p.obj('room-presented.json')) and m.get('room')==name and m.get('visual')==visual else None))
def slot(p,id,state):
 m=p.wait('save UI',lambda:(s if (s:=p.obj('save-ui-presented.json')) and s.get('accepting') else None))
 old=len(p.events('save-events.jsonl'))
 press(p,SAVE,m,btn(m,id))
 return p.packets('save-events.jsonl',old,lambda e:e['event']==state and e['slot']==id)
def safe_wait(p,stage,fn,timeout=35):
 return p.wait(stage,fn,timeout=timeout)
def page(p,n):
 return safe_wait(p,f'page {n}',lambda:p.obj(f'page-{n}.json'))
def lifecycle(p):
 lifecycle=len(p.events('lifecycle-events.jsonl'))
 audio=len(p.events('audio-lifecycle.jsonl'))
 p.key('HOME')
 paused=p.packets('lifecycle-events.jsonl',lifecycle,lambda e:e['event']=='paused')
 stable=[p.packets('audio-lifecycle.jsonl',audio,lambda e:e['event']==f'paused_stable_{i}') for i in [1,2]]
 for kind,first in stable[0]['channels'].items():
  assert not first['playing'],kind
  second=stable[1]['channels'].get(kind)
  if second:assert first['position']==second['position'],kind
 p.adb('shell','am','start','-W','--activity-reorder-to-front','-n',ACTIVITY)
 resumed=p.packets('lifecycle-events.jsonl',lifecycle,lambda e:e['event']=='resumed')
 assert paused['instance']==resumed['instance']
 assert abs(resumed['active_ms']-paused['active_ms'])<120,(paused,resumed)
 bgm=stable[1]['channels']['bgm']
 resumed_audio=p.packets('audio-lifecycle.jsonl',audio,lambda e:e['event']=='resumed')
 after=resumed_audio['channels']['bgm']
 assert after['playing'] and after['token']==bgm['token'] and after['player_id']==bgm['player_id']
 return len(stable[0]['channels'])
def playback(p,name,offset):
 evt=p.packets('ui-audio-events.jsonl',offset,lambda e:e['name']==name)
 p.packets('host-events.jsonl',0,lambda e:e['token']==evt['token'] and e['event']=='playing' and e['kind']=='system')
 return evt
def slots_binary(data):
 assert data[:4]==b'NKSV'
 pos=29
 def number():
  nonlocal pos
  v=struct.unpack_from('<Q',data,pos)[0];pos+=8;return v
 number();number();number()
 for _ in range(number()):
  length=number();pos+=length
  number();number()
  branches=number();pos+=branches*8
 records={}
 for _ in range(number()):
  bank,index=number(),number()
  records[(bank,index)]=pos
  number()
 return records
def write_fixture(original, keys):
 data=bytearray(original)
 records=slots_binary(data)
 for key in keys:
  assert key in records,key
  struct.pack_into('<Q',data,records[key],1)
 digest=0xcbf29ce484222325
 for byte in data[13:]:
  digest=((digest^byte)*0x100000001b3)&0xffffffffffffffff
 struct.pack_into('<Q',data,5,digest)
 return data
def persisted(p,game,index,profile):
 _,_,location=setup(game)
 doc=ET.fromstring(p.private(PREFERENCES))
 key=f'runtime_unlocks_v1_{location}_{index}_probe_{profile}'
 node=doc.find(f"./string[@name='{key}']")
 if node is None:return set()
 buf=base64.b64decode(node.text)
 assert buf[:5]==b'NKUL\x01'
 return {(x>>32,x&0xffffffff) for x, in struct.iter_unpack('<Q',buf[5:])}
def log(game,msg):print(game,msg,flush=True)
def run(game,index):
 profile='seq-'+game.lower()+'-'+uuid.uuid4().hex[:7]
 tree,_,_=setup(game)
 scope=hashlib.sha256(f'{tree}_{index}'.encode()).hexdigest()+'_probe_'+profile
 folder='files/saves/'+scope
 p,title=launch(game,index,profile,'start')
 log(game,'title opened')
 press(p,TITLE,title,btn(title,1))
 first=page(p,1)
 log(game,f"story ready at {first['source']}:{first['line']}")
 if game=='BIRTH':
  x,y,r,b=p.bounds('\u7ee7\u7eed','text')
  for n in range(1,6):
   p.wait(f'dialogue ready {n}',lambda:(t if (t:=p.obj(f'text-{n}.json')) and t.get('complete') else None))
   p.tap((x+r)//2,(y+b)//2)
   first=page(p,n+1)
 channels=lifecycle(p)
 log(game,f'background/resume same instance, paused channels={channels}')
 p.control('\u5267\u60c5\u83dc\u5355')
 m=p.wait('story menu',lambda:(s if (s:=p.obj('story-menu-presented.json')) and s.get('kind')=='story_menu' else None))
 press(p,STORY_MENU,m,next(b for b in m['buttons'] if b.get('action')=='SAVE'))
 slot(p,1,'saved')
 original=p.private(folder+'/slot-001.nksv')
 assert original[:4]==b'NKSV'
 log(game,'saved from original save UI')
 keys={(71,904)}
 if game in ('BIRTH','BRAVE'):keys|={(70,300),(71,200)}
 dst=Path(f'.local/android-probe/input-test/{profile}.nksv')
 remote=f'/data/local/tmp/{profile}.nksv'
 dst.write_bytes(write_fixture(original,keys))
 p.adb('push',str(dst),remote)
 p.adb('shell','run-as',PACKAGE,'cp',remote,folder+'/slot-002.nksv')
 p.adb('shell','rm',remote)
 p,title=launch(game,index,profile,'fixture')
 press(p,TITLE,title,btn(title,2))
 slot(p,2,'loaded')
 restored=page(p,1)
 assert (restored['source'],restored['line'])==(first['source'],first['line'])
 assert all([bank,key,1] in restored['slots'] for bank,key in keys)
 assert keys<=persisted(p,game,index,profile)
 log(game,'read controlled unlock fixture, writes real Android progress')
 p,title=launch(game,index,profile,'restart')
 assert all(btn(title,i)['enabled'] for i in [5,6,7])
 press(p,TITLE,title,btn(title,5))
 cg=room(p,'cg')
 playback(p,'se_PAGE',0)
 if game in ('BIRTH','BRAVE'):
  first_entry=next(b for b in cg['buttons'] if b['id']==1 and sum(peer['group']==b['group'] for peer in cg['buttons'])>5)
  assert first_entry['enabled']
 groups={g:[b for b in cg['buttons'] if b['group']==g] for g in {b['group'] for b in cg['buttons']}}
 radio=next(items for items in groups.values() if {b['id'] for b in items} in [{1,2,3,4},{1,2,3,4,5}])
 x,y=p.point(ROOMS['cg'],next(b['bounds'] for b in radio if b['id']==2))
 off=len(p.events('ui-audio-events.jsonl'))
 import subprocess
 hold=subprocess.Popen([ADB,'-s',SERIAL,'shell','input','swipe',str(x),str(y),str(x),str(y),'3500'])
 playback(p,'se_CLICK' if game=='BLACK' else 'se_SELECT',off)
 assert hold.wait(timeout=12)==0
 if game=='BLACK':
  p.wait('BLACK two fallback cues',lambda:len([e for e in p.events('ui-audio-events.jsonl')[off:] if e['name']=='se_CLICK'])>=2,timeout=20)
 else:playback(p,'se_OK',off)
 cg=room(p,'cg')
 off=len(p.events('ui-audio-events.jsonl'))
 press(p,ROOMS['cg'],cg,btn(cg,1003))
 scene=room(p,'scene')
 playback(p,'se_PAGE',off)
 if game in ('BIRTH','BRAVE'):assert btn(scene,1)['enabled']
 p.key('BACK')
 p.wait('room closed',lambda:(m if (m:=p.obj('room-presented.json')) and m['kind']=='room_closed' else None))
 press(p,TITLE,title,btn(title,7))
 actor=room(p,'actor')
 off=len(p.events('ui-audio-events.jsonl'))
 adjustment=next(b for b in actor['buttons'] if b['id'] in [10001,10005] and b['enabled'])
 press(p,ROOMS['actor'],actor,adjustment)
 playback(p,'se_CLICK',off)
 p.key('BACK')
 p.wait('actor closed',lambda:(m if (m:=p.obj('room-presented.json')) and m['kind']=='room_closed' else None))
 press(p,TITLE,title,btn(title,2))
 slot(p,1,'loaded')
 page1=page(p,1)
 assert (page1['source'],page1['line'])==(first['source'],first['line'])
 assert all([bank,key,1] in page1['slots'] for bank,key in keys)
 assert keys<=persisted(p,game,index,profile)
 log(game,'PASS restart unlock visibility, original/fallback audio starts, older save keeps unlocks')
 dst.unlink()
if __name__=='__main__':
 for game in sys.argv[1:]:
  run(game,{'BIRTH':1,'BRAVE':2,'BLACK':3}[game])
