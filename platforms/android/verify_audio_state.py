"""Native BLESS audio probe: real OS focus transfer and production fade handler."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import time
import uuid
import xml.etree.ElementTree as ET


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--adb', required=True)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--reference', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--execution', choices=('native','translated'), default='native')
    args = parser.parse_args()
    package = 'org.nekopack.probe'
    run = 'audio_state_' + uuid.uuid4().hex

    def adb(*command):
        return subprocess.check_output([args.adb, '-s', args.serial, *command], timeout=45)

    def private(name):
        return adb('exec-out', 'run-as', package, 'cat', f'files/{run}/{name}')

    skipped_movies = set()

    def wait(name):
        deadline = time.monotonic() + 180
        while time.monotonic() < deadline:
            result = private(name)
            if result.startswith(b'{'):
                page=re.fullmatch(r'page-(\d+)\.json',name)
                text=private(f'text-{page[1]}.json') if page else b'{"complete":true}'
                if text.startswith(b'{') and json.loads(text)['complete']:return json.loads(result)
            paths=adb('shell','run-as',package,'sh','-c',f"'printf \"%s\\n\" files/{run}/movie-*-events.jsonl'")
            for entry in [path.rsplit('/',1)[-1] for path in paths.decode().splitlines()]:
                if re.fullmatch(r'movie-\d+-events.jsonl', entry) and entry not in skipped_movies:
                    events = [json.loads(line) for line in private(entry).splitlines()]
                    if any(e['event'] == 'progress' and e['head'] > 0 for e in events) and not any(e['event'] == 'closed' for e in events):
                        adb('shell', 'input', 'tap', *point('返回剧情'))
                        skipped_movies.add(entry)
            error = private('error.txt')
            assert b'No such file' in error, error.decode()
            time.sleep(.15)
        raise AssertionError('timeout: ' + name)

    def point(label):
        adb('shell', 'uiautomator', 'dump', '/sdcard/neko-audio-state.xml')
        nodes = ET.fromstring(adb('shell', 'cat', '/sdcard/neko-audio-state.xml'))
        node = next(n for n in nodes.iter('node') if n.get('text') == label and n.get('enabled') == 'true')
        x, y, r, b = map(int, re.findall(r'\d+', node.get('bounds')))
        return str((x+r)//2), str((y+b)//2)

    build = json.loads(Path('.local/android-probe/build.json').read_text())
    apk = adb('shell', 'pm', 'path', package).decode().strip().removeprefix('package:')
    apk_hash = adb('shell', 'sha256sum', apk).decode().split()[0]
    assert apk_hash == build['sha256']['nekopack-arm64-probe.apk']
    abi = adb('shell', 'getprop', 'ro.product.cpu.abi').decode().strip()
    bridge = adb('shell', 'getprop', 'ro.dalvik.vm.native.bridge').decode().strip()
    machine = adb('shell', 'uname', '-m').decode().strip()
    if args.execution == 'native':
        assert machine == 'aarch64' and abi == 'arm64-v8a' and bridge in ('', '0'), (machine, abi, bridge)
    else:
        assert machine == 'x86_64' and bridge not in ('', '0'), (machine, bridge)
    adb('shell', 'am', 'start', '-W', '-S', '-n', package+'/.MainActivity', '--ei', 'story_game', '0', '--es', 'run_id', run, '--ez', 'audio_state_probe', 'true')
    next_point = None
    for sequence in range(1, 14):
        page = wait(f'page-{sequence}.json')
        assert page == json.loads((args.reference/f'{sequence-1}.json').read_text(encoding='utf8'))
        assert private(f'page-{sequence}.png') == (args.reference/f'{sequence-1}.png').read_bytes()
        if sequence < 13:
            if next_point is None:
                next_point = point('继续')
            adb('shell', 'input', 'tap', *next_point)
    evidence = wait('audio-state-probe.json')
    assert evidence['passed']
    events = [json.loads(line) for line in private('host-events.jsonl').splitlines()]
    assert any(e['event'] == 'focus' and e['value'] == -2 for e in events)
    assert any(e['event'] == 'focus' and e['value'] == 1 for e in events)
    adb('shell', 'input', 'tap', *point('结束运行'))
    deadline = time.monotonic()+15
    while private('closed.txt') != b'closed; VM live bytes=0':
        assert time.monotonic() < deadline, 'VM close timeout'
        time.sleep(.2)
    args.output.write_text(json.dumps(dict(run_id=run, serial=args.serial, execution=args.execution, machine=machine, abi=abi, native_bridge=bridge, apk_sha256=apk_hash, pages=13, evidence=evidence, events=events, closed_vm_live_bytes=0), ensure_ascii=False, indent=2)+'\n', encoding='utf8')
    print(f'PASS: {run}, 13 BLESS pages, focus/repeated fade, VM live bytes=0')


if __name__ == '__main__':
    main()
