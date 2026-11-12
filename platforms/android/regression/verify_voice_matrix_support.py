"""Machine-readable evidence shared by the five-game voice regression."""
import json


def emit(kind, **fields):
    print('MATRIX ' + json.dumps(dict(event=kind, **fields), sort_keys=True), flush=True)


def voice_result(game, tag, cue, active, probe, token=None, *, isolated=False):
    record = dict(game=game, tag=tag, cue=cue, state='playing' if active else 'silent',
                  run=probe.run, origin='isolated' if isolated else 'route',
                  original_audio=not (game == 'BIRTH' and tag == 'KA') or cue == 'KA_EP02_0000')
    if active:
        gain = probe.wait('nonzero voice gain', lambda: next(
            (e for e in probe.events('audio-gain-events.jsonl')
             if e['kind'] == 'voice' and e['token'] == token and e['output'] > 0), None), 12)
        meta = probe.obj(f'audio-{token}.json')
        assert meta['kind'] == 'voice' and meta['name'] == cue, meta
        if game == 'BIRTH' and tag == 'KA' and cue == 'KA_EP02_0000':
            assert meta['sha256'] == 'a4ec2d6cc1bf1abff0089d82574d8789e6ecdffae5b456e01906b5823f8cf21e', meta
        record.update(token=token, sha256=meta['sha256'], output_gain=gain['output'])
    emit('result', **record)
