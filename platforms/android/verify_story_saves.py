"""BLESS raw-DAT/SAF save/load regression; isolated runtime slots, no gallery access."""
import argparse,hashlib,json,re,subprocess,time,uuid
from pathlib import Path
import xml.etree.ElementTree as ET

def main():
    p=argparse.ArgumentParser()
    for key in ('adb','serial','reference','output'):p.add_argument('--'+key,required=True)
    p.add_argument('--quick-check',action='store_true')
    p.add_argument('--log-check',action='store_true')
    p.add_argument('--legacy-report')
    p.add_argument('--restore-system-volume',type=int,choices=range(101),help='Restore a known pre-test value after an interrupted previous run')
    a=p.parse_args();pkg='org.nekopack.probe';run='';runs=[];profile='save_'+uuid.uuid4().hex;results=[]
    def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=60)
    def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
    def obj(name):
        raw=read(name)
        try:return json.loads(raw)
        except json.JSONDecodeError:return None
    def lines(name):return [json.loads(x) for x in read(name).splitlines() if x.startswith(b'{')]
    def wait(fn,timeout=120):
        end=time.monotonic()+timeout
        while time.monotonic()<end:
            error=read('error.txt');assert b'No such file' in error,error
            value=fn()
            if value:return value
            time.sleep(.15)
        raise AssertionError('save regression timeout: '+run)
    def nodes():
        for _ in range(5):
            result=adb('shell','uiautomator','dump','/sdcard/neko-save.xml')
            if b'dumped to' in result:return list(ET.fromstring(adb('shell','cat','/sdcard/neko-save.xml')).iter('node'))
        raise AssertionError('UI hierarchy unavailable')
    def bounds(label,attr='text'):
        choices=[n for n in nodes() if n.get(attr)==label and n.get('enabled')=='true'];assert choices,label
        return list(map(int,re.findall(r'\d+',choices[0].get('bounds'))))
    def click(label,attr='text'):
        l,t,r,b=bounds(label,attr);adb('shell','input','tap',str((l+r)//2),str((t+b)//2))
    def point(label,rect):
        l,t,r,b=bounds(label,'content-desc');x,y,w,h=rect;scale=min((r-l)/1280,(b-t)/720)
        adb('shell','input','tap',str(round(l+(r-l-1280*scale)/2+(x+w/2)*scale)),str(round(t+(b-t-720*scale)/2+(y+h/2)*scale)))
    def launch(menu_probe=-1,title_load=False):
        nonlocal run
        run='story_saves_'+uuid.uuid4().hex;runs.append(run)
        adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','title_game','0','--es','run_id',run,'--es','save_probe',profile,'--es','read_history_probe',profile,'--ez','lifecycle_probe','true','--el','menu_probe_sequence',str(menu_probe))
        title=wait(lambda:obj('title-presented.json'));point('游戏标题菜单',next(b['bounds'] for b in title['buttons'] if b['id']==(2 if title_load else 1)))
        if title_load:wait(lambda:obj('save-ui-presented.json'))
        else:wait(lambda:obj('page-1.json'))
    def page(n):return wait(lambda:obj(f'text-{n}.json') and obj(f'text-{n}.json')['complete'])
    def baseline(token,index):
        value=wait(lambda:obj(f'page-{token}.json'));expected=json.loads((Path(a.reference)/f'{index-1}.json').read_text(encoding='utf8'))
        value=dict(value);value['sequence']=index
        assert value==expected,(token,index,value,expected)
        assert read(f'page-{token}.png')==(Path(a.reference)/f'{index-1}.png').read_bytes()
        return hashlib.sha256(read(f'page-{token}.png')).hexdigest()
    def menu(action,already=False):
        if not already:click('剧情菜单')
        state=wait(lambda:obj('story-menu-presented.json') and obj('story-menu-presented.json')['kind']=='story_menu' and obj('story-menu-presented.json'))
        button=next(b for b in state['buttons'] if b['action']==action);assert button['enabled'] and button['supported']
        old=len(lines('save-events.jsonl'));point('原版剧情菜单',button['bounds'])
        wait(lambda:len(lines('save-events.jsonl'))>old and obj('save-state.json')['event']=='open')
        return state
    def save_button(id):
        state=wait(lambda:obj('save-ui-presented.json') and obj('save-ui-presented.json')['accepting'] and obj('save-ui-presented.json'))
        button=next(b for b in state['buttons'] if b['id']==id);assert button['enabled'],button
        point('原版存读档页',button['bounds'])
        return state
    def save_control(id):
        old=save_button(id)
        wait(lambda:obj('save-ui-presented.json')['input_sequence']>=old['input_sequence']+2 and obj('save-ui-presented.json')['accepting'])
    def slot(id,event):
        old=len(lines('save-events.jsonl'));save_button(id)
        return wait(lambda:len(lines('save-events.jsonl'))>old and obj('save-state.json')['event']==event and obj('save-state.json'))
    def quick(action,event,touch=False):
        old=len(lines('save-events.jsonl'))
        if touch:
            state=wait(lambda:obj('side-menu-presented.json'))
            old_render=state['render_sequence'];point('原版右侧悬浮菜单',state['bounds'])
            state=wait(lambda:obj('side-menu-presented.json')['render_sequence']>old_render and obj('side-menu-presented.json'))
            assert state['expanded']
            button=next(b for b in state['buttons'] if b['action']==action);assert button['enabled'] and button['supported']
            point('原版右侧悬浮菜单',button['bounds'])
        else:adb('shell','input','keyevent','KEYCODE_F7' if action=='QSAVE' else 'KEYCODE_F8')
        value=wait(lambda:len(lines('save-events.jsonl'))>old and obj('save-state.json')['event']==event and obj('save-state.json'))
        bounds('剧情菜单') # operation finished and controls restored
        return value
    def quick_names():
        return json.loads(adb('exec-out','run-as',pkg,'cat',f'files/saves/{directory}/quick-index.json'))
    def quick_hashes():
        return [hashlib.sha256(adb('exec-out','run-as',pkg,'cat',f'files/saves/{directory}/{name}')).hexdigest() for name in quick_names()]
    def cancel():
        adb('shell','input','keyevent','KEYCODE_BACK');wait(lambda:obj('save-state.json')['event']=='cancelled')
    def next_page(token,index):
        page(token);click('继续')
        if index==12:
            wait(lambda:any(x['event']=='started' for x in lines('movie-events.jsonl')) if b'No such file' not in read('movie-events.jsonl') else obj('movie-8-info.json'))
            click('返回剧情')
        page(token+1);baseline(token+1,index+1)
    log_results=[]
    def check_log(token,count,baseline_index,replay=False,background=False):
        before=read(f'page-{token}.json');picture=read(f'page-{token}.png');text_state=read(f'text-{token}.json');old=len(lines('log-events.jsonl'))
        adb('shell','input','keyevent','KEYCODE_F9')
        state=wait(lambda:obj('message-log-presented.json') and obj('message-log-presented.json')['story_sequence']==token and obj('message-log-presented.json'))
        frozen_text=read(f'text-{token}.json')
        assert state['count']==count and state['page']==max(0,count-4),state
        def verify(state):
            for e in state['entries']:
                index=baseline_index-count+e['index']
                expected=json.loads((Path(a.reference)/f'{index-1}.json').read_text(encoding='utf8'))
                assert (e['name'],e['voice'],e['text'])==(expected['speaker'],expected['voice'],expected['text'])
        verify(state)
        def pointer(rect):
            previous=obj('message-log-presented.json')['input_sequence'];point('原版消息日志',rect)
            return wait(lambda:obj('message-log-presented.json')['input_sequence']>=previous+2 and obj('message-log-presented.json')['accepting'] and obj('message-log-presented.json'))
        if count>4:
            state=pointer(state['previous']);assert state['page']==count-5;verify(state)
            x,y,w,h=state['scroll'];state=pointer([x,y,w,1]);assert state['page']==0;verify(state)
            state=pointer([x,y+h-1,w,1]);assert state['page']==count-4;verify(state)
        if background:
            events=read('text-events.jsonl');adb('shell','input','keyevent','KEYCODE_HOME');time.sleep(1)
            assert read('text-events.jsonl')==events
            adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity');bounds('原版消息日志','content-desc')
        played=None
        if replay:
            entry=next(e for e in state['entries'] if e['name'] and e['voice'])
            previous=len(lines('ui-audio-events.jsonl'));pointer(entry['bounds'])
            event=wait(lambda:next((e for e in lines('ui-audio-events.jsonl')[previous:] if e['host_effect']=='voice'),None))
            assert event['host_effect']=='voice' and event['name']==entry['voice'];played=event['token']
            wait(lambda:any(e['event']=='playing' and e['token']==played for e in lines('host-events.jsonl')))
        Path('.local/batch51-log.png').write_bytes(read('message-log.png'))
        assert read(f'text-{token}.json')==frozen_text
        adb('shell','input','keyevent','KEYCODE_BACK');wait(lambda:len(lines('log-events.jsonl'))>old)
        assert read(f'page-{token}.json')==before and read(f'page-{token}.png')==picture
        after_text=obj(f'text-{token}.json');before_text=json.loads(text_state)
        assert all(after_text[k]==before_text[k] for k in ('sequence','visible','text','complete','advance'))
        audio=lines('audio-lifecycle.jsonl');opened=next(e for e in reversed(audio) if e['event']=='log_open');closed=next(e for e in reversed(audio) if e['event']=='log_closed')
        if 'bgm' in opened['channels']:
            assert opened['channels']['bgm']['player_id']==closed['channels']['bgm']['player_id']
            assert closed['channels']['bgm']['playing']
        log_results.append(dict(run=run,token=token,count=count,page=baseline_index,voice_token=played,background=background,same_scene_and_dialogue=True,bgm_same_player=True))
    def system_volume(value=None):
        old=len(lines('story-settings-events.jsonl'));click('原版设置')
        wait(lambda:len(lines('story-settings-events.jsonl'))>old);state=wait(lambda:obj('settings-presented.json'));original=state['config']['VolSYS'];se=state['config']['VolSE']
        def area(key):
            state=obj('settings-presented.json');point('原版设置页',state['areas'][key]);wait(lambda:obj('settings-presented.json')['input_sequence']>=state['input_sequence']+2)
        while obj('settings-presented.json')['page']!=3:area('= PAGE++' if obj('settings-presented.json')['page']<3 else '= PAGE--')
        target=(17 if original!=17 else 23) if value is None else value
        state=obj('settings-presented.json');x,y,w,h=state['areas']['* VolSYS'];point('原版设置页',[x+w*(target+.5)/100,y,0,h])
        wait(lambda:obj('settings-presented.json')['input_sequence']>=state['input_sequence']+2)
        assert obj('settings-presented.json')['config']['VolSYS']==target and obj('settings-presented.json')['config']['VolSE']==se
        old=len(lines('story-settings-events.jsonl'));adb('shell','input','keyevent','KEYCODE_BACK');wait(lambda:len(lines('story-settings-events.jsonl'))>old)
        return original,target
    def finish():click('结束运行');wait(lambda:b'VM live bytes=0' in read('closed.txt'))
    machine=adb('shell','uname','-m').decode().strip();bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip()
    assert machine=='x86_64' and bridge not in ('','0')
    apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:');sha=adb('shell','sha256sum',apk).decode().split()[0]
    assert sha==json.loads(Path('.local/android-probe/build.json').read_text())['sha256']['nekopack-arm64-probe.apk']
    launch();page(1);baseline(1,1)
    if a.log_check:
        original_sys,test_sys=system_volume();check_log(1,1,1)
    if a.quick_check:
        quick('QLOAD','error');baseline(1,1);assert obj('page-2.json') is None
    menu('LOAD');assert not next(b for b in obj('save-ui-presented.json')['buttons'] if b['id']==1)['enabled'];cancel();page(1)
    menu('SAVE');saved=slot(1,'saved');assert saved['bytes']>13;directory=saved['directory']
    first_file=adb('exec-out','run-as',pkg,'cat',f'files/saves/{directory}/slot-001.nksv')
    assert slot(1,'saved')['bytes']==len(first_file)
    assert adb('exec-out','run-as',pkg,'cat',f'files/saves/{directory}/slot-001.nksv')==first_file
    save_control(201);assert obj('save-ui-presented.json')['page']==1
    save_control(209);assert obj('save-ui-presented.json')['page']==9
    save_control(200);assert obj('save-ui-presented.json')['page']==0
    save_control(-2);assert not obj('save-ui-presented.json')['saving']
    assert not next(b for b in obj('save-ui-presented.json')['buttons'] if b['id']==2)['enabled']
    save_control(-2);assert obj('save-ui-presented.json')['saving']
    Path('.local/batch49-populated-save.png').write_bytes(read('save-ui.png'))
    before=lines('text-events.jsonl');adb('shell','input','keyevent','KEYCODE_HOME');time.sleep(2)
    adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity');bounds('原版存读档页','content-desc');assert lines('text-events.jsonl')==before
    cancel();next_page(1,1);menu('LOAD');slot(1,'loaded');page(3);baseline(3,1);next_page(3,1)
    if a.log_check:
        gains=[e for e in lines('audio-gain-events.jsonl') if e['kind']=='system'];assert gains and all(e['category_percent']==test_sys for e in gains)
        restored_sys=original_sys if a.restore_system_volume is None else a.restore_system_volume
        system_volume(restored_sys)
        results.append(dict(case='system_volume',initial=original_sys,tested=test_sys,restored=restored_sys,se_unchanged=True))
    results.append(dict(case='first_page',new_sequence=3,next_sequence=4,background_modal=True,empty_slot_disabled=True,overwrite_exact_bytes=True,page_navigation_and_mode_switch=True));finish()
    print('first save/load, next dialogue, empty load and background passed',flush=True)
    # A separate process must read the same durable slot, without an in-memory snapshot.
    launch();page(1);menu('LOAD');slot(1,'loaded');page(2);baseline(2,1);next_page(2,1);finish()
    results.append(dict(case='process_restart',durable=True));print('process-restart load passed',flush=True)
    quick_history=[]
    launch(menu_probe=56)
    for n in range(1,56):
        page(n);baseline(n,n)
        if a.quick_check and n<=11:
            quick('QSAVE','saved',touch=n==1)
            hashes=quick_hashes();quick_history.insert(0,hashes[0])
            assert hashes==quick_history[:10]
            baseline(n,n);assert obj(f'page-{n+1}.json') is None
        if a.log_check and n==13:check_log(n,n,n)
        if n==13:
            menu('SAVE');slot(2,'saved');cancel();page(13)
        click('继续')
        if n==12:
            wait(lambda:obj('movie-8-info.json'));click('返回剧情')
        if n%10==0:print('story page',n,flush=True)
    # Menu probe starts on the motion page. Device UI latency may let the queue finish;
    # mid-segment clock continuation is separately covered by the Rust queue test.
    frozen=menu('SAVE',already=True);slot(3,'saved');cancel();page(56);baseline(56,56)
    if a.quick_check:
        quick('QSAVE','saved');hashes=quick_hashes();quick_history.insert(0,hashes[0]);assert hashes==quick_history[:10]
    if a.log_check:check_log(56,56,56,replay=True,background=True)
    next_page(56,56)
    menu('LOAD');old_audio=len(lines('host-events.jsonl'));slot(3,'loaded');page(58);baseline(58,56)
    wait(lambda:any(x['event']=='finished' and x['sequence']==58 for x in lines('motion-events.jsonl')))
    if a.log_check:check_log(58,56,56,replay=True)
    next_page(58,56)
    audio=lines('host-events.jsonl')[old_audio:];assert any(x['event']=='load_released' and x['value']==0 for x in audio)
    assert any(x['event']=='playing' and x['kind']=='bgm' for x in audio)
    assert any(x['event']=='playing' and x['kind']=='voice' for x in audio)
    results.append(dict(case='actor_queue_page',menu_checkpoint=frozen['checkpoint'],restored_sequence=58,next_sequence=59,audio_released=True,bgm_and_voice_started=True))
    print('motion checkpoint, restored scene, BGM/voice and next page passed',flush=True)
    # Change scene back to chapter entrance; source/call stack and music must match.
    menu('LOAD');slot(2,'loaded');page(60);baseline(60,13);next_page(60,13)
    # Corrupt only the isolated probe slot. Reject it before audio/VM replacement.
    original=adb('exec-out','run-as',pkg,'cat',f'files/saves/{directory}/slot-002.nksv')
    bad=bytearray(original);bad[-1]^=1
    local=Path('.local')/(profile+'.bad');local.write_bytes(bad)
    adb('push',str(local),'/data/local/tmp/'+profile+'.bad')
    menu('LOAD')
    adb('shell','run-as',pkg,'cp','/data/local/tmp/'+profile+'.bad',f'files/saves/{directory}/slot-002.nksv')
    before_page=read('page-61.json');releases=sum(x['event']=='load_released' for x in lines('host-events.jsonl'))
    error=slot(2,'error');assert 'checksum' in error['message'];assert read('page-61.json')==before_page
    assert sum(x['event']=='load_released' for x in lines('host-events.jsonl'))==releases
    cancel();next_page(61,14);finish()
    results.append(dict(case='corrupt_slot',rejected_before_release=True,current_page_preserved=True,next_page_verified=True))
    story_run=run
    launch(title_load=True)
    state=wait(lambda:obj('save-ui-presented.json'));assert state['from_title'] and not state['saving']
    assert not any(b['id']==-2 for b in state['buttons'])
    assert not next(b for b in state['buttons'] if b['id']==2)['enabled']
    assert 'checksum' in next(slot for slot in state['slots'] if slot['id']==2)['error']
    Path('.local/batch49-title-load.png').write_bytes(read('save-ui.png'))
    assert obj('page-1.json') is None
    cancel();assert obj('page-1.json') is None
    title=wait(lambda:obj('title-presented.json'));point('游戏标题菜单',next(b['bounds'] for b in title['buttons'] if b['id']==2))
    wait(lambda:obj('save-state.json')['event']=='open')
    slot(3,'loaded');page(1);baseline(1,56)
    if a.log_check:check_log(1,56,56,replay=True)
    next_page(1,56)
    effects=[json.loads(adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')) for name in adb('exec-out','run-as',pkg,'ls',f'files/{run}').decode().split() if re.fullmatch(r'effect-\d+\.json',name)]
    assert {e['host_effect'] for e in effects}=={'bgm','voice'},effects
    assert obj('title-loaded.json')['loaded'] and obj('title-started.json') is None
    finish();results.append(dict(case='title_load',cancel_preserves_title=True,no_start_story_execution=True,saved_page=56,next_page=57,corrupt_slot_disabled=True))
    if a.quick_check:
        # Persistent ten-slot history after a fresh process, including the oldest retained entry.
        launch();page(1);menu('LOAD');save_control(210)
        q=wait(lambda:obj('save-ui-presented.json')['page']==10 and obj('save-ui-presented.json'))
        assert all(b['enabled'] for b in q['buttons'] if 101<=b['id']<=110)
        slot(110,'loaded');page(2);baseline(2,3)
        quick('QLOAD','loaded',touch=True);page(3);baseline(3,56)
        if a.log_check:check_log(3,56,56,replay=True)
        next_page(3,56)
        if a.log_check:check_log(4,57,57)
        quick('QLOAD','loaded');page(5);baseline(5,56)
        latest=quick_names()[0];path=f'files/saves/{directory}/{latest}'
        original_quick=adb('exec-out','run-as',pkg,'cat',path);bad_quick=bytearray(original_quick);bad_quick[-1]^=1
        temporary=Path('.local')/(profile+'.quick');temporary.write_bytes(bad_quick)
        adb('push',str(temporary),'/data/local/tmp/'+profile+'.quick');adb('shell','run-as',pkg,'cp','/data/local/tmp/'+profile+'.quick',path)
        releases=sum(e['event']=='load_released' for e in lines('host-events.jsonl'))
        error=quick('QLOAD','error');assert 'checksum' in error['message'];baseline(5,56)
        assert sum(e['event']=='load_released' for e in lines('host-events.jsonl'))==releases
        temporary.write_bytes(original_quick);adb('push',str(temporary),'/data/local/tmp/'+profile+'.quick');adb('shell','run-as',pkg,'cp','/data/local/tmp/'+profile+'.quick',path)
        next_page(5,56)
        audio=lines('host-events.jsonl');resets=[i for i,e in enumerate(audio) if e['event']=='load_released'];assert len(resets)==3
        for at in resets[1:]:
            following=audio[at+1:];assert any(e['event']=='playing' and e['kind']=='bgm' for e in following)
            assert any(e['event']=='playing' and e['kind']=='voice' for e in following)
        quick_run=run;finish()
        launch(title_load=True);save_control(210);slot(101,'loaded');page(1);baseline(1,56);next_page(1,56)
        assert obj('title-loaded.json')['loaded'] and obj('title-started.json') is None
        title_quick_run=run;finish()
        results.append(dict(case='quick_history',writes=12,retained=10,newest_page=56,oldest_page=3,hash_order_verified=True,normal_slot_unchanged=adb('exec-out','run-as',pkg,'cat',f'files/saves/{directory}/slot-001.nksv')==first_file,run=quick_run,title_run=title_quick_run,function_keys=True,original_side_menu=True,title_q_page=True,corruption_preserves_current_state=True))
        assert results[-1]['normal_slot_unchanged']
        print('quick ten-slot rotation, Q page, F7/F8, title load and restored scenes passed',flush=True)
    if a.legacy_report:
        old_report=json.loads(Path(a.legacy_report).read_text(encoding='utf8'));old_run=old_report['story_run']
        events=adb('exec-out','run-as',pkg,'cat',f'files/{old_run}/save-events.jsonl').splitlines();old_directory=json.loads(events[0])['directory']
        old_path=f'files/saves/{old_directory}/slot-003.nksv'
        assert adb('exec-out','run-as',pkg,'cat',old_path)[:5]==b'NKSV\x01'
        adb('shell','run-as',pkg,'cp',old_path,f'files/saves/{directory}/slot-004.nksv')
        launch(title_load=True);slot(4,'loaded');page(1);baseline(1,56)
        check_log(1,1,56);next_page(1,56);check_log(2,2,57);finish()
        results.append(dict(case='legacy_v1',loaded=True,only_restored_current_entry=True,next_page_verified=True))
    report=dict(log_checks=log_results,execution='translated ARM64 on x86_64',machine=machine,native_bridge=bridge,apk_sha256=sha,profile=profile,runs=runs,story_run=story_run,results=results,baseline_pages=57,closed_vm_live_bytes=0,scope='Runtime NKSV slots; no original .sav interoperability. Audio resource/loop/channel restart, not sample-position equivalence.')
    Path(a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(json.dumps(report,ensure_ascii=False),flush=True)
if __name__=='__main__':main()
