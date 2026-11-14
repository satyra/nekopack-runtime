"""Advance an isolated copy of a real old save; inspect metadata, never pictures/text."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import re
import shlex
import uuid

spec = importlib.util.spec_from_file_location('navigation', Path(__file__).with_name('verify-sorakoi-layout-navigation.py'))
nav = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nav)
PACKAGE = 'org.nekopack.probe'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial', required=True)
    parser.add_argument('--adb', default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    parser.add_argument('--entry', required=True)
    parser.add_argument('--scope', required=True, help='existing save directory basename, including _zh when translated')
    parser.add_argument('--original', action='store_true')
    parser.add_argument('--pages', type=int, default=200)
    parser.add_argument('--apk', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert re.fullmatch(r'[a-f0-9]{64}(_zh)?', args.scope) and args.pages > 0
    assert args.scope.endswith('_zh') != args.original
    run = 'old-save-continuity-' + uuid.uuid4().hex[:8]
    p = nav.NavigationProbe(args.adb, args.serial, run, {})
    source = 'files/saves/' + args.scope
    isolated = source + '_probe_' + run
    report = dict(run=run, passed=False, original=args.original, pages=[], choices=[],
                  original_save_modified=False, pictures_read=False, scenario_text_read=False)

    def flush():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')

    def hashes():
        command = 'find ' + shlex.quote(source) + ' -type f -exec sha256sum {} \\;'
        return sorted(p.adb('shell', 'run-as', PACKAGE, 'sh', '-c', shlex.quote(command)).decode().splitlines())

    def boot(slot):
        p.start_activity('--ei', 'title_game', '0', '--es', 'library_entry', args.entry,
                         '--ez', 'library_original', str(args.original).lower(),
                         '--ez', 'dialogue_probe', 'true', '--es', 'save_probe', run,
                         '--es', 'read_history_probe', run, '--ei', 'library_resume_slot', str(slot))

    last_selection = None

    def complete(number):
        def poll():
            nonlocal last_selection
            state = p.obj('save-state.json')
            assert not state or state['event'] != 'error', state
            page = p.obj(f'page-{number}.json')
            if page:
                return page
            selection = p.obj('selection-presented.json')
            if selection and selection['token'] != last_selection:
                last_selection = selection['token']
                count = selection['options_count']
                assert count > 0
                left, top, right, bottom = p.bounds('android:id/select_dialog_listview', 'resource-id')
                p.tap((left + right)//2, top + max(1, min(24, (bottom-top)//(2*count))))
                report['choices'].append(dict(token=last_selection, options_count=count, selected_id=1))
            return None
        page = p.wait('dialogue or choice after old save', poll, timeout=900)
        p.wait('text completed', lambda: (s if (s := p.obj(f'text-{number}.json')) and s['complete'] else None))
        return page

    def press(state, button):
        left, top, right, bottom = p.bounds(nav.SIDE)
        width, height = state['size']
        x, y, w, h = button['bounds']
        scale = min((right-left)/width, (bottom-top)/height)
        p.tap(round((left+right)/2+(x+w/2-width/2)*scale), round((top+bottom)/2+(y+h/2-height/2)*scale))

    try:
        installed = p.adb('shell', 'pm', 'path', PACKAGE).decode().strip().removeprefix('package:')
        report['apk_sha256'] = hashlib.sha256(args.apk.read_bytes()).hexdigest()
        assert p.adb('shell', 'sha256sum', shlex.quote(installed)).split()[0].decode() == report['apk_sha256']
        before = hashes()
        report['source_before'] = before
        names = json.loads(p.private(source + '/quick-index.json'))
        assert names and re.fullmatch(r'quick-[a-f0-9-]{36}\.nksv', names[0])
        old = source + '/' + names[0]
        report['old_save_sha256'] = p.adb('shell', 'run-as', PACKAGE, 'sha256sum', old).split()[0].decode()
        # Only the five-byte signature/version is transferred, never save text.
        header = p.adb('exec-out', 'run-as', PACKAGE, 'head', '-c', '5', old)
        assert header[:4] == b'NKSV'
        report['old_save_version'] = header[4]
        p.adb('shell', 'run-as', PACKAGE, 'mkdir', isolated)
        p.adb('shell', 'run-as', PACKAGE, 'cp', old, isolated + '/slot-001.nksv')
        boot(1)
        for number in range(1, args.pages+1):
            page = complete(number)
            report['pages'].append(page)
            if number == 1:
                loaded = p.wait('old save restored', lambda: (s if (s := p.obj('save-state.json')) and s['event']=='loaded' else None))
                assert loaded['directory'] == isolated.rsplit('/', 1)[-1]
                report['loaded'] = loaded
            flush()
            if number % 25 == 0:
                print(json.dumps(dict(run=run, completed=number, position=page)), flush=True)
            if number < args.pages:
                layout = p.obj('layout.json')
                p.tap(layout['stage_width']//2, round(layout['stage_height']*.4))
        state = p.wait('QSAVE enabled', lambda: (s if (s := p.obj('side-menu-presented.json')) and
                       any(b['action']=='QSAVE' and b['enabled'] for b in s['buttons']) else None))
        button = next(b for b in state['buttons'] if b['action']=='QSAVE')
        if not state['keep_icon'] and not state['expanded']:
            press(state, button)
            state = p.wait('side menu revealed', lambda: (s if (s := p.obj('side-menu-presented.json')) and s['expanded'] else None))
            button = next(b for b in state['buttons'] if b['action']=='QSAVE')
        press(state, button)
        # This fixture uses the existing configuration's original confirmation.
        p.control('确定')
        report['saved'] = p.wait('isolated quick save', lambda: (s if (s := p.obj('save-state.json')) and s['event']=='saved' else None))
        assert report['saved']['directory'] == isolated.rsplit('/', 1)[-1]
        p = nav.NavigationProbe(args.adb, args.serial, run + '-reload', {})
        boot(101)
        restored = complete(1)
        report['restored'] = restored
        assert (restored['source'], restored['line']) == (page['source'], page['line'])
        report['source_after'] = hashes()
        assert report['source_after'] == before, 'original save directory changed'
        assert p.adb('shell', 'sha256sum', shlex.quote(installed)).split()[0].decode() == report['apk_sha256']
        report['passed'] = True
    except Exception as error:
        report['failure_type'] = type(error).__name__
        raise
    finally:
        flush()
    print(json.dumps(dict(passed=True, run=run, pages=len(report['pages']), choices=len(report['choices']))))


if __name__ == '__main__':
    main()
