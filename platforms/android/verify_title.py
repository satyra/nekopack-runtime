"""BLESS title DAT composition, real touch/cancellation, same-VM story entry and cleanup.

Reads title.mng/title.txt/select.nut only; never opens CG or recollection contents.
Screenshots and original images stay in the ignored local work directory.
"""
import argparse, hashlib, io, json, re, subprocess, sys, time, uuid
from pathlib import Path
import xml.etree.ElementTree as ET
import numpy as np
from PIL import Image


def display_error(source, shot, bounds):
    x,y,r,b=bounds;w,h=source.size;scale=min((r-x)/w,(b-y)/h)
    ox=x+(r-x-w*scale)/2;oy=y+(b-y-h*scale)/2
    left,top,right,bottom=round(ox+scale),round(oy+scale),round(ox+(w-1)*scale),round(oy+(h-1)*scale)
    yy,xx=np.mgrid[top:bottom,left:right];sx=(xx+.5-ox)/scale-.5;sy=(yy+.5-oy)/scale-.5
    ix=np.floor(sx).astype(int);iy=np.floor(sy).astype(int);fx=(sx-ix)[...,None];fy=(sy-iy)[...,None]
    pixels=np.array(source.convert('RGB'),dtype=float)
    expected=(pixels[iy,ix]*(1-fx)+pixels[iy,ix+1]*fx)*(1-fy)+(pixels[iy+1,ix]*(1-fx)+pixels[iy+1,ix+1]*fx)*fy
    delta=np.abs(np.array(shot.convert('RGB'),dtype=float)[top:bottom,left:right]-expected)
    result=dict(mean=float(delta.mean()),p99=float(np.percentile(delta,99)),maximum=float(delta.max()))
    assert result['mean']<1 and result['p99']<3,result
    return result


def main():
    p=argparse.ArgumentParser()
    for key in ('adb','serial','games','portkit','reference','desktop','work','output'):p.add_argument('--'+key,required=True)
    a=p.parse_args();work=Path(a.work);work.mkdir(parents=True,exist_ok=True)
    sys.path.insert(0,a.portkit)
    from portkit.neko import Archive
    from portkit.mng import frames
    sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
    archive=Archive(Path(a.games)/next(s['file'] for s in sources if s['file'].endswith('/system.dat')))
    raw=archive.read('system/title.mng');sprites=frames(raw,name_encoding='cp932')
    scenario=archive.read('script/title.txt').decode('cp932')
    script=archive.read('system/select.nut').decode('cp932')
    assert '.map title' in scenario and '.case 1' in scenario and '.load story1' in scenario
    for text in ('spots.Bind(name, i+1)','spots.Enable(i+1, e)','spots.Active(i+1, a)','base.Draw(images["BASE"])'):assert text in script
    # Independent title composition from original PNG chunks; no runtime draw plan as oracle.
    expected=Image.new('RGBA',(1280,720),(0,0,0,255))
    for name in ('BASE','= スタート','= ロード','= コンフィグ','= エンド'):
        sprite=sprites[name];expected.alpha_composite(Image.open(io.BytesIO(sprite['png'])).convert('RGBA'),(sprite['x'],sprite['y']))
    desktop_meta,desktop_png=Path(a.desktop).read_bytes().split(b'\n',1)
    assert np.array_equal(np.array(expected),np.array(Image.open(io.BytesIO(desktop_png))))
    pkg='org.nekopack.probe';run=''
    def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=45)
    def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
    def obj(name):
        data=read(name)
        return json.loads(data) if data.startswith(b'{') else None
    def wait(fn):
        end=time.monotonic()+90
        while time.monotonic()<end:
            error=read('error.txt');assert b'No such file' in error,error
            result=fn()
            if result:return result
            time.sleep(.1)
        raise AssertionError('title verification timed out')
    def absent(name):assert b'No such file' in read(name),name
    def nodes():
        adb('shell','uiautomator','dump','/sdcard/neko-title.xml')
        return list(ET.fromstring(adb('shell','cat','/sdcard/neko-title.xml')).iter('node'))
    def bounds(label,attr='text'):
        n=next(n for n in nodes() if n.get(attr)==label)
        return list(map(int,re.findall(r'\d+',n.get('bounds'))))
    def tap_screen(x,y):adb('shell','input','tap',str(round(x)),str(round(y)))
    def point(x,y):
        l,t,r,b=viewport;scale=min((r-l)/1280,(b-t)/720)
        return (round(l+(r-l-1280*scale)/2+x*scale),round(t+(b-t-720*scale)/2+y*scale))
    def button(id):
        x,y,w,h=next(b['bounds'] for b in initial['buttons'] if b['id']==id)
        return point(x+w/2,y+h/2)
    def motion(kind,where):adb('shell','input','motionevent',kind,*map(str,where))
    def start():
        nonlocal run
        run='title_'+uuid.uuid4().hex
        adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','title_game','0','--es','run_id',run)
        initial=wait(lambda:obj('title-presented.json'))
        assert initial==json.loads(desktop_meta)
        assert read('title.png')==desktop_png
        return initial,bounds('游戏标题菜单','content-desc')
    machine=adb('shell','uname','-m').decode().strip();bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip()
    assert machine=='aarch64' and bridge in ('','0'),(machine,bridge)
    build=json.loads(Path('.local/android-probe/build.json').read_text())
    apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:')
    apk_sha=adb('shell','sha256sum',apk).decode().split()[0];assert apk_sha==build['sha256']['nekopack-arm64-probe.apk']
    initial,viewport=start();first_run=run;print('title ready',run,flush=True)
    time.sleep(.5);assert obj('title.json')==initial;absent('page-1.json')
    shot=adb('exec-out','screencap','-p');(work/'title-screen.png').write_bytes(shot)
    display=display_error(expected,Image.open(io.BytesIO(shot)),viewport)
    tap_screen(*point(700,300));wait(lambda:obj('title-presented.json')['input_sequence']>=2);absent('title-started.json')
    motion('DOWN',button(1));wait(lambda:obj('title-presented.json')['pressed']==1)
    hover=read('title.png');assert hover!=desktop_png;(work/'title-hover.png').write_bytes(hover)
    expected_hover=Image.new('RGBA',(1280,720),(0,0,0,255))
    for name in ('BASE','+ スタート','= ロード','= コンフィグ','= エンド'):
        sprite=sprites[name];expected_hover.alpha_composite(Image.open(io.BytesIO(sprite['png'])).convert('RGBA'),(sprite['x'],sprite['y']))
    assert np.array_equal(np.array(expected_hover),np.array(Image.open(io.BytesIO(hover))))
    print('original normal/hover and screen verified',flush=True)
    # Drag outside, then back inside: activation must remain cancelled.
    motion('MOVE',point(700,300));wait(lambda:obj('title.json')['pressed']==0)
    motion('MOVE',button(1));motion('UP',button(1));time.sleep(.5);absent('title-started.json')
    assert obj('title.json')['steps']==initial['steps']
    # Backgrounding while pressed cancels the gesture and retains the pending map.
    motion('DOWN',button(1));wait(lambda:obj('title-presented.json')['pressed']==1)
    adb('shell','input','keyevent','KEYCODE_HOME');wait(lambda:obj('title.json')['pressed']==0)
    time.sleep(.5);absent('page-1.json')
    adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity')
    viewport=bounds('游戏标题菜单','content-desc');assert obj('title.json')['steps']==initial['steps']
    tap_screen(*button(2));wait(lambda:obj('title-presented.json')['action']==2);absent('page-1.json')
    print('cancellation and background verified',flush=True)
    tap_screen(*button(1));wait(lambda:obj('page-1.json'));wait(lambda:obj('text-1.json'))
    pages=[]
    for n in range(1,6):
        page=wait(lambda:obj(f'page-{n}.json'));wait(lambda:obj(f'text-{n}.json')['complete'])
        ref=Path(a.reference)/f'{n-1}.json'
        assert page==json.loads(ref.read_text(encoding='utf8'))
        assert read(f'page-{n}.png')==ref.with_suffix('.png').read_bytes()
        pages.append(dict(sequence=n,source=page['source'],line=page['line'],scene_sha256=hashlib.sha256(read(f'page-{n}.png')).hexdigest()))
        if n<5:
            x,y,r,b=bounds('继续');tap_screen((x+r)/2,(y+b)/2)
    x,y,r,b=bounds('结束运行');tap_screen((x+r)/2,(y+b)/2)
    wait(lambda:b'VM live bytes=0' in read('closed.txt'))
    initial,viewport=start();close_run=run
    tap_screen(*button(4));wait(lambda:b'VM live bytes=0' in read('closed.txt'));absent('page-1.json')
    report=dict(execution='native ARM64',serial=a.serial,machine=machine,native_bridge=bridge,apk_sha256=apk_sha,
                title_mng_sha256=hashlib.sha256(raw).hexdigest(),original_png_composition_equal=True,display=display,
                title_run=first_run,title_close_run=close_run,checks=['pending map stable','blank no activation','original hover shown','drag out/back cancelled','Home cancels press','unsupported Load stays title','Start enters same runtime','close title VM bytes zero','close story VM bytes zero'],pages=pages,
                scope='BLESS initial title and first five story pages; no CG/recollection content; no claim of full original UI compatibility')
    Path(a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(json.dumps(report,ensure_ascii=False))


if __name__=='__main__':main()
