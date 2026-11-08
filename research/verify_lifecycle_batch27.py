"""Replay final native lifecycle evidence, independently of the ADB test driver."""
import hashlib
import json
from pathlib import Path

def load(suffix):
    return json.loads(Path(f'research/lifecycle-batch27-{suffix}.json').read_text(encoding='utf8'))

native=load('native');destroy=load('destroy');movie=load('movie');audio=load('audio');story=load('story')
frames=load('frames');sound=load('audio-evidence')
sha=native['build']['sha256']['nekopack-arm64-probe.apk']
for report in (destroy,movie,story):
    assert report['build']['sha256']['nekopack-arm64-probe.apk']==sha
    assert report['execution']=='native ARM64'
assert frames['build']['sha256']['nekopack-arm64-probe.apk']==sha
assert sound['reports'][0]['apk_sha256']==sha
assert audio['apk_sha256']==sha and audio['abi']=='arm64-v8a' and audio['native_bridge'] in ('','0')
assert len(native['pages'])==61 and native['vm_live_bytes']==0
assert [c['label'] for c in native['cycles']]==['fade','movie','dialogue_bgm','dialogue_bgm_repeat','motion']
assert len({c['paused']['instance'] for c in native['cycles']})==1
assert len({c['pid'] for c in native['cycles']})==1
for cycle in native['cycles']+movie['cycles']:
    start=cycle['paused'];end=cycle['resumed']
    assert start['instance']==end['instance']
    assert end['time_ms']-start['time_ms']>=1200
    assert 0<=end['active_ms']-start['active_ms']<100
for cycle in native['cycles']:
    start=cycle['paused']['time_ms'];end=cycle['resumed']['time_ms']
    assert not any(start<e['time_ms']<end and e['event']=='ack' for e in native['host_events'])
    assert not any(start<e['time_ms']<end for e in native['motion_events'])
    if cycle['label'].startswith('dialogue_bgm'):
        snapshots=[e for e in native['audio_lifecycle'] if start<=e['time_ms']<=end]
        first=next(e for e in snapshots if e['event']=='paused_stable_1')['channels']['bgm']
        second=next(e for e in snapshots if e['event']=='paused_stable_2')['channels']['bgm']
        before=next(e for e in snapshots if e['event']=='resuming')['channels']['bgm']
        after=next(e for e in snapshots if e['event']=='resumed')['channels']['bgm']
        assert first['position']==second['position']==before['position']
        assert not first['playing'] and not second['playing'] and not before['playing'] and after['playing']
        assert len({e['player_id'] for e in (first,second,before,after)})==1
motion=[e for e in native['motion_events'] if e['sequence']==56]
pause=native['cycles'][-1]['paused']['time_ms']
assert any(e['event']=='frame' and 0<e['elapsed_ms']<250 and e['time_ms']<pause for e in motion)
assert motion[-1]['event']=='finished' and 250<=motion[-1]['elapsed_ms']<500
assert motion[-1]['time_ms']-motion[0]['time_ms']>1500
assert not any(e['event']=='cancelled' for e in motion)
frame_times=[e['elapsed_ms'] for e in motion if e['event']=='frame']
assert frame_times==sorted(frame_times)
for report,reason in ((native,2),(destroy,3),(movie,0)):
    events=report['movie_events']
    assert sum(e['event']=='started' for e in events)==1
    end=events[-1]
    assert end['event']=='closed' and end['error'] is None and end['reason']==reason
    if reason!=3:
        assert sum(e['event']=='surface_created' for e in events)>=2
        paused=next(e for e in reversed(events) if e['event']=='paused')
        resumed=next(e for e in reversed(events) if e['event']=='resumed')
        assert resumed['time_ms']-paused['time_ms']>=1200
        assert 0<=resumed['active_ms']-paused['active_ms']<100
        progress=[e['frames'] for e in events if e['event']=='progress']
        assert progress==sorted(progress)
assert destroy['destroyed_while_paused'] and destroy['vm_live_bytes']==0
assert movie['movie_natural_completion']
end=movie['movie_events'][-1];info=next(e for e in movie['movie_events'] if e['event']=='started')
assert end['head']>=end['samples'] and end['samples']/info['sample_rate']>=info['duration']-.2
assert end['frames']>=int(info['duration']*info['fps'])
assert audio['closed_vm_live_bytes']==0 and audio['evidence']['passed']
game=story['games'][0]
assert len(story['games'])==1 and game['index']==0 and game['closed_vm_live_bytes']==0
previous=json.loads(Path('research/audio-state-batch26-story.json').read_text(encoding='utf8'))['games'][0]
assert game['pages']==previous['pages'] and len(game['pages'])==793 and len(game['audio'])==493
for kind,percent,count in (('bgm',50,19),('env',50,1),('se',100,7)):
    gains=[e for e in game['host_events'] if e['event']=='gain' and e['kind']==kind]
    assert len(gains)==count and all(e['value']==percent for e in gains)
result=dict(scope='Live Activity background recovery, not durable save/process recreation',apk_sha256=sha,
            lifecycle_cycles=6,lifecycle_pages=61,regression_pages=793,audio_requests=493,motion_frames=len(frames['frames']),
            paused_motion_logical_ms=motion[-1]['elapsed_ms'],paused_motion_wall_ms=motion[-1]['time_ms']-motion[0]['time_ms'],
            natural_movie_frames=end['frames'],natural_movie_samples=end['samples'],vm_live_bytes=0,reports={})
for name in ('native','destroy','movie','audio','story','frames','audio-evidence'):
    path=Path(f'research/lifecycle-batch27-{name}.json')
    result['reports'][name]=hashlib.sha256(path.read_text(encoding='utf8').encode()).hexdigest()
Path('research/lifecycle-batch27-summary.json').write_text(json.dumps(result,indent=2)+'\n',encoding='utf8')
print('PASS lifecycle evidence: paused callbacks/clock/player/decoder; normal 793-page regression')
