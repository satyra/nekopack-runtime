"""Mount original quit handlers/assets with a synthetic .quit scenario.

Reads only original startup code; never decodes game artwork or story text.
Does not deploy files or contact Android.
"""
import argparse
import importlib.util
import json
import shutil
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location(
    'packs', Path(__file__).with_name('build-isolated-audio-fixtures.py'))
packs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=ROOT/'.local/sorakoi-import/game/ソラコイ')
    parser.add_argument('--output', type=Path, default=ROOT/'.local/system-integration/exit-fixture')
    parser.add_argument('--cli', type=Path, default=ROOT/'target/debug/nekopack-cli.exe')
    args = parser.parse_args()
    packs.CLI = args.cli
    args.output.mkdir(parents=True, exist_ok=True)
    start, evidence = packs.extract(
        [str(args.source/'system.dat'), str(args.source/'script.dat')], 'script', 'start.txt')
    start = start.decode('cp932')
    assert start.count('.load titlelogo') == 1
    start = start.replace('.load titlelogo', '.system exit_probe.nut\n.load title')
    setup = (b'Config.SysEffect=0;Config.TextSpeed=100;Config.AutoSave=0;'
             b'Config.KeepIcon=1;Config.Dialog.QUIT=true;Config.AutoSelect=0;')
    entries = [
        ('script', 'start.txt', start.encode('cp932')),
        ('system', 'exit_probe.nut', setup),
        ('script', 'titlelogo.txt', b'.load title\n'),
        ('script', 'title.txt', b'.title\n START\n.case 1\n.load story1\n.end\n'),
        ('script', 'story1.txt', b'.route EXIT_PROBE\n.quit\n:Probe::XX\noriginal quit cancelled\n'),
    ]
    for name in ['system.dat', 'sorakoi.exe']:
        shutil.copy2(args.source/name, args.output/name)
    packs.pack(args.output/'exit_probe.dat', entries)
    manifest = dict(
        synthetic_scenario=True,
        original_handlers=['com.quit', 'InvokeCommand(ID_QUIT)', 'ViewDialog("QUIT")', 'QuitGame'],
        source_start=evidence,
        synthetic_page=dict(source='story1', line=4),
        checks=['cancel returns to synthetic dialogue', 'cold start confirmation emits system_exit',
                'original confirmation closes game without another confirmation'],
        files=[dict(name=p.name, sha256=packs.sha(p), bytes=p.stat().st_size)
               for p in sorted(args.output.iterdir()) if p.suffix in ['.dat', '.exe']],
    )
    (args.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n', encoding='utf8')
    print(json.dumps(dict(output=str(args.output), bytes=sum(f['bytes'] for f in manifest['files']))))


if __name__ == '__main__':
    main()
