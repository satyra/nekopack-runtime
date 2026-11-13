"""Build a nested original Sorakoi room fixture, without opening artwork or replay text.

Only the technical startup script and music table are read as text. The mounted
system archive is byte-for-byte original. Entry/return scenario text is synthetic.
This builder never deploys to a device.
"""
import argparse
import hashlib
import importlib.util
import json
import shutil
import struct
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location(
    "packs", Path(__file__).with_name("build-isolated-audio-fixtures.py"))
packs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(packs)
packs.CLI = ROOT / "target/debug/nekopack-cli.exe"


def synthetic_mng(names, width, height):
    """Generate opaque test colors; never inspect or derive pixels from game art."""
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    result = b"\x8aMNG\r\n\x1a\n" + chunk(b"MHDR", struct.pack(">7I", width, height, 0, 0, 0, 0, 0))
    for index, name in enumerate(names):
        color = bytes((32 + index % 8 * 24, 64 + index % 6 * 24, 96, 255))
        result += chunk(b"FRAM", b"\x02" + name.encode("cp932"))
        result += chunk(b"DEFI", b"\0" * 4 + struct.pack(">ii", 0, 0))
        result += chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        result += chunk(b"IDAT", zlib.compress((b"\0" + color * width) * height))
        result += chunk(b"IEND", b"")
    return result + chunk(b"MEND", b"")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=ROOT / ".local/sorakoi-import/game/ソラコイ")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--boundaries", action="store_true", help="unlock only the first original CG/scene entry and substitute generated CG/thumbnail pixels")
    parser.add_argument("--synthetic-replay", action="store_true", help="package a generated replay command under the first original scene name; requires --boundaries")
    args = parser.parse_args()
    if args.synthetic_replay and not args.boundaries:
        parser.error("--synthetic-replay requires --boundaries")
    if args.output is None:
        args.output = ROOT / ".local/system-integration" / ("nested-room-replay-fixture" if args.synthetic_replay else "nested-room-boundaries-fixture" if args.boundaries else "nested-room-fixture")
    if args.output.exists():
        raise FileExistsError(f"refusing to overwrite {args.output}")
    archives = [str(args.source / name) for name in ("system.dat", "script.dat", "sound.dat")]
    start, start_evidence = packs.extract(archives, "script", "start.txt")
    start = start.decode("cp932")
    if start.count(".load titlelogo") != 1:
        raise ValueError("expected one technical startup titlelogo dispatch")
    start = start.replace(".load titlelogo", ".system nested_room_probe.nut\n.load title")
    table, table_evidence = packs.extract(archives, "system", "x_music_list.txt")
    tracks = [line.split("\t")[0].strip() for line in table.decode("cp932").splitlines()
              if line.strip() and not line.lstrip().startswith(";")]
    selected = list(dict.fromkeys([*tracks[:3], "bgm90"]))
    setup = b"Config.SysEffect=0;Config.TextSpeed=100;Config.AutoSave=0;Config.KeepIcon=1;"
    boundary = None
    generated = []
    boundary_evidence = []
    synthetic_replay = None
    if args.boundaries:
        def rows(name):
            data, evidence = packs.extract(archives, "system", name)
            boundary_evidence.append(evidence)
            return [line.split() for line in data.decode("cp932").splitlines()
                    if line.strip() and not line.lstrip().startswith(";")]
        cg_rows = rows("x_cg_list.txt")
        scene_rows = rows("x_scene_list.txt")
        container, *pages = cg_rows[0]
        if not pages:
            raise ValueError("first original CG entry has no named pages")
        icon, scene = scene_rows[0][:2]
        setup += b'var["@"+LIST_CG[0]]=1;var["_"+LIST_SCENE[0].scene]=1;'
        thumbnail_names = list(dict.fromkeys([row[0] for row in cg_rows] + [row[0] for row in scene_rows]))
        generated = [("image/visual", container + ".mng", synthetic_mng(pages, 1600, 900)),
                     ("system", "thumbnail.mng", synthetic_mng(thumbnail_names, 280, 150))]
        boundary = dict(cg=dict(container=container, pages=pages, flag="GF300"),
                        scene=dict(icon=icon, name=scene, flag="G200"),
                        generated_resources=[dict(name=directory + "/" + name, sha256=hashlib.sha256(data).hexdigest())
                                             for directory, name, data in generated],
                        replay_resource_packaged=False, original_cg_extracted=False)
        if args.synthetic_replay:
            replay_bytes = b".native_replay_probe\n"
            replay_name = scene + ".txt"
            setup += b'com["native_replay_probe"] = function(args){local s=Screen();s.Fill(0x204060);s.Update();wait(250);};'
            generated.append(("script", replay_name, replay_bytes))
            boundary["replay_resource_packaged"] = True
            synthetic_replay = dict(resource="script/" + replay_name, command="native_replay_probe",
                                    bytes=len(replay_bytes), sha256=hashlib.sha256(replay_bytes).hexdigest(),
                                    native_operation=82, original_replay_resource_packaged=False,
                                    original_script_archive_packaged=False)
    # The original com[room] owns every UI branch and restoration call.
    scenario = b".route NESTED_ROOM_PROBE\n.room music\n:Probe::XX\nnested original room returned\n"
    entries = [
        ("script", "start.txt", start.encode("cp932")),
        ("system", "nested_room_probe.nut", setup),
        ("script", "titlelogo.txt", b".load title\n"),
        ("script", "title.txt", b".title\n START\n.case 1\n.load story1\n.end\n"),
        ("script", "story1.txt", scenario),
    ]
    entries.extend(generated)
    resources = [start_evidence, table_evidence, *boundary_evidence]
    hashes = {}
    for name in selected:
        raw, evidence = packs.extract(archives, "sound/bgm", name + ".ogg")
        if not raw.startswith(b"OggS"):
            raise ValueError(f"invalid original BGM {name}")
        entries.append(("sound/bgm", name + ".ogg", raw))
        resources.append(evidence)
        hashes[name] = hashlib.sha256(raw).hexdigest()
    args.output.mkdir(parents=True)
    for name in ("system.dat", "sorakoi.exe"):
        shutil.copy2(args.source / name, args.output / name)
    packs.pack(args.output / "nested_room_probe.dat", entries)
    if synthetic_replay:
        replay, evidence = packs.extract([str(args.output / "nested_room_probe.dat")], "script", replay_name)
        if replay != replay_bytes:
            raise ValueError("packaged synthetic replay differs from generated bytes")
        synthetic_replay["packaged_evidence"] = evidence
    files = [dict(name=p.name, bytes=p.stat().st_size, sha256=packs.sha(p))
             for p in sorted(args.output.iterdir())]
    report = dict(
        synthetic_scenario=True, original_system_unchanged=True,
        original_handlers=["room", "ViewRoomMusic", "PlayTitleBGM"],
        source_start=start_evidence, resources=resources, files=files,
        selected_tracks=selected, audio_sha256=hashes,
        original_replay_text_loaded=False, artwork_inspected=False,
        return_page=dict(source="story1", line=4),
        checks=["same coroutine room entry", "original item selection", "stop",
                "volume slider", "original title BGM restoration", "return to caller"],
    )
    if boundary is not None:
        report["boundaries"] = boundary
        report["original_handlers"].extend(["ViewRoomCG", "ViewRoomCGPage", "ViewRoomScene"])
    if synthetic_replay:
        report["synthetic_replay"] = synthetic_replay
        report["packaged_script_resources"] = [dict(name=name, bytes=len(data), sha256=hashlib.sha256(data).hexdigest())
                                               for directory, name, data in entries if directory == "script"]
        report["checks"].extend(["production native replay operation 82", "generated replay EOF returns to retained original room"])
    (args.output / "manifest.json").write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(dict(output=str(args.output), files=len(files), tracks=selected,
                          bytes=sum(file["bytes"] for file in files))))


if __name__ == "__main__":
    main()
