"""BLESS shortcut editor and all five settings pages on an explicitly identified device.

No CG/recollection contents; original media stay in the local work directory.
"""
import argparse,hashlib,io,json,re,subprocess,sys,time,uuid
from pathlib import Path
import xml.etree.ElementTree as ET
import numpy as np
from PIL import Image
from verify_shortcut_page import sprites

def main():
    p=argparse.ArgumentParser()
    for key in ('adb','serial','games','portkit','reference','work','output'):p.add_argument('--'+key,required=True)
    p.add_argument('--execution',choices=('native','translated'),required=True)
    a=p.parse_args();work=Path(a.work);work.mkdir(parents=True,exist_ok=True);pkg='org.nekopack.probe';run=''
    def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=45)
    def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
    def obj(name):
        raw=read(name)
        if not raw.startswith(b'{'):return None
        try:return json.loads(raw)
        except json.JSONDecodeError:return None
    def wait(fn):
        until=time.monotonic()+120
        while time.monotonic()<until:
            error=read('error.txt');assert b'No such file' in error,error
            result=fn()
            if result:return result
            time.sleep(.1)
        raise AssertionError('shortcut edit timeout')
    ui_retries=[]
    def bounds(label,attr='content-desc'):
        for attempt in range(5):
            dump=adb('shell','uiautomator','dump','/sdcard/neko-edit.xml')
            if b'dumped to' not in dump:
                ui_retries.append(dump.decode(errors='replace').strip());time.sleep(.2);continue
            nodes=[n for n in ET.fromstring(adb('shell','cat','/sdcard/neko-edit.xml')).iter('node') if n.get(attr)==label and n.get('enabled')=='true']
            if nodes:return list(map(int,re.findall(r'\d+',nodes[0].get('bounds'))))
            ui_retries.append('waiting for accessible control: '+label);time.sleep(.2)
        raise AssertionError('UI control unavailable: '+label)
    def point(x,y,label='原版设置页'):
        l,t,r,b=bounds(label);s=min((r-l)/1280,(b-t)/720)
        return str(round(l+(r-l-1280*s)/2+x*s)),str(round(t+(b-t-720*s)/2+y*s))
    def tap(x,y,label='原版设置页'):adb('shell','input','tap',*point(x,y,label))
    def control(label):
        x,y,r,b=bounds(label,'text');adb('shell','input','tap',str((x+r)//2),str((y+b)//2))
    def state():return obj('settings-presented.json')
    def area(name):
        before=state()['input_sequence'];x,y,w,h=state()['areas'][name];tap(x+w/2,y+h/2)
        wait(lambda:state() and state()['input_sequence']>=before+2)
    def page(n):
        while state()['page']!=n:area('= PAGE++' if state()['page']<n else '= PAGE--')
    def launch():
        nonlocal run
        run='edit_'+uuid.uuid4().hex
        adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','title_game','0','--es','run_id',run)
        title=wait(lambda:obj('title-presented.json'));x,y,w,h=next(b['bounds'] for b in title['buttons'] if b['id']==3)
        tap(x+w/2,y+h/2,'游戏标题菜单');wait(state)
    def cancel():
        before=state()['input_sequence'];control('取消选择');wait(lambda:state()['input_sequence']>before and not state()['editing_slot'])
    def back():
        before=state()['input_sequence'];control('返回标题');wait(lambda:obj('title-presented.json')['input_sequence']>before)
    def title_button(id):
        t=obj('title-presented.json');x,y,w,h=next(b['bounds'] for b in t['buttons'] if b['id']==id);tap(x+w/2,y+h/2,'游戏标题菜单')
    def close_title():back();title_button(4);wait(lambda:b'VM live bytes=0' in read('closed.txt'))
    machine=adb('shell','uname','-m').decode().strip();bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip()
    assert (machine=='aarch64' and bridge in ('0','')) if a.execution=='native' else (machine=='x86_64' and bridge not in ('0',''))
    apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:');sha=adb('shell','sha256sum',apk).decode().split()[0]
    build=json.loads(Path('.local/android-probe/build.json').read_text());assert sha==build['sha256']['nekopack-arm64-probe.apk']
    sys.path.insert(0,a.portkit)
    from portkit.neko import Archive
    sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
    archive=Archive(Path(a.games)/next(s['file'] for s in sources if s['file'].endswith('/system.dat')))
    common=sprites(archive.read('system/config.mng'))
    script=archive.read('system/config.nut').decode('cp932')
    defaults=re.findall(r'"([A-Z]+)"',re.search(r'ShortCuts\s*=\s*\[([^]]+)\]',script)[1])[2:]
    sheets={n:sprites(archive.read(f'system/config{n}.mng')) for n in range(1,6)}
    def normalized(raw):return raw[0]+' '+raw[1:].lstrip() if raw[0] in '=+@*' else raw
    sheets={n:{normalized(k):v for k,v in sheet.items()} for n,sheet in sheets.items()}
    def reference(n,config,bindings,popup=False):
        canvas=Image.new('RGBA',(1280,720),(0,0,0,255));s=sheets[n]
        def draw(item,delta=(0,0)):
            image,(x,y)=item;canvas.alpha_composite(image,(x+delta[0],y+delta[1]))
        draw(common['BASE']);draw(s['BASE'])
        values=dict(Screen=0,SkipNoRead=0,KeepSkip=0,KeepAuto=0,Movie=1,Popup=1,Effect=1,SysEffect=1,BOLD=0,BORDER=1,CenterKey=0,SpaceKey=0,MuteBGM=1,AutoSpeed=50,BoxOpacity=191,**config)
        for key in sorted(s):
            if not key.startswith('= '):continue
            name=key[2:]
            if n==5 and not (name in ('CLOSE','DEFAULT') or name.startswith(('CenterKey#','SpaceKey#'))):continue
            selected=False
            if '#' in name:
                field,value=name.split('#');selected=int(value)==values.get(field,1) # VoiceCheck defaults true in original ConfigDefault.
            draw(s['@ '+name] if selected else s[key])
        draw(common['= PAGE++']);draw(common['= PAGE--'])
        if n==5:
            origin=s['* F3'][1]
            for i,name in enumerate(bindings,3):
                if name:
                    anchor=s['* F'+str(i)][1];draw(s[name],(anchor[0]-origin[0],anchor[1]-origin[1]))
        for name in ('TextSpeed','AutoSpeed','BoxOpacity','VolBGM','VolVO','VolSE','VolENV','VolSYS'):
            if '* '+name in s:draw(s['@ '+name],(s['* '+name][0].width*values[name]//(255 if name=='BoxOpacity' else 100),0))
        draw(common['@ PAGE'],(0,(common['* PAGE'][0].height-common['@ PAGE'][0].height)*(n-1)//4))
        # Existing static preview policy; dynamic sample text/opacity is not claimed.
        if 'SAMPLE' in s:draw(s['SAMPLE'])
        if popup:
            draw(s['LIST'])
            for name in ('DELETE','AUTO','SKIP','LOG','HIDE','SAVE','LOAD','QSAVE','QLOAD','RESET','QUIT'):draw(s['= '+name])
        return canvas
    def pixels(name,expected):
        raw=read('settings.png');assert np.array_equal(np.array(Image.open(io.BytesIO(raw))),np.array(expected)),name
        (work/(name+'.png')).write_bytes(raw);bounds('原版设置页');(work/(name+'-screen.png')).write_bytes(adb('exec-out','screencap','-p'))
        return hashlib.sha256(raw).hexdigest()
    launch();runs=[run];initial=state()['config'];checked={}
    for n in range(1,6):
        page(n);checked[str(n)]=pixels('page'+str(n),reference(n,initial,defaults));print('page',n,'pixels verified',flush=True)
    original=[s['action'] for s in state()['shortcuts']];assert original==defaults
    area('* F3');assert state()['editing_slot']==3
    pixels('popup',reference(5,initial,original,True));assert '* F4' not in state()['areas'] and '= PAGE++' not in state()['areas']
    # Drag-out cancels a choice, while keeping the picker open.
    x,y,w,h=state()['areas']['= SAVE'];down=point(x+w/2,y+h/2);outside=point(50,50);seq=state()['input_sequence']
    adb('shell','input','swipe',*down,*outside,'350');wait(lambda:state()['input_sequence']>=seq+3)
    assert state()['editing_slot']==3 and [s['action'] for s in state()['shortcuts']]==original
    adb('shell','input','keyevent','KEYCODE_HOME');time.sleep(.5)
    adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity');bounds('原版设置页');assert state()['editing_slot']==3
    cancel();assert [s['action'] for s in state()['shortcuts']]==original
    for slot in range(3,13):area('* F'+str(slot));assert state()['editing_slot']==slot;cancel()
    # Round-trip all supported actions through actual popup hits.
    for name in ('AUTO','SKIP','LOG','HIDE','SAVE','LOAD','QSAVE','QLOAD','RESET','QUIT'):
        area('* F3');area('= '+name);assert not state()['editing_slot'] and state()['shortcuts'][0]['action']==name
    area('* F3');area('= AUTO');area('* F12');area('= DELETE')
    changed=[s['action'] for s in state()['shortcuts']];assert changed[0]=='AUTO' and changed[-1]==''
    pixels('edited',reference(5,initial,changed));assert state()['config']==initial
    close_title();launch();runs.append(run);page(5)
    assert [s['action'] for s in state()['shortcuts']]==changed and state()['config']==initial
    pixels('reloaded',reference(5,initial,changed))
    # Return to the existing story; binding edits must not alter scenario execution.
    back();title_button(1);pages=[]
    for n in range(1,4):
        data=wait(lambda:obj(f'page-{n}.json'));wait(lambda:obj(f'text-{n}.json') and obj(f'text-{n}.json')['complete'])
        ref=Path(a.reference)/f'{n-1}.json';assert data==json.loads(ref.read_text(encoding='utf8'));assert read(f'page-{n}.png')==ref.with_suffix('.png').read_bytes();pages.append(n)
        if n<3:control('继续')
    control('结束运行');wait(lambda:b'VM live bytes=0' in read('closed.txt'))
    launch();runs.append(run);page(5)
    # Restore only edited slots, preserving the seven existing preferences.
    area('* F3');area('= '+original[0]);area('* F12');area('= '+original[-1]);assert state()['config']==initial
    close_title();launch();page(5);assert [s['action'] for s in state()['shortcuts']]==original
    report=dict(execution='native ARM64' if a.execution=='native' else 'translated ARM64 on x86_64',machine=machine,native_bridge=bridge,apk_sha256=sha,runs=runs,all_five_pages=checked,popup_pixels_equal=True,all_ten_slots_checked=True,all_ten_actions_checked=True,clear_persists=True,cancel_and_drag_out_preserve=True,modal_blocks_underlying_controls=True,home_restores_popup=True,restart_bindings=changed,other_preferences_unchanged=initial,restored_bindings=original,story_pages=pages,closed_vm_live_bytes=0,ui_retries=ui_retries,scope='binding edit/storage only; not a claim of executing all keyboard actions; static sample preview on page2')
    Path(a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(json.dumps(report,ensure_ascii=False))

if __name__=='__main__':main()
