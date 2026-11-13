"""Resume a real checkpoint, advance ordinary pages, quick-save and cold-load.

Only source positions and UI control metadata are read; never text or pictures.
"""
import argparse
import hashlib
import importlib.util
import json
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
    parser.add_argument('--translated', action='store_true')
    parser.add_argument('--pages', type=int, default=100)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    run = 'sorakoi-resume-' + uuid.uuid4().hex[:8]
    p = nav.NavigationProbe(args.adb, args.serial, run, {})
    tree = 'content://com.android.providers.downloads.documents/tree/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-Sorakoi'
    scope = hashlib.sha256((tree+'_0').encode()).hexdigest()
    directory = 'files/saves/'+scope+('_zh' if args.translated else '')
    remote = '/data/local/tmp/'+run+'.nksv'
    p.adb('push', str(args.save), remote)
    p.adb('shell', 'run-as', 'org.nekopack.probe', 'mkdir', '-p', directory)
    # A dedicated checkpoint slot keeps earlier user slots intact.
    slot = directory+'/slot-079.nksv'
    existing = p.adb('shell', 'run-as', 'org.nekopack.probe', 'find', directory, '-name', 'slot-079.nksv').strip()
    if existing:
        remote_hash = p.adb('shell', 'run-as', 'org.nekopack.probe', 'sha256sum', slot).split()[0].decode()
        assert remote_hash == hashlib.sha256(args.save.read_bytes()).hexdigest(), 'slot 79 contains a different save; preserve it'
    else:
        p.adb('shell', 'run-as', 'org.nekopack.probe', 'cp', remote, slot)

    def boot(slot_id):
        p.start_title('--ei', 'title_game', '0', '--es', 'library_entry', 'pk02-validation',
                      '--ez', 'library_original', str(not args.translated).lower(),
                      '--ez', 'dialogue_probe', 'true', '--ei', 'library_resume_slot', str(slot_id))

    choices = []
    last_selection_token = None

    def complete(sequence):
        def poll():
            nonlocal last_selection_token
            state = p.obj('save-state.json')
            assert not state or state['event'] != 'error', state
            page = p.obj(f'page-{sequence}.json')
            if page:
                return page
            selection = p.obj('selection-presented.json')
            if selection and selection['token'] != last_selection_token:
                last_selection_token = selection['token']
                left, top, right, bottom = p.bounds('android:id/select_dialog_listview', 'resource-id')
                count = selection['options_count']
                assert count > 0
                p.tap((left+right)//2, top+max(1, min(24, (bottom-top)//(2*count))))
                choices.append(dict(token=last_selection_token, options_count=count, selected_id=1))
            return None
        page = p.wait('ordinary page', poll, timeout=180)
        p.wait('text complete', lambda: (s if (s := p.obj(f'text-{sequence}.json')) and s['complete'] else None))
        return page

    boot(79)
    pages = [complete(1)]
    for sequence in range(2, args.pages+2):
        layout = p.obj('layout.json')
        p.tap(layout['stage_width']//2, round(layout['stage_height']*.4))
        pages.append(complete(sequence))
    def press(label, state, button):
        left, top, right, bottom = p.bounds(label)
        width, height = state['size']
        x, y, w, h = button['bounds']
        scale = min((right-left)/width, (bottom-top)/height)
        p.tap(round((left+right)/2+(x+w/2-width/2)*scale), round((top+bottom)/2+(y+h/2-height/2)*scale))

    def side(action):
        state = p.wait('side action '+action, lambda: (s if (s := p.obj('side-menu-presented.json')) and any(b['action']==action and b['enabled'] for b in s['buttons']) else None))
        button = next(b for b in state['buttons'] if b['action']==action)
        press(nav.SIDE, state, button)

    side('SAVE')
    first = p.wait('save page 0', lambda: (s if (s := p.obj('save-ui-presented.json')) and s['page']==0 and s['accepting'] else None))
    assert first['slot_ids'] == list(range(1, 9))
    press('原版存读档页', first, next(b for b in first['buttons'] if b['id']==209))
    last = p.wait('save page 9', lambda: (s if (s := p.obj('save-ui-presented.json')) and s['page']==9 and s['accepting'] else None))
    assert last['slot_ids'] == list(range(73, 81))
    p.key('BACK')
    p.wait('save UI closed', lambda: (s if (s := p.obj('save-state.json')) and s['event']=='cancelled' else None))
    side('QSAVE')
    p.control('确定')
    saved = p.wait('quick saved', lambda: (s if (s := p.obj('save-state.json')) and s['event']=='saved' else None))
    saved_page = complete(saved['sequence'])
    run_before_restart = p.run
    p = nav.NavigationProbe(args.adb, args.serial, run+'-reload', {})
    boot(101)
    restored = complete(1)
    assert (restored['source'], restored['line']) == (saved_page['source'], saved_page['line'])
    loaded = p.wait('quick load completed', lambda: (s if (s := p.obj('save-state.json')) and s['event']=='loaded' else None))
    apk = p.adb('shell', 'pm', 'path', 'org.nekopack.probe').decode().strip().removeprefix('package:')
    result = dict(run=run_before_restart, reload_run=p.run, translated=args.translated, passed=True,
                  apk_sha256=p.adb('shell', 'sha256sum', apk).split()[0].decode(),
                  pages=pages, choices=choices, save_ui_first=first, save_ui_last=last, saved=saved, saved_page=saved_page, restored=restored, loaded=loaded,
                  checkpoint_sha256=hashlib.sha256(args.save.read_bytes()).hexdigest())
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+'\n', encoding='utf-8')
    p.adb('shell', 'rm', remote)
    if not existing:
        p.adb('shell', 'run-as', 'org.nekopack.probe', 'rm', slot)
    print(json.dumps(dict(passed=True, pages=len(pages), restored=restored, run=p.run)))


if __name__ == '__main__':
    main()
