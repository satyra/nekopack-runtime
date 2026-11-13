"""Original room music regression; numeric UI metadata and audio hashes only.

Run serially after the requested APK and original DAT entry have been prepared.
This script neither installs an APK nor imports/changes game resources.
"""
import argparse
import hashlib
import importlib.util
import json
import shlex
import uuid
from datetime import datetime, timezone
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "room_navigation", Path(__file__).with_name("verify-sorakoi-layout-navigation.py"))
navigation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(navigation)
PACKAGE = "org.nekopack.probe"
ROOM = "原版音乐鉴赏"
OK, STOP, NEXT, BACK = 2000000001, 2000000002, 2000000003, 2000000004


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", choices=["127.0.0.1:5577"], required=True)
    parser.add_argument("--adb", default="D:/codex/tools/android/sdk/platform-tools/adb.exe")
    parser.add_argument("--apk", type=Path, required=True, help="expected local APK, compared before launch")
    parser.add_argument("--audio-manifest", type=Path,
                        help='JSON object mapping original BGM names to SHA256 strings')
    parser.add_argument("--library-entry")
    parser.add_argument("--title-game", type=int, default=0)
    parser.add_argument("--tree", help="existing authorized SAF tree URI")
    parser.add_argument("--directory", help="SAF document ID inside that tree")
    parser.add_argument("--room-action", type=int, required=True,
                        help="existing visible title action that enters music; TD4 branch is 8")
    parser.add_argument("--master-percent", type=int, default=100)
    parser.add_argument("--isolated-command-fixture", action="store_true",
                        help="label a prepared synthetic entry that invokes the mounted original room handler")
    parser.add_argument("--plan-only", action="store_true", help="print inputs; no device access")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    assert 0 <= args.master_percent <= 100
    assert args.library_entry or args.tree, "supply a library entry or an authorized tree"
    assert not args.directory or args.tree, "directory requires tree"
    if args.plan_only:
        print(json.dumps(dict(serial=args.serial, apk=str(args.apk),
                             library_entry=args.library_entry, title_game=args.title_game,
                             tree=args.tree, directory=args.directory, room_action=args.room_action,
                             requires_original_dat=True, installs_or_imports=False)))
        return
    assert args.audio_manifest, "runtime verification requires original audio hash manifest"
    references = json.loads(args.audio_manifest.read_text(encoding="utf-8"))
    assert references and all(isinstance(v, str) and len(v) == 64 for v in references.values())
    run = "original-room-music-" + uuid.uuid4().hex[:8]
    p = navigation.NavigationProbe(args.adb, args.serial, run, {})
    result = dict(run=run, serial=args.serial, checked_at=datetime.now(timezone.utc).isoformat(),
                  scope=("isolated engine command entry with mounted original room handler" if args.isolated_command_fixture
                         else "existing original title action and mounted original room handler"),
                  actions=[], playback=[], passed=False)

    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding="utf-8")

    def press(label, state, ident):
        button = next((b for b in state["buttons"] if b["id"] == ident and b["enabled"]), None)
        assert button, (label, ident, "no enabled original button; do not add a replacement menu")
        left, top, right, bottom = p.bounds(label)
        width, height = state["width"], state["height"]
        x, y, w, h = button["bounds"]
        scale = min((right-left)/width, (bottom-top)/height)
        p.tap(round((left+right)/2+(x+w/2-width/2)*scale),
              round((top+bottom)/2+(y+h/2-height/2)*scale))
        result["actions"].append(dict(id=ident, bounds=button["bounds"]))

    def room():
        return p.wait("original music room", lambda: (m if (m := p.obj("room-presented.json"))
                      and m.get("kind") == "room" and m.get("room") == "music" else None))

    def event(filename, offset, predicate):
        return p.wait(filename, lambda: next((e for e in p.events(filename)[offset:]
                                             if predicate(e)), None), timeout=30)

    def assert_cache_released(token):
        path = f"files/{run}/audio-{token}.ogg"
        check = f"if test ! -e {shlex.quote(path)}; then echo released; fi"
        p.wait("released audio cache", lambda: p.adb("shell", "run-as", PACKAGE, "sh", "-c",
               shlex.quote(check)).strip() == b"released", timeout=10)

    def play(ident):
        offset = len(p.events("ui-audio-events.jsonl"))
        press(ROOM, room(), ident)
        meta = event("ui-audio-events.jsonl", offset, lambda e: e.get("host_effect") == "bgm")
        token = meta["token"]
        assert meta["loop"] is True and meta["volume_percent"] == 100
        started = event("host-events.jsonl", 0, lambda e: e["token"] == token
                        and e["kind"] == "bgm" and e["event"] == "playing" and e["value"] > 0)
        position = event("host-events.jsonl", 0, lambda e: e["token"] == token
                         and e["kind"] == "bgm" and e["event"] == "progress" and e["value"] > 0)
        looping = event("host-events.jsonl", 0, lambda e: e["token"] == token
                        and e["kind"] == "bgm" and e["event"] == "looping")
        assert looping["value"] == 1, "actual player must loop"
        gain = event("audio-gain-events.jsonl", 0, lambda e: e["token"] == token
                     and e["kind"] == "bgm" and e["output"] > 0)
        expected = meta["volume_percent"]/100 * meta["category_percent"]/100 * args.master_percent/100
        assert abs(gain["output"]-expected) < .0001, (gain["output"], expected)
        audio = p.obj(f"audio-{token}.json")
        assert audio and audio["name"] == meta["name"]
        assert audio["sha256"] == references[meta["name"]], "mounted original audio hash differs"
        evidence = dict(id=ident, token=token, name=meta["name"], sha256=audio["sha256"],
                        loop=meta["loop"], player_looping=looping["value"],
                        volume_percent=meta["volume_percent"], category_percent=meta["category_percent"],
                        started=started, position=position, gain=gain)
        presented = room()
        assert presented["room_music_count"] > 0, "room did not expose queued music requests"
        evidence["room_music_count"] = presented["room_music_count"]
        result["playback"].append(evidence)
        save()
        return evidence

    def stopped(offset):
        command = event("room-music-events.jsonl", offset,
                        lambda e: e.get("host_effect") == "bgm_stop")
        stopped_event = event("host-events.jsonl", 0, lambda e: e["token"] == command["token"]
                              and e["kind"] == "bgm_stop" and e["event"] == "stopped")
        assert stopped_event["value"] == 0, "BGM player retained after stop"
        return dict(command=command, released=stopped_event)

    try:
        paths = p.adb("shell", "pm", "path", PACKAGE).decode().splitlines()
        assert len(paths) == 1 and paths[0].startswith("package:"), "expected one installed APK"
        installed = p.adb("shell", "sha256sum", paths[0].removeprefix("package:")).split()[0].decode()
        expected = hashlib.sha256(args.apk.read_bytes()).hexdigest()
        assert installed == expected, "installed APK differs from requested build"
        result["apk_sha256"] = installed
        extras = ["--ei", "title_game", str(args.title_game), "--ez", "library_original", "true",
                  "--ez", "lifecycle_probe", "true", "--ez", "dialogue_probe", "true"]
        for flag, value in [("library_entry", args.library_entry), ("library_tree", args.tree),
                            ("library_directory", args.directory)]:
            if value is not None:
                extras += ["--es", flag, shlex.quote(value)]
        title = p.start_title(*extras)
        if title.get("tick_ms", 0) > 0:
            title = p.wait("title animation", lambda: (m if (m := p.obj("title-presented.json"))
                           and m.get("tick_ms", 0) <= 0 else None))
        press(navigation.startup.TITLE, title, args.room_action)
        room()
        second = play(2)
        replay = play(OK)
        assert replay["name"] == second["name"] and replay["sha256"] == second["sha256"], "PLAY selected another track"
        assert_cache_released(second["token"])
        following = play(NEXT)
        assert following["name"] != replay["name"], "NEXT did not switch track"
        assert_cache_released(replay["token"])
        previous = play(BACK)
        assert previous["name"] == second["name"], "BACK did not restore second track"
        assert_cache_released(following["token"])
        offset = len(p.events("room-music-events.jsonl"))
        press(ROOM, room(), STOP)
        result["stop"] = stopped(offset)
        assert_cache_released(previous["token"])
        final_play = play(2)
        offset = len(p.events("room-music-events.jsonl"))
        p.key("BACK")
        result["exit"] = stopped(offset)
        assert_cache_released(final_play["token"])
        p.wait("room closed", lambda: (m if (m := p.obj("room-presented.json"))
               and m.get("kind") == "room_closed" else None))
        p.bounds(navigation.startup.TITLE)
        result["passed"] = True
        print(json.dumps(dict(run=run, passed=True, apk_sha256=installed, output=str(args.output))))
    finally:
        save()


if __name__ == "__main__":
    main()
