"""BLESS pause/resume through actual Home; execution mode must match the device."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import time
import uuid
import xml.etree.ElementTree as ET

p=argparse.ArgumentParser()
for key in ('adb','serial','reference','output'):p.add_argument('--'+key,required=True)
p.add_argument('--movie-only',action='store_true')
p.add_argument('--destroy-paused',action='store_true')
p.add_argument('--execution',choices=('native','translated'),default='native')
a=p.parse_args();pkg='org.nekopack.probe';run='lifecycle_'+uuid.uuid4().hex
def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=45)
def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
def events(name):
    raw=read(name)
    return [json.loads(line) for line in raw.splitlines()] if raw.startswith(b'{') else []
def wait(predicate,seconds=60):
    until=time.monotonic()+seconds
    while time.monotonic()<until:
        result=predicate()
        if result:return result
        error=read('error.txt');assert b'No such file' in error,error.decode()
        time.sleep(.15)
    raise AssertionError('condition timed out')
point_cache={}
def tap(label):
    if label not in point_cache:
        deadline=time.monotonic()+15
        while time.monotonic()<deadline:
            adb('shell','uiautomator','dump','/sdcard/neko-lifecycle.xml')
            tree=ET.fromstring(adb('shell','cat','/sdcard/neko-lifecycle.xml'))
            nodes=[n for n in tree.iter('node') if n.get('package')==pkg and n.get('text')==label and n.get('enabled')=='true']
            if nodes:
                x,y,r,b=map(int,re.findall(r'\d+',nodes[0].get('bounds')))
                point_cache[label]=(str((x+r)//2),str((y+b)//2));break
            time.sleep(.15)
        else:raise AssertionError('visible control not ready: '+label)
    adb('shell','input','tap',*point_cache[label])
def lifecycle():return events('lifecycle-events.jsonl')
observations=[]
def cycle(label,already_paused=False,extra=None):
    before=lifecycle();old_count=sum(e['event']=='paused' for e in before)
    if not already_paused:adb('shell','input','keyevent','KEYCODE_HOME')
    def new_pause():
        rows=[e for e in lifecycle() if e['event']=='paused']
        return rows if already_paused or len(rows)>old_count else []
    paused=wait(new_pause)[-1]
    time.sleep(.5)
    watched=['host-events.jsonl','motion-events.jsonl']
    if extra:watched.append(extra)
    stable={name:read(name) for name in watched}
    time.sleep(1.2)
    assert all(read(name)==raw for name,raw in stable.items()), 'background continued execution'
    assert b'No such file' in read('lifecycle-closed.json')
    assert b'No such file' in read('closed.txt')
    pid=adb('shell','pidof',pkg).strip()
    adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity')
    resumed=wait(lambda:[e for e in lifecycle() if e['event']=='resumed' and e['time_ms']>paused['time_ms']])[-1]
    assert paused['instance']==resumed['instance']
    assert adb('shell','pidof',pkg).strip()==pid
    assert resumed['time_ms']-paused['time_ms']>=1200
    assert 0<=resumed['active_ms']-paused['active_ms']<100
    if label.startswith('dialogue_bgm'):
        snapshots=events('audio-lifecycle.jsonl')
        steady=[e for e in snapshots if e['time_ms']>paused['time_ms'] and e['event'].startswith('paused_stable_')]
        assert len(steady)==2 and steady[1]['time_ms']-steady[0]['time_ms']>=900
        first=steady[0]['channels']['bgm'];off=steady[1]['channels']['bgm']
        assert not first['playing'] and first['position']==off['position'] and first['token']==off['token']
        on=next(e for e in reversed(snapshots) if e['event']=='resumed')['channels']['bgm']
        assert not off['playing'] and on['playing'] and off['token']==on['token']
        before_start=next(e for e in reversed(snapshots) if e['event']=='resuming')['channels']['bgm']
        assert not before_start['playing'] and before_start['position']==off['position']
        assert first['player_id']==off['player_id']==before_start['player_id']==on['player_id']
    point_cache.clear() # Foreground transitions invalidate previously observed UI coordinates.
    observations.append(dict(label=label,paused=paused,resumed=resumed,pid=pid.decode(),stable_files={name:hashlib.sha256(raw).hexdigest() for name,raw in stable.items()}))

build=json.loads(Path('.local/android-probe/build.json').read_text())
apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:')
assert adb('shell','sha256sum',apk).decode().split()[0]==build['sha256']['nekopack-arm64-probe.apk']
machine=adb('shell','uname','-m').decode().strip()
bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip()
if a.execution=='native':
    assert machine=='aarch64' and bridge in ('','0'),(machine,bridge)
else:
    assert machine=='x86_64' and bridge not in ('','0'),(machine,bridge)
execution='native ARM64' if a.execution=='native' else 'translated ARM64 on x86_64'
args=['shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--es','run_id',run,'--ez','lifecycle_probe','true']
if a.destroy_paused:args+=['--ei','story_game','0','--ez','finish_while_paused','true']
elif a.movie_only:args+=['--ei','movie_probe_game','0']
else:args+=['--ei','story_game','0','--es','pause_on_effect','fade','--ez','close_on_motion','true','--ei','close_on_motion_sequence','56']
adb(*args)
if not a.movie_only and not a.destroy_paused:
    wait(lambda:any(e['event']=='paused' for e in lifecycle()))
    cycle('fade',already_paused=True)
movie_token=1 if a.movie_only else 8
movie_name=f'movie-{movie_token}-events.jsonl'
def movie_cycle():
    first=wait(lambda:[e for e in events(movie_name) if e['event']=='progress' and e['head']>0])[-1]
    cycle('movie',extra=movie_name)
    later=wait(lambda:[e for e in events(movie_name) if e['event']=='progress' and e['frames']>first['frames']])[-1]
    history=events(movie_name)
    assert sum(e['event']=='surface_created' for e in history)>=2
    assert not any(e['event']=='closed' for e in history)
    paused=next(e for e in reversed(history) if e['event']=='paused')
    resumed=next(e for e in reversed(history) if e['event']=='resumed')
    assert 0<=resumed['head']-paused['head']<=4410
    observations[-1].update(before=first,after=later)
if a.destroy_paused:
    pages=[]
    for n in range(1,13):
        raw=wait(lambda:read(f'page-{n}.json') if read(f'page-{n}.json').startswith(b'{') else None)
        page=json.loads(raw)
        assert page==json.loads((Path(a.reference)/'0'/f'{n-1}.json').read_text(encoding='utf8'))
        pages.append(dict(sequence=n,source=page['source'],line=page['line']))
        tap('继续')
    wait(lambda:any(e['event']=='progress' and e['head']>0 for e in events(movie_name)))
    adb('shell','input','keyevent','KEYCODE_HOME')
    wait(lambda:read('lifecycle-closed.json').startswith(b'{'))
    assert json.loads(read('lifecycle-closed.json'))=={'closed':True}
    end=wait(lambda:[e for e in events(movie_name) if e['event']=='closed'])[-1]
    assert end['reason']==3 and end['error'] is None
    assert b'No such file' in read('page-13.json')
    stable=read(movie_name);time.sleep(1);assert read(movie_name)==stable
    assert events('host-events.jsonl')[-1]['event']=='released'
    assert b'.ogg' not in adb('shell','run-as',pkg,'ls',f'files/{run}')
elif a.movie_only:
    movie_cycle()
    end=wait(lambda:[e for e in events(movie_name) if e['event']=='closed'],300)[-1]
    assert end['reason']==0 and end['error'] is None
    info=json.loads(read(f'movie-{movie_token}-info.json'))
    assert end['head']>=end['samples'] and end['samples']/info['sample_rate']>=info['duration']-.2
    wait(lambda:read('movie-complete.json').startswith(b'{'))
    pages=[]
else:
    pages=[]
    for n in range(1,62):
        if n==13:
            movie_cycle();tap('返回剧情');point_cache.clear()
        page=json.loads(wait(lambda:read(f'page-{n}.json') if read(f'page-{n}.json').startswith(b'{') else None))
        ref=Path(a.reference)/'0'/f'{n-1}.json'
        assert page==json.loads(ref.read_text(encoding='utf8'))
        assert read(f'page-{n}.png')==ref.with_suffix('.png').read_bytes()
        pages.append(dict(sequence=n,source=page['source'],line=page['line'],metadata_sha256=hashlib.sha256(json.dumps(page,sort_keys=True,ensure_ascii=False).encode()).hexdigest()))
        if n==13:
            cycle('dialogue_bgm');cycle('dialogue_bgm_repeat')
            assert b'No such file' in read('page-14.json')
        if n==56:
            wait(lambda:len([e for e in lifecycle() if e['event']=='paused'])==5)
            motion_before=[e for e in events('motion-events.jsonl') if e['sequence']==56]
            assert any(e['event']=='frame' and e['elapsed_ms']>0 for e in motion_before) and not any(e['event']=='finished' for e in motion_before)
            cycle('motion',already_paused=True)
        if 'motion' in page:
            wait(lambda:any(e['event']=='finished' and e['sequence']==n for e in events('motion-events.jsonl')))
        if n==56:
            frames=[e for e in events('motion-events.jsonl') if e['sequence']==56 and e['event']=='frame']
            assert frames[-1]['finished'] and 250<=frames[-1]['elapsed_ms']<500
            assert frames[-1]['time_ms']-frames[0]['time_ms']>1500
        if n<61:tap('继续')
    tap('结束运行')
    wait(lambda:read('closed.txt')==b'closed; VM live bytes=0')
    assert b'.ogg' not in adb('shell','run-as',pkg,'ls',f'files/{run}')
    end=events(movie_name)[-1]
    assert end['event']=='closed' and end['reason']==2 and end['error'] is None
result=dict(run_id=run,execution=execution,serial=a.serial,machine=machine,native_bridge=bridge,build=build,cycles=observations,pages=pages,movie_events=events(movie_name),host_events=events('host-events.jsonl'),audio_lifecycle=events('audio-lifecycle.jsonl'),motion_events=events('motion-events.jsonl'),lifecycle=lifecycle(),movie_natural_completion=a.movie_only,destroyed_while_paused=a.destroy_paused,vm_live_bytes=None if a.movie_only else 0)
Path(a.output).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
print('PASS lifecycle',run,'cycles',len(observations),'pages',len(pages),'natural movie',a.movie_only)
