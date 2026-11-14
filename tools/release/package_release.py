"""Package a pinned Android release and its complete dependency sources; no downloads."""
import argparse
import ast
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tempfile
import tomllib
import zipfile

ROOT = Path(__file__).resolve().parents[2]
FORBIDDEN_SUFFIXES = {'.dat', '.sav', '.nksv', '.pem', '.key', '.p12', '.pfx', '.jks', '.keystore'}
FORBIDDEN_PARTS = {'.git', '.local', 'games', 'samples', '__pycache__'}


def digest(data):
    return hashlib.sha256(data).hexdigest()


def file_digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def git(*args):
    # archive obeys core.autocrlf; force canonical Git bytes on every host.
    return subprocess.check_output(['git', '-C', str(ROOT), '-c', 'core.autocrlf=false', *args])


def check_path(name):
    path = PurePosixPath(name)
    lower = {part.lower() for part in path.parts}
    if path.is_absolute() or '..' in path.parts or lower & FORBIDDEN_PARTS:
        raise ValueError(f'Forbidden archive path: {name}')
    if path.suffix.lower() in FORBIDDEN_SUFFIXES:
        raise ValueError(f'Forbidden archive file: {name}')
    if path.suffix.lower() in {'.png', '.jpg', '.jpeg', '.webp', '.bmp'}:
        raise ValueError(f'Image requires explicit source review, refusing: {name}')
    if re.search(r'(^|[/_.-])(password|private[-_]?key|secrets?)([/_.-]|$)', name.lower()):
        raise ValueError(f'Potential secret path: {name}')


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf8')


def add_bytes(bundle, name, data):
    info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
    info.compress_type = zipfile.ZIP_DEFLATED
    info.external_attr = 0o100644 << 16
    bundle.writestr(info, data)


def package_source(commit, output, cargo_cache, ffmpeg_dir, source_check=None):
    """Read every project file from git archive, never from the working tree."""
    archive = git('archive', '--format=zip', commit)
    project = zipfile.ZipFile(io.BytesIO(archive))
    names = project.namelist()
    for name in names:
        check_path(name)
    input_check = None
    if source_check is not None:
        expected = json.loads(source_check.read_text('utf8'))
        for name, checksum in expected.items():
            if digest(project.read(name)) != checksum:
                raise ValueError(f'Build-source evidence mismatch against git archive: {name}')
        input_check = {'sha256': file_digest(source_check), 'matched_files': len(expected)}
    lock = tomllib.loads(project.read('Cargo.lock').decode('utf8'))
    packages = []
    for package in lock['package']:
        source = package.get('source', '')
        if source and not source.startswith('registry+'):
            raise ValueError(f'Unsupported external source: {source}')
        if not source:
            continue
        filename = f"{package['name']}-{package['version']}.crate"
        candidates = sorted(cargo_cache.glob('*/' + filename))
        matches = [path for path in candidates if file_digest(path) == package['checksum']]
        if not matches:
            raise ValueError(f'Missing Cargo.lock-verified source: {filename}')
        packages.append((matches[0], package['checksum']))

    ffmpeg_script = ast.parse(project.read('crates/nekopack-video/tools/build_ffmpeg.py'))
    constants = {node.targets[0].id: ast.literal_eval(node.value)
                 for node in ffmpeg_script.body if isinstance(node, ast.Assign)
                 and isinstance(node.targets[0], ast.Name)
                 and node.targets[0].id in {'VERSION', 'SHA256'}}
    ffmpeg_archive = ffmpeg_dir / f"ffmpeg-{constants['VERSION']}.tar.xz"
    if file_digest(ffmpeg_archive) != constants['SHA256']:
        raise ValueError('FFmpeg archive does not match pinned build script SHA256')
    ffmpeg_manifest = json.loads((ffmpeg_dir / 'android/build-manifest.json').read_text('utf8'))
    if (ffmpeg_manifest['version'], ffmpeg_manifest['sha256']) != (constants['VERSION'], constants['SHA256']):
        raise ValueError('FFmpeg build manifest does not match pinned source')
    dependency_files = [(path, 'dependency-sources/cargo/' + path.name, checksum)
                        for path, checksum in packages]
    for relative in (ffmpeg_archive.name, 'configure-android.txt', 'android/build-manifest.json'):
        path = ffmpeg_dir / relative
        dependency_files.append((path, 'dependency-sources/ffmpeg/' + relative, file_digest(path)))
    source_manifest = {
        'source_commit': commit,
        'build_source_input_check': input_check,
        'git_archive_sha256': digest(archive),
        'project_files': {name: digest(project.read(name)) for name in names if not name.endswith('/')},
        'dependency_files': {name: checksum for _, name, checksum in dependency_files},
        'cargo_registry_packages': len(packages),
        'ffmpeg_version': constants['VERSION'],
        'forbidden_path_check': 'passed; no local data, game/save/key files or raster images',
    }
    instructions = '''完整对应源码说明

项目文件来自 SOURCE-MANIFEST.json 所列提交的 git archive，保留 vendor 源码及许可证。
dependency-sources/cargo 包含 Cargo.lock 全部 registry 依赖的原始 .crate 源码包，
每个 SHA-256 都与该提交 Cargo.lock 的 checksum 一致。可解包查看、修改源码；
如需 Cargo 离线构建，将 .crate 放入对应 Cargo registry cache 并配置索引，或使用
cargo vendor 生成目录及其 source replacement 配置；本包不是预配置的离线工具链。
dependency-sources/ffmpeg 含官方完整 FFmpeg tar.xz、实际 configure 命令及构建清单。
其中绝对路径是原构建环境记录，应替换为本机路径；不要直接照搬路径。

构建指南：docs/RELEASING.md、platforms/android/build_probe.py --help。
需要 Python 3.11+、Rust/Cargo（aarch64-linux-android target）、Android NDK r27d、
Android SDK（platform/build-tools）、JDK 17、Kotlin 编译器库与 Windows C/C++ 工具链。
FFmpeg 另需 MSYS sh/coreutils、GNU make 与 C11 host compiler（原环境 Zig 0.14.1）；
将 FFmpeg tar.xz 放入项目 .local/ffmpeg/，执行
python crates/nekopack-video/tools/build_ffmpeg.py --ndk <NDK> --sh <sh.exe> --make <make.exe>
然后按发布指南执行 build_probe.py，使用自己的签名密钥及递增 versionCode。
Android SDK/NDK、编译器、签名私钥及游戏素材不包含在源码交付中。
FFmpeg 为 LGPL-2.1-or-later 共享库；可使用所附完整源码和 configure 参数重建、替换。
项目及其他第三方许可见 LICENSE、THIRD_PARTY_NOTICES.md 与各 vendor/依赖内许可。
'''
    with zipfile.ZipFile(output, 'w') as bundle:
        for info in project.infolist():
            bundle.writestr(info, project.read(info.filename))
        for path, name, _ in dependency_files:
            add_bytes(bundle, name, path.read_bytes())
        add_bytes(bundle, 'SOURCE-MANIFEST.json', (json.dumps(source_manifest, ensure_ascii=False, indent=2) + '\n').encode())
        add_bytes(bundle, 'BUILD-SOURCE.txt', instructions.encode())
    # Reopen the finished artifact: prove every archived project/dependency byte survived.
    with zipfile.ZipFile(output) as bundle:
        if bundle.testzip() is not None:
            raise ValueError('Source ZIP CRC validation failed')
        for name, checksum in {**source_manifest['project_files'], **source_manifest['dependency_files']}.items():
            check_path(name)
            if digest(bundle.read(name)) != checksum:
                raise ValueError(f'Source ZIP content mismatch: {name}')
    return source_manifest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--commit', required=True)
    parser.add_argument('--version', default='0.1.0-preview.2')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--apk', type=Path, default=ROOT / '.local/android-release/nekopack-arm64-preview.apk')
    parser.add_argument('--build', type=Path, default=ROOT / '.local/android-release/build.json')
    parser.add_argument('--verification', required=True, type=Path)
    parser.add_argument('--notes', type=Path, help='Optional UTF-8 release notes')
    parser.add_argument('--source-check', type=Path, default=ROOT / 'research/verification/save-continuity/source-git-sha256.json',
                        help='JSON mapping build input Git paths to SHA256; compared with commit archive')
    parser.add_argument('--video', type=Path, default=ROOT / '.local/promo-v3/nekopack-showcase-carefree.mp4')
    parser.add_argument('--credits', type=Path, default=ROOT / '.local/promo-v3/music/CREDITS.txt')
    parser.add_argument('--upload-notes', type=Path, default=ROOT / 'tools/promo/upload-notes-v3.txt')
    parser.add_argument('--cargo-cache', type=Path, default=Path(os.environ['LOCALAPPDATA']) / 'RustGNU/cargo/registry/cache')
    parser.add_argument('--ffmpeg-dir', type=Path, default=ROOT / '.local/ffmpeg')
    args = parser.parse_args()
    commit = git('rev-parse', '--verify', args.commit + '^{commit}').decode().strip()
    if not re.fullmatch(r'[0-9A-Za-z][0-9A-Za-z.+-]*', args.version):
        raise ValueError('Invalid version filename component')
    output = (args.output or ROOT / '.local/releases' / args.version).resolve()
    if output.exists():
        raise FileExistsError(f'Refusing to merge/overwrite existing output: {output}')
    apk_sha = file_digest(args.apk)
    build = json.loads(args.build.read_text('utf8'))
    verification = json.loads(args.verification.read_text('utf8'))
    if (build['variant'], build['signed'], build['version_name']) != ('release', True, args.version):
        raise ValueError('Build is not a signed release of the requested version')
    if build['sha256'][args.apk.name] != apk_sha:
        raise ValueError('Build APK SHA256 mismatch')
    if (verification['version'], verification['apk_sha256']) != (args.version, apk_sha):
        raise ValueError('Verification does not describe this APK/version')
    for report in (build, verification):
        if 'source_commit' in report and report['source_commit'] != commit:
            raise ValueError('Report source_commit mismatch')
    output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='release-', dir=output.parent) as temporary:
        stage = Path(temporary)
        source_name = f'NekoPack-source-{commit[:12]}.zip'
        source = package_source(commit, stage / source_name, args.cargo_cache, args.ffmpeg_dir, args.source_check)
        archive = zipfile.ZipFile(stage / source_name)
        for name, member in [('LICENSE-GPL-3.0.txt', 'LICENSE'), ('THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_NOTICES.md')]:
            (stage / name).write_bytes(archive.read(member))
        archive.close()
        for path, name in [(args.apk, f'NekoPack-{args.version}-arm64.apk'),
                           (args.build, 'BUILD.json'), (args.verification, 'APK-VERIFICATION.json'),
                           (args.source_check, 'BUILD-SOURCE-INPUTS.json'),
                           (args.video, 'NekoPack-introduction-Carefree-1080p.mp4'),
                           (args.credits, 'MUSIC-CREDITS.txt'), (args.upload_notes, 'VIDEO-PUBLISHING-NOTES.txt')]:
            shutil.copyfile(path, stage / name)
        default_notes = (f'NekoPack {args.version} Android ARM64 预览版\n\n'
                         f'对应源码提交：{commit}\nAPK SHA-256：{apk_sha}\n\n'
                         '签名构建参数见 BUILD.json；本次 APK 的实际验证范围与未覆盖项见 APK-VERIFICATION.json。\n'
                         '完整对应源码 ZIP 含 Cargo.lock 锁定依赖和 FFmpeg 完整源码；构建说明见包内 BUILD-SOURCE.txt。\n'
                         '不附游戏资源、存档或签名私钥。预览版可能仍有兼容性问题；历史调试回归不代表本包已全量复测。\n'
                         '音乐署名见 MUSIC-CREDITS.txt。SHA256SUMS.txt 校验本目录各交付文件。\n')
        (stage / 'README-RELEASE.txt').write_text(args.notes.read_text('utf8') if args.notes else default_notes, encoding='utf8')
        files = {path.name: {'sha256': file_digest(path), 'bytes': path.stat().st_size}
                 for path in sorted(stage.iterdir())}
        write_json(stage / 'manifest.json', {'version': args.version, 'source_commit': commit,
                   'source_validation': source, 'files': files,
                   'report_commit_binding': {'build': build.get('source_commit'), 'verification': verification.get('source_commit')},
                   'apk_signature_and_device_tests': 'External evidence only; see APK-VERIFICATION.json'})
        (stage / 'SHA256SUMS.txt').write_text(''.join(f'{file_digest(path)}  {path.name}\n'
            for path in sorted(stage.iterdir())), encoding='utf8')
        for line in (stage / 'SHA256SUMS.txt').read_text('utf8').splitlines():
            checksum, name = line.split('  ', 1)
            if file_digest(stage / name) != checksum:
                raise ValueError(f'Delivery hash mismatch: {name}')
        stage.rename(output)
    print(output)


if __name__ == '__main__':
    main()
