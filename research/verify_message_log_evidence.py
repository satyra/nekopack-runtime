"""Re-read device artifacts; compare UI audio with original DAT bytes, not exit codes."""
import argparse,hashlib,json,subprocess
from pathlib import Path

def main():
    p=argparse.ArgumentParser()
    for key in ('adb','serial','report','original','games','cli','output'):p.add_argument('--'+key,required=True)
    a=p.parse_args();report=json.loads(Path(a.report).read_text(encoding='utf8'));original=json.loads(Path(a.original).read_text(encoding='utf8'))
    prefix=[a.adb,'-s',a.serial,'exec-out','run-as','org.nekopack.probe']
    def raw(run,name):return subprocess.check_output(prefix+['cat',f'files/{run}/{name}'],timeout=30)
    def lines(run,name):return [json.loads(x) for x in raw(run,name).splitlines() if x.startswith(b'{')]
    game=next(Path(a.games).rglob('Tiny_BoD.exe')).parent;cache={}
    def voice_hash(name):
        if name not in cache:
            lookup=subprocess.check_output([a.cli,'lookup-name',b'voice'.hex(),(name+'.ogg').encode().hex(),str(game/'voice.dat')]).decode().splitlines()[0].split('\t')
            assert lookup[:2]==['selected','0'],lookup
            data=subprocess.check_output([a.cli,'read-entry',str(game/'voice.dat'),lookup[2]])
            cache[name]=hashlib.sha256(data).hexdigest()
        return cache[name]
    results=[];log_results=[];star_count=0;progress_count=0
    for run in report['runs']:
        assert b'VM live bytes=0' in raw(run,'closed.txt') and b'No such file' in raw(run,'error.txt')
        host=lines(run,'host-events.jsonl');gain=lines(run,'audio-gain-events.jsonl');ui=lines(run,'ui-audio-events.jsonl')
        operations=[e for e in lines(run,'save-events.jsonl') if e['event'] in ('saved','loaded','error')]
        assert sum(e['host_effect']=='system' and e['name']=='se_STAR' for e in ui)==len(operations),(run,len(operations),ui)
        assert host[-1]['event']=='released' and host[-1]['value']==0
        names=subprocess.check_output(prefix+['ls',f'files/{run}'],timeout=30).decode().split()
        assert not any(n.startswith('audio-') and n.endswith('.ogg') for n in names)
        for meta in ui:
            token=meta['token'];kind=meta['host_effect'];assert token<0 and meta['ui_audio'] and not meta['loop']
            data=json.loads(raw(run,f'audio-{token}.json'));assert data['kind']==kind and data['name']==meta['name']
            if kind=='voice':assert meta['category_percent']==lines(run,'audio-config-events.jsonl')[-1]['voice']
            expected=(original['star']['sha256'] if meta['name']=='se_STAR' else original['log_sounds'][meta['name']]['sha256']) if kind=='system' else voice_hash(meta['name'])
            assert data['sha256']==expected
            assert any(e['event']=='playing' and e['token']==token and e['kind']==kind for e in host)
            applied=[e for e in gain if e['token']==token and e['kind']==kind];assert applied and applied[0]['category_percent']==meta['category_percent']
            assert abs(applied[0]['output']-meta['category_percent']/100)<0.00001
            pcm=any(e['event']=='pcm_head' and e['token']==token and e['kind']==kind and e['value']>0 for e in host)
            progress=any(e['event']=='progress' and e['token']==token and e['kind']==kind and e['value']>0 for e in host)
            if kind=='system' and meta['name'] in ('se_PAGE','se_CLICK'):assert progress or pcm,(run,meta)
            if kind=='system' and meta['name']=='se_STAR':
                star_count+=1;progress_count+=progress
            results.append(dict(run=run,kind=kind,token=token,name=meta['name'],sha256=expected,category_percent=meta['category_percent'],playback_position_advanced=progress,pcm_head_advanced=pcm))
        cases=[c for c in report['log_checks'] if c['run']==run]
        expected_log=[sound for c in cases for sound in c.get('system_audio',[])]
        if expected_log:
            assert [e for e in ui if e['host_effect']=='system' and e['name']!='se_STAR']==expected_log
            initial=next(c for c in report['results'] if c['case']=='system_volume')
            for c in cases:
                sounds=c['system_audio'];assert [e['name'] for e in sounds]==['se_PAGE']+(['se_CLICK']*3 if c['count']>4 else [])+['se_PAGE']
                expected_volume=(initial['tested'] if c['run']==report['runs'][0] else 0 if c['run']==report['story_run'] and c['token']==13 else 37 if c['run']==report['story_run'] and c['token']==56 else initial['restored'])
                assert all(e['category_percent']==expected_volume for e in sounds),(c,expected_volume)
        lifecycle=lines(run,'audio-lifecycle.jsonl');opened=None;number=0
        cases=[c for c in report['log_checks'] if c['run']==run]
        for item in lifecycle:
            if item['event']=='log_open':assert opened is None;opened=item
            elif item['event']=='log_closed':
                assert opened is not None;case=cases[number];number+=1;
                for sound in case.get('system_audio',[]):assert sound in ui
                channels=opened['channels'];closed=item['channels'];bgm=False
                if 'bgm' in channels:
                    assert closed['bgm']['player_id']==channels['bgm']['player_id'] and closed['bgm']['playing'];bgm=True
                token=case['voice_token'];voice_state='not requested'
                if token is not None:
                    assert any(e['token']==token and e['host_effect']=='voice' for e in ui)
                    if 'voice' in closed and closed['voice']['token']==token:
                        assert closed['voice']['playing'];voice_state='same replay continues after close'
                    else:
                        assert any(e['event']=='completed' and e['token']==token and opened['time_ms']<=e['time_ms']<=item['time_ms'] for e in host);voice_state='replay completed before close'
                if case['background']:
                    paused=[e for e in lifecycle if opened['time_ms']<e['time_ms']<item['time_ms'] and e['event']=='paused'];assert paused
                    assert all(not c['playing'] for e in paused for c in e['channels'].values())
                log_results.append(dict(run=run,sequence=case['token'],count=case['count'],bgm_same_player=bgm,voice=voice_state,background=case['background']))
                opened=None
        assert opened is None and number==len(cases)
    assert star_count>0 and progress_count>0 and any(c['voice']!='not requested' for c in log_results)
    output=dict(ui_report_sha256=hashlib.sha256(Path(a.report).read_bytes()).hexdigest(),original_report_sha256=hashlib.sha256(Path(a.original).read_bytes()).hexdigest(),apk_sha256=report['apk_sha256'],execution=report['execution'],star_count=star_count,star_with_playback_progress=progress_count,log_sound_count=sum(e['kind']=='system' and e['name']!='se_STAR' for e in results),ui_audio=results,log_returns=log_results,all_sessions_vm_zero_and_audio_cache_empty=True,limitations=['No manual listening or Windows acoustic parity claim','MediaPlayer progress is not an AudioTrack PCM head observation','Replay uses same voice channel; original dialogue voice is not restarted on log close'])
    Path(a.output).write_text(json.dumps(output,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print('PASS message-log return, raw DAT audio identity, STAR gain/progress and cleanup')
if __name__=='__main__':main()
