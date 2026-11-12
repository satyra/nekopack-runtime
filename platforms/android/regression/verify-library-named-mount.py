
import sys,uuid,shlex,xml.etree.ElementTree as E,json
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_menu_sounds import Probe,PACKAGE
p=Probe('D:/codex/tools/android/sdk/platform-tools/adb.exe','emulator-5572','library-named-'+uuid.uuid4().hex[:8],{})
remote='/sdcard/Download/NekoPack-BLESS/Endless Dungeon'
local=Path('D:/codex/td-port-workspace/游戏/Endless Dungeon/Endless Dungeon')
def state():
 root=E.fromstring(p.private('shared_prefs/LibraryActivity.xml'))
 return next(g for g in json.loads(root.find("./string[@name='games']").text) if g['name']=='Endless Dungeon')
def detail():
 p.adb('shell','am','start','-W','-S','-n',PACKAGE+'/.LibraryActivity')
 l,t,r,b=p.bounds('搜索游戏')
 p.tap((l+r)//2,(t+b)//2);p.adb('shell','input','text','Endless');p.key('BACK')
 l,t,r,b=p.bounds('游戏详情 Endless Dungeon')
 p.tap((l+r)//2,(t+b)//2)
added=[]
try:
 for name in ('script_chs.dat','system_chs.dat'):
  assert name not in p.adb('shell','ls',shlex.quote(remote)).decode().splitlines()
  p.adb('push',str(local/name),remote+'/'+name);added.append(name)
 detail()
 record=p.wait('library recognizes translated archives',lambda:(g if (g:=state()).get('chinese') else None),30)
 assert set(record['patches'])==set(added),record['patches']
 print('PASS library inspect named archives and effective patch source indexes',flush=True)
finally:
 p.adb('shell','am','force-stop',PACKAGE)
 for name in added:p.adb('shell','rm',shlex.quote(remote+'/'+name))
 detail()
 p.wait('library restored original metadata',lambda:not state().get('chinese'),30)
 print('PASS library restores original metadata after test patches removed',flush=True)
