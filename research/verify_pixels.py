"""Compare Rust RGBA with Pillow for every MNG-embedded PNG in four games.
Binary pixels are streamed through pipes, never saved or committed.
"""
import argparse
from collections import Counter
import hashlib
import importlib
import io
import json
from pathlib import Path
import struct
import subprocess
import sys
import PIL
from PIL import Image


def require(ok, why):
    if not ok:
        raise ValueError(why)


def exact(stream, n):
    result = bytearray()
    while len(result) < n:
        part = stream.read(n-len(result))
        require(part, 'truncated Rust raster stream')
        result.extend(part)
    return bytes(result)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--games', required=True, type=Path)
    ap.add_argument('--decoder-dir', required=True, type=Path)
    ap.add_argument('--cli', required=True, type=Path)
    ap.add_argument('--output', required=True, type=Path)
    args = ap.parse_args()
    sys.path.insert(0, str(args.decoder_dir))
    ref = importlib.import_module('inspect_neko')
    previous = json.loads(Path(__file__).with_name('metadata-verification.json').read_text('utf8'))
    report = {'scope': 'embedded image RGBA samples, no MNG canvas composition/playback',
              'pillow_version': PIL.__version__, 'cli_sha256': hashlib.sha256(args.cli.read_bytes()).hexdigest(),
              'reference_sha256': hashlib.sha256((args.decoder_dir/'inspect_neko.py').read_bytes()).hexdigest(), 'archives': []}
    alpha_counts = Counter()
    ihdr_counts, fram_modes, defi_lengths, term_values = Counter(), Counter(), Counter(), Counter()
    for game in previous['games']:
        for source_id, source in enumerate(game['sources']):
            resources = [m for m in game['mng'] if m['source'] == source_id]
            if not resources:
                continue
            path = args.games/source['file']
            row = {'file': source['file'], 'sha256': source['sha256'], 'images': [], 'png_exports_checked': 0}
            with path.open('rb') as stream:
                require(hashlib.file_digest(stream, 'sha256').hexdigest() == source['sha256'], 'source changed')
                stream.seek(8); seed, priority, key, size = struct.unpack('<4I', stream.read(16))
                index = ref.decrypt(stream.read(size), key)
                pos, offset, entries = 0, 24+size, []
                while pos < len(index):
                    dh, count = struct.unpack_from('<2I', index, pos); pos += 8
                    for _ in range(count):
                        nh, length = struct.unpack_from('<2I', index, pos); pos += 8
                        entries.append((offset, length)); offset += length+8
                proc = subprocess.Popen([str(args.cli.resolve()), 'decode-mng-archive', str(path)], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                try:
                    require(exact(proc.stdout, 8) == b'NRGBA001', 'wrong raster protocol')
                    for resource in resources:
                        entry = resource['ordinal']; offset, length = entries[entry]
                        stream.seek(offset); bk, bl = struct.unpack('<2I', stream.read(8)); require(bl == length, 'block length')
                        data = ref.decrypt(stream.read(length), bk)
                        require(hashlib.sha256(data).hexdigest() == resource['sha256'], 'MNG changed')
                        at, sequence, image_id, fram, defi = 8, 0, 0, 0xffffffff, 0xffffffff
                        encoded = None
                        while at < len(data):
                            n = struct.unpack_from('>I', data, at)[0]; kind = data[at+4:at+8]; body = data[at+8:at+8+n]
                            if kind == b'FRAM':
                                fram = sequence; fram_modes[str(body[0] if body else 0)] += 1
                            if kind == b'DEFI':
                                defi = sequence; defi_lengths[str(n)] += 1
                            if kind == b'TERM': term_values[body.hex()] += 1
                            if kind == b'IHDR':
                                ihdr_counts[body[8:].hex()] += 1; encoded = bytearray(b'\x89PNG\r\n\x1a\n')
                            if encoded is not None:
                                encoded.extend(data[at:at+n+12])
                            if kind == b'IEND':
                                reference = Image.open(io.BytesIO(encoded)).convert('RGBA')
                                pixels = reference.tobytes(); width, height = reference.size
                                header = struct.unpack('<7I', exact(proc.stdout, 28))
                                require(header == (entry, image_id, width, height, fram, defi, len(pixels)), f'raster header mismatch {entry}:{image_id}')
                                actual = exact(proc.stdout, len(pixels))
                                require(actual == pixels, f'pixel mismatch {entry}:{image_id}')
                                alpha_counts.update(reference.getchannel('A').tobytes())
                                row['images'].append({'entry': entry, 'image': image_id, 'width': width, 'height': height,
                                    'fram_chunk': None if fram == 0xffffffff else fram, 'defi_chunk': None if defi == 0xffffffff else defi,
                                    'rgba_sha256': hashlib.sha256(actual).hexdigest(), 'rgba_bytes': len(actual)})
                                # One exported PNG per resource: verify independent reader + original pixels.
                                if image_id == 0:
                                    export = subprocess.run([str(args.cli.resolve()), 'export-mng-image', str(path), str(entry), '0'], capture_output=True, check=True).stdout
                                    decoded_export = Image.open(io.BytesIO(export)).convert('RGBA')
                                    require(decoded_export.size == reference.size and decoded_export.tobytes() == pixels, 'export pixels mismatch')
                                    row['png_exports_checked'] += 1
                                image_id += 1; encoded = None
                            sequence += 1; at += n+12
                    require(proc.stdout.read(1) == b'', 'unexpected raster records')
                    stderr = proc.stderr.read().decode('utf8', errors='replace')
                    require(proc.wait() == 0, stderr)
                finally:
                    if proc.poll() is None:
                        proc.kill(); proc.wait()
                    proc.stdout.close(); proc.stderr.close()
            report['archives'].append(row)
            print(f'{path.name}: {len(row["images"])} RGBA images and {row["png_exports_checked"]} PNG exports matched', flush=True)
    report['totals'] = {'archives': len(report['archives']), 'images': sum(len(a['images']) for a in report['archives']),
                        'rgba_bytes': sum(i['rgba_bytes'] for a in report['archives'] for i in a['images']),
                        'png_exports': sum(a['png_exports_checked'] for a in report['archives'])}
    report['alpha_counts'] = dict(sorted(alpha_counts.items()))
    report['control_inventory'] = {'ihdr_depth_color_compression_filter_interlace': dict(ihdr_counts), 'fram_modes': dict(fram_modes),
                                  'defi_lengths': dict(defi_lengths), 'term_bytes': dict(term_values)}
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n', encoding='utf8')
    print(report['totals'])


if __name__ == '__main__':
    main()
