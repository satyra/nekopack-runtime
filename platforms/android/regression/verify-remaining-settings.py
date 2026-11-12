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
from verify_voice_matrix_support import emit, voice_result

ADB = 'D:/codex/tools/android/sdk/platform-tools/adb.exe'
SERIAL, GAME = sys.argv[1:3]
TAG = 'remaining-config-' + GAME.lower() + '-' + uuid.uuid4().hex[:8]
DIRECTORY = 'raw:/storage/emulated/0/Download/NekoPack-BLESS' + ('' if GAME == 'BLESS' else '/' + ('Endless Dungeon' if GAME=='ENDLESS' else GAME))
p = Probe(ADB, SERIAL, TAG, {})
prefs = ET.fromstring(p.private(PREFERENCES))
tree = prefs.find("./string[@name='dat_tree']").text
root_key = 'runtime_config_v1_' + tree + '/document/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-BLESS'
key = ('runtime_config_v1_' + tree if GAME == 'BLESS' else root_key + '%2F' + ('Endless%20Dungeon' if GAME=='ENDLESS' else GAME)) + '_0'
original = next(n.text for n in prefs if n.get('name') == key)
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
    colors=im.crop((x-bx,y-by,x-bx+w,y-by+h)).get_flattened_data()
    return dict(white=sum(r>230 and g>230 and b>230 and a>215 for r,g,b,a in colors),
                black=sum(r<28 and g<28 and b<28 and a>225 for r,g,b,a in colors),
                translucent=sum(a for r,g,b,a in colors if a<220),
                layout=[m['origins'],m['lines']])

fixtures=Path('.local/android-probe/remaining-fixtures')
items=json.loads((fixtures/'index.json').read_text())[GAME]
folder='files/saves/e6910f2dd1ee0443a9bc2605bc6b4848927b3db580e5d8f73bf2be69d47637f9_probe_'+TAG
p.adb('shell','run-as',PACKAGE,'mkdir','-p',folder)
for item in items:
    temp='/data/local/tmp/'+item['file']
    p.adb('push',str(fixtures/item['file']),temp)
    p.adb('shell','run-as',PACKAGE,'cp',temp,f"{folder}/slot-{item['slot']:03d}.nksv")
    p.adb('shell','rm',temp)

def configure(suffix, *, voices=None, env=None, style=None):
    boot(suffix)
    state=settings()
    if GAME=='ENDLESS' and state['config']['Dialog']:radio('Dialog',value=0)
    if 'Movie' in state['config']: radio('Movie',value=0)
    if style:
        go(2)
        bold,border,opacity=style
        radio('Bold','BOLD',bold)
        radio('Border','BORDER',border)
        slider('BoxOpacity',opacity)
    if env is not None:
        go(3)
        env=slider('VolENV',env)
    if voices is not None:
        go(3)
        slider('VolVO',.8)
        go(4)
        for tag,value in voices.items():radio(tag,value=value)
    return env

errors=[]
try:
    boot('identify');settings();go(4)
    controls={k[2:4] for k in p.settings()['areas'] if len(k)==6 and k.startswith('= ') and k.endswith('#0')}
    tags=sorted(controls & {i['tag'] for i in items})
    emit('controls', game=GAME, tags=sorted(controls))
    print('COVERAGE',GAME,'voice controls',sorted(controls),'without route fixture',sorted(controls-set(tags)),flush=True)
    if 'style' in sys.argv[3:]:
        style_results={}
        for name,style in [('base',(0,0,.33)),('bold',(1,0,.33)),('border',(0,1,.33)),('opacity',(0,0,.66))]:
            configure('set-style-'+name,style=style)
            boot('style-'+name,items[0]['slot'])
            page_ready(1)
            style_results[name]=render_statistics()
            print('RENDER',GAME,name,{k:v for k,v in style_results[name].items() if k!='layout'},flush=True)
        plain=style_results['base']
        assert style_results['bold']['white']>plain['white'],style_results
        assert style_results['border']['black']>plain['black'],style_results
        assert style_results['opacity']['translucent']>plain['translucent'],style_results
        assert all(r['layout']==plain['layout'] for r in style_results.values())
        print('PASS',GAME,'bold, outline and box opacity change rendered coverage after process restart',flush=True)
    if 'audio' in sys.argv[3:]:
        for enabled,level in [(1,.24),(0,.70)]:
            flags={tag:(i+enabled)%2 for i,tag in enumerate(tags)}
            expected=configure('set-audio-'+str(enabled),voices=flags,env=level)
            for item in items:
                tag,kind=item['tag'],('env' if item['tag']=='env' else 'voice')
                if kind=='voice' and tag not in tags:continue
                try:
                    boot(f'{tag}-{enabled}',item['slot'])
                    after_load()
                    m=p.obj('page-2.json')
                    assert (m['source'],m['line'])==(item['source'],item['line']),(m['source'],m['line'])
                    active=flags.get(tag,1)
                    if kind=='env' or active:
                        token=p.wait(kind+' cue',lambda:sound_token(kind,item['cue']),10)
                        p.wait('player advances',lambda:playback(kind,token),12)
                        if kind=='env':
                            gain=next(e for e in p.events('audio-gain-events.jsonl') if e['kind']=='env' and e['token']==token and e['category_percent']==expected)
                            command=p.obj(f'effect-{token}.json')['volume_percent']
                            assert abs(gain['output']-expected*command/10000)<.015,(gain,command)
                    else:
                        assert not sound_token(kind,item['cue'])
                        assert not m['voice'],m['voice']
                    if kind=='voice':
                        voice_result(GAME,tag,item['cue'],active,p,token if active else None)
                    print('PASS',GAME,tag,item['cue'],'level '+str(expected) if kind=='env' else ('playing' if active else 'silent'),flush=True)
                except (AssertionError,StopIteration) as e:
                    print('FAIL',GAME,tag,enabled,str(e),flush=True)
                    errors.append((tag,enabled))
        if errors:raise AssertionError(errors)
finally:
    p.adb('shell','am','force-stop',PACKAGE)
    current=ET.fromstring(p.private(PREFERENCES))
    next(n for n in current if n.get('name')==key).text=original
    subprocess.run([*p.command,'shell','run-as',PACKAGE,f"sh -c 'cat > {PREFERENCES}.verify && mv {PREFERENCES}.verify {PREFERENCES}'"],
                   input=ET.tostring(current,encoding='utf-8',xml_declaration=True),check=True)
