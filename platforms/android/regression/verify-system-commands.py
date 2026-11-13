"""Exercise original date/dialog, registered branches, mask and repeated waits using metadata."""
import argparse
import importlib.util
import json
import shlex
import uuid
from pathlib import Path

spec = importlib.util.spec_from_file_location('navigation', Path(__file__).with_name('verify-sorakoi-layout-navigation.py'))
nav = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nav)
ROOT = Path(__file__).resolve().parents[3]


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial',required=True)
    parser.add_argument('--adb',default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    parser.add_argument('--tree',required=True)
    parser.add_argument('--directory',required=True)
    parser.add_argument('--remote',required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    manifest=json.loads((ROOT/'.local/system-integration/command-fixture/manifest.json').read_text())
    run='system-command-'+uuid.uuid4().hex[:8]
    p=nav.NavigationProbe(args.adb,args.serial,run,{})
    result=dict(run=run,passed=False,checks=[])
    def events():
        data=p.read('system-command-events.jsonl')
        return [json.loads(line) for line in (data or b'').splitlines() if line.endswith(b'}')]
    extras=['--ei','title_game','0','--es','library_tree',shlex.quote(args.tree),
            '--es','library_directory',shlex.quote(args.directory),'--ez','library_original','true',
            '--ez','dialogue_probe','true','--es','save_probe',run,'--es','read_history_probe',run]
    def press(label,state,bounds):
        left,top,right,bottom=p.bounds(label)
        width,height=state.get('size',[state.get('width'),state.get('height')])
        x,y,w,h=bounds;scale=min((right-left)/width,(bottom-top)/height)
        p.tap(round((left+right)/2+(x+w/2-width/2)*scale),round((top+bottom)/2+(y+h/2-height/2)*scale))
    try:
        for item in manifest['files']:
            digest=p.adb('shell','sha256sum',shlex.quote(args.remote+'/'+item['name'])).split()[0].decode()
            assert digest==item['sha256']
        apk=p.adb('shell','pm','path','org.nekopack.probe').decode().strip().removeprefix('package:')
        result['apk_sha256']=p.adb('shell','sha256sum',shlex.quote(apk)).split()[0].decode()
        title=p.start_title(*extras)
        b=next(b for b in title['buttons'] if b['id']==title['start_action'])
        left,top,right,bottom=p.bounds(nav.startup.TITLE)
        width,height=title['width'],title['height'];x,y,w,h=b['bounds']
        scale=min((right-left)/width,(bottom-top)/height)
        p.tap(round((left+right)/2+(x+w/2-width/2)*scale),round((top+bottom)/2+(y+h/2-height/2)*scale))
        def two_dates():
            rows=[e for e in events() if e['command']=='date']
            return rows if len({e['token'] for e in rows})==2 else None
        rows=p.wait('original timed date return',two_dates,timeout=35)
        first=rows[0]['token'];second=rows[-1]['token']
        first_rows=[e for e in rows if e['token']==first]
        assert any(e['has_frame'] for e in first_rows)
        assert max(e['elapsed_ms'] for e in first_rows)>=2700
        result['checks'].append('original date resumed and drew frames until its own timer returned')
        p.adb('shell','input','keyevent','66')
        wait=p.wait('registered handler first input wait',lambda:next((e for e in reversed(events()) if e['command']=='probe_wait'),None))
        assert wait['delay_ms']==-1 and wait['has_frame']
        result['checks'].append('confirmation exited second original date')
        token=wait['token'];before=len(events())
        p.adb('shell','input','keyevent','66')
        after=p.wait('registered handler second input wait',lambda:events()[before:] or None)
        assert after[-1]['command']=='probe_wait' and after[-1]['token']==token and after[-1]['delay_ms']==-1
        p.adb('shell','input','keyevent','4')
        dialog=p.wait('original nested dialog waits',lambda:next((e for e in reversed(events()) if e['command']=='probe_choice' and e['has_frame'] and e['delay_ms']==-1),None))
        # Synthetic original dialog OK rectangle in the 1600x900 content.
        layout=p.obj('layout.json');sw,sh=layout['stage_width'],layout['stage_height']
        scale=min(sw/1600,sh/900)
        p.tap(round(sw/2+(360-800)*scale),round(sh/2+(400-450)*scale))
        result['checks'].append('original ViewDialog accepted pointer input inside registered branch coroutine')
        page=p.wait('scenario resumed after cancel' ,lambda:p.obj('page-1.json'))
        assert page['source']==manifest['page']['source'] and page['line']==manifest['page']['line']
        result['checks'].append('same coroutine resumed after two separate input waits and returned to scenario')
        def sound_ack():
            data=p.read('ui-audio-events.jsonl')
            return next((json.loads(line) for line in (data or b'').splitlines() if json.loads(line).get('name')=='se_CLICK'),None)
        result['sound']=p.wait('post-return system sound playback',sound_ack)
        result.update(events=events(),page=page)
        p.wait('first text complete',lambda:(t if (t:=p.obj('text-1.json')) and t['complete'] else None))
        layout=p.obj('layout.json');p.tap(layout['stage_width']//2,round(layout['stage_height']*.4))
        delayed=p.wait('delayed scene page',lambda:p.obj('page-2.json'))
        assert delayed['line']==manifest['delay_page']['line']
        p.wait('delayed page text complete',lambda:(t if (t:=p.obj('text-2.json')) and t['complete'] else None))
        motion=p.events('motion-events.jsonl')
        assert any(e['sequence']==2 and e['event']=='begin' for e in motion)
        assert not any(e['sequence']==2 and e['finished'] for e in motion)
        state=p.wait('quick save available',lambda:(s if (s:=p.obj('side-menu-presented.json')) and any(b['action']=='QSAVE' and b['enabled'] for b in s['buttons']) else None))
        button=next(b for b in state['buttons'] if b['action']=='QSAVE')
        press(nav.SIDE,state,button['bounds'])
        config=p.events('text-config-events.jsonl')[-1]
        if config['Dialog']['QSAVE']:
            left,top,right,bottom=p.bounds('android:id/button1',attr='resource-id')
            p.tap((left+right)//2,(top+bottom)//2)
        result['save']=p.wait('saved delayed scene',lambda:(s if (s:=p.obj('save-state.json')) and s['event']=='saved' else None))
        assert result['save']['sequence']==2
        result['pending_motion_before_restart']=p.events('motion-events.jsonl')
        assert not any(e['sequence']==2 and e['finished'] for e in result['pending_motion_before_restart'])
        p=nav.NavigationProbe(args.adb,args.serial,run+'-reload',{})
        p.start_activity(*extras,'--ei','library_resume_slot','101')
        loaded=p.wait('loaded delayed scene',lambda:p.obj('page-1.json'))
        assert loaded['line']==manifest['delay_page']['line']
        result['loaded']=p.wait('load acknowledgement',lambda:(s if (s:=p.obj('save-state.json')) and s['event']=='loaded' else None))
        finished=p.wait('restored delayed scene committed',lambda:next((e for e in p.events('motion-events.jsonl') if e['sequence']==1 and e['event']=='finished'),None),timeout=20)
        assert 0<finished['elapsed_ms']<=manifest['delay_page']['delay_ms']+3000
        result['checks'].append('pending original delay survived save and cold restart, then committed')
        result.update(passed=True,reload_run=p.run,restored_motion=finished)
    finally:
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
    print(json.dumps(dict(run=run,passed=True,checks=result['checks'])))


if __name__=='__main__':main()
