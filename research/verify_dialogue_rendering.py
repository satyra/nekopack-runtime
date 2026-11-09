"""BLESS source facts and real Android/desktop overlay + displayed-pixel checks.

Requires local licensed game data and the existing portkit research reader.
No source text, sprites, fonts or screenshots are copied into the report.
"""
import argparse, hashlib, io, json, re, struct, subprocess, sys, time, uuid, zlib
from pathlib import Path
import xml.etree.ElementTree as ET
import numpy as np
from PIL import Image
import pefile, capstone

def main():
    p = argparse.ArgumentParser()
    for key in ('adb','serial','games','portkit','reference','desktop','work','output'):
        p.add_argument('--'+key, required=True)
    p.add_argument('--original-capture')
    a=p.parse_args(); work=Path(a.work);work.mkdir(parents=True,exist_ok=True)
    sys.path.insert(0,a.portkit)
    from portkit.neko import Archive
    from portkit.mng import frames
    sha=lambda b:hashlib.sha256(b).hexdigest()
    sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
    archive=Archive(Path(a.games)/next(s['file'] for s in sources if s['file'].endswith('/system.dat')))
    raw={key:archive.read('system/'+key) for key in ('message.nut','config.nut','msgbox.mng')}
    script=raw['message.nut'].decode('cp932'); config=raw['config.nut'].decode('cp932')
    for fragment in ('comp.Size  = 22','comp.LineSpan  = 8','comp.Color = 0xFFFFFF','name.Clear(); name.Blt(box.name,0,0,opacity);','text.Clear(); text.Blt(box.text,0,0,opacity);'):
        assert fragment in script, fragment
    assert 'BoxOpacity = OPACITY_MAX * 3/4' in config
    sprites=frames(raw['msgbox.mng'])
    geometry={k:{f:sprites[k][f] for f in ('x','y','width','height')} for k in ('TEXT','NAME','*TEXT','*NAME')}
    assert geometry['TEXT']==dict(x=100,y=605,width=1080,height=107)
    exe_path=next(Path(a.games).rglob('Tiny_BoD.exe'));exe=exe_path.read_bytes();pe=pefile.PE(data=exe)
    md=capstone.Cs(capstone.CS_ARCH_X86,capstone.CS_MODE_32)
    expected=[(0x40d691,'call','0x4bd8ac'),(0x4bd8d8,'call','0x4bdaf4'),(0x4bdb7c,'call','0x4be0b0'),
              (0x4be177,'psrld','mm2, 0x18'),(0x4be17f,'pmullw','mm2, mm6'),(0x4be185,'psrld','mm2, 8'),
              (0x4be19b,'psubw','mm5, mm7'),(0x4be1ad,'pmullw','mm5, mm4'),(0x4be1b6,'psrlw','mm5, 8')]
    facts=[]
    for va,mnemonic,operands in expected:
        ins=next(md.disasm(pe.get_data(va-pe.OPTIONAL_HEADER.ImageBase,16),va))
        assert (ins.mnemonic,ins.op_str)==(mnemonic,operands)
        facts.append(dict(va=hex(va),instruction=mnemonic+' '+operands))
    def adb(*args):return subprocess.check_output([a.adb,'-s',a.serial,*args],timeout=45)
    package='org.nekopack.probe';run='render_'+uuid.uuid4().hex
    def read(name):return adb('exec-out','run-as',package,'cat',f'files/{run}/{name}')
    def wait_page(n):
        end=time.monotonic()+90
        while time.monotonic()<end:
            error=read('error.txt');assert b'No such file' in error,error.decode()
            state=read(f'text-{n}.json')
            if state.startswith(b'{') and json.loads(state)['complete']:return
            time.sleep(.1)
        raise AssertionError('dialogue timeout')
    def nodes():
        adb('shell','uiautomator','dump','/sdcard/neko-render.xml')
        return list(ET.fromstring(adb('shell','cat','/sdcard/neko-render.xml')).iter('node'))
    def bounds(node):return list(map(int,re.findall(r'\d+',node.get('bounds'))))
    def tap(label):
        x,y,r,b=bounds(next(n for n in nodes() if n.get('text')==label and n.get('enabled')=='true'))
        adb('shell','input','tap',str((x+r)//2),str((y+b)//2))
    build=json.loads(Path('.local/android-probe/build.json').read_text())
    apk=adb('shell','pm','path',package).decode().strip().removeprefix('package:')
    apk_sha=adb('shell','sha256sum',apk).decode().split()[0]
    assert apk_sha==build['sha256']['nekopack-arm64-probe.apk']
    machine=adb('shell','uname','-m').decode().strip();bridge=adb('shell','getprop','ro.dalvik.vm.native.bridge').decode().strip()
    assert machine=='x86_64' and bridge not in ('','0')
    font=adb('exec-out','cat','/system/fonts/NotoSansCJK-Regular.ttc');(work/'font.ttc').write_bytes(font)
    (work/'msgbox.mng').write_bytes(raw['msgbox.mng'])
    adb('shell','am','start','-W','-S','-n',package+'/.MainActivity','--ei','story_game','0','--es','run_id',run)
    pages=[];invalid_resources=[]
    for n in (1,2,3):
        wait_page(n)
        page=json.loads(read(f'page-{n}.json'))
        assert page==json.loads((Path(a.reference)/f'{n-1}.json').read_text(encoding='utf8'))
        for key in ('speaker','text'):(work/(key+'.txt')).write_text(page[key],encoding='utf8')
        prefix=work/f'desktop-{n}'
        subprocess.run([a.desktop,str(work/'font.ttc'),str(work/'msgbox.mng'),str(work/'speaker.txt'),str(work/'text.txt'),str(prefix)],check=True)
        if n==1:
            for label,coordinates,error in (
                ('anchor_overflow',[-2147483648],'dialogue anchor x overflow'),
                ('panel_edge_overflow',[2147483637,2147483642],'dialogue panel edge overflow')):
                malformed=bytearray(raw['msgbox.mng']);offset=8;index=0
                while offset<len(malformed):
                    length=struct.unpack_from('>I',malformed,offset)[0]
                    if malformed[offset+4:offset+8]==b'DEFI' and index<len(coordinates):
                        struct.pack_into('>i',malformed,offset+12,coordinates[index]);index+=1
                        struct.pack_into('>I',malformed,offset+8+length,zlib.crc32(malformed[offset+4:offset+8+length]))
                    offset+=12+length
                path=work/(label+'.mng');path.write_bytes(malformed)
                rejected=subprocess.run([a.desktop,str(work/'font.ttc'),str(path),str(work/'speaker.txt'),str(work/'text.txt'),str(work/label)],capture_output=True,text=True)
                assert rejected.returncode==1 and error in rejected.stderr,rejected.stderr
                invalid_resources.append(dict(case=label,error=error,exit_code=rejected.returncode))
        full=read(f'dialogue-{n}.png');empty=read(f'dialogue-{n}-empty.png');layout=json.loads(read(f'dialogue-{n}.json'))
        for suffix,device in (('.png',full),('-empty.png',empty)):
            expected_pixels=np.array(Image.open(str(prefix)+suffix));actual=np.array(Image.open(io.BytesIO(device)))
            assert np.array_equal(expected_pixels,actual),'desktop/Android glyph raster mismatch'
        full_image=Image.open(io.BytesIO(full)).convert('RGBA');empty_image=Image.open(io.BytesIO(empty)).convert('RGBA')
        delta=np.any(np.array(full_image)!=np.array(empty_image),axis=2)
        assert delta.sum()>100
        yy,xx=np.where(delta);assert yy.min()>=638-573 and xx.min()>=136-15
        if not page['speaker']:assert np.array(empty_image)[:32,:,3].max()==0,'anonymous name box remains visible'
        if n==3:assert layout['lines']>=2 and not layout['clipped']
        node=next(t for t in nodes() if t.get('content-desc')==page['speaker']+'\n'+page['text'])
        x,y,r,b=bounds(node);shot=Image.open(io.BytesIO(adb('exec-out','screencap','-p'))).convert('RGB')
        shot.save(work/f'device-{n}.png')
        scene=Image.open(io.BytesIO(read(f'page-{n}.png'))).convert('RGBA')
        scene.alpha_composite(full_image,tuple(layout['bounds'][:2]))
        scale=min((r-x)/scene.width,(b-y)/scene.height);w=round(scene.width*scale);h=round(scene.height*scale)
        left=x+round((r-x-w)/2);top=y+round((b-y-h)/2)
        predicted=scene.convert('RGB').resize((w,h),Image.Resampling.BILINEAR)
        # The actual screenshot is checked, not only the accessibility text or exported PNG.
        roi=(round(100*scale),round(573*scale),round(1180*scale),round(712*scale))
        displayed=shot.crop((left,top,left+w,top+h))
        diff=np.abs(np.array(displayed.crop(roi)).astype(int)-np.array(predicted.crop(roi)).astype(int))
        assert diff.mean()<5 and np.percentile(diff,95)<20,(diff.mean(),np.percentile(diff,95))
        pages.append(dict(sequence=n,named=bool(page['speaker']),layout=layout,overlay_sha256=sha(full),empty_sha256=sha(empty),changed_pixels=int(delta.sum()),desktop_pixels_equal=True,display_roi_mae=float(diff.mean()),display_roi_p95=float(np.percentile(diff,95)),screenshot_sha256=sha((work/f'device-{n}.png').read_bytes())))
        if n<3:tap('继续')
    font_info=json.loads(read('dialogue-font.json'))
    assert font_info['opacity']==191 and font_info['border']==1
    tap('结束运行');end=time.monotonic()+30
    while read('closed.txt')!=b'closed; VM live bytes=0':
        assert time.monotonic()<end;time.sleep(.1)
    report=dict(scope='BLESS first three ordinary dialogue pages; actual translated Android rendering',execution='translated ARM64 on x86_64',run_id=run,machine=machine,native_bridge=bridge,apk_sha256=apk_sha,font_sha256=sha(font),font=font_info,resource_sha256={k:sha(v) for k,v in raw.items()},geometry=geometry,exe_sha256=sha(exe),native_blt_facts=facts,pages=pages,invalid_resources=invalid_resources,vm_live_bytes=0,limits=['Noto CJK host substitute, not MS Gothic/GDI parity','Native Blt scalar edge rounding and scene draw rounding not bit-exact','No ruby, kinsoku, bold or alternate-size support'])
    if a.original_capture:
        saved=(exe_path.parent/'save/config.sav').read_bytes()
        offset=saved.index(b'BoxOpacity')+len(b'BoxOpacity')
        assert saved[offset:offset+4]==b'\x02\x00\x00\x05'
        original_opacity=struct.unpack_from('<i',saved,offset+4)[0]
        assert original_opacity==191, 'capture comparison requires the observed original opacity 191'
        captured=Path(a.original_capture).read_bytes();original=Image.open(io.BytesIO(captured)).convert('RGB').crop((1,26,1281,746))
        bg=Image.open(Path(a.reference)/'0.png').convert('RGBA');src=Image.open(io.BytesIO(sprites['TEXT']['png'])).convert('RGBA')
        mask=np.array(src)[:,:,3]>0;mask[:70]=False
        comparisons={}
        for native in (False,True):
            pixels=np.array(src).astype('uint32')
            if native:pixels=pixels*(pixels[:,:,3:4]*191//256)//256
            else:pixels[:,:,3]=pixels[:,:,3]*191//255
            composed=bg.copy();composed.alpha_composite(Image.fromarray(pixels.astype('uint8')),(100,605))
            diff=np.abs(np.array(composed.convert('RGB').crop((100,605,1180,712))).astype(int)-np.array(original.crop((100,605,1180,712))).astype(int))[mask]
            comparisons['native_blt' if native else 'alpha_only']=dict(mae=float(diff.mean()),p95=float(np.percentile(diff,95)))
        assert comparisons['native_blt']['mae']<comparisons['alpha_only']['mae']
        report['original_capture']=dict(sha256=sha(captured),format=Image.open(io.BytesIO(captured)).format,original_saved_box_opacity=original_opacity,original_config_sha256=sha(saved),roi='TEXT local y>=70, alpha>0; excludes garbled original text',metrics=comparisons,limitation='ANSI locale garbles original glyphs; JPEG is not exact-pixel evidence')
    Path(a.output).write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print('PASS original Blt facts, DAT geometry, desktop/Android glyphs, displayed pixels, VM cleanup')

if __name__=='__main__':main()
