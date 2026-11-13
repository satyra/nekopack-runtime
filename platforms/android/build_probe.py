"""Build separate ARM64 validation or release-preview APKs. No game assets."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import shutil
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[2]
ANDROID = '{http://schemas.android.com/apk/res/android}'


def manifest_for_build(variant, version_code, version_name, destination):
    ET.register_namespace('android', ANDROID[1:-1])
    tree = ET.parse(ROOT/'platforms/android/AndroidManifest.xml')
    root = tree.getroot()
    root.set(ANDROID+'versionCode', str(version_code))
    root.set(ANDROID+'versionName', version_name)
    app = root.find('application')
    app.set(ANDROID+'debuggable', str(variant == 'debug').lower())
    main = next(a for a in app.findall('activity') if a.get(ANDROID+'name') == '.MainActivity')
    main.set(ANDROID+'exported', str(variant == 'debug').lower())
    tree.write(destination, encoding='utf-8', xml_declaration=True)

def run(*args, env=None):
    subprocess.run([str(a) for a in args], cwd=ROOT, env=env, check=True)

def main():
    parser=argparse.ArgumentParser()
    for name in ['ndk','sdk','jdk','kotlin_lib','cargo']:
        parser.add_argument('--'+name.replace('_','-'),type=Path,required=True)
    parser.add_argument('--ffmpeg-prefix', type=Path, default=ROOT/'.local/ffmpeg/android', help='Minimal shared FFmpeg build from crates/nekopack-video/tools/build_ffmpeg.py')
    parser.add_argument('--variant', choices=['debug','release'], default='debug')
    parser.add_argument('--version-code', type=int, default=1)
    parser.add_argument('--version-name', default='0.1.0-preview.1')
    parser.add_argument('--keystore', type=Path)
    parser.add_argument('--key-alias')
    parser.add_argument('--password-file', type=Path, help='UTF-8 file containing the keystore/key password; never logged')
    parser.add_argument('--unsigned', action='store_true', help='Release inspection artifact only; cannot be installed')
    args=parser.parse_args()
    if args.version_code <= 0 or not args.version_name.strip():
        parser.error('version-code must be positive and version-name must be nonempty')
    if args.variant == 'release' and not args.unsigned and not all([args.keystore,args.key_alias,args.password_file]):
        parser.error('release signing requires --keystore, --key-alias and --password-file, or explicitly --unsigned')
    if args.variant == 'debug' and (args.unsigned or args.keystore or args.key_alias or args.password_file):
        parser.error('custom signing and --unsigned are release options')
    out=ROOT/('.local/android-probe' if args.variant == 'debug' else '.local/android-release')
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
    ffmpeg_prefix=args.ffmpeg_prefix.resolve()
    ffmpeg_libraries=[ffmpeg_prefix/'lib'/('lib'+name+'.so') for name in ['avformat','avcodec','avutil','swscale','swresample']]
    for library in ffmpeg_libraries:
        if not library.is_file(): raise RuntimeError(f'missing FFmpeg shared library: {library}; build the pinned video dependency first')
    env['NEKO_FFMPEG_DIR']=str(ffmpeg_prefix)
    run(args.cargo,'build','-p','nekopack-android-probe','--release','--target',target,env=env)
    native=ROOT/'target'/target/'release/libnekopack_probe.so'
    shim=out/'libnekopack_host.so'
    cpp=tools/'sysroot/usr/lib'/target/'libc++_shared.so'
    run(tools/'bin/clang++.exe','--target=aarch64-linux-android23','-shared','-fPIC','-O1',
        '-Wl,-z,max-page-size=16384','-Wl,--no-undefined','-Wl,-soname,libnekopack_host.so',
        ROOT/'platforms/android/jni.cpp','-L'+str(native.parent),'-lnekopack_probe','-o',shim)
    elf={}
    for path in [native,shim,cpp,*ffmpeg_libraries]:
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
        '-jvm-target','1.8','-d',classes,*sorted((ROOT/'platforms/android/src').glob('*.kt')))
    dex=out/'dex'
    dex.mkdir(exist_ok=True)
    run(java,'-cp',build/'lib/d8.jar','com.android.tools.r8.D8','--min-api','23',
        '--lib',android,'--output',dex,classes,kotlin)
    unsigned=out/'unsigned.apk'
    resources=out/'resources.zip'
    manifest=out/'AndroidManifest.xml'
    manifest_for_build(args.variant,args.version_code,args.version_name,manifest)
    run(build/'aapt2.exe','compile','--dir',ROOT/'platforms/android/res','-o',resources)
    run(build/'aapt2.exe','link','-o',unsigned,'--manifest',manifest,'-I',android,resources)
    with zipfile.ZipFile(unsigned,'a',compression=zipfile.ZIP_DEFLATED) as apk:
        apk.write(dex/'classes.dex','classes.dex')
        license_root=ROOT/'platforms/android/licenses'
        if not (license_root/'index.json').is_file():
            raise RuntimeError('missing Android third-party license snapshot/index.json')
        for license_file in sorted(license_root.rglob('*')):
            if license_file.is_file():
                apk.write(license_file,'assets/licenses/'+license_file.relative_to(license_root).as_posix())
        for path in [native,shim,cpp,*ffmpeg_libraries]: apk.write(path,'lib/arm64-v8a/'+path.name)
    aligned=out/'aligned.apk'
    run(build/'zipalign.exe','-f','-P','16','4',unsigned,aligned)
    if args.variant == 'debug':
        key=out/'debug.keystore'
        if not key.exists():
            run(args.jdk/'bin/keytool.exe','-genkeypair','-keystore',key,'-storepass','android',
                '-keypass','android','-alias','androiddebugkey','-keyalg','RSA','-keysize','2048',
                '-validity','3650','-dname','CN=NekoPack Local Validation')
        apk=out/'nekopack-arm64-probe.apk'
        signing=['--ks',key,'--ks-key-alias','androiddebugkey','--ks-pass','pass:android','--key-pass','pass:android']
    else:
        apk=out/('nekopack-arm64-preview-unsigned.apk' if args.unsigned else 'nekopack-arm64-preview.apk')
        signing=['--ks',args.keystore,'--ks-key-alias',args.key_alias,
                 '--ks-pass','file:'+str(args.password_file)]
    if args.unsigned:
        shutil.copyfile(aligned,apk)
    else:
        run(java,'-jar',build/'lib/apksigner.jar','sign',*signing,'--out',apk,aligned)
        run(java,'-jar',build/'lib/apksigner.jar','verify',apk)
    (out/'build.json').write_text(json.dumps({
        'ndk':(args.ndk/'source.properties').read_text(), 'target':target,'api':23,
        'variant':args.variant,'signed':not args.unsigned,'version_code':args.version_code,'version_name':args.version_name,
        'sha256':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [native,shim,cpp,*ffmpeg_libraries,apk]},
        'elf':elf,
    },indent=2)+'\n')
    print(apk)

if __name__=='__main__': main()
