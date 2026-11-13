"""Build an isolated original-handler fixture without reading any scenario or image."""
import importlib.util
import json
import shutil
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('packs', Path(__file__).with_name('build-isolated-audio-fixtures.py'))
packs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packs)
packs.CLI = ROOT/'target/debug/nekopack-cli.exe'
source = ROOT/'.local/sorakoi-import/game/ソラコイ'
output = ROOT/'.local/system-integration/command-fixture'
output.mkdir(parents=True, exist_ok=True)

def chunk(kind, data):
    return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))

def synthetic_eyecatch():
    # These are generated flat test pixels, not extracted or inspected game art.
    data=b'\x8aMNG\r\n\x1a\n'+chunk(b'MHDR',struct.pack('>7I',1600,900,0,0,0,0,0))
    for index,name in enumerate(['ロゴ','BG1','ベース','月日','8月','10日','5日']):
        width,height=(1600,900) if name=='BG1' else (64,32)
        x,y=(0,0) if name=='BG1' else (index*100,100)
        rgba=bytes([20+index*20,70,110,255])
        data+=chunk(b'FRAM',b'\x02'+name.encode('cp932'))
        data+=chunk(b'DEFI',b'\0'*4+struct.pack('>ii',x,y))
        data+=chunk(b'IHDR',struct.pack('>IIBBBBB',width,height,8,6,0,0,0))
        data+=chunk(b'IDAT',zlib.compress((b'\0'+rgba*width)*height))
        data+=chunk(b'IEND',b'')
    return data+chunk(b'MEND',b'')

def synthetic_dialog():
    data=b'\x8aMNG\r\n\x1a\n'+chunk(b'MHDR',struct.pack('>7I',1600,900,0,0,0,0,0))
    for name,x,y,w,h in [('BASE',0,0,1600,900),('QSAVE',0,0,1600,900),('= OK',160,300,400,200),('+ OK',160,300,400,200),('* OK',160,300,400,200),('= CANCEL',960,300,400,200)]:
        data+=chunk(b'FRAM',b'\x02'+name.encode('ascii'))+chunk(b'DEFI',b'\0'*4+struct.pack('>ii',x,y))
        data+=chunk(b'IHDR',struct.pack('>IIBBBBB',w,h,8,6,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes([80,100,120,255])*w)*h))+chunk(b'IEND',b'')
    return data+chunk(b'MEND',b'')

def synthetic_mask():
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',1600,900,8,6,0,0,0))+chunk(b'IDAT',zlib.compress((b'\0'+bytes([255,0,128,255])*1600)*900))+chunk(b'IEND',b'')

start, evidence = packs.extract([str(source/'system.dat'),str(source/'script.dat')], 'script', 'start.txt')
start = start.decode('cp932')
assert start.count('.load titlelogo') == 1
start = start.replace('.load titlelogo', '.system command_probe.nut\n.load title')
host = b'''Config.SysEffect=0;Config.TextSpeed=100;Config.AutoSave=0;Config.KeepIcon=1;
Config.Dialog.QSAVE=false;Config.Dialog.QLOAD=false;
com["probe_wait"] = function(args){
    local screen=Screen();screen.Fill(0x243448);TransBegin();screen.Fill(0x35566a);TransEnd();
    TransSet(100,"probe","fade");RenderTrans(128);screen.Update();
    while(wait(-1)){if(event()==ID_OK)break;}
    screen.Fill(0x35566a);screen.Update();
    while(wait(-1)){if(event()==ID_CANCEL)break;}
    sound("CLICK");
}
com["probe_choice"] = function(args,rows){
    if(rows.len()!=2)throw "original row contract failed";
    Config.Dialog.QSAVE=true;Config.AutoSelect=1;
    if(!ViewDialog("QSAVE"))throw "original confirmation returned false";
    Config.Dialog.QSAVE=false;
    return 2;
}
'''
script = '''.route COMMAND_PROBE
.date 0815 1
.date 0815 1
.probe_wait
.probe_choice
 first
 second
.case 1
.unexpected_branch
.case 2
:Probe::XX
probe command complete
.img0 白
.fade 0
.delay 10000
.img0 黒
:Probe::XX
probe delayed scene
.end
'''.encode('cp932')
click = (ROOT/'.local/system-integration/ui-tone.ogg').read_bytes()
entries = [('script','start.txt',start.encode('cp932')),
           ('script','titlelogo.txt',b'.load title\n'),
           ('script','title.txt',b'.title\n START\n.case 1\n.load story1\n.end\n'),
           ('script','story1.txt',script),('system','command_probe.nut',host),
           ('system','eyecatch.mng',synthetic_eyecatch()),('system','dialog.mng',synthetic_dialog()),('image/mask','probe.png',synthetic_mask())]
entries += [('sound/se',name+'.ogg',click) for name in ['se_CLICK','se_OK','se_PAGE','se_SELECT']]
for name in ['system.dat','sorakoi.exe']:
    shutil.copy2(source/name,output/name)
packs.pack(output/'command_probe.dat',entries)
manifest = dict(synthetic=True,source_start=evidence,original_handlers=['date','ViewDialog'],
                synthetic_graphics=['system/eyecatch.mng'],
                checks=['original timed date coroutine','original date confirmation','two input waits','sound after return'],
                page=dict(source='story1',line=12),
                delay_page=dict(source='story1',line=18,delay_ms=10000),
                files=[dict(name=p.name,sha256=packs.sha(p),bytes=p.stat().st_size) for p in sorted(output.iterdir()) if p.suffix in ['.dat','.exe']])
(output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8')
print(json.dumps(dict(output=str(output),bytes=sum(f['bytes'] for f in manifest['files']))))
