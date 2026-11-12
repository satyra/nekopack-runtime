"""Real translated-title regression; never transfers story text or images.

Example: python platforms/android/regression/verify-translated-speaker.py
         --serial emulator-5572 --game BLESS --max-pages 500
Only absent patch files are deployed; only files created here are removed.
"""
import argparse
import json
import hashlib
import re
import shlex
import sys
import uuid
import xml.etree.ElementTree as ET
from xml.sax.saxutils import escape
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_menu_sounds import PACKAGE, PREFERENCES, SETTINGS, TITLE, Probe

GAMES = {'BLESS': ('', 'TD2'), 'BIRTH': ('BIRTH', 'TD3'),
         'BRAVE': ('BRAVE', 'TD4'), 'BLACK': ('BLACK', 'TD1'),
         'ENDLESS': ('Endless Dungeon', None)}
SAVE = '原版存读档页'


class MetadataProbe(Probe):
    def bounds(self, label, attr='content-desc'):
        # Accessibility descriptions can contain dialogue: return only bounds.
        needle = attr + '="' + escape(label, {'"': '&quot;'}) + '"'
        def locate():
            self.adb('shell', 'uiautomator', 'dump', self.ui_path)
            command = (f"sed 's/></>\\n</g' {shlex.quote(self.ui_path)} | "
                       f"grep -F {shlex.quote(needle)} | grep -F 'enabled=\"true\"' | "
                       r'''sed -n 's/.* bounds="\([^"]*\)".*/\1/p' ''')
            raw = self.adb('shell', 'sh', '-c', shlex.quote(command)).decode().strip()
            return tuple(map(int, re.findall(r'-?\d+', raw))) if raw else None
        return self.wait(label, locate, timeout=20)

    def obj(self, name):
        # These diagnostics contain story text. Filter on-device before transfer.
        if name.startswith('page-'):
            pattern = (r's/^\{"sequence":[0-9]+,"source":("(\\.|[^"\\])*"),'
                       r'"line":([0-9]+),"speaker":("(\\.|[^"\\])*").*$/'
                       r'{"speaker":\4,"source":\1,"line":\3}/p')
        elif name.startswith('text-'):
            pattern = r's/^.*"complete":(true|false).*$/\1/p'
        else:
            return super().obj(name)
        path = f'files/{self.run}/{name}'
        command = (f'if test -f {shlex.quote(path)}; then '
                   f'sed -n -E {shlex.quote(pattern)} {shlex.quote(path)}; fi')
        raw = self.adb('shell', 'run-as', PACKAGE, 'sh', '-c', shlex.quote(command)).strip()
        if not raw:
            return None
        value = json.loads(raw)
        return {'complete': value} if name.startswith('text-') else value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--game', choices=GAMES, required=True)
    parser.add_argument('--max-pages', type=int, default=500)
    parser.add_argument('--phase', choices=('render', 'all'), default='render',
                        help='render stops after the real non-CP932 name; all also saves, reloads and toggles Bold')
    parser.add_argument('--serial', required=True)
    parser.add_argument('--remote-directory', help='ADB path of the selected game directory')
    parser.add_argument('--document', help='SAF document ID of the same game directory')
    parser.add_argument('--adb', default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    parser.add_argument('--patch-root', type=Path, default=Path('D:/codex/td-port-workspace/research/pc-patch'))
    parser.add_argument('--endless-root', type=Path, default=Path('D:/codex/td-port-workspace/游戏/Endless Dungeon/Endless Dungeon'))
    args = parser.parse_args()
    assert args.max_pages > 0
    suffix, code = GAMES[args.game]
    remote = args.remote_directory or '/sdcard/Download/NekoPack-BLESS' + ('/' + suffix if suffix else '')
    document = args.document or 'raw:/storage/emulated/0/Download/NekoPack-BLESS' + ('/' + suffix if suffix else '')
    patches = (sorted((args.patch_root / code).glob('*.dat')) if code else
               [args.endless_root / name for name in ('script_chs.dat', 'system_chs.dat')])
    assert patches and all(path.is_file() for path in patches), 'missing local patch inputs'
    tag = 'translated-speaker-' + args.game.lower() + '-' + uuid.uuid4().hex[:8]
    p = MetadataProbe(args.adb, args.serial, tag, {})
    prefs = ET.fromstring(p.private(PREFERENCES))
    tree = prefs.find("./string[@name='dat_tree']").text
    added = []
    original_bold = None
    bold_changed = False

    def boot(stage):
        nonlocal p
        p = MetadataProbe(args.adb, args.serial, tag + '-' + stage, {})
        title = p.start_title('--ei', 'title_game', '0', '--es', 'library_tree', tree,
                              '--es', 'library_directory', shlex.quote(document),
                              '--es', 'save_probe', tag, '--es', 'read_history_probe', tag,
                              '--ez', 'legacy_probe_controls', 'true')
        assert title['chinese_scenario'], 'translated scenario not mounted'
        return title

    def press(label, state, button):
        assert button['enabled']
        left, top, right, bottom = p.bounds(label)
        width, height = state.get('width', 1280), state.get('height', 720)
        x, y, w, h = button['bounds']
        scale = min((right-left)/width, (bottom-top)/height)
        p.tap(round((left+right)/2+(x+w/2-width/2)*scale),
              round((top+bottom)/2+(y+h/2-height/2)*scale))

    def title_button(title, ident):
        press(TITLE, title, next(b for b in title['buttons'] if b['id'] == ident))

    def menu(action):
        p.key('BACK')
        p.control({'SAVE': '存档', 'LOAD': '读档', 'CONFIG': '原版设置'}[action])
        if action == 'CONFIG':
            p.bounds(SETTINGS)  # The prior presentation JSON remains after close.

    def complete(number):
        return p.wait('text complete', lambda: (m if (m := p.obj(f'text-{number}.json')) and m['complete'] else None))

    def slot(event, confirm):
        state = p.wait('save UI', lambda: (s if (s := p.obj('save-ui-presented.json')) and s['accepting'] else None))
        offset = len(p.events('save-events.jsonl'))
        press(SAVE, state, next(b for b in state['buttons'] if b['id'] == 1))
        if confirm:
            p.control('确定')
        return p.packets('save-events.jsonl', offset, lambda e: e['event'] == event and e['slot'] == 1)

    def bold(value):
        state = p.settings()
        if args.game != 'ENDLESS':
            while state['page'] != 2:
                key = '= PAGE++' if state['page'] < 2 else '= PAGE--'
                p.tap(*p.point(SETTINGS, state['areas'][key]))
                state = p.after_settings(state, 2)
        key = f'= BOLD#{value}'
        area = state['areas'][key]
        if args.game == 'ENDLESS' and not 155 < area[1]+area[3]/2 < 545:
            target = max(0, min(67, round((state['scroll']+area[1]+area[3]/2-350)/50)))
            x, y, w, h = state['areas']['* PAGE']
            p.tap(*p.point(SETTINGS, [x, y+(target+.5)*h/69, w, 0]))
            state = p.wait('settings scroll', lambda: (s if (s := p.settings()) and s['scroll'] == target*50 else None))
        p.tap(*p.point(SETTINGS, state['areas'][key]))
        state = p.after_settings(state, 2)
        assert not state['unsupported'] and state['config']['Bold'] == value, (
            'Bold', value, state['config']['Bold'], state['unsupported'])

    def close_story_settings(number):
        offset = len(p.events('text-events.jsonl'))
        p.key('BACK')
        p.packets('text-events.jsonl', offset,
                  lambda e: e['sequence'] == number and e['complete'] and not e['advance'])
        complete(number)

    try:
        existing = p.adb('shell', 'ls', shlex.quote(remote)).decode().splitlines()
        for patch in patches:
            if patch.name in existing:
                actual = p.adb('shell', 'sha256sum', shlex.quote(remote+'/'+patch.name)).split()[0].decode()
                assert actual == hashlib.sha256(patch.read_bytes()).hexdigest(), 'existing patch differs: '+patch.name
                continue
            destination = remote + '/' + patch.name
            added.append(destination)  # Includes a partially pushed file on failure.
            p.adb('push', str(patch), destination)
        title = boot('start')
        title_button(title, 3)
        config = p.wait('initial settings', p.settings)['config']
        original_bold = config['Bold']
        assert not config.get('Auto', 0), 'disable auto playback before this manual-page regression'
        p.key('BACK')
        title = p.wait('returned title', lambda: p.obj('title-presented.json'))
        title_button(title, 1)
        target = None
        for number in range(1, args.max_pages + 1):
            page = p.wait('page metadata', lambda: p.obj(f'page-{number}.json'))
            complete(number)
            try:
                page['speaker'].encode('cp932')
            except UnicodeEncodeError:
                target = page
                break
            layout = p.obj('layout.json')
            p.tap(layout['stage_width']//2, round(layout['stage_height']*.4))
        assert target is not None, f'no non-CP932 speaker within {args.max_pages} pages'
        print(json.dumps(dict(stage='rendered', game=args.game, page=number, **target), ensure_ascii=True), flush=True)
        if args.phase == 'render':
            print('PASS real translated non-CP932 speaker rendered without runtime error', flush=True)
            return
        menu('SAVE')
        saved = slot('saved', bool(config.get('Dialog', 0)))
        title = boot('reload')
        title_button(title, 2)
        loaded = slot('loaded', bool(config.get('Dialog', 0)))
        assert saved['directory'] == loaded['directory'] and '_zh_probe_' in saved['directory']
        restored = p.wait('restored metadata', lambda: p.obj('page-1.json'))
        assert restored == target
        complete(1)
        menu('CONFIG')
        p.wait('story settings', lambda: (s if (s := p.settings()) and s.get('story_sequence') == 1 else None))
        bold_changed = True
        bold(1-original_bold)
        close_story_settings(1)
        assert p.obj('page-1.json') == target
        menu('CONFIG')
        p.wait('story settings reopened', p.settings)
        bold(original_bold)
        bold_changed = False
        close_story_settings(1)
        assert not p.read('error.txt')
        print(json.dumps(dict(result='PASS', game=args.game, restarted=True, bold_restored=True, **target), ensure_ascii=True), flush=True)
    finally:
        try:
            if bold_changed:
                title = boot('restore')
                title_button(title, 3)
                p.wait('restore settings', p.settings)
                bold(original_bold)
                p.key('BACK')
        finally:
            p.adb('shell', 'am', 'force-stop', PACKAGE)
            for destination in added:
                p.adb('shell', 'rm', shlex.quote(destination))


if __name__ == '__main__':
    main()
