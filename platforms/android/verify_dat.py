"""Verify a completed real SAF run against raw DAT and the independent fixture baseline.
The picker is exercised separately; this tool never grants URI permission.
"""
import argparse, hashlib, json, subprocess
from pathlib import Path

def main():
 p=argparse.ArgumentParser()
 for flag in ['adb','cli','raw','fixtures','output']:p.add_argument('--'+flag,type=Path,required=True)
 for flag in ['serial','run-id','device-root']:p.add_argument('--'+flag,required=True)
 a=p.parse_args();prefix=[str(a.adb),'-s',a.serial]
 def adb(*args):return subprocess.check_output(prefix+list(args),timeout=90)
 def private(name):return adb('exec-out','run-as','org.nekopack.probe','cat',f'files/{a.run_id}/{name}')
 build=json.loads(Path('.local/android-probe/build.json').read_text())
 package_path=adb('shell','pm','path','org.nekopack.probe').decode().strip().removeprefix('package:')
 installed=adb('shell','sha256sum',package_path).decode().split()[0]
 if installed!=build['sha256']['nekopack-arm64-probe.apk']:raise RuntimeError('installed APK differs from build evidence')
 report=private('report.txt').decode('utf8')
 if not report.startswith('RUN\t'+a.run_id+'\n') or 'FAIL\t' in report or 'SAF\tcontent://' not in report:raise RuntimeError('missing, stale or failed SAF report')
 games=[]
 for i in range(4):
  archives=[]
  for f in sorted((a.raw/str(i)).glob('*.dat')):
   digest=hashlib.sha256(f.read_bytes()).hexdigest()
   remote=adb('shell','sha256sum',a.device_root+f'/{i}/{f.name}').decode().split()[0]
   if digest!=remote:raise RuntimeError('device raw archive changed')
   archives.append(dict(name=f.name,bytes=f.stat().st_size,sha256=digest))
  raw=subprocess.check_output([str(a.cli),'run-dat-title',str(a.raw/str(i)),'1'])
  fixture=subprocess.check_output([str(a.cli),'run-title',str(a.fixtures/str(i)),'1'])
  device=private(f'game-{i}-1.txt')
  if raw!=fixture or raw!=device:raise RuntimeError(f'title trace mismatch {i}')
  lines=raw.decode('utf8').splitlines()
  if 'menu-result\t1' not in lines or not any(l.startswith('story-boundary\t') for l in lines):raise RuntimeError('missing execution boundary')
  invalid=private(f'game-{i}-999.txt').decode('utf8')
  if not invalid.startswith('FAIL\t') or 'requested input does not identify an enabled spot' not in invalid:raise RuntimeError('invalid input was not rejected')
  if f'PASS\tdat\t{i}\t{len(archives)} archives' not in report:raise RuntimeError('missing host result')
  games.append(dict(index=i,archives=archives,trace_sha256=hashlib.sha256(raw).hexdigest(),boundary=[l for l in lines if l.startswith('story-boundary\t')],complete=[l for l in lines if l.startswith('complete\t')],state_sha256=hashlib.sha256('\n'.join(l for l in lines if l.startswith('slot\t')).encode()).hexdigest(),raw_fixture_android_byte_equal=True,invalid_choice_rejected=True))
 device={k:adb('shell','getprop',k).decode().strip() for k in ['ro.product.cpu.abilist','ro.dalvik.vm.native.bridge','ro.build.version.sdk']}
 device['kernel_machine']=adb('shell','uname','-m').decode().strip()
 result=dict(scope='SAF original DAT title-to-first-story invocation; headless, no rendering or audio',execution='translated ARM64' if device['kernel_machine']=='x86_64' else 'inspect device properties',device=device,host_report=report,games=games,build=build)
 a.output.write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf8')
 print('PASS: four raw DAT / fixture / Android traces byte-identical; device archives SHA-256 verified')
if __name__=='__main__':main()
