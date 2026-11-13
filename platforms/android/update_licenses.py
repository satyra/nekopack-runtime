"""Rebuild the offline Android notice snapshot (Python 3.11+).

Uses checksum-verified Cargo .crate archives, the matching local NDK and Gradle
distributions, and checked-in Kotlin upstream texts. Network access occurs only
with --refresh-kotlin, against the pinned official v1.9.22 tag and SHA-256 values.
All files under licenses/, including NOTICE and index.json, may go into the APK.
"""
import argparse
import hashlib
import io
import json
import os
from pathlib import Path
import tarfile
import tomllib
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(__file__).with_name("licenses")
KOTLIN_VERSION = "1.9.22"
PROJECT_GPL_SHA256 = "3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986"
KOTLIN_URL = "https://raw.githubusercontent.com/JetBrains/kotlin/v1.9.22/license/"
KOTLIN_HASHES = {
    "LICENSE.txt": "cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30",
    "NOTICE.txt": "ea7099352103ade6a6918bc7efb00f8bcd43db0e76b2da2e83237c0bc0ff1b19",
}


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo-home", type=Path,
                        default=Path(os.environ.get("CARGO_HOME", Path.home() / ".cargo")))
    parser.add_argument("--ndk", type=Path, required=True)
    parser.add_argument("--kotlin-jar", type=Path, required=True)
    parser.add_argument("--refresh-kotlin", action="store_true")
    parser.add_argument("--ffmpeg-source", type=Path, default=ROOT / ".local/ffmpeg/ffmpeg-8.0.1")
    args = parser.parse_args()
    files = {}
    components = []

    def put(name, data):
        path = OUT / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        files[name] = sha256(data)
        return name

    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text("utf-8"))["workspace"]["package"]
    if workspace["license"] != "GPL-3.0-only":
        raise ValueError("expected project license GPL-3.0-only")
    project_license = (ROOT / "LICENSE").read_bytes()
    if sha256(project_license) != PROJECT_GPL_SHA256:
        raise ValueError("project LICENSE differs from the pinned complete GNU GPL v3 text")
    components.append({
        "component": "NekoPack Runtime", "version": workspace["version"],
        "license_expression": workspace["license"],
        "scope": "Project original source; third-party components retain their own notices; excludes external game data and patches",
        "license_files": [put("project/LICENSE", project_license)],
        "source": {"url": "https://www.gnu.org/licenses/gpl-3.0.txt", "paths": ["LICENSE"],
                   "sha256": PROJECT_GPL_SHA256,
                   "note": "GPL version 3 only. The unmodified license text does not itself select an or-later grant; project metadata and documentation specify GPL-3.0-only."},
    })

    lock_bytes = (ROOT / "Cargo.lock").read_bytes()
    lock = tomllib.loads(lock_bytes.decode("utf-8"))
    for package in lock["package"]:
        if "source" not in package:
            continue
        if package["source"] != "registry+https://github.com/rust-lang/crates.io-index":
            raise ValueError(f"unsupported registry source: {package['source']}")
        name, version = package["name"], package["version"]
        stem = f"{name}-{version}"
        archives = sorted((args.cargo_home / "registry/cache").glob(f"*/{stem}.crate"))
        if len(archives) != 1:
            raise ValueError(f"expected one cached .crate for {stem}, found {len(archives)}")
        archive_bytes = archives[0].read_bytes()
        if sha256(archive_bytes) != package["checksum"]:
            raise ValueError(f"Cargo.lock checksum mismatch: {stem}")
        with tarfile.open(fileobj=io.BytesIO(archive_bytes), mode="r:gz") as archive:
            metadata = tomllib.loads(archive.extractfile(f"{stem}/Cargo.toml").read().decode("utf-8"))
            license_files = []
            source_paths = []
            for member in sorted(archive.getmembers(), key=lambda item: item.name):
                relative = Path(member.name).relative_to(stem)
                if member.isfile() and relative.name.upper().startswith(("LICENSE", "NOTICE", "COPYING")):
                    license_files.append(put(f"rust/{stem}/{relative.as_posix()}", archive.extractfile(member).read()))
                    source_paths.append(relative.as_posix())
            if not license_files:
                raise ValueError(f"no original license files found: {stem}")
        components.append({
            "component": name, "version": version,
            "license_expression": metadata["package"].get("license"),
            "scope": "Cargo.lock registry inventory; actual APK linkage depends on target, features and optimization",
            "license_files": license_files,
            "source": {
                "url": f"https://static.crates.io/crates/{name}/{stem}.crate",
                "sha256": package["checksum"],
                "repository": metadata["package"].get("repository"),
                "paths": source_paths,
            },
        })

    with zipfile.ZipFile(args.kotlin_jar) as jar:
        manifest = jar.read("META-INF/MANIFEST.MF").decode("utf-8")
    if f"Implementation-Version: {KOTLIN_VERSION}-" not in manifest:
        raise ValueError("expected kotlin-stdlib 1.9.22 JAR")
    kotlin_files = []
    for name, expected in KOTLIN_HASHES.items():
        destination = f"kotlin-stdlib-{KOTLIN_VERSION}/{name}"
        data = (urllib.request.urlopen(KOTLIN_URL + name).read() if args.refresh_kotlin
                else (OUT / destination).read_bytes())
        if sha256(data) != expected:
            raise ValueError(f"Kotlin upstream text checksum mismatch: {name}")
        kotlin_files.append(put(destination, data))
    components.append({
        "component": "kotlin-stdlib", "version": KOTLIN_VERSION,
        "license_expression": "Apache-2.0", "scope": "JAR passed to D8 for APK classes.dex",
        "license_files": kotlin_files,
        "source": {"url": KOTLIN_URL, "paths": list(KOTLIN_HASHES),
                   "artifact": args.kotlin_jar.name, "artifact_sha256": sha256(args.kotlin_jar.read_bytes()),
                   "note": "The JAR has no license/NOTICE entries. Texts are unchanged from the matching official Kotlin tag; NOTICE identifies the Kotlin Compiler distribution."},
    })

    properties = (args.ndk / "source.properties").read_bytes()
    if b"Pkg.Revision = 27.3.13750724" not in properties:
        raise ValueError("expected Android NDK r27d (27.3.13750724)")
    toolchain = "toolchains/llvm/prebuilt/windows-x86_64"
    runtime = f"{toolchain}/sysroot/usr/lib/aarch64-linux-android/libc++_shared.so"
    ndk_notice = put("android-ndk-r27d/NOTICE.toolchain.txt", (args.ndk / toolchain / "NOTICE").read_bytes())
    components.append({
        "component": "Android NDK libc++_shared (including libc++abi)", "version": "r27d / 27.3.13750724",
        "license_expression": "See complete NDK toolchain NOTICE (LLVM, libc++, libc++abi and accompanying notices)",
        "scope": "libc++_shared.so packaged in lib/arm64-v8a; full toolchain notice retained without filtering",
        "license_files": [ndk_notice],
        "source": {"distribution": "android-ndk-r27d-windows.zip",
                   "url": "https://developer.android.com/ndk/downloads/older_releases",
                   "paths": [f"{toolchain}/NOTICE"], "artifact": runtime,
                   "artifact_sha256": sha256((args.ndk / runtime).read_bytes()),
                   "source_properties_sha256": sha256(properties),
                   "note": "This aggregate notice covers more components than the packaged runtime; inclusion does not assert that every listed component is linked."},
    })

    gradle = args.kotlin_jar.parent.parent
    if not (gradle / "lib/gradle-core-8.7.jar").is_file():
        raise ValueError("expected kotlin JAR from Gradle 8.7 lib/")
    gradle_files = [put(f"gradle-8.7/{name}", (gradle / name).read_bytes()) for name in ("LICENSE", "NOTICE")]
    components.append({
        "component": "Gradle distribution", "version": "8.7",
        "license_expression": "Apache-2.0 and distribution third-party terms (see original LICENSE)",
        "scope": "Build-tool provenance only; Gradle, its compiler plugins and other JARs are not bundled by build_probe.py",
        "license_files": gradle_files,
        "source": {"url": "https://services.gradle.org/distributions/gradle-8.7-bin.zip", "paths": ["LICENSE", "NOTICE"]},
    })

    squirrel_files = [put(f"squirrel-2.2.5/{name}", (ROOT / "vendor/squirrel2" / name).read_bytes())
                      for name in ("COPYRIGHT", "NEKOPACK_CHANGES.md")]
    components.append({
        "component": "Squirrel", "version": "2.2.5 stable (modified subset)",
        "license_expression": "Original zlib-style terms in COPYRIGHT",
        "scope": "Native interpreter compiled into the Android runtime",
        "license_files": squirrel_files,
        "source": {"url": "https://downloads.sourceforge.net/project/squirrel/squirrel2/squirrel%202.2.5%20stable/squirrel_2.2.5_stable.tar.gz",
                   "paths": ["vendor/squirrel2/COPYRIGHT", "vendor/squirrel2/NEKOPACK_CHANGES.md"]},
    })
    plmpeg = json.loads((ROOT / "vendor/pl_mpeg/UPSTREAM.json").read_text("utf-8"))
    plmpeg_files = [put(f"pl_mpeg/{name}", (ROOT / "vendor/pl_mpeg" / name).read_bytes())
                   for name in ("LICENSE", "NEKOPACK_CHANGES.md", "UPSTREAM.json")]
    components.append({
        "component": "pl_mpeg", "version": plmpeg["revision"], "license_expression": "MIT",
        "scope": "Native MPEG decoder compiled into the Android runtime",
        "license_files": plmpeg_files,
        "source": {"url": plmpeg["url"], "revision": plmpeg["revision"],
                   "paths": ["vendor/pl_mpeg/LICENSE", "vendor/pl_mpeg/NEKOPACK_CHANGES.md", "vendor/pl_mpeg/UPSTREAM.json"],
                   "note": "Upstream header declares MIT and attribution; the repository LICENSE retains the full MIT text and provenance."},
    })

    ffmpeg_files = [put(f"ffmpeg-8.0.1/{name}", (args.ffmpeg_source / name).read_bytes())
                    for name in ("COPYING.LGPLv2.1", "COPYING.LGPLv3", "LICENSE.md")]
    components.append({
        "component": "FFmpeg", "version": "8.0.1", "license_expression": "LGPL-2.1-or-later",
        "scope": "Minimal shared ASF/WMV/WMA decoder; GPL, nonfree, network and external codecs disabled",
        "license_files": ffmpeg_files,
        "source": {"url": "https://ffmpeg.org/releases/ffmpeg-8.0.1.tar.xz",
                   "sha256": "05ee0b03119b45c0bdb4df654b96802e909e0a752f72e4fe3794f487229e5a41",
                   "build_script": "crates/nekopack-video/tools/build_ffmpeg.py",
                   "note": "Unmodified upstream source; distribute the pinned source archive and build script with binary releases."},
    })

    notice = """NekoPack Android project license and third-party notices

This directory retains complete third-party license/notice texts and provenance.
index.json records component versions, original sources, file hashes and scope.
All Rust registry packages in Cargo.lock are inventoried, including alternative
license texts supplied by their publishers. This inventory does not assert that
every package, feature or file is linked into every APK. Gradle's aggregate
LICENSE/NOTICE are build-tool provenance; the NDK aggregate NOTICE is preserved
in full and also describes components beyond the packaged libc++ runtime.
Kotlin's original upstream NOTICE describes the Kotlin Compiler distribution.

NekoPack uses modified Squirrel 2.2.5 and pl_mpeg source; the included change logs
identify modifications. NekoPack's original source is GPL-3.0-only (GNU GPL
version 3 only); the complete official text is in project/LICENSE. Third-party
components retain their own notices. These licenses do not cover external game
data or patches. License texts do not replace corresponding source delivery.

Component inventory:
"""
    notice += "".join(f"- {c['component']} {c['version']}: {c['license_expression']}\n" for c in components)
    put("NOTICE", notice.encode("utf-8"))
    index = {
        "schema_version": 1,
        "cargo_lock_sha256": sha256(lock_bytes),
        "packaging": "Recursively copy this entire directory to assets/licenses, including NOTICE and index.json; no exclusions required.",
        "reproduction": "Run platforms/android/update_licenses.py with matching local Cargo cache, NDK r27d and Kotlin JAR from Gradle 8.7. Default is offline; --refresh-kotlin verifies pinned official upstream bytes.",
        "components": components,
        "files_sha256": dict(sorted(files.items())),
    }
    # Reject stale files rather than silently distributing an obsolete snapshot.
    expected = set(files) | {"index.json"}
    existing = {p.relative_to(OUT).as_posix() for p in OUT.rglob("*") if p.is_file()}
    if existing - expected:
        raise ValueError(f"stale snapshot files must be reviewed and removed: {sorted(existing - expected)}")
    (OUT / "index.json").write_bytes((json.dumps(index, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
    print(f"Collected {len(components)} component records, {len(files)} license/notice/provenance files in {OUT}")


if __name__ == "__main__":
    main()
