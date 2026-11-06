"""Read-only four-game verification of names, overlays and every detected MNG.
Only hashes, counts, names and provenance are persisted, never media or dialogue.
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import importlib
import json
from pathlib import Path
import struct
import subprocess
import sys
import zlib


def require(condition, message):
    if not condition:
        raise ValueError(message)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--games', type=Path, required=True)
    ap.add_argument('--decoder-dir', type=Path, required=True)
    ap.add_argument('--cli', type=Path, required=True)
    ap.add_argument('--output', type=Path, required=True)
    args = ap.parse_args()
    sys.path.insert(0, str(args.decoder_dir))
    ref = importlib.import_module('inspect_neko')
    manifest = json.loads(Path(__file__).with_name('sample-audit.json').read_text('utf8'))

    def cli(*argv):
        p = subprocess.run([str(args.cli.resolve()), *map(str, argv)], capture_output=True)
        require(p.returncode == 0, p.stderr.decode('utf8', errors='replace'))
        return p.stdout

    def hashbytes(seed, name):
        for b in name:
            seed = (81 * (ref.shift[b] ^ seed)) & 0xffffffff
        return seed

    checks = [(0, bytes([b])) for b in range(256)]
    seeds = sorted({int(f['archives'][0]['seed'], 16) for f in manifest['files'] if f['archives']})
    checks += [(seed, name) for seed in seeds for name in
               (b'', bytes(range(256)), b'script', b'start.txt', b'image/actor', b'IMAGE\\ACTOR', b'\x82\xa0')]
    for seed, name in checks:
        require(int(cli('hash-name', f'{seed:X}', name.hex()), 16) == hashbytes(seed, name), 'name hash mismatch')
    report = {'scope': 'static names/priority policy/MNG metadata; no engine playback verification',
              'cli_sha256': hashlib.sha256(args.cli.read_bytes()).hexdigest(),
              'reference_sha256': hashlib.sha256((args.decoder_dir/'inspect_neko.py').read_bytes()).hexdigest(),
              'hash_vectors_checked': len(checks), 'games': []}
    games = defaultdict(list)
    for f in manifest['files']:
        if f['archives']:
            games[f['file'].split('/')[0]].append(f)
    total_chunks = Counter()
    for game, files in games.items():
        packs, mng_records = [], []
        for f in files:
            path = args.games / f['file']
            with path.open('rb') as stream:
                require(hashlib.file_digest(stream, 'sha256').hexdigest() == f['sha256'], 'sample changed')
                stream.seek(8)
                seed, priority, key, size = struct.unpack('<4I', stream.read(16))
                index = ref.decrypt(stream.read(size), key)
                require(cli('read-index', path) == index, 'index mismatch')
                rows = cli('inspect-archive', path).decode('ascii').splitlines()[2:]
                entries, pos, offset = [], 0, 24+size
                mng_entries = []
                while pos < len(index):
                    dh, count = struct.unpack_from('<2I', index, pos); pos += 8
                    for _ in range(count):
                        nh, length = struct.unpack_from('<2I', index, pos); pos += 8
                        stream.seek(offset)
                        bk, bl = struct.unpack('<2I', stream.read(8))
                        require(bl == length, 'block length mismatch')
                        prefix = ref.decrypt(stream.read(min(length, 16)), bk)
                        ordinal = len(entries)
                        expected = [str(ordinal), f'{dh:08X}', f'{nh:08X}', str(offset), str(length), f'{bk:08X}', prefix.hex().upper()]
                        require(rows[ordinal].split('\t') == expected, 'entry/prefix regression')
                        entries.append((dh, nh, offset, length, bk))
                        if prefix.startswith(b'\x8aMNG\r\n\x1a\n'):
                            mng_entries.append(ordinal)
                        offset += 8+length
                require(len(rows) == len(entries), 'entry count mismatch')
                # Compare complete chunk framing, CRC and retained metadata bytes.
                expected_lines = []
                for ordinal in mng_entries:
                    _, _, offset, length, bk = entries[ordinal]
                    stream.seek(offset+8)
                    data = ref.decrypt(stream.read(length), bk)
                    h = struct.unpack_from('>7I', data, 16)
                    expected_lines.append('\t'.join(map(str, ['mng', ordinal, *h])))
                    at, sequence, counts = 8, 0, Counter()
                    while at < len(data):
                        length = struct.unpack_from('>I', data, at)[0]
                        kind = data[at+4:at+8]
                        body = data[at+8:at+8+length]
                        crc = struct.unpack_from('>I', data, at+8+length)[0]
                        require(zlib.crc32(kind+body) == crc, 'reference CRC mismatch')
                        counts[kind.decode('ascii')] += 1
                        known = {b'MHDR', b'FRAM', b'DEFI', b'TERM', b'tEXt', b'IHDR', b'IDAT', b'IEND', b'MEND'}
                        unknown = int(kind not in known and 65 <= kind[0] <= 90)
                        payload_hex = '' if kind in (b'IDAT', b'JDAT', b'JDAA') else body.hex().upper()
                        expected_lines.append('\t'.join(map(str, ['chunk', ordinal, sequence, at, kind.decode('ascii'), length, f'{crc:08X}', unknown, payload_hex])))
                        at += length+12; sequence += 1
                    require(at == len(data), 'MNG extent mismatch')
                    total_chunks.update(counts)
                    mng_records.append({'source': len(packs), 'ordinal': ordinal, 'sha256': hashlib.sha256(data).hexdigest(), 'mhdr': list(h), 'chunks': dict(counts), 'crc_and_raw_metadata_equal': True})
                actual = cli('inspect-mng-archive', path).decode('ascii').splitlines()
                require(actual == expected_lines, f'MNG metadata mismatch: {path.name}')
            packs.append({'path': path, 'file': f['file'], 'sha256': f['sha256'], 'seed': seed, 'priority': priority, 'entries': entries})
        grouped = defaultdict(list)
        for source, pack in enumerate(packs):
            for ordinal, (dh, nh, *_rest) in enumerate(pack['entries']):
                grouped[(pack['seed'], dh, nh)].append((source, ordinal, pack['priority']))
        overlaps, expected = [], []
        for (seed, dh, nh), candidates in sorted(grouped.items()):
            if len(candidates) < 2:
                continue
            top = max(c[2] for c in candidates)
            winners = [c for c in candidates if c[2] == top]
            selection = ['selected', *winners[0][:2]] if len(winners) == 1 else ['ambiguous', len(winners)]
            expected.append('\t'.join(map(str, ['overlap', f'{seed:08X}', f'{dh:08X}', f'{nh:08X}', *selection])))
            expected.extend('\t'.join(map(str, ['candidate', *c])) for c in candidates)
            record = {'seed': f'{seed:08X}', 'directory_hash': f'{dh:08X}', 'name_hash': f'{nh:08X}', 'candidates': candidates, 'resolution': selection}
            # Compare all uniquely prioritized overlaps in full (actual patch resources).
            if len(winners) == 1:
                hashes = []
                for source, ordinal, _priority in candidates:
                    pack = packs[source]
                    _, _, offset, size, bk = pack['entries'][ordinal]
                    with pack['path'].open('rb') as stream:
                        stream.seek(offset+8)
                        reference = ref.decrypt(stream.read(size), bk)
                    actual = cli('read-entry', pack['path'], ordinal)
                    require(actual == reference, 'patch payload mismatch')
                    hashes.append(hashlib.sha256(actual).hexdigest())
                record['payload_sha256'] = hashes
                record['payloads_differ'] = len(set(hashes)) > 1
            overlaps.append(record)
        require(cli('inspect-overlays', *(p['path'] for p in packs)).decode('ascii').splitlines() == expected, 'overlay mismatch')
        lookups = []
        for directory in (b'script', b'system'):
            for filename in (b'start.txt', b'title.txt', b'story.txt', b'setup.txt'):
                candidates = []
                for source, pack in enumerate(packs):
                    dh, nh = hashbytes(pack['seed'], directory), hashbytes(pack['seed'], filename)
                    candidates += [(source, ordinal, pack['priority']) for ordinal, e in enumerate(pack['entries']) if e[:2] == (dh, nh)]
                if not candidates:
                    expected = ['missing']
                else:
                    top = max(c[2] for c in candidates); winners = [c for c in candidates if c[2] == top]
                    selected = ['selected', *winners[0][:2]] if len(winners) == 1 else ['ambiguous', len(winners)]
                    expected = ['\t'.join(map(str, selected))]
                expected += ['\t'.join(map(str, ['candidate', *c])) for c in candidates]
                require(cli('lookup-name', directory.hex(), filename.hex(), *(p['path'] for p in packs)).decode('ascii').splitlines() == expected, 'lookup mismatch')
                lookups.append({'name': (directory+b'/'+filename).decode('ascii'), 'candidates': candidates})
        report['games'].append({'game': game, 'sources': [{k:p[k] for k in ('file','sha256','seed','priority')} for p in packs],
                                'entries_compared': sum(len(p['entries']) for p in packs), 'overlaps': overlaps, 'lookups': lookups, 'mng': mng_records})
        print(f'{game.encode("ascii", "replace").decode()}: {len(overlaps)} overlap groups, {len(mng_records)} MNG', flush=True)
    report['totals'] = {'games': len(report['games']), 'entries': sum(g['entries_compared'] for g in report['games']),
                        'mng': sum(len(g['mng']) for g in report['games']), 'chunks': dict(total_chunks),
                        'overlap_groups': sum(len(g['overlaps']) for g in report['games'])}
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n', encoding='utf8')
    print(report['totals'])


if __name__ == '__main__':
    main()
