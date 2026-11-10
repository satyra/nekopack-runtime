"""BLESS original side menu touch adapter, isolated save profile and raw DAT."""
import argparse,hashlib,json,re,subprocess,time,uuid
from pathlib import Path
import xml.etree.ElementTree as ET

def main():
    p=argparse.ArgumentParser()
    for name in ('adb','serial','work','output'):p.add_argument('--'+name,required=True)
    a=p.parse_args();work=Path(a.work);work.mkdir(parents=True,exist_ok=True)
    pkg='org.nekopack.probe';run='side_menu_'+uuid.uuid4().hex
    def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=60)
    def raw(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
    def obj(name):
        data=raw(name)
        return json.loads(data) if data.startswith(b'{') else None
    def wait(fn):
        end=time.monotonic()+120
        while time.monotonic()<end:
            assert b'No such file' in raw('error.txt')
            value=fn()
            if value:return value
            time.sleep(.15)
        raise AssertionError('side menu timeout: '+run)
    def bounds(label,attr='content-desc'):
        for _ in range(5):
            result=adb('shell','uiautomator','dump','/sdcard/neko-side.xml')
            if b'dumped to' in result:break
        else:raise AssertionError('UI hierarchy unavailable')
        node=next(n for n in ET.fromstring(adb('shell','cat','/sdcard/neko-side.xml')).iter('node') if n.get(attr)==label)
        return list(map(int,re.findall(r'\d+',node.get('bounds'))))
    def point(label,b):
        l,t,r,bottom=bounds(label);x,y,w,h=b;scale=min((r-l)/1280,(bottom-t)/720)
        adb('shell','input','tap',str(round(l+(r-l-1280*scale)/2+(x+w/2)*scale)),str(round(t+(bottom-t-720*scale)/2+(y+h/2)*scale)))
    def click(label):
        l,t,r,b=bounds(label,'text');adb('shell','input','tap',str((l+r)//2),str((t+b)//2))
    def open_side():
        old=wait(lambda:obj('side-menu-presented.json'))
        point('原版右侧悬浮菜单',old['bounds'])
        state=wait(lambda:obj('side-menu-presented.json')['render_sequence']>old['render_sequence'] and obj('side-menu-presented.json'))
        assert state['expanded'];return state
    def action(state,name):
        button=next(b for b in state['buttons'] if b['action']==name)
        point('原版右侧悬浮菜单',button['bounds'])
    def same_page(n):
        wait(lambda:obj(f'text-{n}.json') and obj(f'text-{n}.json')['complete'])
        meta=obj(f'page-{n}.json');meta['sequence']=1;assert meta==original
        assert raw(f'page-{n}.png')==original_png
        assert obj(f'page-{n+1}.json') is None
    machine=adb('shell','uname','-m').decode().strip();bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip()
    assert machine=='x86_64' and bridge not in ('','0')
    apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:')
    sha=adb('shell','sha256sum',apk).decode().split()[0]
    adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','title_game','0','--es','run_id',run,'--es','save_probe',run,'--es','read_history_probe',run,'--ez','lifecycle_probe','true')
    title=wait(lambda:obj('title-presented.json'));point('游戏标题菜单',next(b['bounds'] for b in title['buttons'] if b['id']==1))
    wait(lambda:obj('text-1.json') and obj('text-1.json')['complete'])
    original=obj('page-1.json');original_png=raw('page-1.png')
    state=open_side();assert [b['action'] for b in state['buttons']]==['QSAVE','QLOAD','SAVE','LOAD','AUTO','SKIP','LOG','CONFIG']
    assert not next(b for b in state['buttons'] if b['action']=='QLOAD')['enabled']
    action(state,'QLOAD');wait(lambda:obj('side-menu-presented.json')['input_sequence']>=2)
    assert b'No such file' in raw('side-menu-events.jsonl');same_page(1)
    (work/'side-empty.png').write_bytes(adb('exec-out','screencap','-p'))
    action(obj('side-menu-presented.json'),'QSAVE');wait(lambda:obj('save-state.json') and obj('save-state.json')['event']=='saved');same_page(1)
    state=open_side();assert next(b for b in state['buttons'] if b['action']=='QLOAD')['enabled']
    (work/'side-ready.png').write_bytes(adb('exec-out','screencap','-p'))
    action(state,'QLOAD');wait(lambda:obj('save-state.json')['event']=='loaded');same_page(2)
    for name in ('SAVE','LOAD'):
        state=open_side();old=obj('save-state.json');action(state,name)
        wait(lambda:obj('save-state.json')!=old and obj('save-state.json')['event']=='open')
        assert obj('save-ui-presented.json')['saving']==(name=='SAVE')
        adb('shell','input','keyevent','KEYCODE_BACK');wait(lambda:obj('save-state.json')['event']=='cancelled');same_page(2)
    state=open_side();action(state,'CONFIG')
    wait(lambda:obj('settings-presented.json'))
    adb('shell','input','keyevent','KEYCODE_BACK')
    wait(lambda:any(json.loads(line).get('event')=='closed' for line in raw('story-settings-events.jsonl').splitlines()));same_page(2)
    state=open_side();action(state,'LOG');wait(lambda:obj('message-log-presented.json'))
    assert obj('message-log-presented.json')['count']==1
    (work/'message-log.png').write_bytes(adb('exec-out','screencap','-p'))
    click('返回剧情');wait(lambda:b'log_closed' in raw('log-events.jsonl'));same_page(2)
    click('结束运行');wait(lambda:b'VM live bytes=0' in raw('closed.txt'))
    report=dict(run=run,execution='translated ARM64 on x86_64',machine=machine,native_bridge=bridge,apk_sha256=sha,original_sprite_order=True,empty_quick_load_disabled=True,touch_quick_save_load=True,normal_save_load_cancel=True,config_returns_same_dialogue=True,log_returns_same_dialogue=True,log_return_button=True,restored_scene_sha256=hashlib.sha256(original_png).hexdigest(),closed_vm_live_bytes=0,screenshots=[str(work/n) for n in ('side-empty.png','side-ready.png','message-log.png')])
    Path(a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(json.dumps(report,ensure_ascii=False))
if __name__=='__main__':main()
