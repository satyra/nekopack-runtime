"""Verify original nested com.room music with button metadata and audio hashes only.

Run after fixture deployment and APK installation. Never captures screenshots,
opens CG entries, enters a replay, installs an APK, or changes original resources.
"""
import argparse
import hashlib
import importlib.util
import json
import shlex
import uuid
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location("navigation", Path(__file__).with_name("verify-sorakoi-layout-navigation.py"))
nav = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nav)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--apk", type=Path, required=True)
    parser.add_argument("--adb", default="D:/codex/tools/android/sdk/platform-tools/adb.exe")
    parser.add_argument("--remote-name", default="NekoPack-Nested-Room-Probe")
    parser.add_argument("--fixture", type=Path, default=ROOT / ".local/system-integration/nested-room-fixture")
    args = parser.parse_args()
    manifest = json.loads((args.fixture / "manifest.json").read_text(encoding="utf-8"))
    references = manifest["audio_sha256"]
    tree = "content://com.android.providers.downloads.documents/tree/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-Sorakoi"
    remote = "/sdcard/Download/NekoPack-Sorakoi/" + args.remote_name
    directory = "raw:/storage/emulated/0/Download/NekoPack-Sorakoi/" + args.remote_name
    run = "system-room-" + uuid.uuid4().hex[:8]
    p = nav.NavigationProbe(args.adb, args.serial, run, {})
    report = dict(run=run, passed=False, checks=[], playback=[])

    def events():
        return p.events("system-command-events.jsonl")

    def named(state, name):
        return next((b for b in state.get("buttons", []) if b["name"].split(":")[1] == name), None)

    def current(after=0):
        return p.wait("original music controls", lambda: next((d for d in reversed(events()[after:])
                      if d.get("command") == "room" and d.get("has_frame") and named(d, "NEXT")), None), timeout=30)

    def button(state, name):
        if isinstance(name, int):
            return next(b for b in state["buttons"] if b["name"].split(":")[1] == "" and int(b["name"].split(":")[-1]) == name)
        result = named(state, name)
        assert result, (name, state["buttons"])
        return result

    def point(state, control, fraction=.5):
        layout = p.obj("layout.json")
        sw, sh = layout["stage_width"], layout["stage_height"]
        width, height = state["size"]
        x, y, w, h = control["bounds"]
        scale = min(sw / width, sh / height)
        return round(sw / 2 + (x + w * fraction - width / 2) * scale), round(sh / 2 + (y + h / 2 - height / 2) * scale)

    def tap(name, fraction=.5):
        state = current()
        p.tap(*point(state, button(state, name), fraction))

    def event(filename, offset, predicate):
        return p.wait(filename, lambda: next((e for e in p.events(filename)[offset:] if predicate(e)), None), timeout=30)

    def play(name):
        offset = len(p.events("room-native-audio.jsonl"))
        tap(name)
        meta = event("room-native-audio.jsonl", offset, lambda e: e.get("host_effect") == "bgm")
        assert meta["loop"] is False, "Sorakoi original music room requests non-loop playback"
        assert meta["sha256"] == references[meta["name"]]
        token = meta["token"]
        started = event("host-events.jsonl", 0, lambda e: e.get("token") == token and e.get("event") == "playing" and e.get("value", 0) > 0)
        progress = event("host-events.jsonl", 0, lambda e: e.get("token") == token and e.get("event") == "progress" and e.get("value", 0) > 0)
        looping = event("host-events.jsonl", 0, lambda e: e.get("token") == token and e.get("event") == "looping")
        assert looping["value"] == 0
        report["playback"].append(dict(control=name, metadata=meta, started=started, progress=progress, looping=looping))
        return meta

    try:
        for item in manifest["files"]:
            actual = p.adb("shell", "sha256sum", shlex.quote(remote + "/" + item["name"])).split()[0].decode()
            assert actual == item["sha256"], item["name"]
        installed = p.adb("shell", "pm", "path", "org.nekopack.probe").decode().strip().removeprefix("package:")
        digest = p.adb("shell", "sha256sum", shlex.quote(installed)).split()[0].decode()
        assert digest == hashlib.sha256(args.apk.read_bytes()).hexdigest(), "installed APK differs"
        report["apk_sha256"] = digest
        title = p.start_title("--ei", "title_game", "0", "--es", "library_tree", shlex.quote(tree),
                              "--es", "library_directory", shlex.quote(directory), "--ez", "library_original", "true",
                              "--ez", "dialogue_probe", "true", "--ez", "lifecycle_probe", "true",
                              "--es", "save_probe", run, "--es", "read_history_probe", run)
        start = next(b for b in title["buttons"] if b["id"] == title["start_action"])
        left, top, right, bottom = p.bounds(nav.startup.TITLE)
        x, y, w, h = start["bounds"]
        scale = min((right-left)/title["width"], (bottom-top)/title["height"])
        p.tap(round((left+right)/2+(x+w/2-title["width"]/2)*scale), round((top+bottom)/2+(y+h/2-title["height"]/2)*scale))
        first = current()
        token = first["token"]
        report["checks"].append("original room starts on retained system command token")
        second = play(2)
        following = play("NEXT")
        assert following["name"] != second["name"]
        previous = play("BACK")
        assert previous["name"] == second["name"]
        report["checks"].append("original item NEXT BACK produce matching original Ogg hashes and real non-loop playback")
        gain_offset = len(p.events("audio-gain-events.jsonl"))
        state = current()
        control = button(state, "VOLUME")
        begin = point(state, control, .7)
        end = point(state, control, .31)
        p.adb("shell", "input", "swipe", *map(str, (*begin, *end)), "350")
        gain = event("audio-gain-events.jsonl", gain_offset, lambda e: e.get("kind") == "bgm" and .20 < e.get("output", 1) < .45)
        report["volume_gain"] = gain
        report["checks"].append("original VolumeBar drag updates actual player gain")
        offset = len(p.events("room-native-audio.jsonl"))
        tap("STOP")
        stopped = event("room-native-audio.jsonl", offset, lambda e: e.get("host_effect") == "bgm_stop")
        released = event("host-events.jsonl", 0, lambda e: e.get("token") == stopped["token"] and e.get("event") == "stopped")
        assert released["value"] == 0
        report["stop"] = dict(command=stopped, released=released)
        report["checks"].append("original STOP releases the player")
        offset = len(p.events("room-native-audio.jsonl"))
        tap("CLOSE")
        page = p.wait("original room returned to caller", lambda: p.obj("page-1.json"))
        assert page["source"] == "story1"
        restored = event("room-native-audio.jsonl", offset, lambda e: e.get("host_effect") == "bgm" and e.get("name") == "bgm90")
        assert restored["loop"] is True and restored["sha256"] == references["bgm90"]
        assert all(e["token"] == token for e in events() if e.get("command") == "room")
        report["title_bgm_restored"] = restored
        report["checks"].append("same original coroutine restores title BGM and returns to synthetic caller")
        assert not p.read("error.txt")
        report.update(passed=True, events=events())
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(dict(passed=True, run=run, checks=report["checks"])))


if __name__ == "__main__":
    main()
