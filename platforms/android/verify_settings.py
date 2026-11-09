"""BLESS original settings: actual touch, VM preferences, persistence and story use."""
import argparse, hashlib, io, json, re, subprocess, sys, time, uuid
from pathlib import Path
import xml.etree.ElementTree as ET
import numpy as np
from PIL import Image

def main():
    p=argparse.ArgumentParser()
    for key in ('adb','serial','reference','games','portkit','work','output'):p.add_argument('--'+key,required=True)
    a=p.parse_args();work=Path(a.work);work.mkdir(parents=True,exist_ok=True);pkg='org.nekopack.probe';run=''
    def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=45)
    def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
    def obj(name):
        raw=read(name)
        if not raw.startswith(b'{'):return None
        try:return json.loads(raw)
        except json.JSONDecodeError:return None # writer may be between truncate/write
    def wait(fn):
        until=time.monotonic()+90
        while time.monotonic()<until:
            error=read('error.txt');assert b'No such file' in error,error
            result=fn()
            if result:return result
            time.sleep(.1)
        raise AssertionError('settings timeout')
    def bounds(label,attr='text'):
        adb('shell','uiautomator','dump','/sdcard/neko-settings.xml')
        node=next(n for n in ET.fromstring(adb('shell','cat','/sdcard/neko-settings.xml')).iter('node') if n.get(attr)==label and n.get('enabled')=='true')
        return list(map(int,re.findall(r'\d+',node.get('bounds'))))
    def tap(x,y):adb('shell','input','tap',str(round(x)),str(round(y)))
    def control(label):
        x,y,r,b=bounds(label);tap((x+r)/2,(y+b)/2)
    def game_point(x,y,label='原版设置页'):
        l,t,r,b=bounds(label,'content-desc');s=min((r-l)/1280,(b-t)/720)
        return round(l+(r-l-1280*s)/2+x*s),round(t+(b-t-720*s)/2+y*s)
    def state():return obj('settings-presented.json')
    def area(name):
        x,y,w,h=state()['areas'][name];return game_point(x+w/2,y+h/2)
    def tap_area(name):
        before=state()['input_sequence'];tap(*area(name));wait(lambda:state() and state()['input_sequence']>=before+2)
    def launch():
        nonlocal run
        run='settings_'+uuid.uuid4().hex
        adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','title_game','0','--es','run_id',run)
        title=wait(lambda:obj('title-presented.json'))
        x,y,w,h=next(b['bounds'] for b in title['buttons'] if b['id']==3)
        tap(*game_point(x+w/2,y+h/2,'游戏标题菜单'));wait(lambda:state());assert state()['page']==1
    def page(n):
        while state()['page']!=n:
            tap_area('= PAGE++' if state()['page']<n else '= PAGE--')
        print('page',n,'ready',flush=True)
    def close_config():
        before=state()['input_sequence'];control('返回标题');wait(lambda:obj('title-presented.json')['input_sequence']>before)
    def close_title():
        title=obj('title-presented.json');x,y,w,h=next(b['bounds'] for b in title['buttons'] if b['id']==4)
        tap(*game_point(x+w/2,y+h/2,'游戏标题菜单'));wait(lambda:b'VM live bytes=0' in read('closed.txt'))
    machine=adb('shell','uname','-m').decode().strip();bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip();assert machine=='aarch64' and bridge in ('0','')
    build=json.loads(Path('.local/android-probe/build.json').read_text());apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:')
    apk_sha=adb('shell','sha256sum',apk).decode().split()[0];assert apk_sha==build['sha256']['nekopack-arm64-probe.apk']
    launch();first_run=run
    before=state()['input_sequence'];control('恢复已接通选项默认值');wait(lambda:state()['input_sequence']>before)
    defaults=state()['config'];assert defaults==dict(TextSpeed=50,KeepVoice=1,VolBGM=50,VolVO=75,VolSE=50,VolENV=50,VolSYS=50)
    before=state()['config'];tap_area('= Screen#1');assert state()['unsupported'] and state()['config']==before
    page(3)
    # Independent original PNG composition for sound page, without runtime asset plans.
    sys.path.insert(0,a.portkit)
    from portkit.neko import Archive
    from portkit.mng import frames
    sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
    archive=Archive(Path(a.games)/next(s['file'] for s in sources if s['file'].endswith('/system.dat')))
    common=frames(archive.read('system/config.mng'),name_encoding='cp932');sound=frames(archive.read('system/config3.mng'),name_encoding='cp932')
    canvas=Image.new('RGBA',(1280,720),(0,0,0,255))
    def draw(s,dx=0,dy=0):canvas.alpha_composite(Image.open(io.BytesIO(s['png'])).convert('RGBA'),(s['x']+dx,s['y']+dy))
    draw(common['BASE']);draw(sound['BASE']);draw(common['= PAGE++']);draw(common['= PAGE--'])
    for name in ('VolBGM','VolVO','VolSE','VolENV','VolSYS'):draw(sound['@'+name],sound['*'+name]['width']*defaults[name]//100)
    draw(common['@ PAGE'],0,(common['* PAGE']['height']-common['@ PAGE']['height'])*2//4)
    png=read('settings.png');assert np.array_equal(np.array(canvas),np.array(Image.open(io.BytesIO(png))))
    (work/'sound.png').write_bytes(png);(work/'sound-screen.png').write_bytes(adb('exec-out','screencap','-p'))
    desired=dict(TextSpeed=0,KeepVoice=0,VolBGM=23,VolVO=61,VolSE=17,VolENV=33,VolSYS=44)
    for name in ('VolBGM','VolVO','VolSE','VolENV','VolSYS'):
        x,y,w,h=state()['areas']['* '+name];point=game_point(x+w*(desired[name]+.4)/100,y+h/2)
        before=state()['input_sequence']
        if name=='VolBGM':
            down=game_point(x+w/2,y+h/2)
            adb('shell','input','motionevent','DOWN',*map(str,down));wait(lambda:state()['input_sequence']>before)
            adb('shell','input','motionevent','MOVE',*map(str,point));wait(lambda:state()['config'][name]==desired[name])
            adb('shell','input','motionevent','UP',*map(str,point));wait(lambda:state()['input_sequence']>=before+3)
        else:tap(*point);wait(lambda:state()['input_sequence']>=before+2)
        assert state()['config'][name]==desired[name],state()
    page(2);tap_area('* TextSpeed--');assert state()['config']['TextSpeed']==0
    page(4);tap_area('= KeepVoice#0');assert state()['config']==desired
    page(5);page(4) # all original page assets load, unsupported controls remain read-only
    adb('shell','input','keyevent','KEYCODE_HOME');time.sleep(.5)
    adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity');bounds('原版设置页','content-desc');assert state()['config']==desired
    close_config();close_title()
    launch();restart_run=run;assert state()['config']==desired
    close_config();title=obj('title-presented.json');x,y,w,h=next(b['bounds'] for b in title['buttons'] if b['id']==1)
    tap(*game_point(x+w/2,y+h/2,'游戏标题菜单'));wait(lambda:obj('text-1.json'))
    audio=json.loads(read('audio-config-events.jsonl').splitlines()[-1]);text=json.loads(read('text-config-events.jsonl').splitlines()[-1])
    assert audio==dict(bgm=23,voice=61,se=17,env=33,system=44),audio
    assert text==dict(speed=0,keep_voice=0),text
    pages=[];next_bounds=bounds('继续');next_point=((next_bounds[0]+next_bounds[2])/2,(next_bounds[1]+next_bounds[3])/2)
    for n in range(1,6):
        page_data=wait(lambda:obj(f'page-{n}.json'))
        if n==4:wait(lambda:obj('text-4.json'));tap(*next_point)
        wait(lambda:obj(f'text-{n}.json') and obj(f'text-{n}.json')['complete'])
        ref=Path(a.reference)/f'{n-1}.json';assert page_data==json.loads(ref.read_text(encoding='utf8'));assert read(f'page-{n}.png')==ref.with_suffix('.png').read_bytes();pages.append(n)
        if n<5:tap(*next_point)
    gains=[json.loads(l) for l in read('audio-gain-events.jsonl').splitlines()];assert any(g['kind']=='voice' and g['category_percent']==61 and abs(g['output']-.61)<.001 for g in gains)
    voice=[json.loads(l) for l in read('voice-next-events.jsonl').splitlines()];assert any(v['stop'] and v['playing_before'] and not v['playing_after'] for v in voice)
    control('结束运行');wait(lambda:b'VM live bytes=0' in read('closed.txt'))
    launch();reset_run=run;before=state()['input_sequence'];control('恢复已接通选项默认值');wait(lambda:state()['input_sequence']>before);assert state()['config']==defaults
    close_config();close_title();launch();assert state()['config']==defaults;close_config();close_title()
    report=dict(execution='native ARM64',apk_sha256=apk_sha,serial=a.serial,runs=[first_run,restart_run,reset_run],original_sound_page_pixels_equal=True,changed=desired,defaults=defaults,persisted_restart=True,defaults_persisted=True,story_audio=audio,story_text=text,voice_gain_verified=True,voice_stop_verified=True,pages=pages,vm_live_bytes=0,scope='original settings assets; seven supported preferences; other controls and sample text not implemented; no original config.sav compatibility')
    Path(a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(json.dumps(report,ensure_ascii=False))

if __name__=='__main__':main()
