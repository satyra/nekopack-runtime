"""Check captured BLESS audio-state evidence; no simulated playback results."""
import json
from pathlib import Path
import hashlib


def read(name):
    return json.loads(Path(name).read_text(encoding='utf8'))


probe = read('research/audio-state-batch26-native.json')
story = read('research/audio-state-batch26-story.json')
old = read('research/se-batch25-native.json')
assert story['execution'] == 'native ARM64'
assert probe['apk_sha256'] == story['build']['sha256']['nekopack-arm64-probe.apk']
assert probe['closed_vm_live_bytes'] == 0
assert len(story['games']) == 1
game = story['games'][0]
assert game['index'] == 0 and len(game['pages']) == 793 and game['closed_vm_live_bytes'] == 0
assert game['pages'] == [{k:v for k,v in page.items() if k != 'screen'} for page in old['games'][0]['pages']]
assert len(game['audio']) == 493
events = game['host_events']
counts = {}
for begin in events:
    if begin['event'] != 'begin' or begin['kind'] not in ('bgm', 'env', 'se'):
        continue
    kind = begin['kind']
    gains = [e for e in events if e['event'] == 'gain' and e['token'] == begin['token'] and e['kind'] == kind]
    assert len(gains) == 1 and gains[0]['value'] == (100 if kind == 'se' else 50)
    counts[kind] = counts.get(kind, 0) + 1
probe_events = probe['events']
pending = next(i for i,e in enumerate(probe_events) if e['event'] == 'focus_pending' and e['token'] == -3)
gain = next(i for i,e in enumerate(probe_events) if i > pending and e['event'] == 'focus' and e['value'] == 1)
playing = next(i for i,e in enumerate(probe_events) if e['event'] == 'playing' and e['token'] == -3)
acks = [i for i,e in enumerate(probe_events) if e['event'] == 'ack' and e['token'] == -3]
assert len(acks) == 1 and pending < gain < playing < acks[0]
samples = {s['label']: s for s in probe['evidence']['samples']}
assert samples['initial']['gain'] == .5
assert samples['transient_loss']['suspended'] and not samples['transient_loss']['playing']
assert abs(samples['paused_position']['position']-samples['transient_loss']['position']) <= 30
assert samples['regained']['playing'] and not samples['regained']['suspended']
assert samples['prepared_while_paused']['token'] == -3 and not samples['prepared_while_paused']['playing']
assert samples['regained']['position'] > samples['prepared_while_paused']['position']
assert 0 < samples['first_fade']['gain'] < .5
assert abs(samples['repeat_fade_start']['gain']-samples['first_fade']['gain']) < .001
assert 0 < samples['repeat_fade_progress']['gain'] < samples['repeat_fade_start']['gain']
result = dict(scope='Native ARM64 BLESS audio corrections; not full lifecycle/save/interaction compatibility',
              pages=793, audio_requests=493, gain_checks=counts, closed_vm_live_bytes=0,
              apk_sha256=probe['apk_sha256'], reports={})
for name in ('native', 'story'):
    path = Path(f'research/audio-state-batch26-{name}.json')
    result['reports'][name] = hashlib.sha256(path.read_text(encoding='utf8').encode()).hexdigest()
Path('research/audio-state-batch26-summary.json').write_text(json.dumps(result, indent=2)+'\n', encoding='utf8')
print('PASS: audio state, 793 unchanged pages, 493 audio requests, gain checks', counts)
