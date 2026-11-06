"""Reproduce one observed original BLESS config page, without executing game scripts.
The screenshot must be the 1282x747 window capture at the observed settings.
Only hashes and metrics are persisted, never screenshot or commercial sprites.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
from PIL import Image, ImageChops


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--archive', required=True, type=Path)
    ap.add_argument('--screenshot', required=True, type=Path)
    ap.add_argument('--cli', required=True, type=Path)
    ap.add_argument('--output', required=True, type=Path)
    args = ap.parse_args()
    # Resource ordinals and selected options from this sample's scripts and observed UI.
    layers = ['24:0', '19:0', '24:2', '24:4', '24:6']
    selected = {1:34, 4:31, 7:28, 9:26, 10:25, 12:23, 14:21, 16:19}
    layers += [f'19:{selected.get(i, i)}' for i in range(1, 18)]
    png = subprocess.check_output([str(args.cli.resolve()), 'compose-mng', str(args.archive), *layers])
    actual = Image.open(io.BytesIO(png)).convert('RGB')
    screenshot = Image.open(args.screenshot).convert('RGB')
    assert screenshot.size == (1282, 747) and actual.size == (1280, 720)
    # Client offset inferred from the captured window border; no resampling/color fitting.
    original = screenshot.crop((1, 26, 1281, 746))
    # Main controls only: excludes the tool activity overlay in the upper-right corner.
    roi = (100, 200, 1170, 650)
    diff = ImageChops.difference(actual, original).crop(roi)
    values = list(diff.get_flattened_data())
    with args.archive.open('rb') as stream:
        source_hash = hashlib.file_digest(stream, 'sha256').hexdigest()
    report = {'scope': 'single observed static config page; comparison fails pixel equivalence',
              'archive_sha256': source_hash,
              'screenshot_sha256': hashlib.sha256(args.screenshot.read_bytes()).hexdigest(),
              'cli_sha256': hashlib.sha256(args.cli.read_bytes()).hexdigest(),
              'layers': layers, 'client_crop': [1,26,1281,746], 'comparison_roi': roi,
              'compared_pixels': len(values), 'different_pixels': sum(any(p) for p in values),
              'max_channel_error': max(max(p) for p in values),
              'mean_absolute_channel_error': sum(sum(p) for p in values)/(3*len(values)),
              'composed_rgb_sha256': hashlib.sha256(actual.tobytes()).hexdigest(),
              'status': 'unresolved color/edge differences; not original-rendering parity'}
    args.output.write_text(json.dumps(report, indent=2)+'\n', encoding='utf8')
    print(json.dumps(report, indent=2))


if __name__ == '__main__':
    main()
