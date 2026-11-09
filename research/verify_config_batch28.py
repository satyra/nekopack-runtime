"""Independent replay of translated Config, lifecycle and full BLESS audio evidence."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

p=argparse.ArgumentParser()
p.add_argument('--adb',required=True)
p.add_argument('--serial',required=True)
a=p.parse_args()
def load(name):return json.loads(Path(f'research/config-batch28-{name}.json').read_text(encoding='utf8'))
original=load('original');config=load('translated');audio=load('audio-state');lifecycle=load('lifecycle');story=load('story')
frames=load('frames');sound=load('audio-evidence')
sha=config['apk_sha256']
assert frames['execution']=='translated ARM64' and frames['build']['sha256']['nekopack-arm64-probe.apk']==sha
assert sound['reports'][0]['execution']=='translated ARM64' and sound['reports'][0]['apk_sha256']==sha
assert audio['apk_sha256']==sha
assert lifecycle['build']['sha256']['nekopack-arm64-probe.apk']==sha
assert story['build']['sha256']['nekopack-arm64-probe.apk']==sha
for r in (config,lifecycle,audio):assert r['machine']=='x86_64' and r['native_bridge']=='libnb.so'
assert story['execution']=='translated ARM64'
assert config['defaults']==original['defaults']
assert len(config['changes'])==10 and config['vm_live_bytes']==0
expected=config['defaults'].copy()
for change in config['changes']:
    expected[change['channel']]=change['percent']
    assert change['config']==expected
assert audio['closed_vm_live_bytes']==0 and audio['evidence']['passed']
samples={s['label']:s for s in audio['evidence']['samples']}
assert samples['initial']['gain']==.25 and samples['regained']['gain']==.25
assert abs(samples['repeat_fade_start']['gain']-samples['first_fade']['gain'])<.001
assert samples['repeat_fade_progress']['gain']<samples['repeat_fade_start']['gain']
assert len(lifecycle['pages'])==61 and len(lifecycle['cycles'])==5 and lifecycle['vm_live_bytes']==0
for c in lifecycle['cycles']:
    assert c['paused']['instance']==c['resumed']['instance']
    assert 0<=c['resumed']['active_ms']-c['paused']['active_ms']<100
    assert c['resumed']['time_ms']-c['paused']['time_ms']>1200
g=story['games'][0];assert len(g['pages'])==793 and len(g['audio'])==493 and g['closed_vm_live_bytes']==0
run=g['run_id'];assert re.fullmatch(r'story_[0-9a-f]{32}',run)
def device_rows(name):
    data=subprocess.check_output([a.adb,'-s',a.serial,'exec-out','run-as','org.nekopack.probe','cat',f'files/{run}/{name}'],timeout=45)
    return [json.loads(line) for line in data.splitlines()]
gains=device_rows('audio-gain-events.jsonl');configs=device_rows('audio-config-events.jsonl')
assert configs==[original['defaults']], 'new session did not load original defaults'
command=dict(bgm=.5,env=.5,se=1,voice=1)
for e in gains:
    assert e['category_percent']==original['defaults'][e['kind']]
    expected=command[e['kind']]*e['category_percent']/100*e['fade']*e['duck']
    assert abs(e['output']-expected)<.000001
counts={k:0 for k in command}
for e in g['host_events']:
    if e['event']!='playing' or e['kind'] not in command:continue
    rows=[v for v in gains if v['kind']==e['kind'] and v['token']==e['token'] and v['time_ms']<=e['time_ms']]
    assert rows and rows[-1]['output']==command[e['kind']]*original['defaults'][e['kind']]/100
    counts[e['kind']]+=1
assert sum(counts.values())==493 and all(counts.values())
evidence=dict(run_id=run,apk_sha256=sha,execution='translated ARM64 on x86_64',config_events=configs,gain_events=gains,play_counts=counts)
Path('research/config-batch28-gains.json').write_text(json.dumps(evidence,indent=2)+'\n',encoding='utf8')
summary=dict(scope='BLESS translated ARM64; category gain/device state, not acoustic parity or complete Config/message support',apk_sha256=sha,pages=793,audio_requests=493,play_counts=counts,motion_frames=len(frames['frames']),lifecycle_cycles=5,slider_changes=10,vm_live_bytes=0,reports={s:hashlib.sha256(Path(f'research/config-batch28-{s}.json').read_text(encoding='utf8').encode('utf8')).hexdigest() for s in ('original','translated','audio-state','lifecycle','story','gains','frames','audio-evidence')})
Path('research/config-batch28-summary.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8')
print('PASS Config evidence',counts)
