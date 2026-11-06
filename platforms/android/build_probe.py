"""Build an ARM64-only validation APK with local NDK/SDK/JDK/Kotlin tools. No game assets."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import zipfile

ROOT = Path(__file__).resolve().parents[2]

def run(*args, env=None):
    subprocess.run([str(a) for a in args], cwd=ROOT, env=env, check=True)

def main():
    parser=argparse.ArgumentParser()
    for name in ['ndk','sdk','jdk','kotlin_lib','cargo']:
        parser.add_argument('--'+name.replace('_','-'),type=Path,required=True)
    args=parser.parse_args()
    out=ROOT/'.local/android-probe'
    out.mkdir(parents=True,exist_ok=True)
    tools=args.ndk/'toolchains/llvm/prebuilt/windows-x86_64'
    target='aarch64-linux-android'
    env=os.environ.copy()
    env.pop('RUSTFLAGS',None)
    env.pop('CARGO_ENCODED_RUSTFLAGS',None)
    env.update(CXX=str(tools/'bin/clang++.exe'),AR=str(tools/'bin/llvm-ar.exe'),
               NEKO_CXX_ARGS='--target=aarch64-linux-android23',NEKO_AR_ARGS='',
               CARGO_TARGET_DIR=str(ROOT/'target'),
               CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=str(tools/'bin/clang.exe'),
               CARGO_TARGET_AARCH64_LINUX_ANDROID_RUSTFLAGS='-C link-arg=--target=aarch64-linux-android23 -C link-arg=-Wl,-z,max-page-size=16384')
    run(args.cargo,'build','-p','nekopack-android-probe','--release','--target',target,env=env)
    native=ROOT/'target'/target/'release/libnekopack_probe.so'
    shim=out/'libnekopack_host.so'
    cpp=tools/'sysroot/usr/lib'/target/'libc++_shared.so'
    run(tools/'bin/clang++.exe','--target=aarch64-linux-android23','-shared','-fPIC','-O1',
        '-Wl,-z,max-page-size=16384','-Wl,--no-undefined','-Wl,-soname,libnekopack_host.so',
        ROOT/'platforms/android/jni.cpp','-L'+str(native.parent),'-lnekopack_probe','-o',shim)
    elf={}
    for path in [native,shim,cpp]:
        result=subprocess.check_output([str(tools/'bin/llvm-readelf.exe'),'-h','-l','-d',str(path)],text=True)
        if 'AArch64' not in result: raise RuntimeError(f'not AArch64: {path}')
        segments=[line for line in result.splitlines() if line.strip().startswith('LOAD ')]
        if not segments or any(int(line.split()[-1],16)<16384 for line in segments):
            raise RuntimeError(f'ELF load alignment is below 16 KiB: {path}')
        elf[path.name]=result
    if '[libc++_shared.so]' not in elf[native.name] or '[libnekopack_probe.so]' not in elf[shim.name]:
        raise RuntimeError('missing expected runtime dependency')
    java=args.jdk/'bin/java.exe'
    android=args.sdk/'platforms/android-35/android.jar'
    build=args.sdk/'build-tools/35.0.0'
    kotlin=args.kotlin_lib/'kotlin-stdlib-1.9.22.jar'
    classes=out/'host.jar'
    run(java,'-cp',str(args.kotlin_lib/'*'),'org.jetbrains.kotlin.cli.jvm.K2JVMCompiler',
        '-no-stdlib','-no-reflect','-classpath',os.pathsep.join(map(str,[android,kotlin])),
        '-jvm-target','1.8','-d',classes,ROOT/'platforms/android/src/MainActivity.kt')
    dex=out/'dex'
    dex.mkdir(exist_ok=True)
    run(java,'-cp',build/'lib/d8.jar','com.android.tools.r8.D8','--min-api','23',
        '--lib',android,'--output',dex,classes,kotlin)
    unsigned=out/'unsigned.apk'
    run(build/'aapt2.exe','link','-o',unsigned,'--manifest',ROOT/'platforms/android/AndroidManifest.xml','-I',android)
    with zipfile.ZipFile(unsigned,'a',compression=zipfile.ZIP_DEFLATED) as apk:
        apk.write(dex/'classes.dex','classes.dex')
        for license_file in sorted((ROOT/'crates/nekopack-runtime/licenses').iterdir()):
            apk.write(license_file,'assets/licenses/'+license_file.name)
        for path in [native,shim,cpp]: apk.write(path,'lib/arm64-v8a/'+path.name)
    aligned=out/'aligned.apk'
    run(build/'zipalign.exe','-f','-P','16','4',unsigned,aligned)
    key=out/'debug.keystore'
    if not key.exists():
        run(args.jdk/'bin/keytool.exe','-genkeypair','-keystore',key,'-storepass','android',
            '-keypass','android','-alias','androiddebugkey','-keyalg','RSA','-keysize','2048',
            '-validity','3650','-dname','CN=NekoPack Local Validation')
    apk=out/'nekopack-arm64-probe.apk'
    run(java,'-jar',build/'lib/apksigner.jar','sign','--ks',key,'--ks-pass','pass:android',
        '--key-pass','pass:android','--out',apk,aligned)
    run(java,'-jar',build/'lib/apksigner.jar','verify',apk)
    (out/'build.json').write_text(json.dumps({
        'ndk':(args.ndk/'source.properties').read_text(), 'target':target,'api':23,
        'sha256':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [native,shim,cpp,apk]},
        'elf':elf,
    },indent=2)+'\n')
    print(apk)

if __name__=='__main__': main()
