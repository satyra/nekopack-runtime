"""Back up saves/preferences and verify an in-place APK upgrade on a physical device.

No screenshots, story text, save decoding, uninstall or data clearing. The backup
is an opaque local tar. Installation stops if run-as cannot back up existing data.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess

PACKAGE = "org.nekopack.probe"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True,
                        help="new directory for private backup and verification receipt")
    parser.add_argument("--adb", default="D:/codex/tools/android/sdk/platform-tools/adb.exe")
    args = parser.parse_args()
    if args.serial.startswith(("emulator-", "127.", "localhost:")):
        parser.error("physical-device verification must not use a local emulator")
    apk = args.apk.resolve(strict=True)
    args.output.mkdir(parents=True, exist_ok=False)
    prefix = [args.adb, "-s", args.serial]

    def adb(*parts):
        return subprocess.check_output(prefix + list(parts), stderr=subprocess.PIPE)

    def private(command):
        return adb("shell", "run-as", PACKAGE, "sh", "-c", shlex.quote(command))

    def installed_digest():
        paths = adb("shell", "pm", "path", PACKAGE).decode().splitlines()
        assert len(paths) == 1 and paths[0].startswith("package:"), "expected one installed APK"
        return adb("shell", "sha256sum", shlex.quote(paths[0][8:])).split()[0].decode()

    def inventory():
        return private("for p in files/saves shared_prefs; do "
                       "if test -d \"$p\"; then find \"$p\" -type f -exec sha256sum {} \\;; fi; done").decode().splitlines()

    report = {"passed": False, "serial": args.serial, "data_cleared": False,
              "apk_sha256": hashlib.sha256(apk.read_bytes()).hexdigest()}
    try:
        assert adb("get-state").strip() == b"device"
        report["previous_apk_sha256"] = installed_digest()
        assert report["previous_apk_sha256"] != report["apk_sha256"], "same APK: not an upgrade test"
        adb("shell", "am", "force-stop", PACKAGE)
        before = sorted(inventory())
        assert any("files/saves/" in row for row in before), "no existing saves to verify"
        report["before"] = before
        backup = args.output / "private-saves-preferences.tar"
        command = "set --; for p in files/saves shared_prefs; do if test -d \"$p\"; then set -- \"$@\" \"$p\"; fi; done; tar -cf - \"$@\""
        with backup.open("xb") as stream:
            subprocess.run(prefix + ["exec-out", "run-as", PACKAGE, "sh", "-c", shlex.quote(command)],
                           stdout=stream, stderr=subprocess.PIPE, check=True)
        assert backup.stat().st_size >= 1024, "empty backup"
        with backup.open("rb") as stream:
            report["backup_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
        assert sorted(inventory()) == before, "data changed during backup"
        # No uninstall/downgrade/signature workaround: package manager validates
        # the existing signature and version while preserving the data directory.
        result = adb("install", "-r", str(apk)).decode()
        assert "Success" in result, result
        report["installed_apk_sha256"] = installed_digest()
        assert report["installed_apk_sha256"] == report["apk_sha256"]
        report["after"] = sorted(inventory())
        assert report["after"] == before, "save/preferences bytes changed across install"
        report["passed"] = True
    finally:
        (args.output / "upgrade.json").write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    print(json.dumps({"passed": True, "files_preserved": len(before), "output": str(args.output)}))


if __name__ == "__main__":
    main()
