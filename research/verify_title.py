"""Verify owned, decoded fixtures without publishing scripts or asset names."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def tree_hash(root):
    digest = hashlib.sha256()
    for path in sorted(root.rglob('*')):
        if path.is_file():
            name = path.relative_to(root).as_posix().encode('utf8')
            data = path.read_bytes()
            digest.update(len(name).to_bytes(8, 'little'))
            digest.update(name)
            digest.update(len(data).to_bytes(8, 'little'))
            digest.update(data)
    return digest.hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--fixtures', type=Path, required=True)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = {'scope': 'headless title choice 1 through route and first story call; no device rendering',
              'cli_sha256': hashlib.sha256(args.cli.read_bytes()).hexdigest(), 'games': []}
    for index, (game, child, modules) in enumerate([
        ('BLESS', 'P01', 10), ('BIRTH', '012401', 10),
        ('BRAVE', '010901', 11), ('BLACK', 'P01', 10)
    ]):
        root = args.fixtures / str(index)
        result = subprocess.run([str(args.cli.resolve()), 'run-title', str(root), '1'],
                                capture_output=True, timeout=30)
        lines = result.stdout.decode('utf8').splitlines()
        boundary = [line.split('\t')[1:] for line in lines if line.startswith('story-boundary\t')]
        complete = [line.split('\t')[1:] for line in lines if line.startswith('complete\t')]
        slots = [line for line in lines if line.startswith('slot\t')]
        passed = (result.returncode == 0 and 'menu-result\t1' in lines
                  and any(line.startswith('after-menu\tstory1\t') for line in lines)
                  and len(boundary) == 1 and boundary[0][0] == child and boundary[0][2] == '2'
                  and len(complete) == 1 and int(complete[0][0]) == modules
                  and (index not in (1, 2) or 'slot\tG\t800\t1' in slots))
        invalid = subprocess.run([str(args.cli.resolve()), 'run-title', str(root), '999'],
                                 capture_output=True, timeout=30)
        rejected = (invalid.returncode != 0
                    and b'requested input does not identify an enabled spot' in invalid.stderr
                    and b'menu-result' not in invalid.stdout)
        report['games'].append({
            'game': game, 'fixture_sha256': tree_hash(root), 'exit_code': result.returncode,
            'passed': passed, 'boundary': boundary, 'complete': complete,
            'state_sha256': hashlib.sha256('\n'.join(slots).encode()).hexdigest(),
            'invalid_choice_rejected': rejected,
            'stderr': result.stderr.decode('utf8')})
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n', encoding='utf8')
    print(json.dumps({'passed': sum(g['passed'] for g in report['games']),
                      'invalid_choice_rejected': sum(g['invalid_choice_rejected'] for g in report['games'])}))
    if not all(g['passed'] and g['invalid_choice_rejected'] for g in report['games']):
        raise SystemExit(1)


if __name__ == '__main__':
    main()
