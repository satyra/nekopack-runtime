"""Short BLESS DAT/SAF device regression for menu and settings sound cues."""

import argparse
import hashlib
import json
import math
import re
import subprocess
import time
import uuid
import xml.etree.ElementTree as ET
from pathlib import Path


PACKAGE = 'org.nekopack.probe'
ACTIVITY = PACKAGE + '/.MainActivity'
PREFERENCES = 'shared_prefs/MainActivity.xml'
SOUNDS = {1: 'se_PAGE', 2: 'se_CLICK', 3: 'se_OK'}
SETTINGS = '原版设置页'
TITLE = '游戏标题菜单'
MENU = '原版剧情菜单'


def original_sounds(cli, games):
    archive = next(games.rglob('Tiny_BoD.exe')).with_name('sound.dat')
    result = {}
    for name in SOUNDS.values():
        lookup = subprocess.check_output(
            [cli, 'lookup-name', b'sound/se'.hex(), (name + '.ogg').encode().hex(), str(archive)],
            timeout=30,
        ).decode().splitlines()[0].split('\t')
        assert lookup[:2] == ['selected', '0'], (name, lookup)
        data = subprocess.check_output([cli, 'read-entry', str(archive), lookup[2]], timeout=30)
        assert data.startswith(b'OggS'), name
        result[name] = dict(ordinal=int(lookup[2]), sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))
    return result


class Probe:
    def __init__(self, adb, serial, run, originals):
        self.command = [adb, '-s', serial]
        self.run = run
        self.originals = originals
        self.actions = []
        self.audio = []
        self.ui_path = f'/sdcard/{run}.xml'

    def adb(self, *args, **kwargs):
        return subprocess.check_output([*self.command, *args], timeout=45, **kwargs)

    def private(self, path):
        return self.adb('exec-out', 'run-as', PACKAGE, 'cat', path, stderr=subprocess.STDOUT)

    def read(self, name):
        path = f'files/{self.run}/{name}'
        try:
            raw = self.private(path)
        except subprocess.CalledProcessError as error:
            if b'No such file' not in error.output:
                raise
            raw = error.output
        if raw.startswith(b'cat: ') and b'No such file' in raw:
            return b''
        return raw

    def obj(self, name):
        raw = self.read(name)
        if raw.rstrip().endswith(b'}'):
            return json.loads(raw)
        return None

    def events(self, name):
        raw = self.read(name)
        return [json.loads(line) for line in raw.split(b'\n')[:-1] if line]

    def start_activity(self, *extras):
        """Force-stop once and retain ActivityManager's checked launch result."""
        result = subprocess.run(
            [*self.command, 'shell', 'am', 'start', '-W', '-S', '-n', ACTIVITY,
             '--es', 'run_id', self.run, *extras],
            capture_output=True, timeout=45,
        )
        self.launch = dict(exitcode=result.returncode,
                           stdout=result.stdout.decode(errors='replace'),
                           stderr=result.stderr.decode(errors='replace'))
        assert result.returncode == 0 and 'Status: ok' in self.launch['stdout'], (
            self.run, 'ActivityManager did not start the activity', self.launch)
        return self.launch

    def start_title(self, *extras):
        """Launch and wait for title rendering independently of ActivityManager."""
        self.start_activity(*extras)
        saw_process = False

        def presented():
            nonlocal saw_process
            title = self.obj('title-presented.json')
            if title:
                return title
            process = subprocess.run([*self.command, 'shell', 'pidof', PACKAGE],
                                     capture_output=True, timeout=45)
            assert process.returncode in (0, 1), process.stderr.decode(errors='replace')
            # Some Android emulators return from am -W before the replacement
            # process is visible to pidof after -S. Only a process observed and
            # then lost is evidence that this launch exited.
            if process.stdout.strip():
                saw_process = True
            else:
                assert not saw_process, (self.run, 'process exited before title', self.launch)
            return None

        try:
            return self.wait('title', presented)
        except AssertionError as error:
            raise AssertionError(f'{error}; launch={self.launch}') from error

    def wait(self, description, predicate, timeout=90):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            error = self.read('error.txt')
            assert not error, error.decode(errors='replace')
            result = predicate()
            if result:
                return result
            time.sleep(.1)
        raise AssertionError(f'{self.run}: timed out waiting for {description}')

    def bounds(self, label, attr='content-desc'):
        def locate():
            dumped = self.adb('shell', 'uiautomator', 'dump', self.ui_path)
            if b'dumped to' not in dumped:
                return None
            root = ET.fromstring(self.adb('shell', 'cat', self.ui_path))
            for node in root.iter('node'):
                if node.get(attr) == label and node.get('enabled') == 'true':
                    return tuple(map(int, re.findall(r'-?\d+', node.get('bounds'))))
            return None

        return self.wait(label, locate, timeout=20)

    def tap(self, x, y):
        self.adb('shell', 'input', 'tap', str(x), str(y))

    def control(self, label):
        left, top, right, bottom = self.bounds(label, 'text')
        self.tap((left + right) // 2, (top + bottom) // 2)

    def key(self, name):
        self.adb('shell', 'input', 'keyevent', 'KEYCODE_' + name)

    def point(self, label, area):
        left, top, right, bottom = self.bounds(label)
        x, y, width, height = area
        scale = min((right - left) / 1280, (bottom - top) / 720)
        return (
            round(left + (right - left - 1280 * scale) / 2 + (x + width / 2) * scale),
            round(top + (bottom - top - 720 * scale) / 2 + (y + height / 2) * scale),
        )

    def settings(self):
        return self.obj('settings-presented.json')

    def after_settings(self, before, inputs):
        def changed():
            current = self.settings()
            if current and current['input_sequence'] >= before['input_sequence'] + inputs:
                return current
            return None

        return self.wait('settings input committed', changed)

    def packets(self, filename, offset, predicate):
        def matching():
            return next((e for e in self.events(filename)[offset:] if predicate(e)), None)

        return self.wait(filename, matching)

    def verify_audio(self, token, name, percent, next_volume):
        def playback():
            events = [e for e in self.events('host-events.jsonl') if e['token'] == token and e['kind'] == 'system']
            started = next((e for e in events if e['event'] == 'playing' and e['value'] > 0), None)
            progress = next((e for e in events if e['event'] in ('pcm_head', 'progress') and e['value'] > 0), None)
            return (started, progress) if started and progress else None

        started, progress = self.wait(f'{name} token {token} playback', playback, timeout=12)
        meta = self.obj(f'audio-{token}.json')
        assert meta == dict(kind='system', name=name, sha256=self.originals[name]['sha256']), meta

        def applied_gains():
            return [e for e in self.events('audio-gain-events.jsonl') if e['token'] == token and e['kind'] == 'system']

        gains = applied_gains()
        assert gains and gains[0]['category_percent'] == percent, gains
        initial_gain = gains[0]
        assert initial_gain['time_ms'] <= started['time_ms'], initial_gain
        assert math.isclose(initial_gain['output'], percent / 100, abs_tol=0.00001), initial_gain
        updated_gain = None
        if next_volume is not None and next_volume != percent:
            updated_gain = self.wait(
                f'{name} token {token} gain update to {next_volume}',
                lambda: next((e for e in applied_gains()[1:] if e['category_percent'] == next_volume), None),
                timeout=12,
            )
            assert updated_gain['player'] == initial_gain['player'], updated_gain
            assert updated_gain['time_ms'] >= started['time_ms'], updated_gain
            assert math.isclose(updated_gain['output'], next_volume / 100, abs_tol=0.00001), updated_gain
        self.audio.append(dict(token=token, name=name, category_percent=percent, sha256=meta['sha256'],
                               playing=started, progress=progress, first_gain=initial_gain, updated_gain=updated_gain))

    def action(self, label, expected, invoke, *, final_cues=None, next_volume=None):
        """Check the final native packet and every played cue from one physical action."""
        print(label, flush=True)
        offset = len(self.events('ui-audio-events.jsonl'))
        packet = invoke()
        if callable(expected):
            expected = expected(packet)
        if packet is not None:
            cues = [dict(id=cue, category_percent=percent) for cue, percent in (expected if final_cues is None else final_cues)]
            assert packet.get('system_sounds', []) == cues, (label, cues, packet.get('system_sounds'))

        def received():
            played = self.events('ui-audio-events.jsonl')[offset:]
            return played if len(played) >= len(expected) else None

        if expected:
            played = self.wait(label + ' audio', received, timeout=12)
        else:
            time.sleep(.25)
            played = self.events('ui-audio-events.jsonl')[offset:]
        assert [(e['name'], e['category_percent']) for e in played] == [
            (SOUNDS[cue], percent) for cue, percent in expected
        ], (label, played)
        for event in played:
            assert event['host_effect'] == 'system' and event['ui_audio'] and not event['loop'] and event['token'] < 0, event
            self.verify_audio(event['token'], event['name'], event['category_percent'], next_volume)
        self.actions.append(dict(action=label, system_sounds=played))
        return packet

    def area(self, name, label, expected=(), *, next_volume=None):
        before = self.settings()
        coords = self.point(SETTINGS, before['areas'][name])

        def invoke():
            self.tap(*coords)
            return self.after_settings(before, 2)

        return self.action(label, expected, invoke, next_volume=next_volume)

    def slider_tap(self, value, old, coords):
        before = self.settings()

        def invoke():
            self.tap(*coords)
            return self.after_settings(before, 2)

        # DOWN emits CLICK at the previous volume. The same-coordinate UP packet stays silent.
        packet = self.action(f'VolSYS slider tap {old} -> {value}', [(2, old)], invoke, final_cues=[], next_volume=value)
        assert packet['config']['VolSYS'] == value, packet['config']

    def slider_points(self, values):
        left, top, right, bottom = self.bounds(SETTINGS)
        x, y, width, height = self.settings()['areas']['* VolSYS']
        scale = min((right - left) / 1280, (bottom - top) / 720)
        origin_x = left + (right - left - 1280 * scale) / 2
        origin_y = top + (bottom - top - 720 * scale) / 2
        screen_y = round(origin_y + (y + height / 2) * scale)
        points = {}
        for value in values:
            target = origin_x + (x + (value + .5) * width / 100) * scale
            for pixel in sorted(range(math.ceil(origin_x + x * scale), math.ceil(origin_x + (x + width) * scale)),
                                key=lambda px: abs(px - target)):
                logical_x = math.floor((pixel - origin_x) / scale)
                if (logical_x - x) * 100 // width == value:
                    points[value] = (pixel, screen_y)
                    break
            assert value in points, (value, 'VolSYS cannot represent this value at the current viewport')
        return points


def title_settings(probe):
    title = probe.wait('title', lambda: probe.obj('title-presented.json'))

    def tap_title(button_id):
        button = next(b for b in title['buttons'] if b['id'] == button_id)
        assert button['enabled'], button
        probe.tap(*probe.point(TITLE, button['bounds']))

    # Read VolSYS from the newly opened configuration, before any edits.
    def open_settings():
        tap_title(3)
        return probe.wait('title settings', probe.settings)

    first = probe.action('title opens settings', lambda packet: [(1, packet['config']['VolSYS'])], open_settings)
    original = first['config']
    volume = original['VolSYS']

    assert probe.area('= PAGE--', 'first-page arrow stays silent')['page'] == 1
    assert probe.area('= PAGE++', 'settings page 1 -> 2', [(1, volume)])['page'] == 2
    assert probe.area(f'= BOLD#{original["Bold"]}', 'confirm same-value radio', [(3, volume)])['config'] == original
    assert probe.area('= PAGE++', 'settings page 2 -> 3', [(1, volume)])['page'] == 3

    points = probe.slider_points({37, 50, volume} - {100})
    probe.slider_tap(50, volume, points[50])
    assert probe.area('* VolSYS--', 'system volume edge 50 -> 0', [(2, 50)], next_volume=0)['config']['VolSYS'] == 0
    probe.slider_tap(37, 0, points[37])
    if volume == 100:
        assert probe.area('* VolSYS++', 'restore system volume with maximum edge', [(2, 37)], next_volume=100)['config'] == original
    else:
        probe.slider_tap(volume, 37, points[volume])
    assert probe.settings()['config'] == original

    assert probe.area('= PAGE++', 'settings page 3 -> 4', [(1, volume)])['page'] == 4
    assert probe.area('= PAGE++', 'settings page 4 -> 5', [(1, volume)])['page'] == 5
    assert probe.area('= PAGE++', 'last-page arrow stays silent')['page'] == 5
    bindings = probe.settings()['shortcuts']
    original_binding = bindings[0]['action']
    chosen = 'AUTO' if original_binding == 'HIDE' else 'HIDE'
    popup = probe.area('* F3', 'open shortcut picker silently')
    assert popup['editing_slot'] == 3

    def cancel_picker():
        probe.key('BACK')
        return probe.after_settings(popup, 1)

    cancelled = probe.action('cancel shortcut picker silently', [], cancel_picker)
    assert cancelled['editing_slot'] == 0 and cancelled['shortcuts'] == bindings
    assert probe.area('* F3', 'reopen shortcut picker silently')['editing_slot'] == 3
    confirmed = probe.area('= ' + chosen, 'confirm shortcut choice', [(2, volume)])
    assert confirmed['editing_slot'] == 0 and confirmed['shortcuts'][0]['action'] == chosen
    assert probe.area('* F3', 'open shortcut picker for restoration')['editing_slot'] == 3
    restored = probe.area('= ' + (original_binding or 'DELETE'), 'restore shortcut binding', [(2, volume)])
    assert restored['shortcuts'] == bindings and restored['config'] == original

    def close_settings():
        probe.key('BACK')
        probe.bounds(TITLE)
        return probe.obj('title-presented.json')

    probe.action('settings returns to title', [(1, volume)], close_settings)
    tap_title(1)
    return original, bindings


def story_settings(probe, config, bindings):
    volume = config['VolSYS']
    number = 13
    for n in range(1, number + 1):
        def complete():
            text = probe.obj(f'text-{n}.json')
            return text and text['complete']

        probe.wait(f'completed dialogue {n}', complete)
        if n == number:
            break
        probe.control('继续')
        if n == 12:
            probe.wait('opening movie', lambda: probe.obj('movie-8-info.json'))
            probe.control('返回剧情')

    page_file = f'page-{number}.json'
    scene_file = f'page-{number}.png'
    page, scene = probe.obj(page_file), probe.read(scene_file)
    assert page['sequence'] == number and scene.startswith(b'\x89PNG\r\n\x1a\n')

    def open_menu():
        offset = len(probe.events('story-menu-events.jsonl'))
        probe.control('剧情菜单')
        return probe.packets('story-menu-events.jsonl', offset, lambda e: e['kind'] == 'story_menu')

    def menu_point(menu, action):
        button = next(b for b in menu['buttons'] if b['action'] == action)
        assert button['enabled'], button
        return probe.point(MENU, button['bounds'])

    def menu_button(coords):
        offset = len(probe.events('story-menu-events.jsonl'))
        probe.tap(*coords)
        return probe.packets('story-menu-events.jsonl', offset, lambda e: e['kind'] == 'story_menu_closed')

    def same_dialogue(offset):
        restored = probe.packets('text-events.jsonl', offset, lambda e: e['sequence'] == number)
        assert restored['complete'] and not restored['advance'], restored
        assert probe.obj(page_file) == page and probe.read(scene_file) == scene
        assert probe.obj(f'page-{number + 1}.json') is None
        return restored

    first = probe.action('open story menu', [(1, volume)], open_menu)
    assert first['checkpoint']['sequence'] == number
    text_offset = len(probe.events('text-events.jsonl'))

    def back_menu():
        offset = len(probe.events('story-menu-events.jsonl'))
        probe.key('BACK')
        return probe.packets('story-menu-events.jsonl', offset, lambda e: e['kind'] == 'story_menu_closed')

    cancelled = probe.action('cancel story menu with Back', [(1, volume)], back_menu)
    assert cancelled['action'] == '' and cancelled['checkpoint'] == first['checkpoint']
    back_dialogue = same_dialogue(text_offset)

    for action in ('SAVE', 'LOAD'):
        menu = probe.action(f'open story menu for {action}', [(1, volume)], open_menu)
        save_offset = len(probe.events('save-events.jsonl'))
        coords = menu_point(menu, action)
        closed = probe.action(f'open {action} without a menu cue', [], lambda: menu_button(coords))
        assert closed['action'] == action and closed['checkpoint'] == menu['checkpoint']
        probe.packets('save-events.jsonl', save_offset, lambda e: e['event'] == 'open')
        assert probe.obj('save-ui-presented.json')['saving'] == (action == 'SAVE')
        text_offset = len(probe.events('text-events.jsonl'))
        probe.key('BACK')
        probe.packets('save-events.jsonl', save_offset, lambda e: e['event'] == 'cancelled')
        same_dialogue(text_offset)

    menu = probe.action('open story menu for CONFIG', [(1, volume)], open_menu)
    lifecycle_offset = len(probe.events('audio-lifecycle.jsonl'))
    host_offset = len(probe.events('host-events.jsonl'))
    text_offset = len(probe.events('text-events.jsonl'))
    activity_offset = len(probe.events('lifecycle-events.jsonl'))
    before_settings = probe.settings()
    config_point = menu_point(menu, 'CONFIG')

    def open_config():
        closed = menu_button(config_point)
        assert closed['action'] == 'CONFIG' and closed['checkpoint'] == menu['checkpoint']
        assert closed.get('system_sounds', []) == [], closed

        def opened():
            packet = probe.settings()
            if packet != before_settings and packet.get('story_sequence') == number:
                return packet
            return None

        return probe.wait('story settings', opened)

    # Watch from inside the device shell to avoid host/ADB round trips during the short clip.
    # Wait for CONFIG presentation and the UI acknowledgement so Home cannot suspend their publication.
    token = probe.audio[-1]['token'] - 1
    files = f'files/{probe.run}'
    home_when_playing = f'''
        count=0
        while [ "$count" -lt 240 ]; do
            if run-as {PACKAGE} grep -qF '"story_sequence":{number},' {files}/settings-presented.json 2>/dev/null &&
               run-as {PACKAGE} grep -qF '"token":{token},' {files}/ui-audio-events.jsonl 2>/dev/null &&
               run-as {PACKAGE} grep -qE '"event":"(pcm_head|progress)","kind":"system","token":{token},"time_ms":[0-9]+,"value":[1-9][0-9]*' {files}/host-events.jsonl 2>/dev/null; then
                input keyevent KEYCODE_HOME
                exit $?
            fi
            count=$((count + 1))
            sleep 0.03
        done
        exit 1
    '''
    with subprocess.Popen([*probe.command, 'shell', home_when_playing], stdout=subprocess.PIPE, stderr=subprocess.STDOUT) as home:
        opened = probe.action('CONFIG uses only the settings PAGE cue', [(1, volume)], open_config)
        output, _ = home.communicate(timeout=20)
        assert home.returncode == 0, ('could not send Home during the CONFIG PAGE cue', output)
    assert opened['page'] == 1 and opened['config'] == config, opened
    frozen = {k: opened[k] for k in ('story_sequence', 'story_visible', 'story_elapsed_ms', 'story_steps', 'motion_elapsed_ms')}
    paused = probe.packets('audio-lifecycle.jsonl', lifecycle_offset, lambda e: e['event'] == 'paused')
    assert paused['channels']['bgm']['position'] >= 0, paused
    assert all(not channel['playing'] for kind, channel in paused['channels'].items() if kind != 'system'), paused
    assert paused['time_ms'] <= probe.audio[-1]['playing']['time_ms']

    # The same system player and BGM remain paused before bringing the Activity forward.
    stable = [probe.packets('audio-lifecycle.jsonl', lifecycle_offset, lambda e: e['event'] == f'paused_stable_{i}')
              for i in (1, 2)]
    channels = [item['channels'] for item in stable]
    assert {'bgm', 'system'} <= channels[0].keys() and channels[0].keys() == channels[1].keys(), stable
    assert channels[0]['system']['token'] == probe.audio[-1]['token'], stable
    for kind, first_channel in channels[0].items():
        second_channel = channels[1][kind]
        assert not first_channel['playing'] and not second_channel['playing'], stable
        assert first_channel['player_id'] == second_channel['player_id'], stable
        assert abs(first_channel['position'] - second_channel['position']) <= 30, stable
    probe.adb('shell', 'am', 'start', '-W', '--activity-reorder-to-front', '-n', ACTIVITY)
    probe.packets('lifecycle-events.jsonl', activity_offset, lambda e: e['event'] == 'resumed')
    probe.bounds(SETTINGS)
    assert len(probe.events('text-events.jsonl')) == text_offset
    assert probe.settings() == opened

    current = probe.area('= PAGE++', 'settings PAGE plays after Home return while BGM stays paused', [(1, volume)])
    assert current['page'] == 2 and {k: current[k] for k in frozen} == frozen
    # Shortcut bindings are included in the settings payload on page 5 only.
    for page_number in range(3, 6):
        current = probe.area('= PAGE++', f'story settings page {page_number}', [(1, volume)])
        assert current['page'] == page_number and {k: current[k] for k in frozen} == frozen
    assert current['shortcuts'] == bindings, current['shortcuts']
    bgm = channels[1]['bgm']
    intervening = probe.events('host-events.jsonl')[host_offset:]
    assert not any(e['kind'] == 'bgm' and e['event'] in ('playing', 'resumed') for e in intervening), intervening

    def close_config(percent):
        offset = len(probe.events('story-settings-events.jsonl'))

        def back():
            probe.key('BACK')
            return probe.packets('story-settings-events.jsonl', offset, lambda e: e['event'] == 'closed')

        closed = probe.action('settings returns to the same story with PAGE', [(1, percent)], back)
        assert closed['sequence'] == number

    close_config(volume)
    config_dialogue = same_dialogue(text_offset)
    resumed = probe.packets('audio-lifecycle.jsonl', lifecycle_offset,
                            lambda e: e['event'] == 'resumed' and e['channels'].get('bgm', {}).get('playing'))
    assert resumed['channels']['bgm']['player_id'] == bgm['player_id'], resumed
    assert resumed['channels']['bgm']['token'] == bgm['token'], resumed
    assert abs(resumed['channels']['bgm']['position'] - bgm['position']) < 200, resumed

    def reopen():
        before = probe.settings()
        probe.control('原版设置')
        return probe.after_settings(before, 1)

    before_reset = probe.action('open settings to verify reset button', [(1, volume)], reopen)

    def reset():
        probe.control('恢复已接通选项默认值')
        return probe.after_settings(before_reset, 1)

    defaults = probe.action('Android restore-defaults button stays silent', [], reset)
    close_config(defaults['config']['VolSYS'])
    probe.control('结束运行')
    probe.wait('VM live bytes=0', lambda: b'VM live bytes=0' in probe.read('closed.txt'))
    host = probe.events('host-events.jsonl')
    assert host[-1]['event'] == 'released' and host[-1]['value'] == 0, host[-1]
    files = probe.adb('shell', 'run-as', PACKAGE, 'ls', f'files/{probe.run}').decode().splitlines()
    assert not any(f.startswith('audio-') and f.endswith('.ogg') for f in files), files
    assert [e['token'] for e in probe.events('ui-audio-events.jsonl')] == [e['token'] for e in probe.audio]
    return dict(page=number, scene_sha256=hashlib.sha256(scene).hexdigest(), back_dialogue=back_dialogue,
                config_dialogue=config_dialogue, modal_audio_paused=paused, home_audio=stable,
                resumed_audio=resumed, original_config=config, original_bindings=bindings,
                reset_config=defaults['config'], vm_live_bytes=0)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('adb', 'serial', 'cli'):
        parser.add_argument('--' + name, required=True)
    for name in ('work', 'output', 'games'):
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    run = 'menu_sounds_' + uuid.uuid4().hex
    originals = original_sounds(args.cli, args.games)
    probe = Probe(args.adb, args.serial, run, originals)
    machine = probe.adb('shell', 'uname', '-m').decode().strip()
    bridge = probe.adb('shell', 'getprop', 'ro.dalvik.vm.native.bridge').decode().strip()
    assert machine == 'x86_64' and bridge not in ('', '0'), (machine, bridge)
    apk = probe.adb('shell', 'pm', 'path', PACKAGE).decode().strip().removeprefix('package:')
    apk_sha = probe.adb('shell', 'sha256sum', apk).decode().split()[0]

    original_prefs = probe.private(PREFERENCES)
    assert any(node.get('name') == 'dat_tree' for node in ET.fromstring(original_prefs)), 'configure the BLESS SAF tree first'
    try:
        print('run:', run, flush=True)
        probe.start_title('--ei', 'title_game', '0', '--es', 'save_probe', run,
                          '--es', 'read_history_probe', run, '--ez', 'lifecycle_probe', 'true')
        config, bindings = title_settings(probe)
        story = story_settings(probe, config, bindings)
    finally:
        probe.adb('shell', 'am', 'force-stop', PACKAGE)
        if probe.private(PREFERENCES) != original_prefs:
            # Restore exact bytes after the process stops, including if a failed case interrupted a gesture.
            command = f"sh -c 'cat > {PREFERENCES}.probe && mv {PREFERENCES}.probe {PREFERENCES}'"
            probe.adb('shell', '-T', 'run-as', PACKAGE, command, input=original_prefs)
        assert probe.private(PREFERENCES) == original_prefs, 'could not restore application preferences'
        probe.adb('shell', 'rm', '-f', probe.ui_path)
        for name in ('host-events.jsonl', 'ui-audio-events.jsonl', 'audio-gain-events.jsonl', 'audio-lifecycle.jsonl',
                     'story-menu-events.jsonl', 'story-settings-events.jsonl', 'settings-presented.json', 'error.txt', 'closed.txt'):
            raw = probe.read(name)
            if raw:
                (args.work / f'{run}-{name}').write_bytes(raw)

    report = dict(run=run, machine=machine, native_bridge=bridge, apk_sha256=apk_sha,
                  original_sounds=originals, actions=probe.actions, audio=probe.audio, story=story,
                  application_preferences_restored=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print('PASS BLESS story menu and settings cues, playback, gain, lifecycle and VM cleanup', flush=True)


if __name__ == '__main__':
    main()
