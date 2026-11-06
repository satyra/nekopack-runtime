"""Four-game named config composition regression against Pillow; not four UI captures."""
import argparse
import hashlib
import io
import json
from pathlib import Path
from PIL import Image, ImageChops
from config_scene import config_layers, run, sprite_list


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--games', required=True, type=Path)
    ap.add_argument('--cli', required=True, type=Path)
    ap.add_argument('--output', required=True, type=Path)
    args = ap.parse_args()
    previous = json.loads(Path(__file__).with_name('metadata-verification.json').read_text('utf8'))
    report = {'scope':'four static named config recipes against Pillow; not original window parity',
              'cli_sha256':hashlib.sha256(args.cli.read_bytes()).hexdigest(), 'games':[]}
    for game in previous['games']:
        source = next(s for s in game['sources'] if s['file'].endswith('/system.dat'))
        path = args.games/source['file']
        with path.open('rb') as stream:
            assert hashlib.file_digest(stream,'sha256').hexdigest() == source['sha256']
        layers, states = config_layers(args.cli,path)
        assert len(states) == 17 and sum(s['selected'] for s in states) == 8
        assert all(bytes.fromhex(s['name_hex']).startswith(b'@ ') for s in states if s['selected'])
        banks = {e:sprite_list(args.cli,path,e) for e in {layer.split(':')[0] for layer in layers}}
        reference = Image.new('RGBA',(1280,720),(0,0,0,255))
        for layer in layers:
            entry, ordinal = layer.split(':')
            image = Image.open(io.BytesIO(run(args.cli,'export-mng-image',path,entry,ordinal)))
            _,x,y,_ = banks[entry][int(ordinal)]
            positioned = Image.new('RGBA',reference.size)
            positioned.paste(image,(x,y))
            reference = Image.alpha_composite(reference,positioned)
        actual = Image.open(io.BytesIO(run(args.cli,'compose-mng-on',path,'000000',*layers)))
        assert actual.size == reference.size and actual.getchannel('A').getextrema() == (255,255)
        error = max(high for low,high in ImageChops.difference(actual,reference).getextrema())
        assert error <= 1
        report['games'].append({'game':game['game'],'file':source['file'],'sha256':source['sha256'],
            'layers':layers,'states':states,'max_channel_error_vs_pillow':error,
            'rgba_sha256':hashlib.sha256(actual.tobytes()).hexdigest()})
        print(game['game'], 'config scene passed', flush=True)
    args.output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8')


if __name__ == '__main__':
    main()
