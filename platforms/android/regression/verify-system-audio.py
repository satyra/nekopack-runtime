"""Run an explicitly deployed synthetic system/audio fixture, using metadata only.

No deployment, screenshots, image decoding, audio capture, or story text transfer.
Requires a debug APK, a matching fixture directory, and an existing SAF grant.
"""
import argparse
import hashlib
import importlib.util
import json
import math
import shlex
import uuid
from pathlib import Path

spec = importlib.util.spec_from_file_location('navigation', Path(__file__).with_name('verify-sorakoi-layout-navigation.py'))
nav = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nav)
PACKAGE = 'org.nekopack.probe'
ROOT = Path(__file__).resolve().parents[3]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--adb', default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    parser.add_argument('--tree', required=True, help='persisted SAF tree URI containing the dedicated fixture')
    parser.add_argument('--directory', required=True, help='fixture document ID, e.g. raw:/storage/emulated/0/Download/NekoPack-Audio-Probe')
    parser.add_argument('--remote', required=True, help='matching filesystem directory for read-only SHA256 checks')
    parser.add_argument('--manifest', type=Path, default=ROOT/'.local/system-integration/audio-fixture/manifest.json')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--skip-cold-save', action='store_true', help='omit optional QSAVE/restart; report omission explicitly')
    args = parser.parse_args()
    manifest = json.loads(args.manifest.read_text(encoding='utf8'))
    assert manifest['synthetic'] and not manifest['original_scenario_media_copied']
    run = 'system-audio-'+uuid.uuid4().hex[:8]
    p = nav.NavigationProbe(args.adb, args.serial, run, {})
    result = dict(run=run, serial=args.serial, passed=False, pages=[], checks=[], cold_save=False,
                  deferred=manifest['deferred'], manifest_sha256=hashlib.sha256(args.manifest.read_bytes()).hexdigest())
    kinds = ['bgm','bgv1','bgv2']
    effective = manifest['effective']
    command_percent = dict(bgm=50,bgv1=manifest['role_volumes'][0],bgv2=manifest['role_volumes'][1])

    def write():
        args.output.parent.mkdir(parents=True,exist_ok=True)
        args.output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')

    def press(label,state,bounds):
        left,top,right,bottom=p.bounds(label)
        width,height=state.get('size',[state.get('width'),state.get('height')])
        x,y,w,h=bounds
        scale=min((right-left)/width,(bottom-top)/height)
        p.tap(round((left+right)/2+(x+w/2-width/2)*scale),round((top+bottom)/2+(y+h/2-height/2)*scale))

    def boot(slot=0):
        extras=['--ei','title_game','0','--es','library_tree',shlex.quote(args.tree),
                '--es','library_directory',shlex.quote(args.directory),'--ez','library_original','true',
                '--ez','dialogue_probe','true','--es','read_history_probe',run,'--es','save_probe',run]
        if slot:
            p.start_activity(*extras,'--ei','library_resume_slot',str(slot))
        else:
            title=p.start_title(*extras)
            button=next(b for b in title['buttons'] if b['id']==title['start_action'])
            assert button['enabled']
            press(nav.startup.TITLE,title,button['bounds'])

    def page(sequence,index):
        actual=p.wait('synthetic source position',lambda:p.obj(f'page-{sequence}.json'))
        expected=manifest['pages'][index]
        assert (actual['source'],actual['line'])==(expected['source'],expected['line']),(actual,expected['label'])
        p.wait('text complete',lambda:(t if (t:=p.obj(f'text-{sequence}.json')) and t['complete'] else None))
        result['pages'].append(dict(run=p.run,label=expected['label'],**actual));write()
        return actual

    def advance(sequence,index):
        layout=p.obj('layout.json')
        p.tap(layout['stage_width']//2,round(layout['stage_height']*.4))
        return page(sequence,index)

    def events():return p.events('host-events.jsonl')
    def gains():return p.events('audio-gain-events.jsonl')
    def last_play(kind,after=0):
        return next((e for e in reversed(events()[after:]) if e['kind']==kind and e['event']=='playing'),None)

    def playing_loops(after=0):
        tokens={}
        for kind in kinds:
            started=p.wait(kind+' playing',lambda:last_play(kind,after))
            token=started['token'];tokens[kind]=token
            metadata=p.obj(f'effect-{token}.json')
            assert metadata['loop'] and metadata['volume_percent']==command_percent[kind],metadata
            p.wait(kind+' loop advances',lambda:any(e['kind']==kind and e['token']==token and e['event']=='looped' for e in events()),timeout=15)
        assert len(set(tokens.values()))==3,tokens
        return tokens

    def gain(kind,token,category,after=0,at_time=0):
        expected=command_percent.get(kind,100)*category/10000
        def match():
            return next((e for e in reversed(gains()[after:]) if e['kind']==kind and e['token']==token
                         and e['time_ms']>=at_time and e['effective_category_percent']==category
                         and math.isclose(e['output'],expected,abs_tol=0.00002)
                         and e['duck']==1 and e['fade']==1),None)
        event=p.wait(f'{kind} effective category {category}',match,timeout=15)
        result['checks'].append(dict(check='gain',run=p.run,**event));write()
        return event

    def baseline(tokens,after=0):
        gain('bgm',tokens['bgm'],effective['BGM'],after)
        for kind in ['bgv1','bgv2']:gain(kind,tokens[kind],effective['BGV'],after)

    def voice(sequence,index,muted,other,tokens):
        cursor=len(gains());host_cursor=len(events())
        advance(sequence,index)
        started=p.wait('synthetic voice playback',lambda:last_play('voice',host_cursor))
        gain(muted,tokens[muted],0,cursor,started['time_ms'])
        gain(other,tokens[other],effective['BGV'],cursor,started['time_ms'])
        gain('bgm',tokens['bgm'],effective['MV'],cursor,started['time_ms'])
        completed=p.wait('synthetic voice completion',lambda:next((e for e in events() if e['kind']=='voice' and e['token']==started['token'] and e['event']=='completed'),None),timeout=20)
        gain(muted,tokens[muted],effective['BGV'],cursor,completed['time_ms'])
        gain('bgm',tokens['bgm'],effective['BGM'],cursor,completed['time_ms'])
        result['checks'].append(dict(check='voice_suppression_and_restore',voice_token=started['token'],muted=muted,unaffected=other));write()

    try:
        # Refuse to touch a library directory unless every deployed input is our fixture.
        for file in manifest['files']:
            remote=args.remote.rstrip('/')+'/'+file['name']
            digest=p.adb('shell','sha256sum',shlex.quote(remote)).split()[0].decode()
            assert digest==file['sha256'],('fixture SHA256 mismatch',file['name'])
        apk=p.adb('shell','pm','path',PACKAGE).decode().strip().removeprefix('package:')
        result['apk_sha256']=p.adb('shell','sha256sum',shlex.quote(apk)).split()[0].decode()
        boot();page(1,0)
        tokens=playing_loops();baseline(tokens)
        result['checks'].append(dict(check='four_bank_isolation',source_assertion='audio_probe_banks.nut'))
        voice(2,1,'bgv1','bgv2',tokens)
        voice(3,2,'bgv2','bgv1',tokens)
        for sequence,index,categories in [(4,3,dict(bgm=effective['BGM'],bgv1=0,bgv2=0)),
                                           (5,4,dict(bgm=0,bgv1=effective['BGV'],bgv2=effective['BGV'])),
                                           (6,5,dict(bgm=0,bgv1=0,bgv2=0))]:
            cursor=len(gains());advance(sequence,index)
            for kind,category in categories.items():gain(kind,tokens[kind],category,cursor)
        host_cursor=len(events());advance(7,6)
        fresh=playing_loops(host_cursor)
        assert all(tokens[k]!=fresh[k] for k in kinds)
        tokens=fresh;baseline(tokens)
        result['checks'].append(dict(check='reset_local_banks_and_restart_audio',source_assertion='audio_probe_reset.nut'))
        if not args.skip_cold_save:
            state=p.wait('QSAVE action',lambda:(s if (s:=p.obj('side-menu-presented.json')) and any(b['action']=='QSAVE' and b['enabled'] for b in s['buttons']) else None))
            button=next(b for b in state['buttons'] if b['action']=='QSAVE')
            if not state['keep_icon'] and not state['expanded']:
                press(nav.SIDE,state,button['bounds'])
                state=p.wait('expanded side menu',lambda:(s if (s:=p.obj('side-menu-presented.json')) and s['expanded'] else None))
                button=next(b for b in state['buttons'] if b['action']=='QSAVE')
            press(nav.SIDE,state,button['bounds'])
            saved=p.wait('quick saved',lambda:(s if (s:=p.obj('save-state.json')) and s['event']=='saved' else None))
            assert saved['sequence']==7,saved
            result['save']=saved
            p=nav.NavigationProbe(args.adb,args.serial,run+'-reload',{})
            boot(101);page(1,6)
            loaded=p.wait('quick load',lambda:(s if (s:=p.obj('save-state.json')) and s['event']=='loaded' else None))
            tokens=playing_loops();baseline(tokens)
            result.update(cold_save=True,reload_run=p.run,loaded=loaded)
            advance(2,7);advance(3,8)
        else:
            result['deferred'].append('QSAVE cold restart explicitly skipped')
            advance(8,7);advance(9,8)
        for kind in ['bgv_stop','bgm_stop']:
            p.wait(kind,lambda:any(e['kind']==kind and e['event']=='stopped' for e in events()))
        result['checks'].append(dict(check='saved_bank_config_assertions_and_final_stop'))
        assert not p.read('error.txt')
        result['passed']=True
    except Exception as error:
        result['error']=str(error)
        raise
    finally:
        write()
    print(json.dumps(dict(passed=result['passed'],run=run,cold_save=result['cold_save'],checks=len(result['checks'])),ensure_ascii=False))

if __name__=='__main__':
    main()
