"""Independent PCM-head and text-state audit of the completed batch 30 device run."""
import argparse,hashlib,json,re,subprocess
from pathlib import Path
p=argparse.ArgumentParser();p.add_argument('--adb',required=True);p.add_argument('--serial',required=True);p.add_argument('--reference',type=Path,required=True)
a=p.parse_args()
def load(name):return json.loads(Path(f'research/audio-batch30-{name}.json').read_text(encoding='utf8'))
story=load('story');special=load('dialogue');life=load('lifecycle');state=load('state');config=load('config');frames=load('frames');sound=load('evidence');probe=load('short-probe')
sha=story['build']['sha256']['nekopack-arm64-probe.apk']
assert story['execution']==frames['execution']=='translated ARM64' and story['text_speed_override'] is None
assert story['device']['kernel_machine']=='x86_64' and story['device']['ro.dalvik.vm.native.bridge']=='libnb.so'
for report in (special,state,config):assert report['apk_sha256']==sha
for report in (life,frames):assert report['build']['sha256']['nekopack-arm64-probe.apk']==sha
assert sound['reports'][0]['apk_sha256']==sha
assert len(life['pages'])==61 and len(life['cycles'])==5 and life['vm_live_bytes']==0
assert state['evidence']['passed'] and state['closed_vm_live_bytes']==0
assert len(config['changes'])==10 and config['vm_live_bytes']==0
assert [c['keep_voice'] for c in special['cases']]==[1,0]
assert all(c['closed_while_revealing'] and c['vm_live_bytes']==0 for c in special['cases'])
for path,digest in probe['source_sha256_lf_utf8'].items():assert hashlib.sha256(Path(path).read_text(encoding='utf8').encode()).hexdigest()==digest
game=story['games'][0];assert len(game['pages'])==793 and len(game['audio'])==493 and game['closed_vm_live_bytes']==0
host=game['host_events'];pcm=[]
for start in [e for e in host if e['event']=='pcm_frames']:
    token=start['token'];events=[e for e in host if e['token']==token]
    rate=next(e['value'] for e in events if e['event']=='pcm_rate')
    heads=[e['value'] for e in events if e['event']=='pcm_head']
    assert start['kind'] in ('se','voice') and start['value']>0 and rate>0
    assert heads==sorted(heads) and all(0<=h<=start['value'] for h in heads)
    ended=[e for e in events if e['event']=='completed']
    if ended:
        assert len(ended)==1 and heads[-1]==start['value']
        assert ended[0]['value']==start['value']*1000//rate
        assert events.index(ended[0])>max(i for i,e in enumerate(events) if e['event']=='pcm_head')
    for i,e in enumerate(events):
        if e['event']=='progress':
            assert i>0 and events[i-1]['event']=='pcm_head'
            # Separate hardware queries can advance by a few frames; progress must not exceed the sampled head.
            assert 0<=e['value']<=events[i-1]['value']*1000//rate
    pcm.append(dict(token=token,kind=start['kind'],frames=start['value'],rate=rate,heads=heads,completed=bool(ended)))
assert next(r for r in pcm if r['token']==352)['frames']==16384
assert next(r for r in pcm if r['token']==352)['completed']
assert next(r for r in pcm if r['token']==96)['completed']
run=game['run_id'];assert re.fullmatch('story_[0-9a-f]{32}',run)
prefix=[a.adb,'-s',a.serial]
def raw(name):return subprocess.check_output(prefix+['exec-out','run-as','org.nekopack.probe','cat',f'files/{run}/{name}'],timeout=45)
def rows(name):return [json.loads(x) for x in raw(name).splitlines()]
events=rows('text-events.jsonl');voices=rows('voice-next-events.jsonl')
assert rows('text-config-events.jsonl')==[dict(speed=50,keep_voice=1)]
assert raw('closed.txt')==b'closed; VM live bytes=0'
snapshot_raw=subprocess.check_output(prefix+['shell','run-as','org.nekopack.probe','sh','-c',f"'cat files/{run}/text-[0-9]*.json'"],timeout=45).decode('utf8')
decoder=json.JSONDecoder();at=0;snapshots={}
while at<len(snapshot_raw):
    s,at=decoder.raw_decode(snapshot_raw,at);assert s['sequence'] not in snapshots;snapshots[s['sequence']]=s
assert set(snapshots)==set(range(1,794))
sequence=[e['sequence'] for e in events];assert sequence==sorted(sequence) and set(sequence)==set(snapshots)
pages=[]
for n in range(1,794):
    expected=json.loads((a.reference/f'{n-1}.json').read_text(encoding='utf8'));text=expected['text'];s=snapshots[n]
    assert s['text']==text and s['complete'] and s['advance']==(n<793) and not s['stop_voice']
    group=[e for e in events if e['sequence']==n];assert group[0]['elapsed_ms']==0
    assert [e['elapsed_ms'] for e in group]==sorted(e['elapsed_ms'] for e in group)
    assert all(e['visible']==min(len(text),e['elapsed_ms']*60//1000) and e['complete']==(e['visible']==len(text)) and not e['stop_voice'] for e in group)
    clicks=[e for e in group if e['click']];assert len(clicks)==(1 if n<793 else 0)
    assert all(e['advance'] and e['complete'] for e in clicks)
    pages.append(dict(sequence=n,characters=len(text),frames=len(group),complete_ms=next(e['elapsed_ms'] for e in group if e['complete'])))
assert len(voices)==792 and [v['sequence'] for v in voices]==list(range(1,793))
assert all(not v['stop'] and v['retained']==(v['player']!=0) for v in voices)
retained=sum(v['playing_before'] and v['retained'] for v in voices);assert retained>0
result=dict(scope='BLESS translated ARM64, default-speed 793-page run; PCM-head evidence is not acoustic equivalence',apk_sha256=sha,run_id=run,continuous_regression_passed=True,pages=pages,pcm=pcm,retained_playing_voices=retained,motion_frames=len(frames['frames']),audio_requests=len(game['audio']),vm_live_bytes=0,reports={name:hashlib.sha256(Path(f'research/audio-batch30-{name}.json').read_text(encoding='utf8').encode()).hexdigest() for name in ('story','dialogue','lifecycle','state','config','frames','evidence','short-probe')})
Path('research/audio-batch30-summary.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
print('PASS 793-page text replay and',len(pcm),'PCM clips;',len(frames['frames']),'motion frames')
