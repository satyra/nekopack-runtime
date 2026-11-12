"""Compare a tested APK with a fresh build and record current source blob IDs.

Run build_probe.py first. This verifies APK contents and certificates; it does
not itself rebuild or claim that an arbitrary supplied APK was freshly built.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[2]
INPUTS = ['Cargo.toml', 'Cargo.lock', '.cargo', 'crates', 'vendor',
          'platforms/android/native', 'platforms/android/src',
          'platforms/android/res', 'platforms/android/AndroidManifest.xml',
          'platforms/android/jni.cpp', 'platforms/android/build_probe.py']


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--tested', type=Path, required=True)
    parser.add_argument('--rebuilt', type=Path, required=True)
    parser.add_argument('--java', type=Path, required=True)
    parser.add_argument('--apksigner', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    artifacts = []
    for path in (args.tested, args.rebuilt):
        with zipfile.ZipFile(path) as apk:
            assert len(apk.namelist()) == len(set(apk.namelist())), 'duplicate ZIP entry'
            entries = {name: digest(apk.read(name)) for name in sorted(apk.namelist())}
            timestamps = {name: apk.getinfo(name).date_time for name in sorted(apk.namelist())}
        verified = subprocess.check_output(
            [str(args.java), '-jar', str(args.apksigner), 'verify', '--print-certs', str(path)],
            text=True)
        certificates = re.findall(r'Signer #\d+ certificate SHA-256 digest: ([0-9a-f]{64})', verified)
        assert certificates, 'no signer certificate'
        artifacts.append(dict(path=str(path), sha256=digest(path.read_bytes()),
                              entries=entries, zip_timestamps=timestamps,
                              certificates_sha256=certificates))
    assert artifacts[0]['entries'] == artifacts[1]['entries'], 'APK entry bytes differ'
    assert artifacts[0]['certificates_sha256'] == artifacts[1]['certificates_sha256'], 'signer differs'
    names = subprocess.check_output(
        ['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard', '--', *INPUTS],
        cwd=ROOT).decode().split('\0')
    names = sorted(set(filter(None, names)))
    # Git blob IDs use repository newline normalization, so staged/committed
    # content can be checked without relying on checkout timestamps.
    blobs = subprocess.check_output(['git', 'hash-object', '--', *names], cwd=ROOT,
                                    text=True).splitlines()
    report = dict(scope='Fresh build compared with tested APK; all uncompressed ZIP entries and signing certificates match.',
                  artifacts=artifacts, source_git_blobs=dict(zip(names, blobs, strict=True)),
                  source_fingerprint_sha256=digest('\n'.join(f'{n}\t{b}' for n, b in zip(names, blobs)).encode()),
                  apk_entries_identical=True, certificates_identical=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n', encoding='utf-8')
    print(f'PASS: {len(artifacts[0]["entries"])} identical APK entries, same signer, {len(names)} source inputs recorded')


if __name__ == '__main__':
    main()
