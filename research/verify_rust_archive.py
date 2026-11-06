"""Compare Rust /1 output with the separately implemented local u64 Python decoder.

All plaintext stays in memory. Only fingerprints/counts are written to the report.
Requires the Phase 0 sample manifest and external inspect_neko.py dependencies.
"""
import argparse
import hashlib
import importlib
import json
from pathlib import Path
import struct
import subprocess
import sys


def require(condition, message):
    if not condition:
        raise ValueError(message)


def run():
    parser = argparse.ArgumentParser()
    parser.add_argument('--games', type=Path, required=True)
    parser.add_argument('--decoder-dir', type=Path, required=True)
    parser.add_argument('--cli', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.decoder_dir))
    reference = importlib.import_module('inspect_neko')
    manifest = json.loads(Path(__file__).with_name('sample-audit.json').read_text('utf-8'))
    def command(name, path, *extra):
        return subprocess.run([str(args.cli.resolve()), name, str(path), *map(str, extra)],
                              check=True, capture_output=True).stdout
    report = {'scope': '/1 archive static differential verification; no gameplay verification',
        'reference': 'Existing local GARbro-derived inspect_neko.py u64 decoder versus Rust u16 lane decoder',
        'reference_sha256': hashlib.sha256((args.decoder_dir/'inspect_neko.py').read_bytes()).hexdigest(),
        'cli_sha256': hashlib.sha256(args.cli.read_bytes()).hexdigest(), 'archives': []}
    for file in manifest['files']:
        if not file['archives']:
            continue
        require(len(file['archives']) == 1 and file['archives'][0]['offset'] == 0, 'expected standalone archive')
        path = args.games / file['file']
        with path.open('rb') as stream:
            digest = hashlib.file_digest(stream, 'sha256').hexdigest()
            require(digest == file['sha256'], f'sample changed: {path.name}')
            stream.seek(0)
            head = stream.read(24)
            seed, priority, key, length = struct.unpack_from('<4I', head, 8)
            index = reference.decrypt(stream.read(length), key)
            rust_index = command('read-index', path)
            require(index == rust_index, f'index mismatch: {path.name}')
            require(hashlib.sha256(index).hexdigest() == file['archives'][0]['index_sha256'], 'Phase 0 index fingerprint mismatch')
            rows = command('inspect-archive', path).decode('ascii').splitlines()
            summary = rows[0].split('\t')
            require(summary == ['archive', f'{seed:08X}', str(priority), str(length),
                                str(file['archives'][0]['entries']), str(file['archives'][0]['trailing_bytes'])], 'header summary mismatch')
            entries, pos, offset = [], 0, 24 + length
            while pos < len(index):
                directory, count = struct.unpack_from('<2I', index, pos)
                pos += 8
                for _ in range(count):
                    name, size = struct.unpack_from('<2I', index, pos)
                    pos += 8
                    stream.seek(offset)
                    blockkey, blocksize = struct.unpack('<2I', stream.read(8))
                    require(size == blocksize, 'reference block length mismatch')
                    prefix = reference.decrypt(stream.read(min(size, 16)), blockkey)
                    entries.append((directory, name, offset, size, blockkey, prefix))
                    offset += 8 + size
            require(len(rows)-2 == len(entries), 'entry count mismatch')
            for ordinal, entry in enumerate(entries):
                directory, name, offset, size, blockkey, prefix = entry
                expected = [str(ordinal), f'{directory:08X}', f'{name:08X}', str(offset),
                            str(size), f'{blockkey:08X}', prefix.hex().upper()]
                require(rows[ordinal+2].split('\t') == expected, f'entry mismatch: {path.name}:{ordinal}')
            # Stratified, deterministic samples: positions, largest, tail lengths,
            # and first occurrence of each observed four-byte signature.
            selected = {0, len(entries)//2, len(entries)-1,
                        max(range(len(entries)), key=lambda i: entries[i][3])}
            seen_signatures, seen_tails = set(), set()
            for i, entry in enumerate(entries):
                signature, tail = entry[5][:4], entry[3] % 8
                if signature not in seen_signatures or tail not in seen_tails:
                    selected.add(i)
                seen_signatures.add(signature)
                seen_tails.add(tail)
            samples = []
            for ordinal in sorted(selected):
                _, _, offset, size, blockkey, _ = entries[ordinal]
                stream.seek(offset+8)
                expected = reference.decrypt(stream.read(size), blockkey)
                actual = command('read-entry', path, ordinal)
                require(actual == expected, f'payload mismatch: {path.name}:{ordinal}')
                samples.append({'ordinal': ordinal, 'length': size,
                                'sha256': hashlib.sha256(actual).hexdigest()})
        report['archives'].append({'file': file['file'], 'sha256': digest,
            'entries_compared': len(entries), 'decoded_index_equal': True,
            'all_entry_metadata_and_prefixes_equal': True, 'all_parity_checks_passed': True,
            'full_payload_samples': samples})
        print(f'{path.name}: {len(entries)} prefixes; {len(samples)} full payloads equal', flush=True)
    report['totals'] = {'archives': len(report['archives']),
        'entries': sum(a['entries_compared'] for a in report['archives']),
        'full_payload_samples': sum(len(a['full_payload_samples']) for a in report['archives']),
        'full_payload_bytes': sum(s['length'] for a in report['archives'] for s in a['full_payload_samples'])}
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')
    print(report['totals'])


if __name__ == '__main__':
    run()
