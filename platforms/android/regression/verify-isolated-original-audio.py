"""Device checks use only structured runtime diagnostics and real touch events."""
import io, json, math
from pathlib import Path
from PIL import Image
import shlex
import subprocess
import sys
import uuid
import xml.etree.ElementTree as ET

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_menu_sounds import ACTIVITY, PACKAGE, PREFERENCES, SETTINGS, TITLE, Probe
from verify_voice_matrix_support import voice_result

ADB = 'D:/codex/tools/android/sdk/platform-tools/adb.exe'
SERIAL, GAME = sys.argv[1:3]
TAG = 'isolated-original-audio-' + GAME.lower() + '-' + uuid.uuid4().hex[:8]
probe_directory=GAME+('-ENV-TEST' if GAME=='BIRTH' else '-AUDIO-TEST')
DIRECTORY = 'raw:/storage/emulated/0/Download/NekoPack-BLESS/'+probe_directory
p = Probe(ADB, SERIAL, TAG, {})
prefs = ET.fromstring(p.private(PREFERENCES))
tree = prefs.find("./string[@name='dat_tree']").text
root_key = 'runtime_config_v1_' + tree + '/document/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-BLESS'
key = ('runtime_config_v1_' + tree if GAME == 'BLESS' else root_key + '%2F' + probe_directory) + '_0'
original = next((n.text for n in prefs if n.get('name') == key),None)
audio_fields = dict(bgm='VolBGM',voice='VolVO',se='VolSE',env='VolENV',system='VolSYS')

def point(label,area):
    if label not in bounds:
        bounds[label]=p.bounds(label)
    left,top,right,bottom=bounds[label]
    x,y,w,h=area
    scale=min((right-left)/1280,(bottom-top)/720)
    return round((left+right)/2+(x+w/2-640)*scale),round((top+bottom)/2+(y+h/2-360)*scale)

def boot(suffix,slot=0):
    global p, bounds
    bounds={}
    p = Probe(ADB, SERIAL, TAG+'-'+suffix, {})
    launch = p.start_activity if slot else p.start_title
    result = launch('--ei','title_game','0',
          '--es','library_tree',tree,'--es','library_directory',shlex.quote(DIRECTORY),
          '--es','read_history_probe',TAG,'--ez','legacy_probe_controls','true',
          '--es','save_probe',TAG,'--ei','library_resume_slot',str(slot))
    if not slot: return result

def title_button(id):
    state = p.obj('title-presented.json')
    p.tap(*point(TITLE,next(b['bounds'] for b in state['buttons'] if b['id']==id)))

def settings():
    title_button(3)
    return p.wait('settings',p.settings,20)

def click(key):
    m = p.settings()
    m,rect=area(m,key)
    p.tap(*point(SETTINGS,rect))
    result=p.after_settings(m,2)
    assert not result['unsupported'],key
    return result

def radio(field, button=None, value=None):
    config=p.settings()['config']
    if value is None: value=1-config[field]
    result=click(f'= {button or field}#{value}')
    assert result['config'][field]==value,(field,value,result['config'][field])
    return value

def slider(field, fraction):
    state=p.settings()
    state,(x,y,w,h)=area(state,'* '+field)
    p.tap(*point(SETTINGS,[x+w*fraction,y,0,h]))
    value=p.after_settings(state,2)['config'][field]
    assert abs(value-round(fraction*(255 if field=='BoxOpacity' else 100)))<=3,(field,value)
    return value


def area(m,key):
    rect=m['areas'][key]
    if GAME!='ENDLESS' or 155<rect[1]+rect[3]/2<545:return m,rect
    target=max(0,min(67,round((m['scroll']+rect[1]+rect[3]/2-350)/50)))
    x,y,w,h=m['areas']['* PAGE']
    p.tap(*point(SETTINGS,[x,y+(target+.5)*h/69,w,0]))
    m=p.wait('scroll',lambda:(s if (s:=p.settings()) and s['scroll']==target*50 else None),15)
    return m,m['areas'][key]

def go(page):
    if GAME=='ENDLESS':return
    while p.settings()['page']!=page: click('= PAGE++' if p.settings()['page']<page else '= PAGE--')

def page_ready(n):
    return p.wait(f'page {n}',lambda:(s if (s:=p.obj(f'text-{n}.json')) and s['complete'] else None),24)

def after_load():
    page_ready(1)
    p.tap(640,300)
    return page_ready(2)

def files():
    return p.adb('shell','run-as',PACKAGE,'ls','files/'+p.run).decode().splitlines()

def sound_token(kind,name):
    for file in files():
        if file.startswith('audio-') and file.endswith('.json'):
            m=p.obj(file)
            if m and m.get('kind')==kind and m.get('name')==name:
                return int(file[6:-5])
    return None

def playback(kind,token):
    rows=[e for e in p.events('host-events.jsonl') if e['kind']==kind and e['token']==token]
    return any(e['event']=='playing' for e in rows) and any(e['event'] in ('progress','pcm_head') and e['value']>0 for e in rows)

def render_statistics():
    m=p.obj('dialogue-1.json')
    raw=p.read('dialogue-1.png')
    im=Image.open(io.BytesIO(raw)).convert('RGBA')
    x,y,w,h=m['text_rect']; bx,by,_,_=m['bounds']
    colors=im.crop((x-bx,y-by,x-bx+w,y-by+h)).getdata()
    return dict(white=sum(r>230 and g>230 and b>230 and a>215 for r,g,b,a in colors),
                black=sum(r<28 and g<28 and b<28 and a>225 for r,g,b,a in colors),
                translucent=sum(a for r,g,b,a in colors if a<220),
                layout=[m['origins'],m['lines']])



assets=json.loads(Path('.local/android-probe/remaining-extra-voice-assets.json').read_text())[GAME]
try:
    for phase,fraction in [(0,0.0),(1,.71)]:
        boot('set-'+str(phase))
        settings()
        go(3)
        slider('VolVO',.8)
        if GAME=='BIRTH':
            level=slider('VolENV',fraction)
        go(4)
        expected={tag:(i+phase)%2 for i,tag in enumerate(assets)}
        for tag,val in expected.items():radio(tag,value=val)
        boot('play-'+str(phase))
        title_button(1)
        page_ready(1)
        if GAME=='BIRTH':
            token=p.wait('original environment resource',lambda:sound_token('env','ENV_002'),12)
            p.wait('environment playback advances',lambda:playback('env',token),12)
            data=p.obj(f'effect-{token}.json')
            gain=next(e for e in p.events('audio-gain-events.jsonl') if e['token']==token and e['kind']=='env' and e['category_percent']==level)
            assert math.isclose(gain['output'],data['volume_percent']*level/10000,abs_tol=.015),gain
            print('PASS BIRTH environment resource in isolated scene after restart',level,gain['output'],flush=True)
        for page,(tag,cue) in enumerate(assets.items(),1):
            if page>1:
                p.tap(640,300)
                page_ready(page)
            state=p.obj(f'page-{page}.json')
            assert state['line']==3+2*(page-1),(state['source'],state['line'])
            if expected[tag]:
                assert state['voice']==cue,state['voice']
                token=p.wait('original character resource',lambda:sound_token('voice',cue),12)
                p.wait('voice playback advances',lambda:playback('voice',token),12)
            else:
                assert state['voice']=='' and not sound_token('voice',cue)
            voice_result(GAME,tag,cue,expected[tag],p,token if expected[tag] else None,isolated=True)
            print('PASS',GAME,tag,cue,('playing' if expected[tag] else 'silent'),'isolated scene after restart',flush=True)
finally:
    p.adb('shell','am','force-stop',PACKAGE)
    current=ET.fromstring(p.private(PREFERENCES))
    node=next((n for n in current if n.get('name')==key),None)
    if node is not None:
        if original is None: current.remove(node)
        else: node.text=original
    subprocess.run([*p.command,'shell','run-as',PACKAGE,f"sh -c 'cat > {PREFERENCES}.verify && mv {PREFERENCES}.verify {PREFERENCES}'"],
                   input=ET.tostring(current,encoding='utf-8',xml_declaration=True),check=True)
