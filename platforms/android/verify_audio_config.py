"""BLESS Config -> Rust -> JNI -> real Android sliders and active player gains."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import time
import uuid
import xml.etree.ElementTree as ET

p=argparse.ArgumentParser()
for key in ('adb','serial','reference','output'):p.add_argument('--'+key,required=True)
a=p.parse_args();pkg='org.nekopack.probe';run='audio_config_'+uuid.uuid4().hex
def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=45)
def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
def rows(name):
    raw=read(name)
    return [json.loads(line) for line in raw.splitlines()] if raw.startswith(b'{') else []
def wait(fn,seconds=180):
    end=time.monotonic()+seconds
    while time.monotonic()<end:
        value=fn()
        if value:return value
        error=read('error.txt');assert b'No such file' in error,error.decode()
        time.sleep(.2)
    raise AssertionError('timed out')
def bounds(label,attribute='text'):
    end=time.monotonic()+15
    while time.monotonic()<end:
        adb('shell','uiautomator','dump','/sdcard/neko-config.xml')
        tree=ET.fromstring(adb('shell','cat','/sdcard/neko-config.xml'))
        found=[n for n in tree.iter('node') if n.get(attribute)==label and n.get('enabled')=='true']
        if found:return list(map(int,re.findall(r'\d+',found[0].get('bounds'))))
        time.sleep(.2)
    raise AssertionError('missing control '+label)
def tap(label):
    x,y,r,b=bounds(label);adb('shell','input','tap',str((x+r)//2),str((y+b)//2))
build=json.loads(Path('.local/android-probe/build.json').read_text())
apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:')
sha=adb('shell','sha256sum',apk).decode().split()[0]
assert sha==build['sha256']['nekopack-arm64-probe.apk']
machine=adb('shell','uname','-m').decode().strip()
bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip()
assert machine=='x86_64' and bridge not in ('','0'),(machine,bridge)
adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','story_game','0','--es','run_id',run)
movie_skipped=False
def page_ready(n):
    global movie_skipped
    raw=read(f'page-{n}.json')
    if raw.startswith(b'{'):
        text=read(f'text-{n}.json')
        if text.startswith(b'{') and json.loads(text)['complete']:return json.loads(raw)
    if n==13 and not movie_skipped:
        paths=adb('shell','run-as',pkg,'sh','-c',f"'printf \"%s\\n\" files/{run}/movie-*-events.jsonl'")
        for name in [path.rsplit('/',1)[-1] for path in paths.decode().splitlines()]:
            if re.fullmatch(r'movie-\d+-events.jsonl',name) and any(e['event']=='progress' and e['head']>0 for e in rows(name)):
                tap('返回剧情');movie_skipped=True
    return None
for n in range(1,14):
    page=wait(lambda:page_ready(n));ref=Path(a.reference)/f'{n-1}.json'
    assert page==json.loads(ref.read_text(encoding='utf8'))
    assert read(f'page-{n}.png')==ref.with_suffix('.png').read_bytes()
    if n<13:tap('继续')
defaults=dict(bgm=50,voice=75,se=50,env=50,system=50)
assert rows('audio-config-events.jsonl')==[defaults]
gains=rows('audio-gain-events.jsonl');bgm=[e for e in gains if e['kind']=='bgm'][-1]
assert bgm['output']==.25 and bgm['category_percent']==50
assert any(e['kind']=='voice' and e['output']==.75 for e in gains)
tap('音量')
changes=[]
for kind in defaults:
    for value in (0,100):
        before=len(rows('audio-config-events.jsonl'))
        x,y,r,b=bounds('volume_'+kind,'content-desc');y=(y+b)//2
        adb('shell','input','swipe',str((x+r)//2),str(y),str(x+1 if value==0 else r-1),str(y),'250')
        config=wait(lambda:rows('audio-config-events.jsonl') if len(rows('audio-config-events.jsonl'))>before else None)[-1]
        assert config[kind]==value,(kind,value,config)
        changes.append(dict(channel=kind,percent=value,config=config))
        current=[e for e in rows('audio-gain-events.jsonl') if e['kind']=='bgm'][-1]
        assert current['player']==bgm['player'] and current['token']==bgm['token']
        assert abs(current['output']-(0 if config['bgm']==0 else .5))<.000001
        assert current['fade']==1 and current['duck']==1
assert b'No such file' in read('page-14.json'),'configuration advanced story'
tap('返回剧情');tap('音量')
for label in ('BGM 100%','语音 100%','音效 100%','环境音 100%','系统音 100%'):bounds(label)
tap('返回剧情');tap('继续');wait(lambda:page_ready(14))
tap('结束运行');wait(lambda:read('closed.txt')==b'closed; VM live bytes=0',15)
result=dict(run_id=run,execution='translated ARM64 on x86_64',machine=machine,native_bridge=bridge,apk_sha256=sha,pages=14,defaults=defaults,changes=changes,config_events=rows('audio-config-events.jsonl'),gain_events=rows('audio-gain-events.jsonl'),vm_live_bytes=0)
Path(a.output).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS Config defaults, 10 real slider updates, retained player/page, VM live bytes=0')
