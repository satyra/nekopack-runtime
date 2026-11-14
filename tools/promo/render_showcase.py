"""Render a source-traceable Android product showcase from real recordings."""
from pathlib import Path
import argparse
import hashlib
import json
import re
import subprocess
import wave

import imageio_ffmpeg
import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageFont

ROOT = Path(__file__).resolve().parents[2]
W, H, FPS = 1920, 1080, 30
BG, INK, MUTED = '#F4F1EB', '#302638', '#877E87'
FFMPEG = imageio_ffmpeg.get_ffmpeg_exe()


def run(args):
    subprocess.run([FFMPEG, '-hide_banner', '-loglevel', 'warning', '-nostats', '-y', *map(str, args)], check=True)


def font(size, bold=False):
    return ImageFont.truetype('C:/Windows/Fonts/msyh' + ('bd' if bold else '') + '.ttc', size)


def label(draw, xy, content, size, color=INK, bold=False):
    draw.text(xy, content, font=font(size, bold), fill=color)


def centered(draw, y, content, size, color=INK, bold=False):
    width = draw.textlength(content, font=font(size, bold))
    label(draw, ((W-width)/2, y), content, size, color, bold)


def mark(image, x, y, size):
    # Recreates the project's Android vector mark as a raster for the film only.
    draw = ImageDraw.Draw(image)
    k = size/108
    pts = lambda values: [(x+a*k, y+b*k) for a,b in values]
    draw.rounded_rectangle((x+4*k,y+4*k,x+104*k,y+104*k), radius=23*k, fill='#39254F')
    draw.ellipse((x+28*k,y+32*k,x+80*k,y+82*k), fill='#D5BAFF')
    draw.polygon(pts([(28,56),(30,26),(45,36),(63,36),(78,26),(80,56)]), fill='#D5BAFF')
    draw.polygon(pts([(46,46),(46,71),(69,58)]), fill='#39254F')
    for ear in [[(33,33),(42,40),(34,44)],[(75,33),(66,40),(74,44)]]:
        draw.polygon(pts(ear), fill='#F2DDFB')


def plate(path, title, subtitle, ending=False, legal_note='请自备合法游戏资源', verification_note='兼容性持续完善'):
    image = Image.new('RGB', (W,H), BG)
    draw = ImageDraw.Draw(image)
    mark(image, 852, 250 if not ending else 225, 216)
    centered(draw, 500 if not ending else 475, title, 88, bold=True)
    centered(draw, 632 if not ending else 607, subtitle, 32, MUTED)
    if ending:
        centered(draw, 726, 'NekoPack 引擎兼容项目 · Android ARM64 预览版', 23, MUTED)
        centered(draw, 785, verification_note, 24, MUTED)
        centered(draw, 842, legal_note, 22, MUTED)
    image.save(path)


def scene_plate(path, scene, number, count, capture_label):
    image = Image.new('RGB', (W,H), BG)
    draw = ImageDraw.Draw(image)
    label(draw, (198,57), scene['title'], 43, bold=True)
    tag = f"{number:02d} / {count:02d}    {scene.get('tag','')}"
    tag_width = draw.textlength(tag,font=font(22))
    label(draw, (1720-tag_width,77), tag, 22, MUTED)
    # An Android landscape display, with no borrowed vendor hardware or OS branding.
    shadow = Image.new('RGBA', (W,H))
    sd = ImageDraw.Draw(shadow)
    sd.rounded_rectangle((188,175,1732,1024), radius=42, fill=(45,31,50,34))
    image = Image.alpha_composite(image.convert('RGBA'), shadow.filter(ImageFilter.GaussianBlur(22)))
    draw = ImageDraw.Draw(image)
    draw.rounded_rectangle((187,154,1733,1020), radius=38, fill='#16141A')
    draw.rounded_rectangle((192,159,1728,1015), radius=34, outline='#50464F', width=2)
    draw.ellipse((194,579,200,585), fill='#554856')
    label(draw, (198,1035), 'NekoPack  /  Android', 20, MUTED)
    text_width = draw.textlength(capture_label, font=font(20))
    label(draw, (1720-text_width,1035), capture_label, 20, MUTED)
    image.convert('RGB').save(path)


def info(path):
    result = subprocess.run([FFMPEG,'-hide_banner','-i',str(path)], capture_output=True, text=True, encoding='utf-8', errors='replace')
    match = re.search(r'Duration: (\d+):(\d+):([\d.]+)', result.stderr)
    if not match:
        raise ValueError(f'Cannot read video duration: {path}\n{result.stderr}')
    duration = int(match[1])*3600 + int(match[2])*60 + float(match[3])
    video = next(line for line in result.stderr.splitlines() if 'Video:' in line)
    dimensions = re.search(r'\b(\d{2,5})x(\d{2,5})\b', video)
    fps = re.search(r', ([\d.]+) fps', video)
    return {'duration':duration,'width':int(dimensions[1]),'height':int(dimensions[2]),'fps':float(fps[1]) if fps else None,'stream':video.strip()}


def ambient(path, duration):
    sr = 48000
    t = np.arange(round(duration*sr), dtype=np.float64)/sr
    audio = np.zeros((len(t),2), dtype=np.float64)
    # Original, non-melodic C / A-minor suspended textures, slowly overlapping.
    chords = [(130.8128,196.0,293.6648),(110.0,164.8138,246.9417),
              (146.8324,220.0,329.6276),(130.8128,196.0,261.6256)]
    for i, start in enumerate(np.arange(-4,duration,12)):
        u = t-start
        window = np.clip(u/4,0,1)*np.clip((19-u)/6,0,1)
        window[(u<0)|(u>19)] = 0
        for j,freq in enumerate(chords[i%len(chords)]):
            tone = np.sin(2*np.pi*freq*t + .12*np.sin(2*np.pi*.07*t))
            tone += .14*np.sin(2*np.pi*freq*2*t)
            audio[:,j%2] += tone*window*.014
            audio[:,1-j%2] += tone*window*.010
    envelope = np.minimum(np.clip(t/3,0,1),np.clip((duration-t)/4,0,1))
    pcm = np.round(np.clip(audio*envelope[:,None]*5,-1,1)*32767).astype('<i2')
    with wave.open(str(path),'wb') as output:
        output.setnchannels(2); output.setsampwidth(2); output.setframerate(sr)
        output.writeframes(pcm.tobytes())


def audio_metrics(path):
    with wave.open(str(path),'rb') as source:
        samples = np.frombuffer(source.readframes(source.getnframes()),dtype='<i2').astype(np.float64)/32768
    peak = np.max(np.abs(samples))
    rms = np.sqrt(np.mean(samples**2))
    return {'peak_dbfs':float(20*np.log10(peak)), 'rms_dbfs':float(20*np.log10(rms)),
            'clipped_samples':int(np.count_nonzero(np.abs(samples)>=32767/32768))}


def encode_static(image, dest, duration, crf):
    # Very restrained optical push during the short brand bookends.
    frames = round(duration*FPS)
    vf = f"scale=1980:1114,zoompan=z='1+0.014*on/{frames}':x='iw/2-iw/zoom/2':y='ih/2-ih/zoom/2':d=1:s={W}x{H}:fps={FPS},format=yuv420p"
    run(['-loop','1','-framerate',FPS,'-i',image,'-t',duration,'-vf',vf,
         '-an','-c:v','libx264','-preset','fast','-crf',crf,'-threads','4',dest])


def encode_scene(background, source, scene, dest, crf):
    # Native screen pixels are fitted without cropping controls or stretching UI.
    crop = scene.get('crop')
    crop_filter = f'crop={crop[2]}:{crop[3]}:{crop[0]}:{crop[1]},' if crop else ''
    graph = (f'[1:v]{crop_filter}fps={FPS},scale=1500:834:force_original_aspect_ratio=decrease:force_divisible_by=2,'
             'pad=1500:834:(ow-iw)/2:(oh-ih)/2:color=0x16141A,setsar=1[screen];'
             '[0:v][screen]overlay=210:170:shortest=1,format=yuv420p[out]')
    run(['-loop','1','-framerate',FPS,'-i',background,'-ss',scene.get('start',0),'-i',source,
         '-t',scene['duration'],'-filter_complex_threads','1','-filter_complex',graph,'-map','[out]',
         '-an','-c:v','libx264','-preset','fast','-crf',crf,'-threads','4',dest])


def extract(video, dest, at, size=None):
    args = ['-ss',at,'-i',video,'-frames:v','1']
    if size:
        args += ['-vf',f'scale={size[0]}:{size[1]}']
    run([*args,'-update','1',dest])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--scenes',type=Path,default=ROOT/'.local/promo-v2/scenes.json')
    parser.add_argument('--out',type=Path,default=ROOT/'.local/promo-v2')
    parser.add_argument('--crf',type=int,default=19)
    args = parser.parse_args()
    config = json.loads(args.scenes.read_text(encoding='utf-8-sig'))
    out = args.out.resolve(); out.mkdir(parents=True,exist_ok=True)
    work = out/'render'; work.mkdir(exist_ok=True)
    scenes = config['scenes']
    if not scenes:
        raise ValueError('scenes must include real Android recordings')
    sources = []
    layout_qa = []
    for scene in scenes:
        title_width = font(43, True).getlength(scene['title'])
        tag = f"{len(layout_qa)+1:02d} / {len(scenes):02d}    {scene.get('tag','')}"
        tag_left = 1720-font(22).getlength(tag)
        if 198+title_width+32 > tag_left:
            raise ValueError(f"Scene {scene['id']} title overlaps tag")
        layout_qa.append({'id':scene['id'], 'title_right':198+title_width, 'tag_left':tag_left, 'minimum_gap':32, 'passed':True})
        source = (args.scenes.parent/scene['file']).resolve()
        metadata = info(source)
        if scene.get('start',0)<0 or scene['duration']<=0 or scene.get('start',0)+scene['duration']>metadata['duration']+.05:
            raise ValueError(f"Scene {scene['id']} exceeds source duration {metadata['duration']}")
        if not scene.get('source'):
            raise ValueError(f"Scene {scene['id']} requires a factual source description")
        if 'crop' in scene:
            x,y,width,height = scene['crop']
            if min(x,y)<0 or min(width,height)<=0 or x+width>metadata['width'] or y+height>metadata['height']:
                raise ValueError(f"Scene {scene['id']} crop exceeds source dimensions")
        with source.open('rb') as source_file:
            digest = hashlib.file_digest(source_file,'sha256').hexdigest()
        sources.append({'id':scene['id'],'file':str(source),'sha256':digest,
                        'capture_metadata':scene.get('capture_metadata',config.get('capture_metadata',{})),
                        'source':scene['source'],'media':metadata,'start':scene.get('start',0),'duration':scene['duration']})
    capture_label = config.get('capture_label','Android 模拟器录制')
    plate(work/'intro.png',config.get('title','NekoPack'),config.get('subtitle','Android 上的视觉小说兼容运行时'))
    plate(work/'outro.png','NekoPack',config.get('outro_subtitle','让下一段故事，从这里继续'),True,config.get('legal_note','请自备合法游戏资源'),config.get('verification_note','兼容性持续完善'))
    segments = [work/'intro.mp4']; durations = [3.5]
    encode_static(work/'intro.png',segments[0],durations[0],args.crf)
    for i,(scene,source) in enumerate(zip(scenes,sources)):
        background = work/f'scene-{i:02}.png'; segment = work/f'scene-{i:02}.mp4'
        scene_plate(background,scene,i+1,len(scenes),capture_label)
        encode_scene(background,source['file'],scene,segment,args.crf)
        segments.append(segment); durations.append(scene['duration'])
    segments.append(work/'outro.mp4'); durations.append(4.5)
    encode_static(work/'outro.png',segments[-1],durations[-1],args.crf)
    transition = .5
    total = sum(durations)-transition*(len(segments)-1)
    ambient(work/'original-ambient.wav',total)
    inputs = []
    for segment in segments:
        inputs += ['-i',segment]
    inputs += ['-i',work/'original-ambient.wav']
    filters = []
    prior = '0:v'; elapsed = durations[0]
    for i in range(1,len(segments)):
        output = f'v{i}'
        filters.append(f'[{prior}][{i}:v]xfade=transition=fade:duration={transition}:offset={elapsed-transition}[{output}]')
        prior = output; elapsed += durations[i]-transition
    final = out/'nekopack-showcase-zh.mp4'
    run([*inputs,'-filter_complex_threads','1','-filter_complex',';'.join(filters),'-map',f'[{prior}]',
         '-map',f'{len(segments)}:a','-t',f'{total:.3f}','-r',FPS,'-c:v','libx264','-preset','fast','-crf',args.crf,
         '-threads','4','-pix_fmt','yuv420p','-c:a','aac','-b:a','192k','-movflags','+faststart',final])
    # Full decoding is mandatory; ffmpeg exits on corrupt input.
    run(['-v','error','-xerror','-i',final,'-f','null','-'])
    run(['-i',final,'-vn','-c:a','pcm_s16le',work/'decoded-audio-qa.wav'])
    audio_qa = audio_metrics(work/'decoded-audio-qa.wav')
    if audio_qa['clipped_samples']:
        raise ValueError(f'Final AAC audio clips: {audio_qa}')
    result = info(final)
    if (result['width'],result['height']) != (W,H) or result['fps']!=FPS or abs(result['duration']-total)>.15:
        raise ValueError(f'Output metadata mismatch: {result}, expected {total}')
    elapsed = 0
    shots = [('片头',1.5)]
    for i,scene in enumerate(scenes):
        start = sum(durations[:i+1])-transition*(i+1)
        shots.append((scene['title'],start+scene['duration']/2))
    shots.extend([('中段',total/2),('片尾',total-2)])
    tiles=[]
    for i,(title,at) in enumerate(shots):
        path=work/f'qa-{i:02}.jpg'; extract(final,path,at,(640,360))
        tile=Image.new('RGB',(640,410),BG); tile.paste(Image.open(path),(0,0))
        label(ImageDraw.Draw(tile),(16,371),f'{at:.1f}s  {title}',20)
        tiles.append(tile)
    sheet=Image.new('RGB',(1920,410*((len(tiles)+2)//3)),BG)
    for i,tile in enumerate(tiles): sheet.paste(tile,((i%3)*640,(i//3)*410))
    sheet.save(out/'contact-sheet.jpg',quality=94)
    cover_scene = next((i for i,s in enumerate(scenes) if s['id']=='menu'),0)
    extract(final,out/'cover.jpg',config.get('cover_time',shots[cover_scene+1][1]))
    manifest={'output':str(final),'media':result,'expected_duration':total,'fps':FPS,'capture_label':capture_label,
              'sources':sources,'scenes':scenes,'capture_metadata':config.get('capture_metadata',{}),
              'cover_time':config.get('cover_time',shots[cover_scene+1][1]),'legal_note':config.get('legal_note','请自备合法游戏资源'),'audio':'Original NumPy synthesized ambient; no speech or borrowed music',
              'audio_qa':audio_qa,'audio_gain_db':20*np.log10(5),
              'brand_mark':'Project Android platforms/android/res/drawable/neko_mark.xml, raster recreation',
              'validation':{'full_decode':'passed','dimensions':'1920x1080','duration':'passed',
                            'title_layout':layout_qa,'visual_inspection':'not performed'},'qa_samples':shots,
              'verification_note':config.get('verification_note','兼容性持续完善')}
    (out/'source-manifest.json').write_text(json.dumps(manifest,ensure_ascii=False,indent=2),encoding='utf-8')
    print(json.dumps({'output':str(final),'duration':result['duration'],'qa':str(out/'contact-sheet.jpg')},ensure_ascii=False))


if __name__=='__main__':
    main()
