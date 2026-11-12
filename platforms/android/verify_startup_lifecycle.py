"""Metadata-only, alternating five-game cold-start and Home/resume regression.

Run on an already configured SAF device with the specified APK installed. Each
attempt performs two resumes and a third, final Home observation: the next pause
supplies an actual playback position after each resume (BGM has no recurring
progress telemetry). No story/image files are copied to the host. JSON is saved
after every stage; a failed attempt is recorded and raised without retrying.
"""

import argparse
import hashlib
import json
import re
import shlex
import subprocess
import time
import uuid
import xml.etree.ElementTree as ET
from pathlib import Path

from verify_menu_sounds import ACTIVITY, PACKAGE, PREFERENCES, Probe, TITLE


GAMES = [('BLESS', '', 0), ('BIRTH', 'BIRTH', 0), ('BRAVE', 'BRAVE', 0),
         ('BLACK', 'BLACK', 0), ('Endless Dungeon', 'Endless Dungeon', 0)]


def save(path, report):
    # Windows readers may hold handles without FILE_SHARE_DELETE. Write in
    # place; monitor the progress log, and read this JSON after the writer exits.
    path.write_text(json.dumps(report, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def resume_state(previous, source, expected, rounds):
    assert previous['apk_sha256'] == expected, 'resume report APK differs from --apk'
    assert previous['rounds'] == rounds, 'resume report rounds differ from --rounds'
    for field in ('installed_sha256_before', 'installed_sha256_after', 'apk_sha256_after'):
        if field in previous:
            assert previous[field] == expected, ('resume report APK integrity differs', field)
    planned = {(game, number) for number in range(1, rounds + 1) for game, _, _ in GAMES}
    seen = set()
    passed, unfinished = [], []
    for attempt in previous['attempts']:
        key = (attempt['game'], attempt['round'])
        assert key in planned and key not in seen, ('invalid or duplicate resume attempt', key)
        seen.add(key)
        (passed if attempt['status'] == 'passed' else unfinished).append(attempt)
    interruption = {key: value for key, value in previous.items()
                    if key not in ('attempts', 'interruptions')}
    interruption.update(source_report=str(source), attempts=unfinished)
    return passed, [*previous.get('interruptions', []), interruption]


def pending_attempts(rounds, passed):
    complete = {(attempt['game'], attempt['round']) for attempt in passed}
    return [(number, game, directory, index) for number in range(1, rounds + 1)
            for game, directory, index in GAMES if (game, number) not in complete]


def shell_script(probe, script):
    return probe.adb('exec-out', 'run-as', PACKAGE, 'sh', '-c', script).decode()


def story_metadata(probe, filename, fields):
    """Extract allowed scalar fields on-device, before any narrative reaches us."""
    path = shlex.quote(f'files/{probe.run}/{filename}')
    expressions = []
    for key, kind in fields.items():
        value = '"([^"]*)"' if kind == 'string' else '(true|false|[0-9]+)'
        expressions.append(f's/.*"{key}"[[:space:]]*:[[:space:]]*{value}.*/{key}=\\1/p')
    sed = shlex.quote('h;' + ';'.join('g;' + expression for expression in expressions))
    raw = shell_script(probe, f'if [ -f {path} ]; then sed -nE {sed} {path}; fi\n')
    result = {}
    for line in raw.splitlines():
        key, value = line.split('=', 1)
        result[key] = value if fields[key] == 'string' else (
            value == 'true' if value in ('true', 'false') else int(value))
    return result


def page_sequences(probe):
    names = shell_script(probe, f'for f in files/{probe.run}/page-*.json; do '
                         'if [ -f "$f" ]; then echo "${f##*/}"; fi; done\n').splitlines()
    return sorted(int(re.fullmatch(r'page-([0-9]+)\.json', name)[1]) for name in names)


def page(probe, sequence):
    state = story_metadata(probe, f'page-{sequence}.json', {'source': 'string', 'line': 'number'})
    if not state:
        return None
    assert set(state) == {'source', 'line'}, ('missing page coordinates', state)
    sequences = page_sequences(probe)
    assert max(sequences) == sequence, ('story advanced without input', sequences, sequence)
    return dict(sequence=sequence, **state)


def check_boundaries(probe, row, checkpoint):
    names = shell_script(probe, f'for f in files/{probe.run}/effect-*.json; do '
                         'if [ -f "$f" ]; then echo "${f##*/}"; fi; done\n').splitlines()
    seen = row.setdefault('effect_tokens_inspected', [])
    for name in names:
        token = int(re.fullmatch(r'effect-([0-9]+)\.json', name)[1])
        if token in seen:
            continue
        metadata = story_metadata(probe, name, {'host_effect': 'string'})
        if not metadata:
            continue
        seen.append(token)
        if metadata['host_effect'] in ('select', 'movie'):
            row['unsupported_boundary'] = dict(token=token, kind=metadata['host_effect'])
            checkpoint()
            raise AssertionError(('audio observation search reached unsupported boundary', row['unsupported_boundary']))


def complete_page(probe, sequence, row, checkpoint):
    def ready():
        check_boundaries(probe, row, checkpoint)
        state = story_metadata(probe, f'text-{sequence}.json',
                               {'sequence': 'number', 'complete': 'number'})
        return state if state.get('complete') else None
    text = wait(probe, f'dialogue {sequence} complete', ready)
    assert text['sequence'] == sequence
    return page(probe, sequence)


def looping_channels(events):
    """Current continuous channels from playback/completion metadata, not game names."""
    active = {}
    loops = set()
    progress = set()
    for item in events:
        kind, event_name, token = item['kind'], item['event'], item['token']
        if event_name in ('released', 'load_released', 'stopped_all'):
            active.clear()
        if kind not in ('bgm', 'env'):
            continue
        if event_name == 'playing':
            active[kind] = token
        elif event_name in ('stopped', 'completed'):
            active.pop(kind, None)
        elif event_name == 'looping' and item['value'] == 1:
            loops.add((kind, token))
        elif event_name == 'progress' and item['value'] > 0:
            progress.add((kind, token))
    return {kind: token for kind, token in active.items()
            if (kind == 'bgm' or (kind, token) in loops) and (kind, token) in progress}


def pid(probe):
    result = subprocess.run([*probe.command, 'shell', 'pidof', PACKAGE], capture_output=True, timeout=45)
    assert result.returncode in (0, 1), result.stderr.decode(errors='replace')
    return result.stdout.decode().strip()


def wait(probe, description, predicate, timeout=90):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        error = probe.read('error.txt')
        assert not error, (probe.run, 'application error.txt exists', hashlib.sha256(error).hexdigest())
        assert pid(probe), (probe.run, 'process exited', description)
        result = predicate()
        if result:
            return result
        time.sleep(.1)
    raise AssertionError((probe.run, 'timed out', description))


def event(probe, filename, offset, label):
    return wait(probe, f'{filename}:{label}', lambda: next(
        (e for e in probe.events(filename)[offset:] if e['event'] == label), None))


def checked_am(probe, *args):
    result = subprocess.run([*probe.command, 'shell', 'am', 'start', '-W', *args],
                            capture_output=True, timeout=45)
    observed = dict(exitcode=result.returncode, stdout=result.stdout.decode(errors='replace'),
                    stderr=result.stderr.decode(errors='replace'))
    return observed


def enter_first_page(probe, title, row, checkpoint):
    button = next(b for b in title['buttons'] if b['id'] == 1)
    assert button['enabled'], 'title start button disabled'
    probe.tap(*probe.point(TITLE, button['bounds']))
    confirmed = False

    def ready():
        nonlocal confirmed
        check_boundaries(probe, row, checkpoint)
        state = page(probe, 1)
        if state:
            return state
        # Only a visible Android positive button is confirmation evidence. The
        # XML is filtered on-device: dialogue accessibility text is never read.
        if not confirmed:
            probe.adb('shell', 'uiautomator', 'dump', probe.ui_path)
            path = shlex.quote(probe.ui_path)
            script = (f'sed \'s/></>\\n</g\' {path} | '
                      'sed -nE \'/resource-id="android:id\\/button1"/ '
                      's/.*bounds="([^\"]*)".*/\\1/p\'\n')
            bounds = shell_script(probe, script).strip()
            if bounds:
                x1, y1, x2, y2 = map(int, re.findall(r'\d+', bounds))
                probe.tap((x1 + x2) // 2, (y1 + y2) // 2)
                confirmed = True
                row['positive_confirmation_observed'] = True
                checkpoint()
        return None

    state = wait(probe, 'first story page', ready)
    state = complete_page(probe, 1, row, checkpoint)
    row['first_page'] = state
    row['first_text_complete'] = True
    checkpoint()
    return state


def select_audio_page(probe, first, row, checkpoint, maximum):
    state = first
    row['audio_page_search'] = []
    for attempt in range(maximum):
        check_boundaries(probe, row, checkpoint)
        events = probe.events('host-events.jsonl')
        channels = looping_channels(events)
        observed = dict(page=state, looping_channels=channels,
                        played_channels=sorted({e['kind'] for e in events if e['event'] == 'playing'}))
        row['audio_page_search'].append(observed)
        checkpoint()
        if channels:
            row['page'] = state
            row['audio_observation_channels'] = channels
            checkpoint()
            return state
        assert attempt + 1 < maximum, ('no sustained bgm/env found within page search limit', maximum)
        layout = probe.obj('layout.json')
        assert layout and layout['stage_width'] > 0 and layout['stage_height'] > 0, 'missing story stage layout'
        point = (layout['stage_width'] // 2, layout['stage_height'] // 2)
        observed['advance_stage_point'] = point
        checkpoint()
        probe.tap(*point)

        def next_page():
            check_boundaries(probe, row, checkpoint)
            sequences = page_sequences(probe)
            return max(sequences) if sequences and max(sequences) > state['sequence'] else None

        sequence = wait(probe, 'next page while seeking sustained audio', next_page)
        state = complete_page(probe, sequence, row, checkpoint)


def pause(probe, expected_page, row, checkpoint):
    offsets = {name: len(probe.events(name)) for name in
               ('lifecycle-events.jsonl', 'audio-lifecycle.jsonl', 'host-events.jsonl')}
    record = dict(pid_before=pid(probe))
    row['pauses'].append(record)
    checkpoint()
    probe.key('HOME')
    paused = event(probe, 'lifecycle-events.jsonl', offsets['lifecycle-events.jsonl'], 'paused')
    audio = event(probe, 'audio-lifecycle.jsonl', offsets['audio-lifecycle.jsonl'], 'paused')
    stable = [event(probe, 'audio-lifecycle.jsonl', offsets['audio-lifecycle.jsonl'],
                    f'paused_stable_{i}') for i in (1, 2)]
    assert stable[1]['time_ms'] - stable[0]['time_ms'] >= 900, 'pause probes too close'
    assert set(stable[0]['channels']) == set(stable[1]['channels']), 'paused channel set changed'
    for kind, first in stable[0]['channels'].items():
        second = stable[1]['channels'][kind]
        assert not first['playing'] and not second['playing'], ('background audio playing', kind)
        assert all(first[k] == second[k] for k in ('token', 'player_id', 'position')), (
            'background player changed', kind, first, second)
    active = {e['kind']: e['token'] for e in probe.events('host-events.jsonl')[offsets['host-events.jsonl']:]
              if e['event'] == 'paused'}
    for kind, token in active.items():
        assert kind in stable[1]['channels'] and stable[1]['channels'][kind]['token'] == token, (
            'paused active channel disappeared', kind, token)
    for kind, token in row['audio_observation_channels'].items():
        assert active.get(kind) == token, ('selected sustained channel not active at pause', kind, token)
    assert page(probe, expected_page['sequence']) == expected_page, 'background changed page coordinates'
    assert pid(probe) == record['pid_before'], 'background process changed'
    record.update(lifecycle=paused, audio=audio, stable=stable, active=active,
                  page=expected_page, host_offset=offsets['host-events.jsonl'])
    checkpoint()
    return record


def resume(probe, paused, expected_page, row, checkpoint):
    lifecycle_offset = len(probe.events('lifecycle-events.jsonl'))
    audio_offset = len(probe.events('audio-lifecycle.jsonl'))
    record = {}
    row['resumes'].append(record)
    checkpoint()
    record['am'] = checked_am(probe, '--activity-reorder-to-front', '-n', ACTIVITY)
    checkpoint()
    assert record['am']['exitcode'] == 0 and 'Status: ok' in record['am']['stdout'], record['am']
    lifecycle = event(probe, 'lifecycle-events.jsonl', lifecycle_offset, 'resumed')
    audio = event(probe, 'audio-lifecycle.jsonl', audio_offset, 'resumed')
    before = paused['lifecycle']
    assert lifecycle['instance'] == before['instance'] == row['instance'], 'Activity recreated'
    assert pid(probe) == paused['pid_before'] == row['pid'], 'resume process changed'
    assert 0 <= lifecycle['active_ms'] - before['active_ms'] < 150, 'active clock advanced in background'
    for kind in paused['active']:
        off = paused['stable'][1]['channels'][kind]
        on = audio['channels'][kind]
        assert on['playing'], ('active channel did not resume', kind)
        assert all(on[k] == off[k] for k in ('token', 'player_id')), ('resume replaced player', kind)
    assert page(probe, expected_page['sequence']) == expected_page, 'resume changed page coordinates'
    record.update(lifecycle=lifecycle, audio=audio, page=expected_page,
                  active_channels=sorted(paused['active']), progress_verified=[],
                  continuous_channels=sorted(row['audio_observation_channels']))
    checkpoint()
    # An observation interval, not a startup retry/delay: next Home samples the
    # position after foreground playback. Paused probes themselves wait 1.5 s.
    time.sleep(0.6)
    return record


def verify_progress(probe, resumed, next_pause):
    # Include completion events that happened between resume and the next Home.
    completions = [e for e in probe.events('host-events.jsonl')
                   if e['time_ms'] >= resumed['audio']['time_ms'] and e['event'] == 'completed']
    for kind in resumed['active_channels']:
        on = resumed['audio']['channels'][kind]
        later = next_pause['audio']['channels'].get(kind)
        if later is None:
            assert any(e['kind'] == kind and e['token'] == on['token'] for e in completions), (
                'channel vanished without completion', kind)
            resumed.setdefault('completed_before_observation', []).append(kind)
            continue
        assert all(later[k] == on[k] for k in ('token', 'player_id')), ('foreground replaced player', kind)
        assert later['position'] != on['position'], ('resumed playback did not progress', kind, on, later)
        resumed['progress_verified'].append(kind)
    assert set(resumed['continuous_channels']) <= set(resumed['progress_verified']), (
        'selected sustained channels lack playback progress', resumed['continuous_channels'])


def installed_hash(probe):
    paths = probe.adb('shell', 'pm', 'path', PACKAGE).decode().splitlines()
    assert len(paths) == 1 and paths[0].startswith('package:'), ('expected single APK', paths)
    return probe.adb('shell', 'sha256sum', paths[0][8:]).decode().split()[0]


def diagnostics(probe):
    result = {'pid': pid(probe)}
    for name, args in {
        'crash': ['logcat', '-b', 'crash', '-d', '-v', 'threadtime'],
        'main_system': ['logcat', '-b', 'main', '-b', 'system', '-d', '-v', 'threadtime',
                        '-s', 'ActivityManager:I', 'ActivityTaskManager:I', 'AndroidRuntime:E', 'libc:F'],
        'activity': ['shell', 'dumpsys', 'activity', 'activities'],
        'process': ['shell', 'dumpsys', 'activity', 'processes'],
    }.items():
        command = subprocess.run([*probe.command, *args], capture_output=True, timeout=45)
        lines = command.stdout.decode(errors='replace').splitlines()
        if name in ('activity', 'process'):
            lines = [line for line in lines if PACKAGE in line or 'mResumedActivity' in line]
        result[name] = dict(exitcode=command.returncode, lines=lines[-100:],
                            stderr=command.stderr.decode(errors='replace'))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--rounds', type=int, default=10, help='rounds per game (default 10 = 50 starts)')
    parser.add_argument('--serial', default='emulator-5572')
    parser.add_argument('--output', type=Path, default=Path('.local/android-probe/startup-lifecycle.json'))
    parser.add_argument('--resume-from', type=Path,
                        help='explicitly carry over passed attempts from an interrupted report into a new output')
    parser.add_argument('--apk', type=Path, default=Path('.local/android-probe/nekopack-core-redirect-voice-arm64.apk'))
    parser.add_argument('--adb', default='D:/codex/tools/android/sdk/platform-tools/adb.exe')
    parser.add_argument('--game-root', default='raw:/storage/emulated/0/Download/NekoPack-BLESS')
    parser.add_argument('--max-audio-pages', type=int, default=32,
                        help='maximum completed dialogue pages to seek continuous bgm/env; no choice/movie traversal')
    args = parser.parse_args()
    assert args.rounds > 0, '--rounds must be positive'
    assert args.max_audio_pages > 0, '--max-audio-pages must be positive'
    args.output.parent.mkdir(parents=True, exist_ok=True)
    assert not args.output.exists(), 'choose a new output path; existing evidence is preserved'
    expected = hashlib.sha256(args.apk.read_bytes()).hexdigest()
    passed, interruptions = [], []
    if args.resume_from is not None:
        previous = json.loads(args.resume_from.read_text(encoding='utf-8'))
        passed, interruptions = resume_state(previous, args.resume_from.resolve(), expected, args.rounds)
    pending = {(game, number) for number, game, _, _ in pending_attempts(args.rounds, passed)}
    session = 'startup-life-' + uuid.uuid4().hex[:10]
    probe = Probe(args.adb, args.serial, session, {})
    prefs = ET.fromstring(probe.private(PREFERENCES))
    tree = prefs.find("./string[@name='dat_tree']").text
    report = dict(schema=1, session=session, serial=args.serial, rounds=args.rounds,
                  apk_sha256=expected, status='running', attempts=passed, interruptions=interruptions,
                  carried_passed_attempts=len(passed),
                  procedure='Each attempt: cold launch, complete first dialogue, advance completed pages to sustained bgm/env metadata, 2 resumes and 3 Home pauses; final pause verifies last resumed position.',
                  max_audio_pages=args.max_audio_pages,
                  narrative_policy='Page/text scalar fields and positive-button bounds are filtered on-device; no image or narrative copied.')
    checkpoint = lambda: save(args.output, report)
    checkpoint()
    try:
        report['installed_sha256_before'] = installed_hash(probe)
        assert report['installed_sha256_before'] == expected, 'installed APK differs from --apk'
        for round_index in range(args.rounds):
            for game, subdirectory, index in GAMES:
                if (game, round_index + 1) not in pending:
                    continue
                run = f'{session}-{round_index + 1}-{game.lower().replace(" ", "-")}'
                probe = Probe(args.adb, args.serial, run, {})
                row = dict(game=game, round=round_index + 1, run=run, status='running', pauses=[], resumes=[])
                report['attempts'].append(row)
                checkpoint()
                directory = args.game_root + ('/' + subdirectory if subdirectory else '')
                started = time.monotonic()
                try:
                    row['launch'] = probe.start_activity(
                        '--ei', 'title_game', str(index), '--es', 'library_tree', tree,
                        '--es', 'library_directory', shlex.quote(directory), '--ez', 'library_original', 'true',
                        '--es', 'save_probe', run, '--es', 'read_history_probe', run,
                        '--ez', 'lifecycle_probe', 'true', '--ez', 'dialogue_probe', 'true')
                    row['am_elapsed_ms'] = round((time.monotonic() - started) * 1000)
                    checkpoint()
                    created = wait(probe, 'session created', lambda: probe.obj('session-created.json'))
                    row['session_elapsed_ms'] = round((time.monotonic() - started) * 1000)
                    row['instance'] = created['instance']
                    row['pid'] = pid(probe)
                    checkpoint()
                    title = wait(probe, 'title presented', lambda: probe.obj('title-presented.json'))
                    row['title_elapsed_ms'] = round((time.monotonic() - started) * 1000)
                    checkpoint()
                    first = enter_first_page(probe, title, row, checkpoint)
                    selected = select_audio_page(probe, first, row, checkpoint, args.max_audio_pages)
                    previous_resume = None
                    for pause_index in range(3):
                        paused = pause(probe, selected, row, checkpoint)
                        if previous_resume is not None:
                            verify_progress(probe, previous_resume, paused)
                            checkpoint()
                        if pause_index < 2:
                            previous_resume = resume(probe, paused, selected, row, checkpoint)
                    row['status'] = 'passed'
                    row['elapsed_ms'] = round((time.monotonic() - started) * 1000)
                    checkpoint()
                    print(f'PASS {game} round {round_index + 1}: 2 resumes, 3 pauses', flush=True)
                except Exception:
                    row['status'] = 'failed'
                    if hasattr(probe, 'launch'):
                        row['launch'] = probe.launch
                    checkpoint()
                    raise
        report['audio_progress_coverage'] = {
            game: sum(bool(resume['progress_verified']) for row in report['attempts'] if row['game'] == game
                      for resume in row['resumes']) for game, _, _ in GAMES}
        assert all(report['audio_progress_coverage'].values()), ('missing actual audio resume coverage', report['audio_progress_coverage'])
        report['status'] = 'passed'
    except Exception as error:
        report['status'] = 'failed'
        report['failure'] = dict(type=type(error).__name__, message=str(error))
        checkpoint()
        report['diagnostics'] = diagnostics(probe)
        checkpoint()
        raise
    finally:
        report['apk_sha256_after'] = hashlib.sha256(args.apk.read_bytes()).hexdigest()
        report['installed_sha256_after'] = installed_hash(probe)
        if report['apk_sha256_after'] != expected or report['installed_sha256_after'] != expected:
            report['status'] = 'failed'
            checkpoint()
            raise AssertionError('APK changed during regression')
        checkpoint()


if __name__ == '__main__':
    main()
