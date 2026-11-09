"""Replay batch 29 feature evidence and preserve the rejected long-run audio evidence.

Passing this audit does not make the 793-page continuous regression pass.
"""
import argparse,hashlib,json,re,subprocess
from pathlib import Path
p=argparse.ArgumentParser()
p.add_argument('--adb',required=True);p.add_argument('--serial',required=True);p.add_argument('--reference',type=Path,required=True)
p.add_argument('--run-id',required=True)
a=p.parse_args()
def load(s):return json.loads(Path(f'research/text-batch29-{s}.json').read_text(encoding='utf8'))
special=load('translated');life=load('lifecycle');audio=load('audio-state');config=load('audio-config');original=load('original')
sha=special['apk_sha256']
for r in (audio,config):assert r['apk_sha256']==sha
assert life['build']['sha256']['nekopack-arm64-probe.apk']==sha
assert special['machine']=='x86_64' and special['native_bridge']=='libnb.so'
assert [c['keep_voice'] for c in special['cases']]==[1,0]
for case in special['cases']:
    assert case['vm_live_bytes']==0 and case['pages']==5 and case['closed_while_revealing']
    assert not case['paused']['complete'] and not case['resumed']['complete']
    assert 0<case['resumed']['elapsed_ms']-case['paused']['elapsed_ms']<500
    for n in (2,3,4):
        clicks=[e for e in case['text_events'] if e['sequence']==n and e['click']]
        assert len(clicks)==2 and clicks[0]['complete'] and not clicks[0]['advance'] and not clicks[0]['stop_voice']
        assert clicks[1]['complete'] and clicks[1]['advance']
    voice=next(e for e in case['voice_events'] if e['sequence']==4)
    assert voice['playing_before'] and voice['player']!=0
    assert voice['stop']==(case['keep_voice']==0) and voice['retained']==(case['keep_voice']==1)
assert len(life['pages'])==61 and len(life['cycles'])==5 and life['vm_live_bytes']==0
assert audio['evidence']['passed'] and audio['closed_vm_live_bytes']==0
assert len(config['changes'])==10 and config['vm_live_bytes']==0
run=a.run_id;assert re.fullmatch(r'story_[0-9a-f]{32}',run)
prefix=[a.adb,'-s',a.serial]
apk=subprocess.check_output(prefix+['shell','pm','path','org.nekopack.probe'],timeout=30).decode().strip().removeprefix('package:')
assert subprocess.check_output(prefix+['shell','sha256sum',apk],timeout=30).decode().split()[0]==sha
assert subprocess.check_output(prefix+['exec-out','run-as','org.nekopack.probe','cat',f'files/{run}/closed.txt'],timeout=30)==b'closed; VM live bytes=0'
def rows(name):
    raw=subprocess.check_output([a.adb,'-s',a.serial,'exec-out','run-as','org.nekopack.probe','cat',f'files/{run}/{name}'],timeout=45)
    return [json.loads(line) for line in raw.splitlines()]
events=rows('text-events.jsonl');voices=rows('voice-next-events.jsonl');settings=rows('text-config-events.jsonl')
raw=subprocess.check_output([a.adb,'-s',a.serial,'shell','run-as','org.nekopack.probe','sh','-c',f"'cat files/{run}/text-[0-9]*.json'"],timeout=45).decode('utf8')
decoder=json.JSONDecoder();at=0;snapshots={}
while at<len(raw):
    snapshot,at=decoder.raw_decode(raw,at)
    assert snapshot['sequence'] not in snapshots
    snapshots[snapshot['sequence']]=snapshot
assert set(snapshots)==set(range(1,543))
sequences=[e['sequence'] for e in events]
assert sequences==sorted(sequences) and set(sequences)==set(range(1,543)), 'late or missing page callbacks'
assert settings==[dict(speed=original['facts']['default_speed'],keep_voice=original['facts']['default_keep_voice'])]
pages=[]
png_rows=subprocess.check_output(prefix+['shell','run-as','org.nekopack.probe','sh','-c',f"'sha256sum files/{run}/page-*.png'"],timeout=45).decode().splitlines()
png_hashes={int(re.search(r'page-(\d+)\.png$',line)[1]):line.split()[0] for line in png_rows}
assert set(png_hashes)==set(snapshots)
for n in range(1,543):
    expected=json.loads((a.reference/f'{n-1}.json').read_text(encoding='utf8'));length=len(expected['text'])
    snapshot=snapshots[n]
    assert snapshot['text']==expected['text'] and snapshot['complete']
    assert snapshot['advance']==(n<542) and not snapshot['stop_voice']
    assert png_hashes[n]==hashlib.sha256((a.reference/f'{n-1}.png').read_bytes()).hexdigest()
    group=[e for e in events if e['sequence']==n];assert group and group[0]['elapsed_ms']==0
    assert [e['elapsed_ms'] for e in group]==sorted(e['elapsed_ms'] for e in group)
    for e in group:
        assert e['visible']==min(length,e['elapsed_ms']*60//1000)
        assert e['complete']==(e['visible']==length)
        assert not e['stop_voice']
    clicks=[e for e in group if e['click']]
    assert len(clicks)==(1 if n<542 else 0)
    assert all(e['advance'] and e['complete'] for e in clicks)
    assert group[-1]['complete']
    pages.append(dict(sequence=n,characters=length,png_sha256=png_hashes[n],frames=len(group),partial_frames=sum(0<e['visible']<length for e in group),complete_ms=next(e['elapsed_ms'] for e in group if e['complete']),advance_clicks=len(clicks)))
assert len(voices)==541 and [v['sequence'] for v in voices]==list(range(1,542))
assert all(not v['stop'] and v['retained']==(v['player']!=0) for v in voices)
retained=sum(v['playing_before'] and v['retained'] for v in voices);assert retained>0
import importlib.util
spec=importlib.util.spec_from_file_location('story_verify',Path('platforms/android/verify_story.py'));v=importlib.util.module_from_spec(spec);spec.loader.exec_module(v)
host=rows('host-events.jsonl');token=352;rejected=[e for e in host if e['token']==token]
playing=next(e for e in rejected if e['event']=='playing');completed=next(e for e in rejected if e['event']=='completed')
assert playing['kind']=='se' and playing['value']==372
assert completed['time_ms']-playing['time_ms']==368
assert not any(e['event'] in ('progress','completed') and e['value']>0 for e in rejected)
assert not v.completed_audio(host,token) and v.replacement_audio_token(host,token) is None
failure=dict(page=542,reason='audio did not advance within the observation window',events=rejected,duration_ms=playing['value'],completion_elapsed_ms=completed['time_ms']-playing['time_ms'],strict_audio_evidence_passed=False)
result=dict(scope='542-page text/image evidence from a rejected continuous run; not a successful 793-page regression or original glyph rendering parity',continuous_regression_passed=False,run_id=run,apk_sha256=sha,settings=settings,pages=pages,retained_playing_voices=retained,voice_transitions=voices,audio_failure=failure,vm_live_bytes=0)
Path('research/text-batch29-state.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
summary=dict(apk_sha256=sha,execution='translated ARM64',continuous_regression_passed=False,planned_continuous_pages=793,text_and_png_pages_verified=542,continuous_audio_failure=failure,retained_playing_voices=retained,lifecycle_cycles=5,keep_voice_cases=[1,0],vm_live_bytes=0,reports={s:hashlib.sha256(Path(f'research/text-batch29-{s}.json').read_text(encoding='utf8').encode('utf8')).hexdigest() for s in ('original','translated','lifecycle','audio-state','audio-config','state')})
Path('research/text-batch29-summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8')
print('Feature evidence verified:',len(pages),'text/PNG pages;',retained,'playing voices retained. Continuous regression FAILED audio evidence; preserved without relaxation.')
raise SystemExit(2)  # Do not let an automated caller count this as full regression success.
