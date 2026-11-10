"""Real BLESS DAT/SAF story-settings regression. No CG/recollection inspection."""
import argparse,hashlib,io,json,re,subprocess,time,uuid
from pathlib import Path
import xml.etree.ElementTree as ET
import numpy as np
from PIL import Image

def main():
    p=argparse.ArgumentParser()
    for key in ('adb','serial','reference','work','output'):p.add_argument('--'+key,required=True)
    p.add_argument('--text-style-check',action='store_true')
    a=p.parse_args();work=Path(a.work);work.mkdir(parents=True,exist_ok=True)
    pkg='org.nekopack.probe';run='';runs=[];retries=[];transport_retries=[]
    def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=45)
    def read(name):
        try:return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
        except subprocess.TimeoutExpired:
            transport_retries.append(dict(run=run,file=name))
            return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
    def obj(name):
        raw=read(name)
        if not raw.startswith(b'{'):return None
        try:return json.loads(raw)
        except json.JSONDecodeError:return None
    def lines(name):
        raw=read(name)
        return [json.loads(x) for x in raw.splitlines() if x.startswith(b'{')]
    def wait(fn):
        until=time.monotonic()+120
        while time.monotonic()<until:
            error=read('error.txt');assert b'No such file' in error,error
            result=fn()
            if result:return result
            time.sleep(.1)
        raise AssertionError('story settings timeout')
    def bounds(label,attr='text'):
        for _ in range(5):
            dump=subprocess.check_output([a.adb,'-s',a.serial,'shell','uiautomator','dump','/sdcard/neko-story-settings.xml'],stderr=subprocess.STDOUT,timeout=45)
            if b'dumped to' not in dump:retries.append(dump.decode(errors='replace'));continue
            nodes=[n for n in ET.fromstring(adb('shell','cat','/sdcard/neko-story-settings.xml')).iter('node') if n.get(attr)==label and n.get('enabled')=='true']
            if nodes:return list(map(int,re.findall(r'\d+',nodes[0].get('bounds'))))
            time.sleep(.2)
        raise AssertionError('unavailable control: '+label)
    def control(label):
        l,t,r,b=bounds(label);adb('shell','input','tap',str((l+r)//2),str((t+b)//2))
    def point(x,y,label='原版设置页'):
        l,t,r,b=bounds(label,'content-desc');scale=min((r-l)/1280,(b-t)/720)
        return str(round(l+(r-l-1280*scale)/2+x*scale)),str(round(t+(b-t-720*scale)/2+y*scale))
    def state():return obj('settings-presented.json')
    def area(key):
        s=state();x,y,w,h=s['areas'][key];adb('shell','input','tap',*point(x+w/2,y+h/2))
        wait(lambda:state()['input_sequence']>=s['input_sequence']+2)
    def page(n):
        while state()['page']!=n:area('= PAGE++' if state()['page']<n else '= PAGE--')
    def set_value(name,value,maximum=100):
        if value in (0,maximum):area('* '+name+('--' if value==0 else '++'))
        else:
            s=state();x,y,w,h=s['areas']['* '+name]
            adb('shell','input','tap',*point(x+w*(value+.5)/maximum,y+h/2))
            wait(lambda:state()['input_sequence']>=s['input_sequence']+2)
        assert state()['config'][name]==value,(name,state()['config'])
    def launch(probe,title_settings=False):
        nonlocal run
        run='story_settings_'+uuid.uuid4().hex;runs.append(run)
        adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','title_game','0','--el','settings_probe_sequence',str(probe),'--ez','lifecycle_probe','true','--es','run_id',run)
        title=wait(lambda:obj('title-presented.json'));x,y,w,h=next(b['bounds'] for b in title['buttons'] if b['id']==(3 if title_settings else 1))
        adb('shell','input','tap',*point(x+w/2,y+h/2,'游戏标题菜单'))
    def open_menu():
        old=state()['input_sequence'];control('原版设置');wait(lambda:state()['input_sequence']>old)
    def close_menu():
        old=len(lines('story-settings-events.jsonl'));adb('shell','input','keyevent','KEYCODE_BACK')
        wait(lambda:len(lines('story-settings-events.jsonl'))>old)
    def finish():control('结束运行');wait(lambda:b'VM live bytes=0' in read('closed.txt'))
    def baseline(n):
        d=wait(lambda:obj(f'page-{n}.json'));ref=Path(a.reference)/f'{n-1}.json'
        assert d==json.loads(ref.read_text(encoding='utf8'))
        assert read(f'page-{n}.png')==ref.with_suffix('.png').read_bytes()
    def snapshot(s):return {k:s[k] for k in ('story_sequence','story_visible','story_elapsed_ms','motion_elapsed_ms','story_steps')}
    def background_check():
        before=snapshot(state());events=len(lines('story-settings-events.jsonl'))
        adb('shell','input','keyevent','KEYCODE_HOME');time.sleep(2)
        paused=[x for x in lines('audio-lifecycle.jsonl') if x['event'].startswith('paused_stable_')][-2:]
        assert len(paused)==2
        for kind,v in paused[0]['channels'].items():
            w=paused[1]['channels'][kind];assert not v['playing'] and not w['playing']
            assert v['player_id']==w['player_id'] and abs(v['position']-w['position'])<=30
        adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity');bounds('原版设置页','content-desc')
        assert snapshot(state())==before and len(lines('story-settings-events.jsonl'))==events
        return paused
    machine=adb('shell','uname','-m').decode().strip();bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip()
    assert machine=='x86_64' and bridge not in ('','0')
    apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:');sha=adb('shell','sha256sum',apk).decode().split()[0]
    assert sha==json.loads(Path('.local/android-probe/build.json').read_text())['sha256']['nekopack-arm64-probe.apk']
    text_styles={}
    if a.text_style_check:
        def preview(label):
            raw=read('settings.png');image=Image.open(io.BytesIO(raw));x,y,w,h=state()['preview']['bounds']
            crop=np.array(image)[y:y+h,x:x+w];assert crop[:,:,:3].max()>200
            (work/(label+'.png')).write_bytes(raw)
            return dict(sha256=hashlib.sha256(crop.tobytes()).hexdigest(),white=int(np.all(crop[:,:,:3]>200,axis=2).sum()))
        launch(1,True);wait(state);page(2);original=state()['config'];title_preview=preview('title-preview')
        before=state()['input_sequence'];adb('shell','input','keyevent','KEYCODE_BACK')
        wait(lambda:obj('title-presented.json')['input_sequence']>before)
        title=obj('title-presented.json');x,y,w,h=next(b['bounds'] for b in title['buttons'] if b['id']==1)
        adb('shell','input','tap',*point(x+w/2,y+h/2,'游戏标题菜单'));wait(lambda:state().get('story_sequence')==1)
        page(2);assert preview('story-preview')==title_preview
        origins=None;style_results={}
        for bold,border in ((0,1),(0,0),(1,1),(1,0)):
            area('= BOLD#'+str(bold));area('= BORDER#'+str(border))
            assert state()['config']['Bold']==bold and state()['config']['Border']==border
            sample=preview(f'preview-{bold}-{border}');events=len(lines('text-events.jsonl'))
            close_menu();wait(lambda:len(lines('text-events.jsonl'))>events and obj('text-1.json')['complete'])
            baseline(1);raw=read('dialogue-1.png');image=np.array(Image.open(io.BytesIO(raw)))
            geometry=obj('dialogue-1.json')
            if origins is None:origins=geometry['origins']
            assert geometry['origins']==origins,'synthetic weight must preserve layout advances'
            style_results[f'{bold}-{border}']=dict(preview=sample,dialogue_sha256=hashlib.sha256(raw).hexdigest(),dialogue_white=int(np.all(image[:,:,:3]>200,axis=2).sum()))
            (work/f'dialogue-{bold}-{border}.png').write_bytes(raw)
            open_menu();page(2)
        assert len({r['preview']['sha256'] for r in style_results.values()})==4
        assert len({r['dialogue_sha256'] for r in style_results.values()})==4
        for border in (0,1):
            assert style_results[f'1-{border}']['preview']['white']>style_results[f'0-{border}']['preview']['white']
            assert style_results[f'1-{border}']['dialogue_white']>style_results[f'0-{border}']['dialogue_white']
        opacity=[]
        for value in (0,127,255):
            set_value('BoxOpacity',value,255);opacity.append(preview('preview-opacity-'+str(value))['sha256'])
        assert len(set(opacity))==3
        set_value('BoxOpacity',original['BoxOpacity'],255)
        close_menu();finish()
        launch(1);wait(state)
        assert state()['config']['Bold']==1 and state()['config']['Border']==0
        assert obj('dialogue-font.json')['bold']==1 and obj('dialogue-font.json')['border']==0
        page(2);assert preview('restarted-preview')==style_results['1-0']['preview']
        area('= BOLD#'+str(original['Bold']));area('= BORDER#'+str(original['Border']))
        assert state()['config']==original
        close_menu();wait(lambda:obj('text-1.json')['complete']);finish()
        text_styles=dict(combinations=style_results,title_story_preview_equal=True,preview_opacity_hashes=opacity,restart_styles_preserved=True,layout_advances_unchanged=True,original_preferences_restored=original)
        print('four text styles, live preview, opacity and restart passed',flush=True)
    launch(1);wait(state);initial=state()['config'];frozen=snapshot(state())
    assert initial['TextSpeed']<100, 'partial-text regression requires non-instant text; restore pre-test configuration first'
    (work/('initial-config-'+run+'.json')).write_text(json.dumps(initial),encoding='utf8')
    assert frozen['story_visible']>0
    baseline(1);paused=background_check()
    page(2);set_value('TextSpeed',100);set_value('BoxOpacity',0,255)
    page(3);set_value('VolVO',0);set_value('VolBGM',0)
    page(4);area('= KeepVoice#0')
    assert snapshot(state())==frozen
    page(5);original_bindings=[x['action'] for x in state()['shortcuts']]
    area('* F3');assert state()['editing_slot']==3
    before_cancel=len(lines('story-settings-events.jsonl'))
    adb('shell','input','keyevent','KEYCODE_BACK');wait(lambda:not state()['editing_slot']);assert snapshot(state())==frozen
    assert len(lines('story-settings-events.jsonl'))==before_cancel
    assert [x['action'] for x in state()['shortcuts']]==original_bindings
    (work/'settings-screen.png').write_bytes(adb('exec-out','screencap','-p'))
    count=len(lines('text-events.jsonl'));close_menu()
    wait(lambda:len(lines('text-events.jsonl'))>count)
    resumed=lines('text-events.jsonl')[count:][0]
    assert resumed['sequence']==1 and resumed['elapsed_ms']-frozen['story_elapsed_ms']<200 and resumed['visible']>=frozen['story_visible'] and not resumed['advance'] and not resumed['complete']
    wait(lambda:obj('text-1.json')['complete']);baseline(1)
    full=np.array(Image.open(io.BytesIO(read('dialogue-1.png'))));assert full[-40:,:,3].max()==0 and full[:,:,3].max()>0
    closed=[x for x in lines('story-settings-events.jsonl') if x['event']=='closed'][-1]
    opened=[x for x in lines('story-settings-events.jsonl') if x['event']=='opened'][-1]
    assert closed['active_ms']-opened['active_ms']<200
    resumed_audio=[x for x in lines('audio-lifecycle.jsonl') if x['event']=='resuming'][-1]
    assert resumed_audio['channels']['voice']['gain']==0
    gains=lines('audio-gain-events.jsonl');assert any(x['kind']=='voice' and x['category_percent']==0 for x in gains)
    print('partial dialogue, opacity refresh and background modal passed',flush=True)
    control('继续');wait(lambda:obj('text-2.json'));baseline(2)
    second=[x for x in lines('text-events.jsonl') if x['sequence']==2][0];assert second['elapsed_ms']==0 and second['complete']
    control('继续');wait(lambda:obj('text-3.json'));baseline(3)
    # Reopening must preserve complete text and restore opacity from original pixels.
    open_menu();page(2);set_value('TextSpeed',initial['TextSpeed']);set_value('BoxOpacity',initial['BoxOpacity'],255)
    page(3);set_value('VolVO',initial['VolVO']);set_value('VolBGM',initial['VolBGM'])
    page(4);area('= KeepVoice#'+str(initial['KeepVoice']));assert state()['config']==initial
    count=len(lines('text-events.jsonl'));close_menu();wait(lambda:len(lines('text-events.jsonl'))>count)
    assert obj('text-3.json')['complete'];full=np.array(Image.open(io.BytesIO(read('dialogue-3.png'))));assert full[-40:,:,3].max()>0
    finish()
    # BGM and concurrent character motion occur later in the same ordinary route.
    launch(56)
    wait(lambda:obj('text-1.json') and obj('text-1.json')['complete'])
    l,t,r,b=bounds('继续');continue_point=(str((l+r)//2),str((t+b)//2))
    for n in range(1,56):
        wait(lambda:obj(f'text-{n}.json') and obj(f'text-{n}.json')['complete']);baseline(n)
        if n==13:
            control('原版设置');wait(state);bgm=background_check()
            before_bgm=bgm[-1]['channels']['bgm'];assert not before_bgm['playing']
            close_menu()
            after_bgm=[x for x in lines('audio-lifecycle.jsonl') if x['event']=='resumed'][-1]['channels']['bgm']
            assert after_bgm['playing'] and before_bgm['player_id']==after_bgm['player_id'] and before_bgm['token']==after_bgm['token']
            assert abs(after_bgm['position']-before_bgm['position'])<200
            print('BGM modal pause/resume passed',flush=True)
        if n==13:control('继续')
        else:adb('shell','input','tap',*continue_point)
        if n==12:
            wait(lambda:obj('movie-8-info.json'));control('返回剧情')
        if n%10==0:print('story page',n,flush=True)
    wait(lambda:state() and state()['story_sequence']==56);motion_before=snapshot(state())
    assert 0<motion_before['motion_elapsed_ms']<250
    background_check();assert snapshot(state())==motion_before
    close_menu();wait(lambda:any(x['event']=='finished' and x['sequence']==56 for x in lines('motion-events.jsonl')))
    baseline(56);wait(lambda:obj('text-56.json')['complete'])
    control('继续');wait(lambda:obj('text-57.json') and obj('text-57.json')['complete']);baseline(57)
    finish()
    launch(1);wait(state)
    assert state()['config']==initial
    close_menu();wait(lambda:obj('text-1.json') and obj('text-1.json')['complete'])
    assert b'No such file' in read('lifecycle-closed.json')
    baseline(1);finish()
    report=dict(execution='translated ARM64 on x86_64',machine=machine,native_bridge=bridge,apk_sha256=sha,runs=runs,partial_dialogue=frozen,resumed_dialogue=resumed,settings_time_excluded=True,opacity_applies_to_current_page=True,text_speed_applies_next_page=True,other_settings_restored=initial,background_keeps_modal_paused=True,bgm_pause_resume=True,motion_checkpoint=motion_before,motion_finishes_after_return=True,story_pages_verified=57,closed_vm_live_bytes=0,ui_retries=retries,read_transport_retries=transport_retries,back_keeps_activity_alive=True,back_cancels_popup_before_leaving_settings=True,back_returns_to_same_dialogue=True,bgm_same_player_resumed=True,new_gain_applied_before_resume=True,intro_return_button_used=True,text_styles=text_styles)
    Path(a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(json.dumps(report,ensure_ascii=False))
if __name__=='__main__':main()
