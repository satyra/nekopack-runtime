"""Verify mounted original quit cancel/confirm using metadata and hashes only.

Requires a predeployed exit-fixture and SAF grant. Does not install an APK or
transfer screenshots, original dialogue, or accessibility descriptions.
"""
import argparse
import importlib.util
import json
import shlex
import uuid
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location(
    'navigation', Path(__file__).with_name('verify-sorakoi-layout-navigation.py'))
nav = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nav)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--adb', default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    parser.add_argument('--manifest', type=Path, default=ROOT/'.local/system-integration/exit-fixture/manifest.json')
    parser.add_argument('--remote-directory', default='/sdcard/Download/NekoPack-Sorakoi/NekoPack-Exit-Probe')
    parser.add_argument('--document', default='raw:/storage/emulated/0/Download/NekoPack-Sorakoi/NekoPack-Exit-Probe')
    parser.add_argument('--tree', default='content://com.android.providers.downloads.documents/tree/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-Sorakoi')
    parser.add_argument('--apk-sha256', help='require the installed APK to match this build digest')
    args = parser.parse_args()
    run = 'system-exit-'+uuid.uuid4().hex[:8]
    p = nav.NavigationProbe(args.adb, args.serial, run+'-cancel', {})
    report = dict(run=run, serial=args.serial, passed=False, checks=[],
                  checked_at=datetime.now(timezone.utc).isoformat(), runs=[])
    extras = ['--ei', 'title_game', '0', '--es', 'library_tree', shlex.quote(args.tree),
              '--es', 'library_directory', shlex.quote(args.document), '--ez', 'library_original', 'true',
              '--ez', 'dialogue_probe', 'true', '--es', 'save_probe', run, '--es', 'read_history_probe', run]

    def events():
        return p.events('system-command-events.jsonl')

    def named(frame, name):
        return next((b for b in frame.get('buttons', []) if b['name'].split(':')[1] == name), None)

    def press_title(title):
        button = next(b for b in title['buttons'] if b['id'] == title['start_action'])
        assert button['enabled']
        left, top, right, bottom = p.bounds(nav.startup.TITLE)
        x, y, w, h = button['bounds']
        scale = min((right-left)/title['width'], (bottom-top)/title['height'])
        p.tap(round((left+right)/2+(x+w/2-title['width']/2)*scale),
              round((top+bottom)/2+(y+h/2-title['height']/2)*scale))

    def start():
        title = p.start_title(*extras)
        if title.get('tick_ms', 0) > 0:
            title = p.wait('title animation complete', lambda: (
                t if (t := p.obj('title-presented.json')) and t.get('tick_ms', 0) <= 0 else None))
        press_title(title)
        frame = p.wait('original QUIT confirmation controls', lambda: next((
            d for d in reversed(events()) if d.get('has_frame') and named(d, 'OK') and named(d, 'CANCEL')), None))
        assert not p.obj('page-1.json'), 'quit must suspend before synthetic dialogue'
        assert not p.read('closed.txt')
        assert not any(e.get('kind') == 'system_exit' for e in events())
        return frame

    def tap(frame, name):
        layout = p.obj('layout.json')
        sw, sh = layout['stage_width'], layout['stage_height']
        w, h = frame['size']
        scale = min(sw/w, sh/h)
        x, y, bw, bh = named(frame, name)['bounds']
        p.tap(round(sw/2+(x+bw/2-w/2)*scale), round(sh/2+(y+bh/2-h/2)*scale))

    def apk_hash():
        path = p.adb('shell', 'pm', 'path', 'org.nekopack.probe').decode().strip().removeprefix('package:')
        return p.adb('shell', 'sha256sum', shlex.quote(path)).split()[0].decode()

    def record(stage):
        assert not p.read('error.txt')
        digest = apk_hash()
        assert digest == report['apk_sha256'], 'installed APK changed during verification'
        report['runs'].append(dict(stage=stage, run=p.run, apk_sha256=digest, events=events(),
                                   error_absent=True, closed=p.read('closed.txt').decode()))

    try:
        manifest = json.loads(args.manifest.read_text(encoding='utf8'))
        assert manifest['synthetic_scenario']
        for item in manifest['files']:
            actual = p.adb('shell', 'sha256sum', shlex.quote(args.remote_directory+'/'+item['name'])).split()[0].decode()
            assert actual == item['sha256'], item['name']
        report['fixture_files'] = manifest['files']
        report['apk_sha256'] = apk_hash()
        if args.apk_sha256:
            assert report['apk_sha256'] == args.apk_sha256.lower(), 'installed APK differs from expected build'
        frame = start()
        tap(frame, 'CANCEL')
        page = p.wait('cancel returns to synthetic dialogue', lambda: p.obj('page-1.json'))
        expected = manifest['synthetic_page']
        assert (page['source'], page['line']) == (expected['source'], expected['line'])
        assert not any(e.get('kind') == 'system_exit' for e in events())
        assert not p.read('closed.txt')
        report['cancel_page'] = page
        record('cancel')
        report['checks'].append('original ViewDialog CANCEL returns to the synthetic scenario without exiting')

        p = nav.NavigationProbe(args.adb, args.serial, run+'-confirm', {})
        frame = start()  # start_title force-stops: confirmation is tested on a cold launch.
        tap(frame, 'OK')
        terminal = p.wait('original quit terminal packet', lambda: next((
            e for e in events() if e.get('kind') == 'system_exit'), None))
        assert terminal['reason'] == 'original_quit'
        closed = p.wait('game VM closed without another confirmation', lambda: p.read('closed.txt'))
        assert closed == b'closed; VM live bytes=0'
        assert not p.obj('page-1.json'), 'confirmed quit resumed the scenario'
        assert sum(e.get('kind') == 'system_exit' for e in events()) == 1
        report['terminal'] = terminal
        record('confirm')
        report['checks'].extend(['cold launch original OK emits exactly one original_quit system_exit',
                                 'closed.txt confirms VM release with no second confirmation and no error',
                                 'APK SHA-256 is identical before and after both runs'])
        report['passed'] = True
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n', encoding='utf8')
    print(json.dumps(dict(passed=True, run=run, checks=report['checks']), ensure_ascii=False))


if __name__ == '__main__':
    main()
