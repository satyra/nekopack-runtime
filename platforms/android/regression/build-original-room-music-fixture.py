"""Build a small isolated original music-room command fixture, never a full game.

The original system archive, room handler, music table and artwork stay intact.
Only the entry scripts are synthetic; three original BGM clips and UI SE are packed.
"""
import argparse
import hashlib
import importlib.util
import json
import re
import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location(
    "audio_fixture", Path(__file__).with_name("build-isolated-audio-fixtures.py"))
helper = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helper)
DEFAULT_GAME = Path("D:/codex/td-port-workspace/游戏/[120727][Rosebleu]Tiny Dungeon ～BRAVE or SLAVE～ [汉化硬盘版+RR5]/[120727][Rosebleu]Tiny Dungeon ～BRAVE or SLAVE～")
REMOTE = "/sdcard/Download/NekoPack-Sorakoi/NekoPack-Music-Probe"
TREE = "content://com.android.providers.downloads.documents/tree/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-Sorakoi"
DIRECTORY = "raw:/storage/emulated/0/Download/NekoPack-Sorakoi/NekoPack-Music-Probe"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--game", type=Path, default=DEFAULT_GAME)
    parser.add_argument("--output", type=Path, default=ROOT / ".local/system-integration/music-fixture")
    parser.add_argument("--deploy", action="store_true")
    parser.add_argument("--serial", choices=["127.0.0.1:5577"], default="127.0.0.1:5577")
    parser.add_argument("--adb", default="D:/codex/tools/android/sdk/platform-tools/adb.exe")
    args = parser.parse_args()
    assert not args.output.exists(), f"refusing to overwrite {args.output}"
    packs = [str(args.game / "system.dat"), str(args.game / "sound.dat")]
    resources = []

    def original(directory, name):
        raw, metadata = helper.extract(packs, directory, name)
        resources.append(metadata)
        return raw

    start = original("script", "start.txt")
    title = original("script", "title.txt").decode("cp932").splitlines()
    map_line = next(i for i, line in enumerate(title) if line.strip().startswith(".map title"))
    rows = []
    for line in title[map_line + 1:]:
        if line.lstrip().startswith(".case"):
            break
        if line.strip() and not line.lstrip().startswith(";"):
            rows.append(line.split()[0])
    # Asset labels come from the original title mapping, with gates removed in
    # this isolated entry. The original musical handler/table remain untouched.
    assert len(rows) >= 8
    synthetic_title = (f"; isolated engine command fixture, not original game menu\n"
                       f".route\n.title\n.map title\n{rows[7]}\n{rows[0]}\n"
                       f".case 1\n.room music\n.case 2\n.load music_probe\n.end\n.load title\n").encode("cp932")
    start, count = re.subn(rb"(?m)^\.load[ \t]+titlelogo[ \t]*(?=\r?$)", b".load title", start)
    assert count == 1, "expected the original start's titlelogo dispatch"
    entries = [("script", "start.txt", start), ("script", "title.txt", synthetic_title),
               ("script", "music_probe.txt", b"; isolated start destination\n.load title\n")]
    table = original("system", "x_music.txt")
    tracks = [line.split()[0] for line in table.decode("cp932").splitlines()
              if line.strip() and not line.lstrip().startswith(";")]
    # Include the first track too: the original PLAY-from-empty branch selects
    # it, while the automated second/NEXT/BACK checks need tracks 2 and 3.
    selected = tracks[:3]
    hashes = {}
    for name in selected:
        raw = original("sound/bgm", name + ".ogg")
        assert raw.startswith(b"OggS")
        entries.append(("sound/bgm", name + ".ogg", raw))
        hashes[name] = hashlib.sha256(raw).hexdigest()
    for name in ["se_PAGE.ogg", "se_CLICK.ogg", "se_OK.ogg", "se_SELECT.ogg"]:
        raw = original("sound/se", name)
        assert raw.startswith(b"OggS")
        entries.append(("sound/se", name, raw))
    args.output.mkdir(parents=True)
    shutil.copy2(args.game / "system.dat", args.output / "system.dat")
    helper.pack(args.output / "music_probe.dat", entries)
    files = [dict(name=p.name, bytes=p.stat().st_size, sha256=helper.sha(p))
             for p in sorted(args.output.glob("*.dat"))]
    assert 34_000_000 <= sum(f["bytes"] for f in files) <= 45_000_000
    for file in files:
        subprocess.run([str(helper.CLI), "inspect-archive", str(args.output / file["name"])],
                       stdout=subprocess.DEVNULL, check=True)
    (args.output / "audio-hashes.json").write_text(json.dumps(hashes, indent=2), encoding="utf-8")
    report = dict(scope="isolated engine command entry, mounted original room handler and table",
                  original_game=str(args.game), files=files, resources=resources,
                  original_track_count=len(tracks), selected_tracks=selected,
                  original_system_unchanged=True, room_action=1, title_game=0,
                  tree=TREE, directory=DIRECTORY, remote=REMOTE)
    (args.output / "manifest.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    if args.deploy:
        command = [args.adb, "-s", args.serial]
        exists = subprocess.run([*command, "shell", "test", "-e", REMOTE])
        assert exists.returncode == 1, "remote fixture exists or device check failed; refusing overwrite"
        subprocess.run([*command, "push", str(args.output), REMOTE], check=True)
        for file in files:
            digest = subprocess.check_output([*command, "shell", "sha256sum", REMOTE + "/" + file["name"]]).split()[0].decode()
            assert digest == file["sha256"]
        report["deployed_serial"] = args.serial
        (args.output / "manifest.json").write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    print(json.dumps(dict(output=str(args.output), dat_bytes=sum(f["bytes"] for f in files),
                          tree=TREE, directory=DIRECTORY, room_action=1, deployed=args.deploy)))


if __name__ == "__main__":
    main()
