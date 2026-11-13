"""Mount original settings code/assets with a synthetic scenario; inspect no artwork."""
import importlib.util,json,shutil
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
spec=importlib.util.spec_from_file_location('packs',Path(__file__).with_name('build-isolated-audio-fixtures.py'))
packs=importlib.util.module_from_spec(spec);spec.loader.exec_module(packs)
packs.CLI=ROOT/'target/debug/nekopack-cli.exe'
source=ROOT/'.local/sorakoi-import/game/ソラコイ';output=ROOT/'.local/system-integration/config-fixture';output.mkdir(parents=True,exist_ok=True)
start,evidence=packs.extract([str(source/'system.dat'),str(source/'script.dat')],'script','start.txt')
start=start.decode('cp932').replace('.load titlelogo','.system config_probe.nut\n.load title')
setup=b'Config.SysEffect=0;Config.TextSpeed=100;Config.AutoSave=0;Config.KeepIcon=1;'
script=b'.route CONFIG_PROBE\n.config\n:Probe::XX\nsettings returned\n'
entries=[('script','start.txt',start.encode('cp932')),('system','config_probe.nut',setup),('script','titlelogo.txt',b'.load title\n'),('script','title.txt',b'.title\n START\n.case 1\n.load story1\n.end\n'),('script','story1.txt',script)]
for name in ['system.dat','sorakoi.exe']:shutil.copy2(source/name,output/name)
packs.pack(output/'config_probe.dat',entries)
manifest=dict(synthetic_scenario=True,original_handlers=['config','ViewConfig','ViewConfigSound','ViewConfigDialog','ViewConfigShortcut','SaveConfig'],source_start=evidence,files=[dict(name=p.name,sha256=packs.sha(p),bytes=p.stat().st_size) for p in sorted(output.iterdir()) if p.suffix in ['.dat','.exe']])
(output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8')
print(json.dumps(dict(output=str(output),bytes=sum(f['bytes'] for f in manifest['files']))))
