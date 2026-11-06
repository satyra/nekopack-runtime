"""Phase 0 read-only audit. Requires the existing local inspect_neko research tool.

Writes metadata only; never exports game text, images or executable bytes.
Not a runtime parser and not an independent verification of the imported decoder.
"""
import argparse
from collections import Counter
import hashlib
import importlib
import json
import mmap
from pathlib import Path
import re
import struct
import sys
import zlib


def mng_info(data):
    pos = 8
    chunks = Counter()
    mhdr = None
    while pos < len(data):
        if pos + 12 > len(data):
            raise ValueError('truncated MNG chunk')
        size = struct.unpack_from('>I', data, pos)[0]
        end = pos + 12 + size
        if end > len(data):
            raise ValueError('MNG chunk outside payload')
        kind = data[pos+4:pos+8]
        crc = struct.unpack_from('>I', data, end-4)[0]
        if zlib.crc32(data[pos+4:end-4]) != crc:
            raise ValueError('MNG CRC mismatch')
        chunks[kind.decode('ascii')] += 1
        if kind == b'MHDR':
            mhdr = list(struct.unpack('>7I', data[pos+8:end-4]))
        pos = end
    return {'chunks': dict(chunks), 'mhdr': mhdr, 'crc': 'passed',
            'playback_verified': False}


def inspect_archive(stream, base, total, decoder):
    stream.seek(base)
    head = stream.read(24)
    if len(head) != 24 or head[:8] != b'NEKOPACK':
        raise ValueError('not NEKOPACK header')
    seed, priority, key, length = struct.unpack_from('<4I', head, 8)
    if length < 8 or length > total-base-24 or length > 16*1024*1024:
        raise ValueError('invalid index size')
    index = decoder.decrypt(stream.read(length), key)
    folders = {decoder.hashname(n, seed): n for n in decoder.dirs}
    pos, offset, count = 0, base+24+length, 0
    dirs, types, commands, prefixes = Counter(), Counter(), Counter(), Counter()
    mngs, scripts = [], []
    while pos < length:
        if pos+8 > length:
            raise ValueError('truncated directory')
        dh, n = struct.unpack_from('<II', index, pos)
        pos += 8
        if pos+n*8 > length:
            raise ValueError('directory count outside index')
        directory = folders.get(dh, f'{dh:08X}')
        dirs[directory] += n
        for _ in range(n):
            nh, size = struct.unpack_from('<II', index, pos)
            pos += 8
            if offset+8+size > total:
                raise ValueError('entry outside file')
            stream.seek(offset)
            blockkey, blocklen = struct.unpack('<II', stream.read(8))
            if blocklen != size:
                raise ValueError('index/block length mismatch')
            prefix = decoder.decrypt(stream.read(min(size, 16)), blockkey)
            kind = next((k for sig, k in [(b'\x8aMNG\r\n\x1a\n', 'MNG'),
                (b'\x89PNG\r\n\x1a\n', 'PNG'), (b'OggS', 'Ogg'),
                (b'8BPS', 'PSD'), (b'RIFF', 'RIFF')] if prefix.startswith(sig)), 'other')
            types[kind] += 1
            if directory in ('script', 'system') or (kind == 'MNG' and len(mngs) < 3):
                stream.seek(offset+8)
                data = decoder.decrypt(stream.read(size), blockkey)
                if kind == 'MNG':
                    if len(mngs) < 3:
                        mngs.append({'name_hash': f'{nh:08X}', 'sha256': hashlib.sha256(data).hexdigest(), **mng_info(data)})
                elif kind == 'other' and b'\0' not in data:
                    # ASCII syntax only: do not infer Japanese/Chinese text encoding.
                    for line in data.splitlines():
                        line = line.lstrip(b' \t')
                        if line:
                            prefixes[chr(line[0]) if line[0] in b'.@?;#:' else 'text_or_other'] += 1
                        match = re.match(rb'\.([A-Za-z_][A-Za-z_0-9]*)', line)
                        if match:
                            commands[match[1].decode('ascii')] += 1
                    scripts.append({'directory': directory, 'name_hash': f'{nh:08X}',
                        'size': size, 'sha256': hashlib.sha256(data).hexdigest()})
            count += 1
            offset += size+8
    return {'offset': base, 'seed': f'{seed:08X}', 'priority': priority,
        'index_size': length, 'entries': count, 'directories': dict(dirs),
        'payload_signatures': dict(types), 'index_sha256': hashlib.sha256(index).hexdigest(),
        'trailing_bytes': total-offset, 'all_block_lengths_checked': True,
        'mng_samples': mngs, 'text_candidates': scripts,
        'dot_command_tokens': dict(sorted(commands.items())), 'line_prefixes': dict(prefixes)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--games', type=Path, required=True)
    parser.add_argument('--decoder-dir', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    sys.path.insert(0, str(args.decoder_dir))
    decoder = importlib.import_module('inspect_neko')
    result = {'scope': 'Tiny Dungeon four local editions; static inspection only',
        'decoder_sha256': hashlib.sha256((args.decoder_dir/'inspect_neko.py').read_bytes()).hexdigest(),
        'decoder_dependency': 'Existing local inspect_neko.py plus ArcNEKO.cs and nekopack.lst; not bundled',
        'files': []}
    for game in sorted(args.games.iterdir()):
        if 'Tiny Dungeon' not in game.name:
            continue
        for path in sorted(game.rglob('*')):
            if path.suffix.lower() not in ('.dat', '.exe'):
                continue
            if path.name.lower().startswith('unins'):
                continue
            total = path.stat().st_size
            row = {'file': path.relative_to(args.games).as_posix(), 'size': total, 'archives': []}
            with path.open('rb') as stream:
                row['sha256'] = hashlib.file_digest(stream, 'sha256').hexdigest()
                stream.seek(0)
                if path.suffix.lower() == '.dat':
                    if stream.read(8) == b'NEKOPACK':
                        row['archives'].append(inspect_archive(stream, 0, total, decoder))
                    else:
                        row['status'] = 'not NEKOPACK at offset zero'
                elif total:
                    with mmap.mmap(stream.fileno(), 0, access=mmap.ACCESS_READ) as mapped:
                        offset = mapped.find(b'NEKOPACK')
                        rejected = 0
                        while offset != -1:
                            try:
                                row['archives'].append(inspect_archive(stream, offset, total, decoder))
                            except ValueError:
                                # EXEs contain literal format names, not just embedded archives.
                                rejected += 1
                            offset = mapped.find(b'NEKOPACK', offset+8)
                        row['rejected_signature_candidates'] = rejected
            result['files'].append(row)
            print(path.name, sum(a['entries'] for a in row['archives']), flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2)+'\n', encoding='utf-8')


if __name__ == '__main__':
    main()
