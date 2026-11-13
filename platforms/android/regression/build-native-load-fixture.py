"""Mount original save/load code/assets with a synthetic scenario; inspect no artwork."""
import importlib.util,json,shutil,subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location('packs',Path(__file__).with_name('build-isolated-audio-fixtures.py'))
packs=importlib.util.module_from_spec(spec);spec.loader.exec_module(packs)
packs.CLI=ROOT/'target/debug/nekopack-cli.exe'
source=ROOT/'.local/sorakoi-import/game/ソラコイ';output=ROOT/'.local/system-integration/native-load-fixture';output.mkdir(parents=True,exist_ok=True)
start,evidence=packs.extract([str(source/'system.dat'),str(source/'script.dat')],'script','start.txt')
start=start.decode('cp932').replace('.load titlelogo','.system native_load_probe.nut\n.load title')
setup=b'Config.SysEffect=0;Config.TextSpeed=100;Config.AutoSave=0;Config.KeepIcon=1;Config.Dialog.QSAVE=true;Config.Dialog.LOAD=true;'
script=b'.route LOAD_PROBE\n:Probe::XX\nfirst\n:Probe::XX\nsecond\n.sysload\n:Probe::XX\nafter cancelled load\n.continue\n:Probe::XX\nafter empty continue\n'
entries=[('script','start.txt',start.encode('cp932')),('system','native_load_probe.nut',setup),('script','titlelogo.txt',b'.load title\n'),('script','title.txt',b'.title\n START\n.case 1\n.load story1\n.end\n'),('script','story1.txt',script)]
# Resolve only finite technical UI cue names. Do not enumerate story audio,
# decode images, or manufacture aliases for cues absent from the original pack.
ui_sounds=[]
for name in ['se_OK','se_NO','se_ON_MOUSE','se_CLICK','se_PAGE','se_SELECT','se_CANCEL']:
    filename=name+'.ogg'
    lookup=subprocess.check_output([str(packs.CLI),'lookup-name',b'sound/se'.hex(),filename.encode('ascii').hex(),str(source/'sound.dat')]).decode('ascii').splitlines()
    if lookup[0]=='missing':
        ui_sounds.append(dict(name=filename,present=False));continue
    data,meta=packs.extract([str(source/'sound.dat')],'sound/se',filename)
    assert data.startswith(b'OggS'),meta
    entries.append(('sound/se',filename,data));ui_sounds.append(dict(present=True,**meta))
assert any(item.get('name')=='sound/se/se_OK.ogg' and item['present'] for item in ui_sounds)
for name in ['system.dat','sorakoi.exe']:shutil.copy2(source/name,output/name)
packs.pack(output/'native_load_probe.dat',entries)
manifest=dict(synthetic_scenario=True,ui_sounds=ui_sounds,original_handlers=['sysload','ViewSavePage','SaveInfo','LoadData','continue'],source_start=evidence,files=[dict(name=p.name,sha256=packs.sha(p),bytes=p.stat().st_size) for p in sorted(output.iterdir()) if p.suffix in ['.dat','.exe']])
(output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8')
print(json.dumps(dict(output=str(output),bytes=sum(f['bytes'] for f in manifest['files']))))
