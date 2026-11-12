from pathlib import Path
import json,re,subprocess,wave
from PIL import ImageDraw,Image
import imageio_ffmpeg
from render_intro import OUT,SCENES,font

def main():
    scenes=json.loads((OUT/'storyboard.json').read_text(encoding='utf-8'))
    total=sum(s['duration'] for s in scenes)
    d=ImageDraw.Draw(Image.new('RGB',(1920,1080)))
    widths=[d.textbbox((0,0),c,font=font(32))[2] for s in scenes for c in s['captions']]
    assert max(widths)<=1760, f'Subtitle exceeds safe width: {max(widths)}'
    with wave.open(str(OUT/'nekopack-intro-zh.wav'),'rb') as w:
        audio=w.getnframes()/w.getframerate()
    assert abs(audio-total)<.002,(audio,total)
    p=subprocess.run([imageio_ffmpeg.get_ffmpeg_exe(),'-hide_banner','-i',str(OUT/'nekopack-intro-zh.mp4'),'-f','null','-'],capture_output=True,text=True,encoding='utf-8',errors='replace')
    (OUT/'qa-decode.log').write_text(p.stderr,encoding='utf-8')
    assert p.returncode==0,p.stderr
    for fragment in ['Video: h264','yuv420p','1920x1080','30 fps','Audio: aac']:
        assert fragment in p.stderr,fragment
    match=re.search(r'Duration: (\d+):(\d+):(\d+\.\d+)',p.stderr)
    media=int(match[1])*3600+int(match[2])*60+float(match[3])
    assert abs(media-audio)<.06,(media,audio)
    summary={'duration_seconds':total,'wav_duration_seconds':audio,'mp4_duration_seconds':media,'width':1920,'height':1080,'fps':30,'video_codec':'H.264','pixel_format':'yuv420p','audio_codec':'AAC','max_subtitle_width_px':max(widths),'subtitle_safe_width_px':1760,'decode':'passed','scene_count':len(scenes)}
    (OUT/'qa.json').write_text(json.dumps(summary,ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps(summary,ensure_ascii=False,indent=2))
if __name__=='__main__':main()
