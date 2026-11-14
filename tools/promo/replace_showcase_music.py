"""Replace showcase audio with the official CC BY 4.0 Carefree recording."""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess
import imageio_ffmpeg

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / '.local/promo-v2'
FFMPEG = imageio_ffmpeg.get_ffmpeg_exe()
CREDIT = ('Carefree by Kevin MacLeod (incompetech.com)\n'
          'https://www.incompetech.com/music/royalty-free/index.html?isrc=USUAN1400037\n'
          'Licensed under CC BY 4.0: https://creativecommons.org/licenses/by/4.0/\n'
          'Changes: excerpted, loudness adjusted, fade-in and fade-out applied.')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,default=OUT)
    parser.add_argument('--music',type=Path,default=OUT/'music/Carefree.mp3')
    args = parser.parse_args()
    out = args.out.resolve()
    source = out / 'nekopack-showcase-zh.mp4'
    music = args.music.resolve()
    target = out / 'nekopack-showcase-carefree.mp4'
    duration = json.loads((out / 'source-manifest.json').read_text(encoding='utf-8'))['media']['duration']
    filters = (f'atrim=duration={duration},asetpts=PTS-STARTPTS,'
               'loudnorm=I=-19:TP=-2:LRA=9,'
               f'afade=t=in:d=0.6,afade=t=out:st={duration-3.5}:d=3.5')
    subprocess.run([FFMPEG, '-hide_banner', '-loglevel', 'error', '-y',
                    '-i', str(source), '-i', str(music), '-map', '0:v:0', '-map', '1:a:0',
                    '-c:v', 'copy', '-af', filters, '-c:a', 'aac', '-b:a', '192k',
                    '-ar', '48000', '-t', str(duration), '-metadata', 'comment='+CREDIT,
                    '-movflags', '+faststart', str(target)], check=True)
    subprocess.run([FFMPEG, '-v', 'error', '-xerror', '-i', str(target), '-f', 'null', '-'], check=True)
    def video_hash(path):
        return subprocess.check_output([FFMPEG, '-v', 'error', '-i', str(path),
                                        '-map', '0:v:0', '-c:v', 'copy', '-f', 'hash', '-']).decode().strip()
    original_hash, final_hash = video_hash(source), video_hash(target)
    assert original_hash == final_hash, 'Video stream changed'
    (out / 'music').mkdir(exist_ok=True)
    (out / 'music/CREDITS.txt').write_text(CREDIT+'\n', encoding='utf-8')
    (out / 'carefree-delivery.json').write_text(json.dumps({
        'output': str(target), 'duration_seconds': duration,
        'sha256': hashlib.sha256(target.read_bytes()).hexdigest(),
        'music_sha256': hashlib.sha256(music.read_bytes()).hexdigest(),
        'music_source': str(music),
        'credit': CREDIT, 'audio_filter': filters,
        'full_decode': 'passed', 'video_stream_unchanged': True,
        'video_stream_hash': final_hash,
    }, indent=2)+'\n', encoding='utf-8')
    print(target)


if __name__ == '__main__':
    main()
