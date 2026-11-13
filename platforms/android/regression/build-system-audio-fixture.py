"""Build a <40 MiB metadata-only system/audio fixture; never deploy or operate Android."""
import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DEFAULT_SOURCE = ROOT / '.local/sorakoi-import/game/ソラコイ'
DEFAULT_OUTPUT = ROOT / '.local/system-integration/audio-fixture'


def load_pack_tools(cli):
    spec = importlib.util.spec_from_file_location('pack_tools', Path(__file__).with_name('build-isolated-audio-fixtures.py'))
    tools = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(tools)
    tools.CLI = cli
    return tools


def profile(exe, workspace, rustc):
    source = workspace / 'audio-profile-probe.rs'
    source.write_text('''#![allow(dead_code, unused_imports)]
#[path = "''' + (ROOT / 'crates/nekopack-runtime/src/engine_abi.rs').as_posix() + '''"] mod engine_abi;
fn main() {
 let b=std::fs::read(std::env::args().nth(1).unwrap()).unwrap();
 let p=engine_abi::probe_bgv(&b).unwrap().expect("native BGV ABI required");
 println!("prefix\\t{}",p.name_prefix);
 for c in p.channels { println!("channel\\t{}\\t{}\\t{}",c.channel,c.tag,c.voice_suppression_substring); }
}
''', encoding='utf8')
    binary = workspace / 'audio-profile-probe.exe'
    environment = os.environ.copy()
    environment.setdefault('RUSTUP_HOME', str(Path(os.environ['LOCALAPPDATA']) / 'RustGNU/rustup'))
    environment.setdefault('CARGO_HOME', str(Path(os.environ['LOCALAPPDATA']) / 'RustGNU/cargo'))
    subprocess.run([str(rustc), '--edition=2021', '-C', 'debuginfo=0', str(source), '-o', str(binary)], check=True, env=environment)
    rows = subprocess.check_output([str(binary), str(exe)], text=True).splitlines()
    return dict(name_prefix=rows[0].split('\t')[1], channels=[dict(channel=int(r[1]), tag=r[2], voice_suppression_substring=r[3]) for r in (line.split('\t') for line in rows[1:])])


def tone(ffmpeg, path, duration, frequency):
    subprocess.run([str(ffmpeg), '-v', 'error', '-y', '-f', 'lavfi', '-i', f'sine=frequency={frequency}:sample_rate=16000:duration={duration}', '-ac', '1', '-c:a', 'libvorbis', '-q:a', '0', str(path)], check=True)
    data = path.read_bytes()
    assert data.startswith(b'OggS')
    return data


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, default=DEFAULT_SOURCE)
    parser.add_argument('--output', type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument('--cli', type=Path, default=ROOT / 'target/debug/nekopack-cli.exe')
    parser.add_argument('--rustc', type=Path, default=Path(os.environ.get('LOCALAPPDATA', '')) / 'RustGNU/cargo/bin/rustc.exe')
    parser.add_argument('--ffmpeg', type=Path)
    parser.add_argument('--title-smoke', type=Path, default=ROOT / 'target/debug/examples/title_smoke.exe')
    parser.add_argument('--validate-route', action='store_true', help='requires newly built CLI with this BGV/system implementation; emits no pictures')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    workspace = output.parent
    tools = load_pack_tools(args.cli.resolve())
    exe = args.source / 'sorakoi.exe'
    abi = profile(exe, workspace, args.rustc)
    assert [c['channel'] for c in abi['channels']] == [2, 3]
    ffmpeg = args.ffmpeg
    if ffmpeg is None:
        import imageio_ffmpeg
        ffmpeg = Path(imageio_ffmpeg.get_ffmpeg_exe())
    loop = tone(ffmpeg, workspace / 'loop-tone.ogg', 1.0, 440)
    voice = tone(ffmpeg, workspace / 'voice-tone.ogg', 6.0, 660)
    click = tone(ffmpeg, workspace / 'ui-tone.ogg', .12, 880)
    packs = [str(args.source / 'system.dat'), str(args.source / 'script.dat')]
    start, start_meta = tools.extract(packs, 'script', 'start.txt')
    # Read only the pure startup script, never any scenario/title/recollection body.
    start_text = start.decode('cp932')
    assert start_text.count('.load titlelogo') == 1
    start_text = start_text.replace('.load titlelogo', '.system audio_probe_boot.nut\n.load title')
    entries = [('script', 'start.txt', start_text.encode('cp932')),
               ('script', 'titlelogo.txt', b'.load title\n'),
               ('script', 'title.txt', b'.title\n START\n.case 1\n.load story1\n.end\n'),
               ('system', 'audio_probe_boot.nut', b'Config.SysEffect=0;Config.TextSpeed=100;Config.SkipNoRead=1;Config.AutoSave=0;Config.Dialog.QSAVE=false;Config.Dialog.QLOAD=false;\n')]
    baseline = dict(MAS=67, BGM=75, VO=100, SE=71, ENV=63, BGV=55, SYS=57, MV=49)
    config = ''.join(f'Config.Vol{k}={v};Config.Mute{k}=0;\n' for k, v in baseline.items())
    config += 'Config.TextSpeed=100;Config.KeepVoice=1;Config.KeepIcon=1;Config.StereoVoice=0;Config.AutoSave=0;Config.Dialog.QSAVE=false;Config.Dialog.QLOAD=false;StoryRoute="AUDIO_PROBE";\n'
    for c, value in zip(abi['channels'], [73, 61]):
        config += f'Config.VoiceMute[{json.dumps(c["tag"])}]<-false;Config.VoiceVolume[{json.dumps(c["tag"])}]<-{value};\n'
    config += 'Config.VoiceMute.XX<-false;Config.VoiceVolume.XX<-100;RestoreVolume();\n'
    entries.append(('system', 'audio_probe_setup.nut', config.encode('ascii')))
    checks = {'audio_probe_banks.nut': 'if(var.ProbeG!=47 || var.ProbeL!=31 || var["@ProbeGF"]!=59 || var["@ProbeLF"]!=43)throw "four-bank isolation failed";',
              'audio_probe_reset.nut': 'if(var.ProbeG!=47 || var.ProbeL!=0 || var["@ProbeGF"]!=59 || var["@ProbeLF"]!=0)throw "reset bank contract failed";',
              'audio_probe_saved.nut': 'if(var.ProbeG!=47 || var.ProbeL!=0 || var["@ProbeGF"]!=59 || var["@ProbeLF"]!=0)throw "saved bank contract failed";\nif(Config.VolMAS!=67 || Config.VolBGV!=55 || Config.VolMV!=49 || Config.MuteMAS!=0)throw "saved config contract failed";',
              'audio_probe_mute_bgv.nut': 'Config.MuteBGV=1;RestoreVolume();',
              'audio_probe_mute_bgm.nut': 'Config.MuteBGV=0;Config.MuteBGM=1;RestoreVolume();',
              'audio_probe_mute_master.nut': 'Config.MuteBGM=0;Config.MuteMAS=1;RestoreVolume();',
              'audio_probe_restore.nut': 'Config.MuteMAS=0;Config.MuteBGM=0;Config.MuteBGV=0;RestoreVolume();'}
    entries.extend(('system', name, code.encode('ascii')) for name, code in checks.items())
    channels = abi['channels']
    bgv = [f'{c["tag"]}{abi["name_prefix"]}{i:03}' for i, c in enumerate(channels, 1)]
    cues = [f'PROBE_{c["voice_suppression_substring"]}_TONE' for c in channels]
    assert cues[0] != cues[1]
    entries += [('sound/bgm', 'PROBE_BGM.ogg', loop)]
    entries += [('sound/bgv', name + '.ogg', loop) for name in bgv]
    entries += [('voice', name + '.ogg', voice) for name in cues]
    entries += [('sound/se', name + '.ogg', click) for name in ['se_PAGE', 'se_CLICK', 'se_OK', 'se_SELECT']]
    lines = ['.var G700 ProbeG', '.var L700 ProbeL', '.var GF700 ProbeGF', '.var LF700 ProbeLF',
             '? ProbeG = 47', '? ProbeL = 31', '? @ProbeGF = 59', '? @ProbeLF = 43',
             '.system audio_probe_setup.nut', '.route AUDIO_PROBE', '.system audio_probe_banks.nut']
    pages = []
    def page(label, voice_cue=''):
        lines.extend([f':Probe:{voice_cue}:XX', 'probe ' + label])
        pages.append(dict(label=label, source='story1', line=len(lines), voice=voice_cue))
    def loops():
        lines.extend(['.bgm PROBE_BGM', f'.bgv {channels[0]["tag"]} 001', f'.bgv {channels[1]["tag"]} 002'])
    loops()
    page('baseline')
    page('suppress_first', cues[0])
    page('suppress_second', cues[1])
    lines.append('.system audio_probe_mute_bgv.nut'); page('mute_bgv')
    lines.append('.system audio_probe_mute_bgm.nut'); page('mute_bgm')
    lines.append('.system audio_probe_mute_master.nut'); page('mute_master')
    lines.extend(['.system audio_probe_restore.nut', '.reset', '.system audio_probe_reset.nut'])
    loops(); page('reset_checkpoint')
    lines.append('.system audio_probe_saved.nut'); page('saved_assertion')
    lines.extend(['.bgv', '.bgm']); page('stopped')
    entries.append(('script', 'story1.txt', ('\n'.join(lines)+'\n').encode('ascii')))
    for name in ['system.dat', 'sorakoi.exe']:
        shutil.copy2(args.source / name, output / name)
    patch = output / 'audio_probe.dat'
    tools.pack(patch, entries)
    subprocess.run([str(args.cli), 'inspect-archive', str(patch)], check=True, stdout=subprocess.DEVNULL)
    total = sum(p.stat().st_size for p in output.iterdir() if p.is_file())
    assert total < 40 * 1024 * 1024, total
    manifest = dict(version=1, synthetic=True, fixture_bytes=total, abi=abi, baseline=baseline,
                    effective={k:v*baseline['MAS']//100 for k,v in baseline.items() if k!='MAS'},
                    role_volumes=[73,61], voice_seconds=6, pages=pages, bgv=bgv, bgm='PROBE_BGM',
                    source_start=start_meta, original_scenario_media_copied=False,
                    files=[dict(name=p.name, bytes=p.stat().st_size, sha256=tools.sha(p)) for p in sorted(output.iterdir()) if p.suffix in ['.dat','.exe']],
                    desktop_validation=['PE ABI and independently recovered suppression constants', 'synthetic archive parse'],
                    device_validation='not run; scripts generated only',
                    deferred=['ordinary SKIP loop retention and NEXT/MegaSkip: runtime unit tests, not claimed as device coverage'])
    if args.validate_route:
        validation = workspace / 'audio-route-validation'
        command = [str(args.cli), 'preview-route', str(validation), str(len(pages)), '1', str(output/'system.dat'), str(patch), str(output/'sorakoi.exe')]
        result = subprocess.run(command, capture_output=True)
        (workspace/'audio-route-validation.log').write_bytes(result.stdout+result.stderr)
        if result.returncode:
            raise RuntimeError('desktop route validation failed; see '+str(workspace/'audio-route-validation.log'))
        for i, expected in enumerate(pages):
            actual = json.loads((validation/f'{i}.json').read_text(encoding='utf8'))
            assert (actual['source'],actual['line']) == (expected['source'],expected['line'])
        for obsolete in ['boundary.json', 'boundary.txt']:
            (validation/obsolete).unlink(missing_ok=True)
        manifest['desktop_validation'].append('all nine synthetic source positions and system assertions')
        title_result = subprocess.run([str(args.title_smoke), '--start', '--side', str(output/'system.dat'), str(patch), str(output/'sorakoi.exe')], capture_output=True, check=True)
        (workspace/'audio-title-validation.log').write_bytes(title_result.stdout+title_result.stderr)
        for marker in [b'title-ready', b'side-ready', b'kind=bgm', b'kind=bgv1', b'kind=bgv2', b'start-ready kind=page']:
            assert marker in title_result.stdout, marker
        manifest['desktop_validation'].append('native title/side menu and three real synthetic OGG host-effect payloads')
    (output/'manifest.json').write_text(json.dumps(manifest, ensure_ascii=False, indent=2)+'\n', encoding='utf8')
    print(json.dumps(dict(output=str(output), bytes=total, pages=len(pages), abi=abi, desktop_validation=manifest['desktop_validation']), ensure_ascii=False))

if __name__ == '__main__':
    main()
