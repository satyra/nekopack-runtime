"""Load a real runtime checkpoint and verify scenario WMV playback; metadata only."""
import argparse
import hashlib
import importlib.util
import json
import shlex
import time
import uuid
from pathlib import Path

spec = importlib.util.spec_from_file_location('navigation', Path(__file__).with_name('verify-sorakoi-layout-navigation.py'))
nav = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nav)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--adb', default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    parser.add_argument('--save', type=Path, required=True)
    parser.add_argument('--media', type=Path, required=True)
    parser.add_argument('--translated', action='store_true')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run = 'movie-chain-' + uuid.uuid4().hex[:8]
    p = nav.NavigationProbe(args.adb, args.serial, run, {})
    tree = 'content://com.android.providers.downloads.documents/tree/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-Sorakoi'
    scope = hashlib.sha256((tree+'_0').encode()).hexdigest()
    directory = f'files/saves/{scope}' + ('_zh' if args.translated else '') + '_probe_' + run
    remote = '/data/local/tmp/' + run + '.nksv'
    p.adb('push', str(args.save), remote)
    p.adb('shell', 'run-as', 'org.nekopack.probe', 'mkdir', '-p', directory)
    p.adb('shell', 'run-as', 'org.nekopack.probe', 'cp', remote, directory+'/slot-001.nksv')
    p.start_title('--ei', 'title_game', '0', '--es', 'library_entry', 'pk02-validation',
                  '--ez', 'library_original', str(not args.translated).lower(),
                  '--ez', 'dialogue_probe', 'true', '--es', 'save_probe', run,
                  '--ei', 'library_resume_slot', '1')
    def loaded_page():
        state = p.obj('save-state.json')
        assert not state or state['event'] != 'error', state
        return p.obj('page-1.json')
    page = p.wait('loaded checkpoint', loaded_page)
    p.wait('text complete', lambda: (s if (s := p.obj('text-1.json')) and s['complete'] else None))
    state = p.wait('loaded state', lambda: (s if (s := p.obj('save-state.json')) and s['event'] == 'loaded' else None))
    layout = p.obj('layout.json')
    p.tap(layout['stage_width']//2, round(layout['stage_height']*.4))

    def movie_file():
        command = f'find files/{run} -maxdepth 1 -name "movie-*-info.json"'
        paths = p.adb('shell', 'run-as', 'org.nekopack.probe', 'sh', '-c', shlex.quote(command)).decode().splitlines()
        assert len(paths) <= 1
        return paths[0].rsplit('/', 1)[-1] if paths else None

    filename = p.wait('scenario movie request', movie_file, timeout=180)
    info = p.obj(filename)
    assert info['source_sha256'] == hashlib.sha256(args.media.read_bytes()).hexdigest()
    stem = filename.removesuffix('-info.json')
    end = p.wait('scenario movie natural EOF', lambda: next((e for e in reversed(p.events(stem+'-events.jsonl')) if e['event'] == 'closed'), None), timeout=900)
    assert end['reason'] == 0 and end['error'] is None, end
    after = p.wait('scenario resumed after movie', lambda: p.obj('page-2.json'), timeout=180)
    assert after['source'] == 'CS01_C_07', after
    assert end['head'] >= end['samples'] > 0
    apk = p.adb('shell', 'pm', 'path', 'org.nekopack.probe').decode().strip().removeprefix('package:')
    result = dict(run=run, translated=args.translated, passed=True,
                  scope='original scenario movie command after imported runtime checkpoint; natural EOF and resumed scenario',
                  checkpoint_sha256=hashlib.sha256(args.save.read_bytes()).hexdigest(),
                  apk_sha256=p.adb('shell', 'sha256sum', apk).split()[0].decode(),
                  before=page, loaded=state, after=after, info=info, events=p.events(stem+'-events.jsonl'))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
    p.adb('shell', 'rm', remote)
    print(json.dumps(dict(run=run, passed=True, before=page, after=after)))


if __name__ == '__main__':
    main()
