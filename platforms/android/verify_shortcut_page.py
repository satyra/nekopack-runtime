"""BLESS fifth settings page: independent PNG composition and real device navigation.

Preserves the user's preferences. No CG/recollection resources are accessed.
"""
import argparse, hashlib, io, json, re, struct, subprocess, sys, time, uuid, zlib
from pathlib import Path
import xml.etree.ElementTree as ET
import numpy as np
from PIL import Image

def sprites(data):
    """Independent extraction retaining last named image, including repeated names."""
    assert data[:8]==b'\x8aMNG\r\n\x1a\n'
    pos=8;name='';xy=(0,0);png=None;result={}
    while pos<len(data):
        size=struct.unpack_from('>I',data,pos)[0];end=pos+size+12;assert end<=len(data)
        kind=data[pos+4:pos+8];payload=data[pos+8:pos+8+size]
        assert zlib.crc32(kind+payload)==struct.unpack_from('>I',data,pos+8+size)[0]
        chunk=data[pos:end];pos=end
        if kind==b'FRAM':name=payload[1:].split(b'\0')[0].decode('cp932')
        elif kind==b'DEFI':assert len(payload)==12;xy=struct.unpack_from('>ii',payload,4)
        elif kind==b'IHDR':assert png is None;png=bytearray(b'\x89PNG\r\n\x1a\n'+chunk)
        elif png is not None:
            png.extend(chunk)
            if kind==b'IEND':
                image=Image.open(io.BytesIO(png)).convert('RGBA');result[name]=(image,xy);png=None
    assert png is None
    return result

def main():
    p=argparse.ArgumentParser()
    for key in ('adb','serial','games','portkit','work','output'):p.add_argument('--'+key,required=True)
    a=p.parse_args();work=Path(a.work);work.mkdir(parents=True,exist_ok=True)
    sys.path.insert(0,a.portkit)
    from portkit.neko import Archive
    sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
    archive=Archive(Path(a.games)/next(s['file'] for s in sources if s['file'].endswith('/system.dat')))
    raw=archive.read('system/config5.mng');sheet=sprites(raw);common=sprites(archive.read('system/config.mng'))
    script=archive.read('system/config.nut').decode('cp932')
    actions=re.findall(r'"([A-Z]+)"',re.search(r'ShortCuts\s*=\s*\[([^]]+)\]',script)[1])[2:]
    assert len(actions)==10
    for fact in ('s.Draw(images[Config.ShortCuts[i]],s2.x-s1.x,s2.y-s1.y)','m_window = images["LIST"]'):assert fact in script
    expected=Image.new('RGBA',(1280,720),(0,0,0,255))
    def draw(sprite,offset=(0,0)):
        image,(x,y)=sprite;expected.alpha_composite(image,(x+offset[0],y+offset[1]))
    draw(common['BASE']);draw(sheet['BASE'])
    for key in ('= PAGE++','= PAGE--'):draw(common[key])
    for key in ('=CLOSE','=DEFAULT'):draw(sheet[key])
    # Unsupported CenterKey/SpaceKey remain their original default zero.
    for name in ('CenterKey','SpaceKey'):
        for value in range(3):draw(sheet[('@' if value==0 else '=')+name+'#'+str(value)])
    origin=sheet['*F3'][1]
    for number,action in enumerate(actions,3):
        anchor=sheet['*F'+str(number)][1];draw(sheet[action],(anchor[0]-origin[0],anchor[1]-origin[1]))
    draw(common['@ PAGE'],(0,common['* PAGE'][0].height-common['@ PAGE'][0].height))
    expected.save(work/'expected-page5.png')
    pkg='org.nekopack.probe';run='shortcuts_'+uuid.uuid4().hex
    def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=45)
    def read(name):return adb('exec-out','run-as',pkg,'cat',f'files/{run}/{name}')
    def obj(name):
        raw=read(name)
        if not raw.startswith(b'{'):return None
        try:return json.loads(raw)
        except json.JSONDecodeError:return None
    def wait(fn):
        until=time.monotonic()+90
        while time.monotonic()<until:
            error=read('error.txt');assert b'No such file' in error,error
            result=fn()
            if result:return result
            time.sleep(.1)
        raise AssertionError('shortcut page timeout')
    def bounds(label):
        adb('shell','uiautomator','dump','/sdcard/neko-shortcuts.xml')
        node=next(n for n in ET.fromstring(adb('shell','cat','/sdcard/neko-shortcuts.xml')).iter('node') if n.get('content-desc')==label)
        return list(map(int,re.findall(r'\d+',node.get('bounds'))))
    def tap_game(x,y,label):
        l,t,r,b=bounds(label);s=min((r-l)/1280,(b-t)/720)
        adb('shell','input','tap',str(round(l+(r-l-1280*s)/2+x*s)),str(round(t+(b-t-720*s)/2+y*s)))
    def state():return obj('settings-presented.json')
    def navigate(page):
        while state()['page']!=page:
            old=state();key='= PAGE++' if old['page']<page else '= PAGE--';x,y,w,h=old['areas'][key]
            tap_game(x+w/2,y+h/2,'原版设置页');wait(lambda:state() and state()['input_sequence']>=old['input_sequence']+2)
    machine=adb('shell','uname','-m').decode().strip();bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip();assert machine=='aarch64' and bridge in ('0','')
    apk=adb('shell','pm','path',pkg).decode().strip().removeprefix('package:');sha=adb('shell','sha256sum',apk).decode().split()[0]
    build=json.loads(Path('.local/android-probe/build.json').read_text());assert sha==build['sha256']['nekopack-arm64-probe.apk']
    adb('shell','am','start','-W','-S','-n',pkg+'/.MainActivity','--ei','title_game','0','--es','run_id',run)
    title=wait(lambda:obj('title-presented.json'));x,y,w,h=next(b['bounds'] for b in title['buttons'] if b['id']==3)
    tap_game(x+w/2,y+h/2,'游戏标题菜单');wait(state);original=state()['config']
    navigate(5);first=read('settings.png');assert np.array_equal(np.array(expected),np.array(Image.open(io.BytesIO(first))))
    assert [s['action'] for s in state()['shortcuts']]==actions
    assert all('= '+k not in state()['areas'] for k in ('SAVE','LOAD','RESET','DELETE','AUTO','QUIT'))
    (work/'page5.png').write_bytes(first);bounds('原版设置页');(work/'page5-screen.png').write_bytes(adb('exec-out','screencap','-p'))
    navigate(4);navigate(5);assert read('settings.png')==first
    navigate(3);navigate(5);assert read('settings.png')==first and state()['config']==original
    # An unimplemented F3 binding remains read-only and cannot open the modal list.
    before=state()['input_sequence'];x,y,w,h=state()['areas']['* F3'];tap_game(x+w/2,y+h/2,'原版设置页')
    wait(lambda:state()['input_sequence']>=before+2);assert state()['unsupported'] and state()['config']==original and read('settings.png')==first
    adb('shell','input','keyevent','KEYCODE_HOME');time.sleep(.5)
    adb('shell','am','start','-W','--activity-reorder-to-front','-n',pkg+'/.MainActivity');bounds('原版设置页');assert read('settings.png')==first
    report=dict(execution='native ARM64',run_id=run,apk_sha256=sha,config5_sha256=hashlib.sha256(raw).hexdigest(),original_page5_pixels_equal=True,shortcut_actions=actions,modal_choices_absent=True,repeat_navigation_pixels_equal=True,preferences_unchanged=original,unsupported_shortcut_read_only=True,home_restore=True)
    Path(a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8');print(json.dumps(report,ensure_ascii=False))

if __name__=='__main__':main()
