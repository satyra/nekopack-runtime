"""Real APK native sysload/continue regression: coordinates, metadata and hashes only.

Uses a unique save_probe directory. Only test-created quick snapshots are copied
into ordinary slots; user saves are never read or modified. No screenshots,
original dialogue, save payload bytes or artwork leave the device.
"""
import argparse, hashlib, importlib.util, json, shlex, time, uuid
from pathlib import Path

spec=importlib.util.spec_from_file_location('navigation',Path(__file__).with_name('verify-sorakoi-layout-navigation.py'))
nav=importlib.util.module_from_spec(spec);spec.loader.exec_module(nav)
ROOT=Path(__file__).resolve().parents[3]
PACKAGE='org.nekopack.probe'
TREE='content://com.android.providers.downloads.documents/tree/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-Sorakoi'
REMOTE='/sdcard/Download/NekoPack-Sorakoi/NekoPack-Native-Load-Probe'
DIRECTORY='raw:/storage/emulated/0/Download/NekoPack-Sorakoi/NekoPack-Native-Load-Probe'


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial',required=True)
    parser.add_argument('--adb',default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    run='native-load-'+uuid.uuid4().hex[:8]
    p=nav.NavigationProbe(args.adb,args.serial,run,{})
    report=dict(run=run,passed=False,steps=[],runs=[])
    sequence=0
    save_tag=run
    scope=hashlib.sha256((TREE+'_0').encode()).hexdigest()
    save_dir='files/saves/'+scope+'_probe_'+save_tag

    def flush():
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    def step(name,**data):
        report['steps'].append(dict(name=name,run=p.run,**data));flush()
    def apk_hash():
        path=p.adb('shell','pm','path',PACKAGE).decode().strip().removeprefix('package:')
        return p.adb('shell','sha256sum',shlex.quote(path)).split()[0].decode()
    def shell(*words):
        # All file writes are checked against this invocation's isolated scope.
        return p.adb('shell','run-as',PACKAGE,*words)
    def own(path):
        assert path.startswith(save_dir+'/') and '_probe_'+save_tag+'/' in path
        return path
    def press(label,state,button):
        left,top,right,bottom=p.bounds(label)
        width,height=state.get('size',[state.get('width'),state.get('height')])
        x,y,w,h=button['bounds'];scale=min((right-left)/width,(bottom-top)/height)
        p.tap(round((left+right)/2+(x+w/2-width/2)*scale),round((top+bottom)/2+(y+h/2-height/2)*scale))
    def complete(number):
        nonlocal sequence
        page=p.wait('synthetic page metadata',lambda:p.obj(f'page-{number}.json'))
        p.wait('synthetic page complete',lambda:(s if (s:=p.obj(f'text-{number}.json')) and s['complete'] else None))
        assert page['source'].lower().removesuffix('.txt')=='story1',page
        sequence=number
        step('page',**page)
        return page
    def position(page):return page['source'],page['line']
    def advance():
        layout=p.obj('layout.json');p.tap(layout['stage_width']//2,round(layout['stage_height']*.4))
    def events():return p.events('system-command-events.jsonl')
    def button(frame,name=None,slot=None):
        return next((b for b in frame.get('buttons',[]) if
                     (name is not None and b['name'].split(':')[1]==name) or
                     (slot is not None and b['name'].split(':')[1]=='' and b['name'].rsplit(':',1)[-1]==str(slot))),None)
    def frame(name=None,slot=None,after=0):
        return p.wait('original native control',lambda:next((f for f in reversed(events()[after:]) if f.get('has_frame') and button(f,name,slot)),None),timeout=40)
    def tap_native(f,name=None,slot=None):
        mark=len(events());layout=p.obj('layout.json');w,h=f['size']
        sw,sh=layout['stage_width'],layout['stage_height'];scale=min(sw/w,sh/h)
        x,y,bw,bh=button(f,name,slot)['bounds']
        p.tap(round(sw/2+(x+bw/2-w/2)*scale),round(sh/2+(y+bh/2-h/2)*scale))
        return mark
    def side(action):
        s=p.wait('enabled '+action,lambda:(s if (s:=p.obj('side-menu-presented.json')) and
                 any(b['action']==action and b['enabled'] for b in s['buttons']) else None))
        b=next(b for b in s['buttons'] if b['action']==action)
        assert b.get('supported',True)
        press(nav.SIDE,s,b)
        if not s['keep_icon'] and not s['expanded']:
            s=p.wait('expanded menu',lambda:(v if (v:=p.obj('side-menu-presented.json')) and v['expanded'] and v['render_sequence']>s['render_sequence'] else None))
            press(nav.SIDE,s,next(b for b in s['buttons'] if b['action']==action))
    def quick_save():
        mark=len(p.events('save-events.jsonl'));side('QSAVE');p.control('确定')
        saved=p.wait('real quick snapshot',lambda:next((e for e in p.events('save-events.jsonl')[mark:] if e['event']=='saved'),None))
        assert saved['sequence']==sequence,saved
        assert saved['directory']==save_dir.rsplit('/',1)[1]
        names=json.loads(p.private(own(save_dir+'/quick-index.json')))
        name=names[0]
        assert name.startswith('quick-') and name.endswith('.nksv') and '/' not in name
        path=own(save_dir+'/'+name)
        digest=shell('sha256sum',path).split()[0].decode()
        step('real_quick_save',state=saved,path=path,sha256=digest)
        return path
    def copy_slot(source,slot,stamp):
        target=own(save_dir+f'/slot-{slot:03d}.nksv')
        shell('cp',own(source),target);shell('touch','-t',stamp,target)
        info=shell('stat','-c','%Y:%s',target).decode().strip()
        digest=shell('sha256sum',target).split()[0].decode()
        step('isolated_ordinary_slot',slot=slot,stat=info,sha256=digest)
    def start():
        nonlocal sequence
        sequence=0
        t=p.start_title('--ei','title_game','0','--es','library_tree',shlex.quote(TREE),
            '--es','library_directory',shlex.quote(DIRECTORY),'--ez','library_original','true',
            '--ez','dialogue_probe','true','--es','save_probe',save_tag,'--es','read_history_probe',save_tag)
        digest=apk_hash();assert digest==report['apk_sha256']
        report['runs'].append(dict(run=p.run,apk_sha256=digest,save_probe=save_tag))
        press(nav.startup.TITLE,t,next(b for b in t['buttons'] if b['id']==t['start_action']))
        return complete(1)
    def enter_load():
        mark=len(events());advance();return frame(name='CLOSE',after=mark)
    def cancel_load(f):
        mark=tap_native(f,name='CLOSE');page=complete(sequence+1)
        step('original_sysload_cancel',events=events()[mark:])
        return page
    def confirm_slot(f,slot,expected):
        mark=tap_native(f,slot=slot);dialog=frame(name='OK',after=mark)
        started=time.monotonic();tap_native(dialog,name='OK')
        page=complete(sequence+1)
        relevant=events()[mark:]
        sleep=next(e for e in relevant if e['delay_ms']==1000)
        assert position(page)==position(expected),(page,expected)
        assert time.monotonic()-started>=.9,'original Sleep(1000) was bypassed'
        step('original_confirmed_load',slot=slot,sleep=sleep,events=relevant)
        return page
    def continue_to(expected):
        mark=len(events());started=time.monotonic();advance();page=complete(sequence+1)
        assert position(page)==position(expected),(page,expected)
        relevant=events()[mark:];sleep=next(e for e in relevant if e['delay_ms']==1000)
        assert time.monotonic()-started>=.9
        step('continue_uses_latest_ordinary',sleep=sleep,events=relevant)
        return page

    try:
        manifest=json.loads((ROOT/'.local/system-integration/native-load-fixture/manifest.json').read_text(encoding='utf8'))
        for item in manifest['files']:
            assert p.adb('shell','sha256sum',shlex.quote(REMOTE+'/'+item['name'])).split()[0].decode()==item['sha256']
        report['fixture']=manifest;report['apk_sha256']=apk_hash()
        first=start();qfirst=quick_save();copy_slot(qfirst,2,'202401010100.00')
        advance();second=complete(2);qsecond=quick_save();copy_slot(qsecond,1,'202401010200.00')
        load=enter_load();mark=tap_native(load,slot=2);dialog=frame(name='OK',after=mark)
        mark=tap_native(dialog,name='CANCEL');load=frame(name='CLOSE',after=mark)
        assert p.obj(f'page-{sequence+1}.json') is None
        assert load['token']==dialog['token']
        step('load_confirmation_cancel_preserves_original_page',token=load['token'])
        confirm_slot(load,2,first)
        advance();assert position(complete(sequence+1))==position(second)
        page3=cancel_load(enter_load())
        qlate=quick_save();shell('touch','-t','202401010300.00',own(qlate))
        stamps={name:int(shell('stat','-c','%Y',own(path)).decode()) for name,path in
                [('slot1',save_dir+'/slot-001.nksv'),('slot2',save_dir+'/slot-002.nksv'),('quick',qlate)]}
        assert stamps['slot2']<stamps['slot1']<stamps['quick'],stamps
        step('mtime_order_differs_from_slot_order_and_newer_quick',mtime_seconds=stamps)
        assert position(page3)!=position(second)
        continue_to(second)
        assert not p.read('error.txt')
        step('warm_cycle_passed',events=events(),audio=p.events('ui-audio-events.jsonl'))
        p=nav.NavigationProbe(args.adb,args.serial,run+'-cold',{})
        start();advance();complete(2)
        confirm_slot(enter_load(),1,second)
        page3=cancel_load(enter_load());continue_to(second)
        assert not p.read('error.txt');step('cold_cycle_passed',events=events(),audio=p.events('ui-audio-events.jsonl'))
        # A separate empty scope verifies the original no-record continue branch.
        save_tag=run+'-empty';save_dir='files/saves/'+scope+'_probe_'+save_tag
        p=nav.NavigationProbe(args.adb,args.serial,run+'-empty',{})
        start();advance();complete(2);cancel_load(enter_load())
        mark=len(events());advance();empty=complete(sequence+1)
        assert empty['line']>page3['line']
        assert not any(e.get('native_ack') for e in events()[mark:])
        assert not p.read('error.txt')
        step('empty_continue_returns_to_scenario',events=events()[mark:])
        report['passed']=True
    except Exception as error:
        report['failure_type']=type(error).__name__
        raise
    finally:
        flush()
    print(json.dumps(dict(passed=True,run=run,apk_sha256=report['apk_sha256'],steps=len(report['steps']))))

if __name__=='__main__':main()
