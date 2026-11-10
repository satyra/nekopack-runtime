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
    p.add_argument('--auto-check',action='store_true')
    p.add_argument('--skip-check',action='store_true')
    p.add_argument('--shortcut-check',action='store_true')
    p.add_argument('--hide-check',action='store_true')
    p.add_argument('--menu-check',action='store_true')
    a=p.parse_args();work=Path(a.work);work.mkdir(parents=True,exist_ok=True)
    pkg='org.nekopack.probe';run='';runs=[];retries=[];transport_retries=[];history_profile=''
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
    def wait(fn,timeout=120):
        until=time.monotonic()+timeout
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
    def launch(probe,title_settings=False,hide_probe=-1,menu_probe=-1):
        nonlocal run
        run='story_settings_'+uuid.uuid4().hex;runs.append(run)
        adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','title_game','0','--el','settings_probe_sequence',str(probe),'--el','hide_probe_sequence',str(hide_probe),'--el','menu_probe_sequence',str(menu_probe),'--ez','lifecycle_probe','true','--es','run_id',run,*(['--es','read_history_probe',history_profile] if history_profile else []))
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
    menu_results={}
    def menu_state():return obj('story-menu-presented.json')
    def menu_action(name):
        old=menu_state();button=next(b for b in old['buttons'] if b['action']==name);assert button['enabled']
        x,y,w,h=button['bounds'];adb('shell','input','tap',*point(x+w/2,y+h/2,'原版剧情菜单'))
        return wait(lambda:menu_state() if menu_state()['kind']=='story_menu_closed' or (menu_state()['input_sequence']>old['input_sequence']+1 and menu_state()['pointer_kind']==2 and menu_state()['accepting']) else None)
    def menu_cancel():
        before=menu_state()['checkpoint'];count=len(lines('text-events.jsonl'))
        adb('shell','input','keyevent','KEYCODE_BACK');wait(lambda:menu_state()['kind']=='story_menu_closed')
        assert menu_state()['checkpoint']==before
        wait(lambda:len(lines('text-events.jsonl'))>count)
        resumed=lines('text-events.jsonl')[count];assert resumed['sequence']==before['sequence'] and not resumed['advance']
        assert b'No such file' in read('lifecycle-closed.json')
    if a.menu_check:
        launch(-1,menu_probe=1);wait(lambda:menu_state() and menu_state()['kind']=='story_menu')
        first=menu_state();assert first['checkpoint']['visible']>0 and not obj('text-1.json')['complete']
        route=obj('page-1.json')['labels']['in_route']
        assert all(b['enabled']==route for b in first['buttons'] if b['action'] in ('LOG','SAVE','QSAVE'))
        count=len(lines('text-events.jsonl'));checkpoint=first['checkpoint'];time.sleep(1)
        assert len(lines('text-events.jsonl'))==count
        # A drag leaving a button must not activate it after returning.
        b=next(b for b in first['buttons'] if b['action']=='CONFIG');x,y,w,h=b['bounds']
        start=point(x+w/2,y+h/2,'原版剧情菜单');end=point(0,0,'原版剧情菜单')
        adb('shell','input','swipe',*start,*end,'400');wait(lambda:menu_state()['input_sequence']>=2 and menu_state()['pointer_kind']==2 and menu_state()['accepting'])
        assert menu_state()['kind']=='story_menu' and menu_state()['checkpoint']==checkpoint
        for action in ('SAVE','LOAD','RESET'):
            unsupported=menu_action(action);assert unsupported['unsupported']==action and unsupported['checkpoint']==checkpoint
        adb('shell','input','keyevent','KEYCODE_F1');time.sleep(.3);assert menu_state()['kind']=='story_menu' and state() is None
        adb('shell','input','keyevent','KEYCODE_HOME');time.sleep(2)
        adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity');bounds('原版剧情菜单','content-desc')
        assert menu_state()['checkpoint']==checkpoint and len(lines('text-events.jsonl'))==count
        menu_cancel();baseline(1);assert obj('text-2.json') is None
        old=len(lines('story-menu-events.jsonl'));adb('shell','input','keyevent','KEYCODE_F2')
        wait(lambda:len(lines('story-menu-events.jsonl'))>old and menu_state()['kind']=='story_menu')
        menu_action('CONFIG');wait(state);assert state()['story_sequence']==1
        saved=state()['config'];page(2);set_value('TextSpeed',100);set_value('TextSpeed',saved['TextSpeed']);close_menu()
        assert obj('text-2.json') is None
        control('继续');wait(lambda:obj('text-2.json'));baseline(2)
        control('剧情菜单');wait(lambda:menu_state()['kind']=='story_menu');menu_action('QUIT')
        wait(lambda:b'VM live bytes=0' in read('closed.txt'))
        menu_results=dict(partial_checkpoint=checkpoint,original_buttons=first['buttons'],drag_cancel=True,unsupported_save_load_reset_preserve_page=True,function_keys_blocked=True,background_preserves_menu=True,back_consumed=True,f2_opens=True,config_returns_same_page=True,next_after_close=True,menu_quit_closes_vm=True,closed_vm_live_bytes=0)
        print('original story menu, F2, unsupported action, nested config and Back passed',flush=True)
    hide_results={}
    hidden_label='隐藏对话框；点击恢复'
    def hide_state():return obj('hide-state.json')
    def hidden_pixels():
        l,t,r,b=bounds(hidden_label,'content-desc')
        actual=Image.open(io.BytesIO(adb('exec-out','screencap','-p'))).convert('RGB').crop((l,t,r,b))
        raw=Image.open(io.BytesIO(read('hide-base.png'))).convert('RGBA')
        expected=Image.new('RGBA',raw.size,(0,0,0,255));expected.alpha_composite(raw)
        x=np.array(actual).astype(int);y=np.array(expected.convert('RGB')).astype(int)
        assert x.shape==y.shape and np.abs(x-y).max()<=1,'hidden viewport differs from frozen scene snapshot'
        return hashlib.sha256(actual.tobytes()).hexdigest()
    def hide_restore(method):
        before=hide_state();assert before['hidden'];events=len(lines('text-events.jsonl'))
        if method=='tap':
            l,t,r,b=bounds(hidden_label,'content-desc');adb('shell','input','tap',str((l+r)//2),str((t+b)//2))
        else:adb('shell','input','keyevent',method)
        wait(lambda:hide_state() and not hide_state()['hidden'])
        after=hide_state();assert all(before[k]==after[k] for k in ('sequence','steps','visible','elapsed_ms'))
        wait(lambda:len(lines('text-events.jsonl'))>events)
        resumed=lines('text-events.jsonl')[events]
        assert resumed['sequence']==before['sequence'] and not resumed['advance'] and not resumed['auto'] and not resumed['skip']
        assert b'No such file' in read('lifecycle-closed.json')
        return resumed
    if a.hide_check:
        launch(-1,hide_probe=1);wait(lambda:hide_state() and hide_state()['hidden'])
        initial=hide_state();assert initial['visible']>0 and not obj('text-1.json')['complete']
        events=len(lines('text-events.jsonl'));first_hash=hidden_pixels();time.sleep(2)
        assert hidden_pixels()==first_hash and len(lines('text-events.jsonl'))==events
        resumed=hide_restore('KEYCODE_BACK');assert resumed['elapsed_ms']-initial['elapsed_ms']>=2000
        baseline(1);assert obj('text-2.json') is None
        control('原版设置');wait(state);page(5);bindings=[x['action'] for x in state()['shortcuts']]
        area('* F3');area('= HIDE');close_menu()
        old=len(lines('hide-events.jsonl'));adb('shell','input','keyevent','KEYCODE_F3')
        wait(lambda:len(lines('hide-events.jsonl'))>old and hide_state()['hidden'])
        key_events=len(lines('shortcut-events.jsonl'));adb('shell','input','keyevent','KEYCODE_F1');time.sleep(.5)
        assert len(lines('shortcut-events.jsonl'))==key_events and hide_state()['hidden']
        hide_restore('KEYCODE_SPACE');assert obj('text-2.json') is None
        control('隐藏对白');wait(lambda:hide_state()['hidden']);hide_restore('tap')
        control('隐藏对白');wait(lambda:hide_state()['hidden']);hide_restore('KEYCODE_ENTER')
        assert obj('text-2.json') is None
        open_menu();page(5);area('* F3');area('= '+(bindings[0] or 'DELETE'))
        assert [x['action'] for x in state()['shortcuts']]==bindings
        close_menu();finish()
        hide_results=dict(partial_text=initial,restored_text=resumed,frozen_viewport_sha256=first_hash,back_space_touch_enter_restore=True,hide_binding_executes=True,function_keys_blocked_while_hidden=True,original_binding_restored=True)
        print('hidden partial dialogue, frozen scene, Back/Space/touch and HIDE shortcut passed',flush=True)
    shortcut_results={}
    if a.shortcut_check:
        def key(slot,expected):
            count=len(lines('shortcut-events.jsonl'))
            adb('shell','input','keyevent','KEYCODE_F'+str(slot))
            events=wait(lambda:lines('shortcut-events.jsonl') if len(lines('shortcut-events.jsonl'))>count else None)
            result=events[-1];assert result['slot']==slot and result['delivery']==expected,result
            return result
        launch(1);wait(state);shortcut_config=state()['config'];page(5)
        original_shortcuts=[x['action'] for x in state()['shortcuts']]
        edits={3:'AUTO',4:'SKIP',5:'SAVE',6:'QUIT',12:''}
        for slot,action in edits.items():area('* F'+str(slot));area('= '+(action or 'DELETE'))
        page(1);area('= SkipNoRead#1');close_menu();wait(lambda:obj('text-1.json')['complete'])
        assert key(3,'applied')['auto'];assert not key(3,'applied')['auto']
        time.sleep(.3);before_keys=len([x for x in lines('text-events.jsonl') if x['advance']])
        assert key(5,'unsupported')['action']=='SAVE';assert key(12,'unbound')['action']==''
        assert len([x for x in lines('text-events.jsonl') if x['advance']])==before_keys,'unsupported/unbound input advanced the story'
        assert key(4,'applied')['skip'];assert not key(4,'applied')['skip']
        time.sleep(.5);advances=len([x for x in lines('text-events.jsonl') if x['advance']]);time.sleep(1)
        assert len([x for x in lines('text-events.jsonl') if x['advance']])==advances
        old=state()['input_sequence'];key(1,'settings');wait(lambda:state()['input_sequence']>old)
        key_count=len(lines('shortcut-events.jsonl'));adb('shell','input','keyevent','KEYCODE_F6');time.sleep(.5)
        assert len(lines('shortcut-events.jsonl'))==key_count and b'No such file' in read('closed.txt')
        close_menu();key(6,'quit');wait(lambda:b'VM live bytes=0' in read('closed.txt'))
        launch(1);wait(state);page(5)
        for slot,action in edits.items():assert state()['shortcuts'][slot-3]['action']==action
        for slot in edits:area('* F'+str(slot));area('= '+(original_shortcuts[slot-3] or 'DELETE'))
        restored_shortcuts=[x['action'] for x in state()['shortcuts']]
        assert restored_shortcuts==original_shortcuts
        page(1);area('= SkipNoRead#'+str(shortcut_config['SkipNoRead']));assert state()['config']==shortcut_config
        close_menu();finish()
        shortcut_results=dict(original_bindings=original_shortcuts,restored_bindings=restored_shortcuts,current_binding_auto=True,current_binding_skip=True,cancel_skip_while_running=True,f1_opens_settings=True,modal_blocks_quit=True,unsupported_save_no_advance=True,unbound_no_advance=True,quit_vm_live_bytes=0,bindings_survive_restart=True,original_bindings_restored=True)
        print('real function-key dispatch, live bindings, modal boundary and VM close passed',flush=True)
    skip_results={}
    if a.skip_check:
        history_profile=uuid.uuid4().hex
        launch(-1);wait(lambda:obj('text-1.json'))
        assert not obj('text-1.json')['read']
        control('快进：关');wait(lambda:obj('text-2.json'))
        assert obj('text-1.json')['advance'] and obj('text-1.json')['skip']
        assert not obj('text-2.json')['skip'] and not obj('text-2.json')['read']
        for n in range(2,6):
            wait(lambda:obj(f'text-{n}.json') and obj(f'text-{n}.json')['complete']);control('继续')
        wait(lambda:obj('text-6.json'));assert not obj('text-6.json')['read'];finish()
        launch(-1);wait(lambda:obj('text-1.json'));assert obj('text-1.json')['read']
        control('快进：关');wait(lambda:obj('text-6.json'))
        events=lines('text-events.jsonl');advances=[x for x in events if x['advance']]
        assert [x['sequence'] for x in advances]==[1,2,3,4,5]
        assert all(x['read'] and x['skip'] and not x['click'] for x in advances)
        assert not obj('text-6.json')['skip'] and not obj('text-6.json')['read']
        voice_starts=[x for x in lines('host-events.jsonl') if x['event']=='playing' and x['kind']=='voice']
        assert len(voice_starts)==1,'read pages 4 and 5 must not start voice during skip'
        control('原版设置');wait(state);page(1);old_skip=state()['config']['SkipNoRead'];area('= SkipNoRead#1')
        assert state()['config']['SkipNoRead']==1;close_menu()
        wait(lambda:obj('text-6.json')['complete'])
        l,t,r,b=bounds('快进：关');point_skip=(str((l+r)//2),str((t+b)//2))
        adb('shell','input','tap',*point_skip)
        wait(lambda:obj('text-7.json') and obj('text-7.json')['advance'])
        assert obj('text-7.json')['skip'] and not obj('text-7.json')['read']
        adb('shell','input','tap',*point_skip)
        wait(lambda:lines('skip-controls.jsonl') and not lines('skip-controls.jsonl')[-1]['skip'])
        time.sleep(.5);count=len([x for x in lines('text-events.jsonl') if x['advance']]);time.sleep(2)
        assert len([x for x in lines('text-events.jsonl') if x['advance']])==count
        finish()
        launch(1);wait(state);assert state()['config']['SkipNoRead']==1
        page(1);area('= SkipNoRead#'+str(old_skip));close_menu();finish()
        skip_results=dict(profile=history_profile,current_unread_toggle_advances_once=True,read_pages_after_restart=5,stops_on_unread_page=6,skipped_voice_pages=[4,5],voice_starts_during_read_run=1,force_unread_page=7,manual_cancel=True,skip_preference_restart=True,original_skip_preference=old_skip)
        history_profile=''
        print('skip read-history restart, unread boundary, voice suppression and cancellation passed',flush=True)
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
    launch(56,hide_probe=56 if a.hide_check else -1)
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
            if a.menu_check:
                control('剧情菜单');wait(lambda:menu_state() and menu_state()['kind']=='story_menu')
                checkpoint=menu_state()['checkpoint'];events=len(lines('text-events.jsonl'))
                time.sleep(.5);assert len(lines('text-events.jsonl'))==events
                adb('shell','input','keyevent','KEYCODE_HOME');time.sleep(2)
                paused=[x for x in lines('audio-lifecycle.jsonl') if x['event'].startswith('paused_stable_')][-2:]
                a0,a1=[x['channels']['bgm'] for x in paused];assert not a0['playing'] and not a1['playing'] and abs(a0['position']-a1['position'])<=30
                adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity');bounds('原版剧情菜单','content-desc')
                assert menu_state()['checkpoint']==checkpoint
                resumed_audio=[x for x in lines('audio-lifecycle.jsonl') if x['event']=='resumed'][-1]['channels']['bgm']
                assert resumed_audio['playing'] and resumed_audio['player_id']==a0['player_id']
                menu_cancel();menu_results.update(bgm_background_pauses=True,bgm_same_player_resumes=True)
            if a.hide_check:
                control('隐藏对白');wait(lambda:hide_state() and hide_state()['hidden'])
                hidden_before=hide_state();before_audio=[x for x in lines('audio-lifecycle.jsonl') if x['event']=='dialogue_hidden'][-1]['channels']['bgm']
                assert before_audio['playing'];frozen_hash=hidden_pixels();time.sleep(1)
                adb('shell','input','keyevent','KEYCODE_HOME');time.sleep(2)
                paused_audio=[x for x in lines('audio-lifecycle.jsonl') if x['event'].startswith('paused_stable_')][-2:]
                a0,a1=[x['channels']['bgm'] for x in paused_audio]
                assert not a0['playing'] and not a1['playing'] and abs(a0['position']-a1['position'])<=30
                adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity')
                assert hidden_pixels()==frozen_hash and hide_state()==hidden_before
                hide_restore('KEYCODE_BACK')
                after_audio=[x for x in lines('audio-lifecycle.jsonl') if x['event']=='dialogue_restored'][-1]['channels']['bgm']
                assert after_audio['playing'] and after_audio['player_id']==before_audio['player_id'] and after_audio['token']==before_audio['token']
                assert after_audio['position']>before_audio['position']+500
                hide_results.update(bgm_continues_hidden=True,bgm_same_player=True,background_preserves_hidden=True,background_pauses_audio=True)

        if n==13:control('继续')
        else:adb('shell','input','tap',*continue_point)
        if n==12:
            wait(lambda:obj('movie-8-info.json'));control('返回剧情')
        if n%10==0:print('story page',n,flush=True)
    wait(lambda:state() and state()['story_sequence']==56);motion_before=snapshot(state())
    assert 0<motion_before['motion_elapsed_ms']<250
    background_check();assert snapshot(state())==motion_before
    close_menu();wait(lambda:any(x['event']=='finished' and x['sequence']==56 for x in lines('motion-events.jsonl')))
    if a.hide_check:
        wait(lambda:hide_state() and hide_state()['hidden'] and hide_state()['sequence']==56)
        frozen_motion_view=hidden_pixels();time.sleep(.5);assert hidden_pixels()==frozen_motion_view
        hide_restore('KEYCODE_BACK');hide_results['motion_continues_behind_frozen_snapshot']=True
    baseline(56);wait(lambda:obj('text-56.json')['complete'])
    control('继续');wait(lambda:obj('text-57.json') and obj('text-57.json')['complete']);baseline(57)
    finish()
    launch(1);wait(state)
    assert state()['config']==initial
    close_menu();wait(lambda:obj('text-1.json') and obj('text-1.json')['complete'])
    assert b'No such file' in read('lifecycle-closed.json')
    baseline(1);finish()
    auto_results={}
    if a.auto_check:
        launch(1);wait(state);auto_original=state()['config'];page(2);set_value('AutoSpeed',100)
        close_menu();finish()
        launch(56)
        wait(lambda:obj('text-1.json'))
        l,t,r,b=bounds('自动：关');auto_point=(str((l+r)//2),str((t+b)//2))
        adb('shell','input','tap',*auto_point)
        wait(lambda:any(x.get('auto') for x in lines('text-events.jsonl')))
        adb('shell','input','keyevent','KEYCODE_HOME');time.sleep(.5)
        frozen_events=lines('text-events.jsonl');time.sleep(2)
        assert lines('text-events.jsonl')==frozen_events,'auto advanced while backgrounded'
        adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity')
        wait(lambda:obj('movie-8-info.json'));control('返回剧情')
        print('auto passed first 12 pages and background freeze',flush=True)
        wait(lambda:state() and state().get('story_sequence')==56,300)
        assert state()['config']['AutoSpeed']==100,'AutoSpeed was not persisted'
        auto_frozen=snapshot(state());background_check();assert snapshot(state())==auto_frozen
        events=lines('text-events.jsonl');advances=[x for x in events if x['advance']]
        assert [x['sequence'] for x in advances]==list(range(1,56))
        assert all(x['auto'] and not x['click'] for x in advances)
        voiced=unvoiced=0
        for n in range(1,56):
            baseline(n);meta=obj(f'page-{n}.json');history=[x for x in events if x['sequence']==n];end=history[-1]
            if meta['voice']:
                voiced+=1;assert end['voice_finished']
            else:
                unvoiced+=1;complete=next(x for x in history if x['complete'])
                assert end['elapsed_ms']-complete['elapsed_ms']>501
        assert voiced and unvoiced
        completions=[x for x in lines('host-events.jsonl') if x['event']=='completed' and x['kind']=='voice']
        assert len(completions)==voiced,'each voiced advance requires a real completion callback'
        voice_next={x['sequence']:x for x in lines('voice-next-events.jsonl')}
        for n in range(1,56):
            if obj(f'page-{n}.json')['voice']:
                assert voice_next[n]['player']==0 and not voice_next[n]['playing_before']
        # The current page retains its entry delay; restored preference applies next page.
        page(2);set_value('AutoSpeed',auto_original['AutoSpeed']);close_menu()
        adb('shell','input','tap',*auto_point)
        wait(lambda:any(x['sequence']>=56 and not x['auto'] for x in lines('text-events.jsonl')))
        count=len([x for x in lines('text-events.jsonl') if x['advance']]);time.sleep(2)
        assert len([x for x in lines('text-events.jsonl') if x['advance']])==count
        finish()
        launch(1);wait(state);assert state()['config']==auto_original;close_menu();finish()
        auto_results=dict(auto_only_advances=55,voice_completion_callbacks=len(completions),voiced_pages=voiced,unvoiced_pages=unvoiced,unvoiced_delay_min_ms=502,background_freezes=True,settings_freezes=True,auto_speed_saved_and_restored=True,manual_cancel_stops_progress=True,motion_checkpoint=auto_frozen)
        print('auto playback, voice completion, settings and cancellation passed',flush=True)
    report=dict(execution='translated ARM64 on x86_64',machine=machine,native_bridge=bridge,apk_sha256=sha,runs=runs,partial_dialogue=frozen,resumed_dialogue=resumed,settings_time_excluded=True,opacity_applies_to_current_page=True,text_speed_applies_next_page=True,other_settings_restored=initial,background_keeps_modal_paused=True,bgm_pause_resume=True,motion_checkpoint=motion_before,motion_finishes_after_return=True,story_pages_verified=57,closed_vm_live_bytes=0,ui_retries=retries,read_transport_retries=transport_retries,back_keeps_activity_alive=True,back_cancels_popup_before_leaving_settings=True,back_returns_to_same_dialogue=True,bgm_same_player_resumed=True,new_gain_applied_before_resume=True,intro_return_button_used=True,text_styles=text_styles,auto_playback=auto_results,skip_playback=skip_results,shortcut_execution=shortcut_results,hide_dialogue=hide_results,story_menu=menu_results)
    Path(a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(json.dumps(report,ensure_ascii=False))
if __name__=='__main__':main()
