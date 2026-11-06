"""Install the validation APK; compare Android logs to desktop headless evidence."""
import argparse
import hashlib
import io
import json
from pathlib import Path
import subprocess
import tarfile
import time
import uuid

ROOT=Path(__file__).resolve().parents[2]
PACKAGE='org.nekopack.probe'

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--adb',type=Path,required=True)
    parser.add_argument('--serial',required=True)
    parser.add_argument('--fixtures',type=Path,required=True)
    parser.add_argument('--reference',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    args=parser.parse_args()
    prefix=[str(args.adb),'-s',args.serial]
    def adb(*commands,check=True):
        return subprocess.run(prefix+list(commands),capture_output=True,check=check,timeout=90)
    def shell(*commands): return adb('shell',*commands).stdout.decode('utf8').strip()
    def private(*commands): return shell('run-as',PACKAGE,*commands)
    build=json.loads((ROOT/'.local/android-probe/build.json').read_text())
    apk=ROOT/'.local/android-probe/nekopack-arm64-probe.apk'
    if hashlib.sha256(apk.read_bytes()).hexdigest()!=build['sha256'][apk.name]: raise RuntimeError('APK changed after build inspection')
    print(adb('install','-r',str(apk)).stdout.decode(),flush=True)
    nonce='probe_'+uuid.uuid4().hex
    remote='/data/local/tmp/'+nonce
    app='files/'+nonce
    shell('mkdir','-p',remote)
    adb('push',str(args.fixtures.resolve()),remote+'/fixtures')
    shell('chmod','-R','a+rX',remote)
    private('mkdir','-p',app)
    private('cp','-R',remote+'/fixtures',app+'/fixtures')
    # Verify the actual app-private copies, without storing game data in the public report.
    stream=adb('exec-out','run-as',PACKAGE,'tar','-C',app+'/fixtures','-cf','-','.').stdout
    with tarfile.open(fileobj=io.BytesIO(stream)) as archive:
        copied={member.name.removeprefix('./'):archive.extractfile(member).read() for member in archive if member.isfile()}
    local={p.relative_to(args.fixtures).as_posix():p.read_bytes() for p in args.fixtures.rglob('*') if p.is_file()}
    if copied!=local: raise RuntimeError('device fixture copy mismatch')
    # Remove only this run's staging directory, after its private copy was verified.
    shell('rm','-r',remote)
    shell('am','force-stop',PACKAGE)
    shell('am','start','-n',PACKAGE+'/.MainActivity','--es','run_id',nonce)
    deadline=time.monotonic()+90
    report=''
    while time.monotonic()<deadline:
        result=adb('exec-out','run-as',PACKAGE,'cat',app+'/report.txt',check=False)
        candidate=result.stdout.decode('utf8')
        # Older adbd exec-out can return 0 even when cat failed; never trust exit code alone.
        if candidate.startswith('RUN\t'+nonce+'\n'):
            report=candidate
            break
        time.sleep(1)
    if not report:
        raise RuntimeError('host did not produce a report; inspect adb logcat for this package')
    print(report,flush=True)
    if not report.startswith('RUN\t'+nonce+'\n') or 'FAIL\t' in report or 'PASS\tall\tlive_bytes=0' not in report or 'RUST\taarch64\tpointer_bits=64' not in report:
        raise RuntimeError('host validation failed or result is stale')
    sweeps=[line.split('\t') for line in report.splitlines() if line.startswith('PASS\tallocation-sweep\t')]
    if len(sweeps)!=3 or [row[2] for row in sweeps]!=['0','1','2'] or not all(int(row[3])>0 for row in sweeps):
        raise RuntimeError('allocation sweeps were not completed')
    if 'PASS\tffi-and-budget-destroy\t32 cycles\tlive_bytes=0' not in report:
        raise RuntimeError('FFI/destruction cycles were not completed')
    reference=json.loads(args.reference.read_text())
    games=[]
    for i,expected in enumerate(reference['games']):
        text=adb('exec-out','run-as',PACKAGE,'cat',app+f'/results/game-{i}.txt').stdout.decode('utf8')
        lines=text.splitlines()
        boundary=[line.split('\t')[1:] for line in lines if line.startswith('story-boundary\t')]
        slots=[line for line in lines if line.startswith('slot\t')]
        state=hashlib.sha256('\n'.join(slots).encode()).hexdigest()
        completed=[line.split('\t')[1:] for line in lines if line.startswith('complete\t')]
        digest=hashlib.sha256()
        for name,data in sorted((n.split('/',1)[1],b) for n,b in local.items() if n.startswith(str(i)+'/')):
            encoded=name.encode();digest.update(len(encoded).to_bytes(8,'little'));digest.update(encoded)
            digest.update(len(data).to_bytes(8,'little'));digest.update(data)
        fixture=digest.hexdigest()
        passed=('menu-result\t1' in lines and any(s.startswith('after-menu\tstory1\t') for s in lines)
                and boundary==expected['boundary'] and state==expected['state_sha256']
                and fixture==expected['fixture_sha256'] and completed==expected['complete']
                and f'PASS\ttitle\t{i}\tinvalid-choice-rejected' in report)
        if not passed: raise RuntimeError(f'Android/desktop mismatch: {expected["game"]}')
        games.append(dict(game=expected['game'],passed=passed,boundary=boundary,state_sha256=state,fixture_sha256=fixture,complete=completed,invalid_choice_rejected=True))
    device={key:shell('getprop',key) for key in ['ro.product.cpu.abilist','ro.dalvik.vm.native.bridge','ro.build.version.sdk']}
    device['kernel_machine']=shell('uname','-m')
    device['page_size']=next(line.split('\t')[1] for line in report.splitlines() if line.startswith('PAGE_SIZE\t'))
    package=shell('dumpsys','package',PACKAGE)
    device['package_abi']=[s.strip() for s in package.splitlines() if 'primaryCpuAbi=' in s or 'secondaryCpuAbi=' in s]
    result=dict(scope='ARM64-only Kotlin/JNI validation; headless title paths, no game presentation',
                execution='translated ARM64' if device['kernel_machine']=='x86_64' else 'device; inspect properties',
                device=device,build=build,fixtures_verified_byte_for_byte=True,host_report=report,games=games)
    args.output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
    print('Android validation and four desktop comparisons passed')

if __name__=='__main__': main()
