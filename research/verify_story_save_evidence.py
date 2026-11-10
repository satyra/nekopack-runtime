"""Independently read device artifacts after the UI save/load regression."""
import argparse,hashlib,json,subprocess
from pathlib import Path

def main():
    p=argparse.ArgumentParser()
    for key in ('adb','serial','report','reference','output'):p.add_argument('--'+key,required=True)
    a=p.parse_args();report=json.loads(Path(a.report).read_text(encoding='utf8'))
    prefix=[a.adb,'-s',a.serial,'exec-out','run-as','org.nekopack.probe']
    def raw(run,name):return subprocess.check_output(prefix+['cat',f'files/{run}/{name}'],timeout=30)
    def lines(run,name):return [json.loads(x) for x in raw(run,name).splitlines() if x.startswith(b'{')]
    sessions=[]
    for run in report['runs']:
        assert b'No such file' in raw(run,'error.txt')
        assert b'VM live bytes=0' in raw(run,'closed.txt')
        events=lines(run,'text-events.jsonl');tokens=[e['sequence'] for e in events]
        assert tokens==sorted(tokens),'old text callback delivered after load'
        host=lines(run,'host-events.jsonl');assert host[-1]['event']=='released' and host[-1]['value']==0
        files=subprocess.check_output(prefix+['ls',f'files/{run}'],timeout=30).decode().split()
        assert not any(n.startswith('audio-') and n.endswith('.ogg') for n in files)
        sessions.append(dict(run=run,text_events=len(events),monotonic_page_tokens=True,closed_vm_live_bytes=0,audio_cache_files=0))
    run=report.get('story_run',report['runs'][-1]);host=lines(run,'host-events.jsonl')
    resets=[i for i,e in enumerate(host) if e['event']=='load_released'];assert len(resets)==2
    first,second=resets;before=host[:first];after=host[first+1:second]
    old_tokens={e['token'] for e in before if e['kind'] in ('bgm','voice','env','se')}
    assert not any(e['token'] in old_tokens and e['kind'] in ('bgm','voice','env','se') for e in host[first+1:])
    def resource(e):return json.loads(raw(run,f"audio-{e['token']}.json"))
    expected_voice=json.loads((Path(a.reference)/'55.json').read_text(encoding='utf8'))['voice']
    channels={}
    for kind in ('bgm','voice'):
        started=next(e for e in after if e['kind']==kind and e['event']=='playing')
        restored=resource(started)
        if kind=='voice':assert restored['name']==expected_voice
        original=next(e for e in reversed(before) if e['kind']==kind and e['event']=='playing' and resource(e)['name']==restored['name'])
        assert resource(original)==restored
        if kind=='bgm':assert json.loads(raw(run,f"effect-{started['token']}.json"))['loop']
        channels[kind]=dict(original_token=original['token'],restored_token=started['token'],sha256=restored['sha256'],resource_identity_equal=True)
    events=lines(run,'save-events.jsonl');errors=[e for e in events if e['event']=='error'];assert len(errors)==1 and 'checksum' in errors[0]['message']
    assert events[-1]['event']=='cancelled'
    quick_audio=[]
    quick_storage=[]
    quick_cases=[r for r in report['results'] if r['case']=='quick_history']
    for case in quick_cases:
        directory=lines(case['run'],'save-events.jsonl')[0]['directory']
        names=json.loads(subprocess.check_output(prefix+['cat',f'files/saves/{directory}/quick-index.json'],timeout=30))
        assert len(names)==len(set(names))==10
        files=subprocess.check_output(prefix+['ls',f'files/saves/{directory}'],timeout=30).decode().split()
        snapshots=[n for n in files if n.startswith('quick-') and n.endswith('.nksv')]
        assert len(snapshots)<=11 and all(n in snapshots for n in names)
        quick_storage.append(dict(directory=directory,indexed_slots=10,physical_snapshots=len(snapshots),all_index_entries_present=True))
        for quick_run,expected_pages in ((case['run'],[3,56,56]),(case['title_run'],[56])):
            events=lines(quick_run,'host-events.jsonl')
            indices=[i for i,e in enumerate(events) if e['event']=='load_released']
            assert len(indices)==len(expected_pages)
            for n,(at,expected_page) in enumerate(zip(indices,expected_pages)):
                end=indices[n+1] if n+1<len(indices) else len(events)
                following=events[at+1:end]
                old_tokens={e['token'] for e in events[:at] if e['kind'] in ('bgm','voice','env','se')}
                assert not any(e['token'] in old_tokens and e['kind'] in ('bgm','voice','env','se') for e in following)
                expected=json.loads((Path(a.reference)/f'{expected_page-1}.json').read_text(encoding='utf8'))
                for kind in ('bgm','voice'):
                    playing=next((e for e in following if e['kind']==kind and e['event']=='playing'),None)
                    if kind=='voice' and not expected['voice']:
                        assert playing is None
                        continue
                    if kind=='bgm' and expected_page==3 and playing is None:continue
                    assert playing is not None,(quick_run,expected_page,kind)
                    restored=json.loads(raw(quick_run,f"audio-{playing['token']}.json"))
                    if kind=='voice':assert restored['name']==expected['voice']
                    source=next(e for e in before if e['kind']==kind and e['event']=='playing' and resource(e)==restored)
                    if kind=='bgm':assert json.loads(raw(quick_run,f"effect-{playing['token']}.json"))['loop']
                    quick_audio.append(dict(run=quick_run,page=expected_page,kind=kind,token=playing['token'],sha256=restored['sha256'],resource_identity_equal=True))
    report_hash=hashlib.sha256(Path(a.report).read_bytes()).hexdigest()
    result=dict(ui_report_sha256=report_hash,execution=report['execution'],apk_sha256=report['apk_sha256'],sessions=sessions,audio=channels,quick_audio=quick_audio,quick_storage=quick_storage,no_old_audio_callbacks=True,corruption_error_then_cancel=True)
    Path(a.output).write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print('PASS independent save/audio identity, old callback and cleanup evidence')
if __name__=='__main__':main()
