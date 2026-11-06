"""Check sprite state for all four games; compare selected compositions with Pillow.
No commercial pixels are persisted. Source hashes pin the prior archive validation.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import struct
import subprocess
from PIL import Image, ImageChops


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--games', required=True, type=Path)
    ap.add_argument('--cli', required=True, type=Path)
    ap.add_argument('--output', required=True, type=Path)
    args = ap.parse_args()
    previous = json.loads(Path(__file__).with_name('metadata-verification.json').read_text('utf8'))
    report = {'scope': 'sprite positions/names and explicit first+last layer composition; not playback',
              'cli_sha256': hashlib.sha256(args.cli.read_bytes()).hexdigest(), 'games': []}

    def cli(*argv):
        return subprocess.run([str(args.cli.resolve()), *map(str, argv)], check=True, capture_output=True).stdout

    for game in previous['games']:
        row = {'game': game['game'], 'resources': []}
        for source_id, source in enumerate(game['sources']):
            resources = [r for r in game['mng'] if r['source'] == source_id]
            if not resources:
                continue
            path = args.games / source['file']
            with path.open('rb') as stream:
                assert hashlib.file_digest(stream, 'sha256').hexdigest() == source['sha256']
            for resource in resources:
                entry = resource['ordinal']
                data = cli('read-entry', path, entry)
                assert hashlib.sha256(data).hexdigest() == resource['sha256']
                at, position, name, images = 8, (0, 0), b'', []
                encoded = None
                while at < len(data):
                    n = struct.unpack_from('>I', data, at)[0]
                    kind, body = data[at+4:at+8], data[at+8:at+8+n]
                    if kind == b'MHDR':
                        dimensions = struct.unpack_from('>2I', body)
                    if kind == b'DEFI':
                        assert len(body) == 12 and body[:4] == bytes(4)
                        position = struct.unpack_from('>2i', body, 4)
                    if kind == b'FRAM':
                        assert body[0] == 2
                        name = body[1:].split(b'\0', 1)[0]
                    if kind == b'IHDR':
                        encoded = bytearray(b'\x89PNG\r\n\x1a\n')
                    if encoded is not None:
                        encoded.extend(data[at:at+n+12])
                    if kind == b'IEND':
                        images.append((position, name, bytes(encoded)))
                        encoded = None
                    at += n+12
                actual = cli('inspect-sprites', path, entry).decode('ascii').splitlines()
                expected = [f'{i}\t{p[0]}\t{p[1]}\t{name.hex().upper()}'
                            for i, (p, name, _) in enumerate(images)]
                assert actual == expected, (entry, 'sprite metadata')
                selected = [0, len(images)-1]
                checks = []
                for background in [None, '000000', '193B79']:
                    color = (0,0,0,0) if background is None else (*bytes.fromhex(background),255)
                    reference = Image.new('RGBA', dimensions, color)
                    for i in selected:
                        position, _, png = images[i]
                        layer = Image.new('RGBA', dimensions)
                        layer.paste(Image.open(io.BytesIO(png)).convert('RGBA'), position)
                        reference = Image.alpha_composite(reference, layer)
                    command = ['compose-mng', path] if background is None else ['compose-mng-on', path, background]
                    actual_png = cli(*command, *(f'{entry}:{i}' for i in selected))
                    actual_image = Image.open(io.BytesIO(actual_png)).convert('RGBA')
                    assert actual_image.size == dimensions
                    extrema = ImageChops.difference(actual_image, reference).getextrema()
                    max_error = max(v[1] for v in extrema)
                    # Pillow uses fixed-point intermediate rounding; Rust rounds once.
                    assert max_error <= 1, (entry, background, max_error)
                    if background is not None:
                        assert actual_image.getchannel('A').getextrema() == (255,255)
                    checks.append({'background': background, 'max_channel_error_vs_pillow': max_error,
                                   'rgba_sha256': hashlib.sha256(actual_image.tobytes()).hexdigest()})
                row['resources'].append({'file': source['file'], 'entry': entry,
                    'sprites': len(images), 'layers': selected, 'canvas': dimensions, 'checks': checks})
        report['games'].append(row)
        print(game['game'], len(row['resources']), 'MNG checked', flush=True)
    args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2)+'\n', encoding='utf8')


if __name__ == '__main__':
    main()
