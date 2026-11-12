from pathlib import Path
import argparse, json, math, subprocess, wave
from PIL import Image, ImageDraw, ImageFont
import imageio_ffmpeg

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / '.local' / 'promo'
W,H,FPS = 1920,1080,30
BG='#111525'; WHITE='#F4F1EA'; MUTED='#A8ADC4'; PURPLE='#CEB6FF'; MINT='#9DDFC6'; PANEL='#1D2339'; LINE='#323A54'
FONTS={}
def font(n,bold=False):
    key=(n,bold)
    if key not in FONTS: FONTS[key]=ImageFont.truetype('C:/Windows/Fonts/msyh'+('bd' if bold else '')+'.ttc',n)
    return FONTS[key]
def text(d,xy,s,n=32,color=WHITE,bold=False): d.text(xy,s,font=font(n,bold),fill=color)
def box(d,r,fill=PANEL,radius=24,outline=None,width=2): d.rounded_rectangle(r,radius=radius,fill=fill,outline=outline,width=width)
def logo(d,x,y,size):
    k=size/108
    def pts(a): return [(x+u*k,y+v*k) for u,v in a]
    box(d,(x,y,x+size,y+size),'#39254F',int(size*.23))
    d.ellipse((x+28*k,y+34*k,x+80*k,y+84*k),fill=PURPLE)
    d.polygon(pts([(29,57),(30,27),(45,36),(63,36),(77,27),(80,57)]),fill=PURPLE)
    d.polygon(pts([(48,46),(48,71),(69,58)]),fill='#39254F')
    d.polygon(pts([(33,33),(42,40),(34,44)]),fill='#F2DDFB')
    d.polygon(pts([(75,33),(66,40),(74,44)]),fill='#F2DDFB')
SCENES=[
 dict(title='NekoPack',sub='在 Android 上，继续你的视觉小说旅程',tag='01 / 认识 NekoPack',voice='这是 NekoPack，一个面向 Android 的视觉小说引擎项目。把游戏库、阅读体验和常用操作，放进同一个入口。',captions=['这是 NekoPack，一个面向 Android 的视觉小说引擎项目。','把游戏库、阅读体验和常用操作，放进同一个入口。']),
 dict(title='从游戏库开始',sub='搜索 · 导入 · 单游戏详情',tag='02 / 整理与进入',voice='从游戏库搜索和导入，打开单游戏详情，再进入游戏。你需要自行准备合法的游戏资源。项目不附带游戏资源或补丁。',captions=['搜索、导入，再从单游戏详情进入游戏。','请自备合法游戏资源；项目不附带资源或补丁。']),
 dict(title='常用操作，就在手边',sub='阅读界面 · 右侧快捷菜单 · 原版设置',tag='03 / 阅读与操作',voice='阅读时，从右侧快捷菜单呼出常用操作，也能进入原版设置。这里展示的是功能示意，不是实际游戏录屏。',captions=['从右侧快捷菜单呼出常用操作，也能进入原版设置。','画面为功能示意，非实际游戏录屏。']),
 dict(title='保存进度，下次继续',sub='存档 · 读档 · 重启后的状态恢复验证',tag='04 / 进度管理',voice='随时保存进度，再从存档继续。项目已进行五款样本的汉化姓名、存档、重启和样式回归验证。',captions=['保存进度，再从存档继续。','五款样本已做汉化姓名、存档、重启与样式回归验证。']),
 dict(title='让阅读更顺手',sub='音频控制 · 汉化支持',tag='05 / 阅读体验',voice='音频控制和汉化支持，让阅读更顺手。具体表现仍取决于游戏、资源与当前实现，不代表所有作品都能完整运行。',captions=['音频控制和汉化支持，让阅读更顺手。','具体表现依赖游戏与资源，不代表所有作品完整兼容。']),
 dict(title='验证，以事实为准',sub='样本覆盖与已知进展',tag='06 / 当前进展',voice='样本包括 BLACK、BLESS、BIRTH、BRAVE 和 Endless Dungeon。其中 BLESS 一条主线已通关，并不代表五款都已全通关。',captions=['样本：BLACK、BLESS、BIRTH、BRAVE、Endless Dungeon。','BLESS 一条主线已通关；不等于五款全部通关。']),
 dict(title='同一个核心，走向 Android',sub='共享 Rust 核心 + Kotlin / JNI',tag='07 / 项目基础',voice='共享 Rust 核心，通过 Kotlin 和 JNI 接入 Android。已有一百二十九项 runtime 测试，以及 BLACK 在 Android 十五真机上的验证。',captions=['共享 Rust 核心，通过 Kotlin / JNI 接入 Android。','129 项 runtime 测试；BLACK 已在 Android 15 真机验证。']),
 dict(title='NekoPack',sub='让下一段故事，有一个清晰的入口',tag='08 / 预览版',voice='目前处于 Android ARM 六十四位预览阶段，兼容性持续完善。带上你合法拥有的游戏资源，从一次真实的体验开始。',captions=['当前处于 Android ARM64 预览阶段，兼容性持续完善。','带上合法拥有的游戏资源，从一次真实的体验开始。'])]

def make_audio():
    ps=OUT/'speak.ps1'
    ps.write_text('''Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
$s.SelectVoice('Microsoft Huihui Desktop')
$s.Rate = 4
$scenes = Get-Content -Raw -Encoding UTF8 $args[0] | ConvertFrom-Json
$i=0
foreach ($scene in $scenes) {
 $s.SetOutputToWaveFile((Join-Path $args[1] ("voice-{0:D2}.wav" -f $i)))
 $s.Speak($scene.voice)
 $i++
}
$s.Dispose()
''',encoding='utf-8-sig')
    subprocess.run(['powershell','-NoProfile','-ExecutionPolicy','Bypass','-File',str(ps),str(OUT/'storyboard.json'),str(OUT)],check=True)
    chunks=[]; sr=None
    for i,s in enumerate(SCENES):
        with wave.open(str(OUT/f'voice-{i:02}.wav'),'rb') as f:
            fmt=(f.getnchannels(),f.getsampwidth(),f.getframerate()); data=f.readframes(f.getnframes())
        if sr is None: sr=fmt
        assert sr==fmt
        actual=len(data)/(fmt[0]*fmt[1]*fmt[2])
        s['duration']=max(8.0,math.ceil((actual+1.4)*FPS)/FPS)
        s['speech_duration']=actual
        silence=lambda sec: bytes(round(sec*fmt[2])*fmt[0]*fmt[1])
        chunks.append(silence(.5)+data+silence(s['duration']-actual-.5))
    with wave.open(str(OUT/'nekopack-intro-zh.wav'),'wb') as f:
        f.setnchannels(sr[0]); f.setsampwidth(sr[1]); f.setframerate(sr[2]);f.writeframes(b''.join(chunks))

def library(d,x,y):
    box(d,(x,y,x+1510,y+540))
    text(d,(x+35,y+25),'游戏库',36,bold=True)
    box(d,(x+1020,y+20,x+1465,y+80),'#13192B',18)
    text(d,(x+1045,y+34),'搜索游戏…',27,MUTED)
    names=['BLACK','BLESS','BIRTH','BRAVE','Endless Dungeon']
    for i,name in enumerate(names):
        cx=x+35+i*292
        box(d,(cx,y+112,cx+266,y+465),'#151B2E',18,LINE)
        col=[PURPLE,MINT,'#E7BC98','#A6C8EE','#E4B6C9'][i]
        box(d,(cx+16,y+130,cx+250,y+335),'#282C45',14)
        d.ellipse((cx+75,y+173,cx+191,y+289),outline=col,width=4)
        d.line((cx+95,y+268,cx+172,y+192),fill=col,width=4)
        text(d,(cx+20,y+360),name,22 if i==4 else 29,col,True)
        text(d,(cx+20,y+410),'查看详情  ›',24,MUTED)
    text(d,(x+37,y+493),'+ 导入游戏',27,MINT)

def render(idx,t,subtitle=True):
    s=SCENES[idx]; im=Image.new('RGB',(W,H),BG); d=ImageDraw.Draw(im)
    # Background accents stay deliberately quiet.
    d.ellipse((1390,-430,2250,430),fill='#191D32')
    d.ellipse((-420,600,290,1310),fill='#151F2D')
    text(d,(100,49),'NEKOPACK  /  PROJECT PREVIEW',23,MUTED)
    text(d,(1410,49),'功能示意 · 非游戏录屏',23,MUTED)
    text(d,(100,112),s['tag'],24,MINT)
    text(d,(100,165),s['title'],76,WHITE,True)
    text(d,(104,265),s['sub'],32,MUTED)
    q=min(1,t/0.85); ease=1-(1-q)**3; y=350+int((1-ease)*65)
    if idx==0:
        logo(d,160,y+40,285)
        text(d,(535,y+40),'游戏库 → 阅读 → 继续',56,WHITE,True)
        for i,label in enumerate(['Android','共享 Rust 核心','合法自备资源']):
            xx=540+i*365; box(d,(xx,y+150,xx+330,y+225),'#252B43',18)
            text(d,(xx+24,y+168),label,30,[PURPLE,MINT,WHITE][i])
        text(d,(540,y+275),'清晰的入口，持续验证的体验',35,MUTED)
    elif idx==1:
        library(d,205,y)
    elif idx==2:
        box(d,(205,y,1715,y+530),'#1D2940')
        # Abstract scene made solely from geometry, without game artwork/text.
        d.polygon([(205,y+365),(600,y+120),(1040,y+365)],fill='#2A3B54')
        d.polygon([(650,y+365),(1120,y+80),(1550,y+365)],fill='#34465B')
        d.ellipse((1120,y+58,1210,y+148),fill='#B9C9CD')
        box(d,(245,y+350,1405,y+490),'#12182A',18)
        text(d,(280,y+375),'阅读区域',30,PURPLE,True)
        text(d,(280,y+427),'这里呈现游戏内容；本片仅展示界面结构。',28,MUTED)
        box(d,(1435,y+30,1685,y+495),'#12182A',18,LINE)
        for j,l in enumerate(['快捷菜单','存档','读档','原版设置','音频']):
            text(d,(1470,y+58+j*83),l,29,MINT if j==0 else WHITE,j==0)
    elif idx==3:
        for j in range(3):
            xx=210+j*510; box(d,(xx,y+15,xx+475,y+475),'#1D2339',24, MINT if j==1 else LINE)
            box(d,(xx+26,y+42,xx+449,y+255),'#2A344B',18)
            text(d,(xx+52,y+106),'SAVE  0'+str(j+1),42,PURPLE,True)
            text(d,(xx+30,y+290),'进度存档',34,WHITE,True)
            text(d,(xx+30,y+350),'游戏内存读档功能示意',25,MUTED)
            if j==1:
                box(d,(xx+30,y+403,xx+445,y+455),'#315448',14)
                text(d,(xx+150,y+410),'继续阅读',27,MINT)
    elif idx==4:
        box(d,(220,y+15,1695,y+480))
        for j,(l,ratio) in enumerate([('背景音乐',.7),('音效',.55),('语音',.85)]):
            yy=y+65+j*105; text(d,(265,yy),l,33)
            box(d,(570,yy+16,1575,yy+28),'#3D445D',6)
            box(d,(570,yy+16,570+int(1005*ratio),yy+28),PURPLE,6)
            v=570+int(1005*ratio); d.ellipse((v-12,yy+9,v+12,yy+36),fill=WHITE)
        d.line((265,y+382,1635,y+382),fill=LINE,width=2)
        text(d,(265,y+411),'汉化支持',32,MINT,True)
        text(d,(570,y+415),'中文显示 · 姓名 · 样式回归',28,MUTED)
    elif idx==5:
        names=['BLACK','BLESS','BIRTH','BRAVE','Endless Dungeon']
        for j,l in enumerate(names):
            xx=205+(j%3)*510; yy=y+(j//3)*145
            box(d,(xx,yy+10,xx+480,yy+130))
            text(d,(xx+27,yy+42),l,31,PURPLE if j!=1 else MINT,True)
        box(d,(205,y+335,1710,y+495),'#20382F',22)
        text(d,(245,y+361),'BLESS：一条主线已通关',37,MINT,True)
        text(d,(245,y+424),'样本验证 ≠ 全作品兼容 ≠ 五款全部通关',29,WHITE)
    elif idx==6:
        box(d,(225,y+25,825,y+245),'#2D2944',28)
        text(d,(286,y+70),'共享 Rust 核心',47,PURPLE,True)
        text(d,(286,y+150),'游戏逻辑与 runtime',30,MUTED)
        d.line((850,y+140,1020,y+140),fill=MINT,width=6)
        d.polygon([(1020,y+140),(998,y+126),(998,y+154)],fill=MINT)
        box(d,(1050,y+25,1670,y+245),'#20382F',28)
        text(d,(1115,y+70),'Android',47,MINT,True)
        text(d,(1115,y+150),'Kotlin / JNI',30,MUTED)
        text(d,(245,y+310),'129',86,WHITE,True)
        text(d,(452,y+340),'项 runtime 测试',35,MUTED)
        text(d,(1050,y+318),'BLACK · Android 15',37,WHITE,True)
        text(d,(1050,y+383),'PGKM10 真机已验证',29,MUTED)
    else:
        logo(d,220,y+25,250)
        text(d,(560,y+28),'ARM64 预览版',56,PURPLE,True)
        text(d,(565,y+117),'兼容性持续完善',35,WHITE)
        text(d,(565,y+187),'自备合法资源 · 不含游戏资源或补丁',30,MUTED)
        box(d,(220,y+345,1690,y+460),'#222940',20)
        text(d,(270,y+373),'从你的游戏库，继续下一段故事。',42,WHITE,True)
    # Persistent safe-area caption panel.
    if subtitle:
        box(d,(80,920,1840,1033),'#0B0F1C',18)
        line=s['captions'][0 if t<s['duration']/2 else 1]
        bbox=d.textbbox((0,0),line,font=font(32)); tw=bbox[2]-bbox[0]
        text(d,((W-tw)/2,950),line,32,WHITE)
    elapsed=sum(z['duration'] for z in SCENES[:idx])+t; total=sum(z['duration'] for z in SCENES)
    d.rectangle((0,H-5,int(W*elapsed/total),H),fill=MINT)
    # A brief fade through the background makes scene cuts gentle without blurring text.
    fade=min(1,t/.35,(s['duration']-t)/.35)
    if fade<1: im=Image.blend(Image.new('RGB',(W,H),BG),im,max(0,fade))
    return im

def stamp(sec):
    ms=round(sec*1000); h,ms=divmod(ms,3600000); m,ms=divmod(ms,60000); s,ms=divmod(ms,1000)
    return f'{h:02}:{m:02}:{s:02},{ms:03}'
def main():
    ap=argparse.ArgumentParser();ap.add_argument('--preview-only',action='store_true');args=ap.parse_args()
    OUT.mkdir(parents=True,exist_ok=True)
    (OUT/'storyboard.json').write_text(json.dumps(SCENES,ensure_ascii=False,indent=2),encoding='utf-8')
    make_audio()
    (OUT/'storyboard.json').write_text(json.dumps(SCENES,ensure_ascii=False,indent=2),encoding='utf-8')
    subs=[];at=0
    for i,s in enumerate(SCENES):
        s['start']=at
        for j,c in enumerate(s['captions']):
            subs.append(f'{i*2+j+1}\n{stamp(at+j*s["duration"]/2)} --> {stamp(at+(j+1)*s["duration"]/2)}\n{c}\n')
        at+=s['duration']
    (OUT/'nekopack-intro-zh.srt').write_text('\n'.join(subs),encoding='utf-8-sig')
    (OUT/'storyboard.json').write_text(json.dumps(SCENES,ensure_ascii=False,indent=2),encoding='utf-8')
    render(0,3,False).save(OUT/'cover.png')
    sheet=Image.new('RGB',(1280,4*360),BG)
    for i in range(8): sheet.paste(render(i,3).resize((640,360)),((i%2)*640,(i//2)*360))
    sheet.save(OUT/'contact-sheet.jpg',quality=92)
    if args.preview_only: return
    cmd=[imageio_ffmpeg.get_ffmpeg_exe(),'-y','-f','rawvideo','-vcodec','rawvideo','-s',f'{W}x{H}','-pix_fmt','rgb24','-r',str(FPS),'-i','-','-i',str(OUT/'nekopack-intro-zh.wav'),'-c:v','libx264','-preset','fast','-crf','20','-pix_fmt','yuv420p','-c:a','aac','-b:a','192k','-movflags','+faststart','-shortest',str(OUT/'nekopack-intro-zh.mp4')]
    with (OUT/'render.log').open('w',encoding='utf-8') as log:
        p=subprocess.Popen(cmd,stdin=subprocess.PIPE,stderr=log)
        try:
            for i,s in enumerate(SCENES):
                print(f'Rendering scene {i+1}/8 ({s["duration"]:.2f}s)',flush=True)
                for frame in range(round(s['duration']*FPS)):p.stdin.write(render(i,frame/FPS).tobytes())
        finally:p.stdin.close()
        if p.wait():raise RuntimeError('FFmpeg failed; inspect .local/promo/render.log')
    print(f'Done: {at:.2f}s / {W}x{H} / {FPS}fps',flush=True)
if __name__=='__main__':main()
