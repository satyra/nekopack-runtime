"""Real BLESS controls: partial text, Home freeze, two clicks, KeepVoice 0/1."""
import argparse,json,re,subprocess,time,uuid
from pathlib import Path
import xml.etree.ElementTree as ET
p=argparse.ArgumentParser()
for key in ('adb','serial','reference','output'):p.add_argument('--'+key,required=True)
p.add_argument('--execution',choices=('native','translated'),default='translated')
a=p.parse_args();pkg='org.nekopack.probe'
def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=45)
def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
def rows(name):
    raw=read(name);return [json.loads(l) for l in raw.splitlines()] if raw.startswith(b'{') else []
def obj(name):
    raw=read(name);return json.loads(raw) if raw.startswith(b'{') else None
def wait(fn,seconds=60):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        result=fn()
        if result:return result
        error=read('error.txt');assert b'No such file' in error,error.decode()
        time.sleep(.05)
    raise AssertionError('timeout')
def bounds(label,attr='text'):
    adb('shell','uiautomator','dump','/sdcard/neko-dialogue.xml')
    tree=ET.fromstring(adb('shell','cat','/sdcard/neko-dialogue.xml'))
    node=next(n for n in tree.iter('node') if n.get(attr)==label and n.get('enabled')=='true')
    return list(map(int,re.findall(r'\d+',node.get('bounds'))))
def tap(label,attr='text'):
    x,y,r,b=bounds(label,attr);adb('shell','input','tap',str((x+r)//2),str((y+b)//2))
def state(n):return obj(f'text-{n}.json')
def ready(n):
    page=wait(lambda:obj(f'page-{n}.json'));ref=Path(a.reference)/f'{n-1}.json'
    assert page==json.loads(ref.read_text(encoding='utf8'))
    assert read(f'page-{n}.png')==ref.with_suffix('.png').read_bytes()
    wait(lambda:state(n))
    return page
def safe(s):return {k:v for k,v in s.items() if k!='text'}
build=json.loads(Path('.local/android-probe/build.json').read_text())
apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:')
sha=adb('shell','sha256sum',apk).decode().split()[0];assert sha==build['sha256']['nekopack-arm64-probe.apk']
machine=adb('shell','uname','-m').decode().strip();bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip()
if a.execution=='native':assert machine=='aarch64' and bridge in ('','0'),(machine,bridge)
else:assert machine=='x86_64' and bridge not in ('','0'),(machine,bridge)
execution='native ARM64' if a.execution=='native' else 'translated ARM64 on x86_64'
cases=[]
for keep in (1,0):
    run='dialogue_'+uuid.uuid4().hex
    adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','story_game','0','--es','run_id',run)
    ready(1);wait(lambda:state(1)['complete'])
    tap('文字');x,y,r,b=bounds('text_speed','content-desc');y=(y+b)//2
    adb('shell','input','swipe',str((x+r)//2),str(y),str(x+1),str(y),'250')
    wait(lambda:rows('text-config-events.jsonl')[-1]['speed']==0)
    if keep==0:tap('keep_voice','content-desc');wait(lambda:rows('text-config-events.jsonl')[-1]['keep_voice']==0)
    tap('返回剧情');x,y,r,b=bounds('继续');point=(str((x+r)//2),str((y+b)//2))
    x,y,r,b=bounds('结束运行');close_point=(str((x+r)//2),str((y+b)//2))
    def click():adb('shell','input','tap',*point)
    def reveal(n):
        click()
        clicks=wait(lambda:[e for e in rows('text-events.jsonl') if e['sequence']==n and e['click']])
        assert len(clicks)==1 and not clicks[0]['advance'] and not clicks[0]['stop_voice']
        wait(lambda:state(n)['complete'])
    click();page=ready(2)
    before=wait(lambda:state(2) if state(2)['visible']>0 else None)
    assert not before['complete']
    reveal(2);assert b'No such file' in read('page-3.json')
    click();ready(3)
    # Page 3 independently checks freeze/resume and natural completion. Android
    # UIAutomator waits for accessibility idle, which can last until text is
    # complete; partial-click semantics are checked on pages 2 and 4 instead.
    wait(lambda:state(3)['visible']>0)
    adb('shell','input','keyevent','KEYCODE_HOME')
    wait(lambda:rows('lifecycle-events.jsonl')[-1]['event']=='paused')
    time.sleep(.2);paused=state(3);time.sleep(1.8);assert state(3)==paused
    adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity')
    after=wait(lambda:state(3) if state(3)['elapsed_ms']>paused['elapsed_ms'] else None)
    assert after['elapsed_ms']-paused['elapsed_ms']<500
    assert not after['complete']
    wait(lambda:state(3)['complete'])
    x,y,r,b=bounds('继续');point=(str((x+r)//2),str((y+b)//2))
    assert b'No such file' in read('page-4.json')
    # Refresh the close control after the foreground transition as well.
    x,y,r,b=bounds('结束运行');close_point=(str((x+r)//2),str((y+b)//2))
    click();ready(4);reveal(4);click();ready(5)
    transitions=rows('voice-next-events.jsonl');voice=next(e for e in transitions if e['sequence']==4)
    assert voice['playing_before'] and voice['player']!=0
    assert voice['stop']==(keep==0) and voice['retained']==(keep==1) and voice['playing_after']==(keep==1)
    events=rows('text-events.jsonl')
    for n in (2,4):
        clicks=[e for e in events if e['sequence']==n and e['click']]
        assert len(clicks)==2 and not clicks[0]['advance'] and clicks[1]['advance']
    resumed_clicks=[e for e in events if e['sequence']==3 and e['click']]
    assert len(resumed_clicks)==1 and resumed_clicks[0]['advance']
    assert not state(5)['complete']
    adb('shell','input','tap',*close_point);wait(lambda:read('closed.txt')==b'closed; VM live bytes=0')
    stable=read('text-events.jsonl');time.sleep(.5);assert read('text-events.jsonl')==stable
    events=[json.loads(line) for line in stable.splitlines()]
    assert all(not e['complete'] for e in events if e['sequence']==5), 'close did not interrupt revealing'
    cases.append(dict(run_id=run,keep_voice=keep,pages=5,paused=safe(paused),resumed=safe(after),text_events=events,voice_events=transitions,config_events=rows('text-config-events.jsonl'),closed_while_revealing=True,vm_live_bytes=0))
Path(a.output).write_text(json.dumps(dict(execution=execution,machine=machine,native_bridge=bridge,apk_sha256=sha,cases=cases),ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS partial text, background freeze, two-phase NEXT, KeepVoice 0/1, VM cleanup')
