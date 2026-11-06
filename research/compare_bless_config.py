"""Compare a named static config scene with a captured original window.
JPEG references yield diagnostics only, never an exact-rendering pass.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path
from PIL import Image, ImageChops, JpegImagePlugin
from config_scene import config_layers, run


def metrics(a, b):
    diff = ImageChops.difference(a.convert('RGB'), b.convert('RGB'))
    pixels = list(diff.get_flattened_data())
    histogram = [0]*256
    for p in pixels:
        for c in p:
            histogram[c] += 1
    count = len(pixels)*3
    cumulative = 0
    for i, n in enumerate(histogram):
        cumulative += n
        if cumulative >= count*0.99:
            p99 = i
            break
    return {'pixels': len(pixels), 'different_pixels': sum(any(p) for p in pixels),
            'max_channel_error': max(max(p) for p in pixels),
            'p99_channel_error': p99,
            'mean_absolute_channel_error': sum(i*n for i,n in enumerate(histogram))/count}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--archive', required=True, type=Path)
    ap.add_argument('--screenshot', required=True, type=Path)
    ap.add_argument('--cli', required=True, type=Path)
    ap.add_argument('--output', required=True, type=Path)
    ap.add_argument('--preview', type=Path)
    args = ap.parse_args()
    screenshot = Image.open(args.screenshot)
    if screenshot.size != (1282,747):
        raise ValueError('expected observed 1282x747 window; no automatic scaling')
    crop = (1,26,1281,746)
    roi = (100,200,1170,650)  # excludes upper-right tool overlay
    reference = screenshot.convert('RGB').crop(crop).crop(roi)
    layers, states = config_layers(args.cli, args.archive)
    old_layers, _ = config_layers(args.cli, args.archive, b'+ ')
    def compose(command, layers, background=None):
        argv = [command, args.archive]
        if background is not None:
            argv.append(background)
        return Image.open(io.BytesIO(run(args.cli,*argv,*layers)))
    old = compose('compose-mng', old_layers)
    background_only = compose('compose-mng-on', old_layers, '000000')
    corrected = compose('compose-mng-on', layers, '000000')
    assert corrected.getchannel('A').getextrema() == (255,255)
    report = {'scope': 'static config scene; no VM or animation verification',
        'cli_sha256': hashlib.sha256(args.cli.read_bytes()).hexdigest(),
        'screenshot_sha256': hashlib.sha256(args.screenshot.read_bytes()).hexdigest(),
        'reference_format': screenshot.format, 'client_crop':crop, 'comparison_roi':roi,
        'layers':layers, 'states':states, 'background':'000000',
        'before_alpha_drop_and_hover_state':metrics(old.crop(roi),reference),
        'background_fixed_only':metrics(background_only.crop(roi),reference),
        'background_and_selected_state_fixed':metrics(corrected.crop(roi),reference),
        'composed_rgba_sha256':hashlib.sha256(corrected.tobytes()).hexdigest()}
    with args.archive.open('rb') as stream:
        report['archive_sha256'] = hashlib.file_digest(stream,'sha256').hexdigest()
    if screenshot.format == 'JPEG':
        sampling = JpegImagePlugin.get_sampling(screenshot)
        # Match the measured codec settings/block origin, not fitted color or blur.
        window = Image.new('RGB',screenshot.size)
        window.paste(corrected.convert('RGB'),crop[:2])
        buffer = io.BytesIO()
        window.save(buffer,format='JPEG',qtables=screenshot.quantization,subsampling=sampling)
        encoded = Image.open(io.BytesIO(buffer.getvalue())).crop(crop).crop(roi)
        report['jpeg_subsampling'] = sampling
        report['jpeg_quantization_tables'] = screenshot.quantization
        report['matched_jpeg_diagnostic'] = metrics(encoded,reference)
        report['jpeg_self_error'] = metrics(encoded,corrected.crop(roi))
        report['exact_parity'] = 'not measurable from lossy reference'
    else:
        report['exact_parity'] = report['background_and_selected_state_fixed']['different_pixels'] == 0
    if args.preview:
        corrected.save(args.preview)
    args.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf8')
    for key in ['before_alpha_drop_and_hover_state','background_fixed_only',
                'background_and_selected_state_fixed','matched_jpeg_diagnostic','exact_parity']:
        if key in report:
            print(key,report[key])


if __name__ == '__main__':
    main()
