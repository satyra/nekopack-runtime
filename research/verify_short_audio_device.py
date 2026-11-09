"""Compile the real short-audio adapter into a standalone Android framework probe.

Uses a raw Ogg extracted independently from DAT. This is not the SAF/ARM64 VM test.
"""
import argparse,hashlib,json,subprocess,sys
from pathlib import Path
p=argparse.ArgumentParser()
for name in ('sdk','jdk','kotlin-lib','games','reference-root','output'):p.add_argument('--'+name,type=Path,required=True)
p.add_argument('--serial',required=True)
a=p.parse_args();root=Path('.local/audio-batch30');root.mkdir(exist_ok=True)
sys.path.insert(0,str(a.reference_root));from portkit.neko import Archive
sources=json.loads(Path('research/metadata-verification.json').read_text(encoding='utf8'))['games'][0]['sources']
pack=Archive(a.games/next(s['file'] for s in sources if s['file'].endswith('/sound.dat')))
raw=pack.read('sound/se/se_024.ogg');digest=hashlib.sha256(raw).hexdigest()
assert digest==json.loads(Path('research/se-batch25-audio-reference.json').read_text())['games'][0]['se/se_024']
(root/'se024.ogg').write_bytes(raw)
java=a.jdk/'bin/java.exe';android=a.sdk/'platforms/android-35/android.jar';stdlib=a.kotlin_lib/'kotlin-stdlib-1.9.22.jar'
def run(args):return subprocess.check_output(list(map(str,args)),stderr=subprocess.STDOUT,timeout=120)
adapter=Path('platforms/android/src/ShortAudio.kt');probe=Path('platforms/android/tests/ShortAudioProbe.kt')
run([java,'-cp',str(a.kotlin_lib/'*'),'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler','-no-stdlib','-no-reflect','-classpath',f'{android};{stdlib}','-jvm-target','1.8','-d',root/'probe.jar',adapter,probe])
run([java,'-cp',a.sdk/'build-tools/35.0.0/lib/d8.jar','com.android.tools.r8.D8','--min-api','23','--lib',android,'--output',root/'probe.zip',root/'probe.jar',stdlib])
adb=[a.sdk/'platform-tools/adb.exe','-s',a.serial]
run(adb+['push',root/'probe.zip',root/'se024.ogg','/data/local/tmp/'])
result=run(adb+['shell','env','CLASSPATH=/data/local/tmp/probe.zip','app_process','/system/bin','ShortAudioProbeKt','/data/local/tmp/se024.ogg'])
evidence=json.loads(next(line for line in result.splitlines() if line.startswith(b'{')))
assert evidence['rate']==44100 and evidence['channels']==2 and evidence['frames']==16384
assert [c['case'] for c in evidence['cases']]==['complete','pause_resume','cancel_playing','cancel_prepared']
for case in evidence['cases'][:2]:
    assert case['completion_callbacks']==1 and case['heads'][0]==0 and case['heads'][-1]==evidence['frames']
    assert case['heads']==sorted(case['heads']) and any(0<h<evidence['frames'] for h in case['heads'])
assert 0<evidence['cases'][1]['paused_head']<evidence['frames']
assert all(c['completion_callbacks']==0 for c in evidence['cases'][2:])
report=dict(scope='Standalone Android framework adapter probe via app_process; no Rust VM, not ARM64 app validation or acoustic comparison',machine=run(adb+['shell','uname','-m']).decode().strip(),resource='sound/se/se_024.ogg',resource_sha256=digest,source_sha256_lf_utf8={str(f):hashlib.sha256(f.read_text(encoding='utf8').encode()).hexdigest() for f in (adapter,probe)},evidence=evidence)
a.output.write_text(json.dumps(report,indent=2)+'\n',encoding='utf8')
print('PASS short PCM head, pause/resume, cancel-playing and cancel-prepared')
