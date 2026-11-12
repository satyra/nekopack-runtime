"""Sequential, APK-pinned voice matrix. No images or narrative text are inspected.

Run --plan-only to validate local fixture metadata without touching adb.
Every child uses real settings taps, force-stop/restart, and both voice states.
"""
import argparse
import datetime
import hashlib
import json
import re
import subprocess
import sys
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / '.local/android-probe'
SCRIPTS = Path(__file__).resolve().parent
ADB = 'D:/codex/tools/android/sdk/platform-tools/adb.exe'
PACKAGE = 'org.nekopack.probe'
GAMES = {'BLESS':16, 'BIRTH':18, 'BRAVE':18, 'BLACK':16, 'ENDLESS':18}


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def installed_hash(serial):
    command = [ADB, '-s', serial]
    rows = subprocess.check_output([*command, 'shell', 'pm', 'path', PACKAGE], timeout=45).decode().splitlines()
    assert len(rows) == 1 and rows[0].startswith('package:'), rows
    data = subprocess.check_output([*command, 'exec-out', 'cat', rows[0][8:]], timeout=90)
    return hashlib.sha256(data).hexdigest()


def load_fixture_manifests(paths):
    fixtures = {}
    for path in [BASE / 'isolated-fixtures/manifest.json', *paths]:
        records = json.loads(path.read_text(encoding='utf-8'))
        if isinstance(records, dict):
            records = [records]
        for record in records:
            fixtures[record['game']] = dict(record, manifest_path=str(path.resolve()))
    return fixtures


def build_plan(games, fixtures):
    index = json.loads((BASE / 'remaining-fixtures/index.json').read_text())
    plan = []
    for game in games:
        for fixture in index[game]:
            assert (BASE / 'remaining-fixtures' / fixture['file']).is_file(), fixture['file']
        isolated = fixtures[game]['voice_resources'] if game in fixtures else []
        gaps = [dict(game=game, tag=voice['tag'], cue=voice['cue'],
                     reason=f"Borrowed {voice['original_game']} resource; {game} original audio not verified")
                for voice in isolated if voice['borrowed']]
        plan.append(dict(game=game, expected_controls=GAMES[game], expected_states=GAMES[game]*2,
                         route_candidates=[x['tag'] for x in index[game] if x['tag'] != 'env'],
                         isolated_candidates=[voice['tag'] for voice in isolated],
                         fixture_manifest=fixtures[game]['manifest_path'] if game in fixtures else None,
                         original_audio_gaps=gaps))
    return plan


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--serial')
    parser.add_argument('--apk', type=Path)
    parser.add_argument('--sha256')
    parser.add_argument('--games', nargs='+', choices=list(GAMES), default=list(GAMES))
    parser.add_argument('--output', type=Path)
    parser.add_argument('--fixture-manifest', type=Path, action='append', default=[],
                        help='Additional fixture manifest; repeatable, later entries override defaults by game')
    parser.add_argument('--plan-only', action='store_true')
    args = parser.parse_args()
    fixtures = load_fixture_manifests(args.fixture_manifest)
    plan = build_plan(args.games, fixtures)
    if args.plan_only:
        print(json.dumps(plan, indent=2))
        return 0
    assert args.serial and args.apk and args.sha256, 'Run requires --serial --apk --sha256'
    expected_hash = args.sha256.lower()
    assert re.fullmatch('[0-9a-f]{64}', expected_hash), '--sha256 must be a full SHA256'
    assert digest(args.apk) == expected_hash, 'Local APK differs from pinned SHA256'
    for fixture in fixtures.values():
        if fixture['game'] not in args.games:
            continue
        remote = '/storage/emulated/0/Download/NekoPack-BLESS/' + fixture['directory']
        for entry in fixture['files']:
            checked = subprocess.run([ADB,'-s',args.serial,'shell','sha256sum',remote+'/'+entry['name']],
                                     capture_output=True,timeout=45)
            assert checked.returncode == 0 and checked.stdout.decode().split()[0] == entry['sha256'], (
                'Missing or changed isolated test fixture',fixture['game'],entry['name'],
                checked.stderr.decode(errors='replace'))
    output = args.output or BASE / ('voice-matrix-' + datetime.datetime.now().strftime('%Y%m%d-%H%M%S') + '-' + uuid.uuid4().hex[:6])
    output.mkdir(parents=True, exist_ok=False)
    report = dict(apk=str(args.apk.resolve()), sha256=expected_hash, serial=args.serial,
                  plan=plan, jobs=[], controls={}, results=[], failures=[],
                  original_audio_gaps=[gap for game_plan in plan for gap in game_plan['original_audio_gaps']])
    def save():
        (output/'summary.json').write_text(json.dumps(report,indent=2),encoding='utf-8')
    save()
    for game in args.games:
        scripts = ['verify-remaining-settings.py']
        if game in ('BIRTH','BRAVE','BLACK'):
            scripts.append('verify-isolated-original-audio.py')
        for script in scripts:
            # Recheck before each independent process so results cannot mix builds.
            assert digest(args.apk) == expected_hash, 'Local APK changed during regression'
            actual_hash = installed_hash(args.serial)
            assert actual_hash == expected_hash, ('Installed APK differs from pinned SHA256',actual_hash)
            command = [sys.executable,'-u',str(SCRIPTS/script),args.serial,game]
            if script == 'verify-remaining-settings.py':
                command.append('audio')
            logfile = output / f'{game.lower()}-{Path(script).stem}.log'
            print('START',game,script,'APK',expected_hash,flush=True)
            with logfile.open('w',encoding='utf-8') as log:
                process = subprocess.Popen(command,cwd=ROOT,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,
                                           text=True,encoding='utf-8',errors='replace')
                for line in process.stdout:
                    log.write(line); log.flush()
                    print(line,end='',flush=True)
                    if line.startswith('MATRIX '):
                        record = json.loads(line[7:])
                        if record['event'] == 'controls':
                            report['controls'][game] = record['tags']
                        elif record['event'] == 'result':
                            report['results'].append(record)
                        save()
                code = process.wait()
            report['jobs'].append(dict(game=game,script=script,exit_code=code,log=str(logfile)))
            if code:
                report['failures'].append(f'{game} {script} exited {code}')
            save()
    matrix = []
    for game in args.games:
        controls = report['controls'].get(game,[])
        if len(controls) != GAMES[game]:
            report['failures'].append(f'{game}: expected {GAMES[game]} controls, found {len(controls)}')
        for tag in controls:
            rows = [r for r in report['results'] if r['game']==game and r['tag']==tag]
            states = {r['state'] for r in rows}
            complete = states == {'playing','silent'}
            matrix.append(dict(game=game,tag=tag,playing='playing' in states,silent='silent' in states,
                               original_audio=bool(rows) and all(r['original_audio'] for r in rows),
                               origins=sorted({r['origin'] for r in rows})))
            if not complete:
                report['failures'].append(f'{game} {tag}: missing states {sorted({"playing","silent"}-states)}')
    report.update(matrix=matrix,controls_expected=sum(GAMES[g] for g in args.games),
                  states_expected=sum(GAMES[g]*2 for g in args.games),
                  controls_verified=sum(r['playing'] and r['silent'] for r in matrix),
                  states_verified=sum(r['playing']+r['silent'] for r in matrix),
                  matrix_passed=not report['failures'],
                  all_original_audio_verified=not report['failures'] and not report['original_audio_gaps']
                  and all(row['original_audio'] for row in matrix))
    save()
    print('SUMMARY',json.dumps({k:report[k] for k in ['controls_expected','states_expected','controls_verified','states_verified','matrix_passed','all_original_audio_verified','original_audio_gaps','failures']}),flush=True)
    print('REPORT',output/'summary.json',flush=True)
    return int(bool(report['failures']))


if __name__ == '__main__':
    raise SystemExit(main())
