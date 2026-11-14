"""Validate the final music edition without inspecting game images."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import wave

from render_showcase import FFMPEG, W, H, FPS, audio_metrics, font, info


def signal_check(path):
    result = subprocess.run([FFMPEG,'-hide_banner','-i',str(path),'-an','-vf',
                             'blackdetect=d=0.2:pix_th=0.1,signalstats,metadata=print:key=lavfi.signalstats.YAVG',
                             '-f','null','-'],capture_output=True,text=True,encoding='utf-8',errors='replace',check=True)
    values = [float(x) for x in re.findall(r'lavfi.signalstats.YAVG=([\d.]+)',result.stderr)]
    assert values and max(values)>17, f'No non-black signal in {path}'
    black = re.findall(r'black_start:([\d.]+) black_end:([\d.]+) black_duration:([\d.]+)',result.stderr)
    return {'file':str(path),'frames_analyzed':len(values),'yavg_min':min(values),'yavg_max':max(values),
            'yavg_mean':sum(values)/len(values),'black_intervals':[{'start':float(a),'end':float(b),'duration':float(c)} for a,b,c in black],
            'nonblack_signal':'passed','scope':'numeric signal only; does not identify title content'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    video = out / 'nekopack-showcase-carefree.mp4'
    manifest = json.loads((out/'source-manifest.json').read_text(encoding='utf-8'))
    config = json.loads((out/'scenes.json').read_text(encoding='utf-8'))
    subprocess.run([FFMPEG,'-v','error','-xerror','-i',str(video),'-f','null','-'],check=True)
    decoded = out/'render/final-audio-qa.wav'
    subprocess.run([FFMPEG,'-v','error','-y','-i',str(video),'-vn','-c:a','pcm_s16le',str(decoded)],check=True)
    media = info(video)
    assert (media['width'],media['height'],media['fps']) == (W,H,FPS), media
    assert abs(media['duration']-manifest['expected_duration']) < .15, media
    audio = audio_metrics(decoded)
    with wave.open(str(decoded),'rb') as stream:
        audio.update(channels=stream.getnchannels(), sample_rate=stream.getframerate(), duration=stream.getnframes()/stream.getframerate())
    assert audio['clipped_samples']==0, audio
    assert audio['channels']==2 and audio['sample_rate']==48000, audio
    assert abs(audio['duration']-media['duration']) < .15, audio
    outro = []
    for text,size in [(config.get('outro_subtitle','让下一段故事，从这里继续'),32),(config.get('verification_note','兼容性持续完善'),24),(config['legal_note'],22)]:
        width = font(size).getlength(text)
        assert width <= W-120, (text,width)
        outro.append({'text':text,'width_px':width,'horizontal_margin_px':(W-width)/2})
    def digest(path):
        with path.open('rb') as stream:
            return hashlib.file_digest(stream,'sha256').hexdigest()
    report = {'output':str(video),'sha256':digest(video),'media':media,'expected_duration':manifest['expected_duration'],
              'full_decode':'passed','audio':audio,'scene_title_layout':manifest['validation']['title_layout'],
              'outro_text_layout':outro,'source_count':len(manifest['sources']),
              'source_manifest_sha256':digest(out/'source-manifest.json'),'config_sha256':digest(out/'scenes.json'),
              'visual_inspection':'not performed; no contact sheet or game image opened',
              'limitations':'Technical checks do not certify frame content, dynamic operation, or visual readability.'}
    report['signal_checks'] = [signal_check(video)] + [signal_check(Path(source['file'])) for source in manifest['sources'] if source['id'].startswith('sorakoi-')]
    (out/'qa-report.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
    print(json.dumps(report,ensure_ascii=False))


if __name__ == '__main__':
    main()
