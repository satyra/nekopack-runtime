"""Build pinned LGPL FFmpeg shared libraries for the common ASF decoder.

Requires Python, MSYS sh/coreutils, GNU make, a C11 host compiler and Android NDK.
Only official FFmpeg source is downloaded; no game executable or media is read.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shlex
import subprocess
import tarfile
import urllib.request

VERSION = '8.0.1'
SHA256 = '05ee0b03119b45c0bdb4df654b96802e909e0a752f72e4fe3794f487229e5a41'
ROOT = Path(__file__).resolve().parents[3]


def msys(path):
    value = Path(path).resolve().as_posix()
    return '/' + value[0].lower() + value[2:] if len(value) > 1 and value[1] == ':' else value


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--ndk', type=Path, required=True)
    parser.add_argument('--sh', type=Path, required=True)
    parser.add_argument('--make', type=Path, required=True)
    parser.add_argument('--host-cc', default=msys(ROOT / '.local/toolchain/zig-x86_64-windows-0.14.1/zig.exe') + ' cc',
                        help='C11 host compiler command in MSYS syntax; default: local Zig cc')
    parser.add_argument('--jobs', type=int, default=8)
    parser.add_argument('--output', type=Path, default=ROOT / '.local/ffmpeg/android')
    args = parser.parse_args()
    cache = ROOT / '.local/ffmpeg'
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / f'ffmpeg-{VERSION}.tar.xz'
    source_url = f'https://ffmpeg.org/releases/ffmpeg-{VERSION}.tar.xz'
    if not archive.exists():
        urllib.request.urlretrieve(source_url, archive)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != SHA256:
        raise RuntimeError('FFmpeg source SHA256 mismatch')
    source = cache / f'ffmpeg-{VERSION}'
    if not source.exists():
        with tarfile.open(archive) as bundle:
            bundle.extractall(cache, filter='data')
    build = cache / 'build-android'
    build.mkdir(exist_ok=True)
    output = args.output.resolve()
    tools = args.ndk.resolve() / 'toolchains/llvm/prebuilt/windows-x86_64/bin'
    options = [
        '--prefix=' + msys(output), '--host-cc=' + args.host_cc,
        '--target-os=android', '--arch=aarch64', '--enable-cross-compile',
        '--cc=' + msys(tools / 'clang.exe') + ' --target=aarch64-linux-android23',
        '--cxx=' + msys(tools / 'clang++.exe') + ' --target=aarch64-linux-android23',
        '--ar=' + msys(tools / 'llvm-ar.exe'),
        '--ranlib=' + msys(tools / 'llvm-ranlib.exe'),
        '--strip=' + msys(tools / 'llvm-strip.exe'),
        '--nm=' + msys(tools / 'llvm-nm.exe'),
        '--extra-ldflags=-Wl,-z,max-page-size=16384',
        '--disable-everything', '--disable-autodetect', '--disable-programs',
        '--disable-doc', '--disable-debug', '--disable-network',
        '--disable-avdevice', '--disable-avfilter', '--disable-static',
        '--enable-shared', '--enable-pic', '--enable-avformat',
        '--enable-avcodec', '--enable-avutil', '--enable-swscale',
        '--enable-swresample', '--enable-demuxer=asf',
        '--enable-decoder=wmv1,wmv2,wmv3,vc1,wmav1,wmav2,wmapro,wmalossless',
        '--enable-parser=vc1',
    ]
    env = os.environ.copy()
    env['PATH'] = os.pathsep.join([str(args.sh.resolve().parent),
                                  str(args.make.resolve().parent), str(tools), env['PATH']])
    configure = [msys(args.sh), msys(source / 'configure'), *options]
    command = ' '.join(shlex.quote(value) for value in configure)
    (cache / 'configure-android.txt').write_text(command + '\n', encoding='utf8')

    def run(command):
        subprocess.run([str(args.sh), '-c', command], cwd=build, env=env, check=True)

    run(command)
    run(shlex.quote(msys(args.make)) + f' -j{args.jobs}')
    run(shlex.quote(msys(args.make)) + ' install')
    manifest = dict(version=VERSION, source=source_url, sha256=SHA256, configure=options,
                    license='LGPL-2.1-or-later',
                    libraries=[path.name for path in (output / 'lib').glob('*.so')])
    (output / 'build-manifest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf8')
    print(output)


if __name__ == '__main__':
    main()
