"""Exercise the real Android movie host without reading images or story text.

This standalone SAF probe verifies decoding, presentation, audio drain and EOF;
it does not claim execution of a scenario movie command.
"""
import argparse
import hashlib
import json
import shlex
import subprocess
import time
import uuid
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--adb', default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    parser.add_argument('--serial', required=True)
    parser.add_argument('--media', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--pause', action='store_true')
    args = parser.parse_args()
    package = 'org.nekopack.probe'
    run = 'movie-metadata-' + uuid.uuid4().hex[:8]

    def adb(*command):
        return subprocess.check_output([args.adb, '-s', args.serial, *command], timeout=90)

    def read(name):
        path = f'files/{run}/{name}'
        command = f'if test -f {shlex.quote(path)}; then cat {shlex.quote(path)}; fi'
        return adb('shell', 'run-as', package, 'sh', '-c', shlex.quote(command))

    apk = adb('shell', 'pm', 'path', package).decode().strip().removeprefix('package:')
    apk_sha = adb('shell', 'sha256sum', apk).split()[0].decode()
    adb('shell', 'am', 'start', '-W', '-S', '-n', package+'/.MainActivity',
        '--es', 'movie_probe_name', args.media.name, '--es', 'run_id', run)
    deadline = time.monotonic()+900
    paused = False
    while time.monotonic() < deadline:
        error = read('error.txt')
        assert not error, error.decode(errors='replace')
        events = [json.loads(line) for line in read('movie-1-events.jsonl').splitlines()]
        if events and events[-1]['event'] == 'closed':
            break
        if args.pause and not paused and any(e['event'] == 'progress' for e in events):
            adb('shell', 'input', 'keyevent', 'KEYCODE_HOME')
            time.sleep(3)
            adb('shell', 'am', 'start', '-W', '--activity-reorder-to-front', '-n', package+'/.MainActivity')
            paused = True
        time.sleep(2)
    else:
        raise AssertionError('movie playback deadline')
    end = events[-1]
    info = json.loads(read('movie-1-info.json'))
    assert end['reason'] == 0 and end['error'] is None, end
    assert json.loads(read('movie-complete.json'))['reason'] == 0
    assert info['source_sha256'] == hashlib.sha256(args.media.read_bytes()).hexdigest()
    assert end['frames'] > 0 and end['head'] >= end['samples'] > 0
    assert end['pcm_nonzero_samples'] > 0 and end['pcm_peak_amplitude'] > 0
    assert end.get('decoded_frames', end['frames']) == end.get('presented_frames', end['frames'])+end.get('dropped_frames', 0)
    assert end.get('presented_frames', end['frames']) > 0
    eof = next(e for e in events if e['event'] == 'decoder_ended')
    assert abs(end['samples']-eof['audio_end_time']*info['sample_rate']) <= 1
    assert end['elapsed_ms'] >= max(eof['audio_end_time'], eof['video_end_time'])*1000-200
    if args.pause:
        assert paused and any(e['event'] == 'paused' for e in events)
        assert any(e['event'] == 'resumed' for e in events)
    progress = [e for e in events if e['event'] == 'progress']
    max_av_delta = max(abs(e['video_time']-e['head']/info['sample_rate']) for e in progress)
    assert max_av_delta < 0.5, ('audio/video timeline drift', max_av_delta)
    duration = max(eof['audio_end_time'], eof['video_end_time'])
    assert end['elapsed_ms'] < duration*1150+5000, ('playback slower than real time', end)
    result = dict(scope='standalone real Android movie host; no scenario-command claim',
                  run=run, serial=args.serial, apk_sha256=apk_sha, media=args.media.name,
                  info=info, events=events, paused=paused, max_av_delta_seconds=max_av_delta, passed=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
    print(json.dumps(dict(run=run, passed=True, frames=end['frames'], samples=end['samples'])))


if __name__ == '__main__':
    main()
