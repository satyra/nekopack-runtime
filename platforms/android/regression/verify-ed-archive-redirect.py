
import sys,uuid,shlex,xml.etree.ElementTree as ET,runpy
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_menu_sounds import Probe,PACKAGE,PREFERENCES,SETTINGS
x=runpy.run_path(str(Path(__file__).with_name('verify-sequels-device.py')))
adb=x['ADB'];serial='emulator-5572';tag='redirect-ed-'+uuid.uuid4().hex[:8]
p=Probe(adb,serial,tag,{})
tree=ET.fromstring(p.private(PREFERENCES)).find("./string[@name='dat_tree']").text
directory='raw:/storage/emulated/0/Download/NekoPack-BLESS/Endless Dungeon'
remote='/sdcard/Download/NekoPack-BLESS/Endless Dungeon'
local=Path('D:/codex/td-port-workspace/游戏/Endless Dungeon/Endless Dungeon')
def boot(suffix,original=False):
 global p
 p=Probe(adb,serial,tag+'-'+suffix,{})
 t=p.start_title('--ei','title_game','0','--es','library_tree',tree,'--es','library_directory',shlex.quote(directory),'--es','save_probe',tag,'--es','read_history_probe',tag,'--ez','legacy_probe_controls','true','--ez','library_original',str(original).lower())
 assert t['chinese_scenario']==(not original)
 return t
def press_title(t,id):
 x['press'](p,x['TITLE'],t,x['btn'](t,id))
def slot_confirmed(id,state,confirm):
 m=p.wait('save UI',lambda:(v if (v:=p.obj('save-ui-presented.json')) and v.get('accepting') else None))
 old=len(p.events('save-events.jsonl'))
 x['press'](p,x['SAVE'],m,x['btn'](m,id))
 if confirm:p.control('\u786e\u5b9a')
 return p.packets('save-events.jsonl',old,lambda e:e['event']==state and e['slot']==id)
added=[]
try:
 for name in ('script_chs.dat','system_chs.dat'):
  assert name not in p.adb('shell','ls',shlex.quote(remote)).decode().splitlines()
  p.adb('push',str(local/name),remote+'/'+name);added.append(name)
 t=boot('translated')
 press_title(t,3)
 m=p.wait('translated settings',p.settings)
 assert not m['unsupported']
 config=m['config']
 p.key('BACK')
 t=p.wait('back title',lambda:p.obj('title-presented.json'))
 press_title(t,1)
 first=p.wait('first page',lambda:p.obj('page-1.json'))
 p.wait('text complete',lambda:(v if (v:=p.obj('text-1.json')) and v.get('complete') else None))
 p.key('BACK')
 p.control('\u5b58\u6863')
 saved=slot_confirmed(1,'saved',bool(config.get('Dialog',0)))
 t=boot('reload')
 press_title(t,2)
 loaded=slot_confirmed(1,'loaded',bool(config.get('Dialog',0)))
 after=p.wait('loaded page',lambda:p.obj('page-1.json'))
 assert (first['source'],first['line'])==(after['source'],after['line'])
 assert saved['directory']==loaded['directory'] and '_zh' in saved['directory']
 boot('original',True)
 print('PASS ED whole-archive Chinese boot, original settings, save/restart/load, original mode',flush=True)
finally:
 p.adb('shell','am','force-stop',PACKAGE)
 for name in added:p.adb('shell','rm',shlex.quote(remote+'/'+name))
