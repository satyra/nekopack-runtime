"""Verify original room page changes, synthetic CG rendering, and replay metadata.

Uses a separately prepared boundaries fixture and room_metadata_only=true.
Never takes screenshots, reads original CG pixels, or executes replay scripts.
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
    parser.add_argument("--remote-name", default="NekoPack-Nested-Room-Boundaries-Probe")
    parser.add_argument("--fixture", type=Path, default=ROOT / ".local/system-integration/nested-room-boundaries-fixture")
    args = parser.parse_args()
    manifest = json.loads((args.fixture / "manifest.json").read_text(encoding="utf-8"))
    boundary = manifest["boundaries"]
    assert boundary["replay_resource_packaged"] is False
    assert boundary["original_cg_extracted"] is False
    tree = "content://com.android.providers.downloads.documents/tree/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-Sorakoi"
    remote = "/sdcard/Download/NekoPack-Sorakoi/" + args.remote_name
    directory = "raw:/storage/emulated/0/Download/NekoPack-Sorakoi/" + args.remote_name
    run = "system-room-boundaries-" + uuid.uuid4().hex[:8]
    p = nav.NavigationProbe(args.adb, args.serial, run, {})
    report = dict(run=run, passed=False, checks=[], fixture=boundary)

    def events():
        return p.events("system-command-events.jsonl")

    def control(state, name):
        return next((button for button in state.get("buttons", [])
                     if button["name"].split(":")[1] == name), None)

    def item(state, ident):
        return next((button for button in state.get("buttons", [])
                     if button["name"].split(":")[1] == "" and int(button["name"].split(":")[-1]) == ident), None)

    def state(after=0, entry=False, previous_group=None):
        return p.wait("original room menu", lambda: next((d for d in reversed(events()[after:])
                      if d.get("command") == "room" and d.get("has_frame") and control(d, "SCENE")
                      and (not entry or (item(d, 1) and not control(d, "PLAY")))
                      and (previous_group is None or control(d, "SCENE")["name"].split(":")[0] != previous_group)), None), timeout=30)

    def tap(frame, button):
        assert button, "expected enabled original control"
        layout = p.obj("layout.json")
        sw, sh = layout["stage_width"], layout["stage_height"]
        width, height = frame["size"]
        x, y, w, h = button["bounds"]
        scale = min(sw / width, sh / height)
        mark = len(events())
        p.tap(round(sw / 2 + (x + w / 2 - width / 2) * scale),
              round(sh / 2 + (y + h / 2 - height / 2) * scale))
        return mark

    try:
        for resource in manifest["files"]:
            digest = p.adb("shell", "sha256sum", shlex.quote(remote + "/" + resource["name"])).split()[0].decode()
            assert digest == resource["sha256"], resource["name"]
        installed = p.adb("shell", "pm", "path", "org.nekopack.probe").decode().strip().removeprefix("package:")
        digest = p.adb("shell", "sha256sum", shlex.quote(installed)).split()[0].decode()
        assert digest == hashlib.sha256(args.apk.read_bytes()).hexdigest(), "installed APK differs"
        report["apk_sha256"] = digest
        title = p.start_title("--ei", "title_game", "0", "--es", "library_tree", shlex.quote(tree),
                              "--es", "library_directory", shlex.quote(directory), "--ez", "library_original", "true",
                              "--ez", "dialogue_probe", "true", "--ez", "room_metadata_only", "true",
                              "--es", "save_probe", run, "--es", "read_history_probe", run)
        start = next(b for b in title["buttons"] if b["id"] == title["start_action"])
        left, top, right, bottom = p.bounds(nav.startup.TITLE)
        x, y, w, h = start["bounds"]
        scale = min((right-left)/title["width"], (bottom-top)/title["height"])
        p.tap(round((left+right)/2+(x+w/2-title["width"]/2)*scale), round((top+bottom)/2+(y+h/2-title["height"]/2)*scale))
        music = state()
        token = music["token"]
        assert control(music, "PLAY")
        mark = tap(music, control(music, "CG"))
        cg = state(mark, True)
        assert not control(cg, "PLAY")
        assert item(cg, 2) is None, "fixture must only unlock the first original CG"
        report["checks"].append("original room music to CG switch keeps the same coroutine")
        mark = tap(cg, item(cg, 1))
        expected_visual = "0 #" + boundary["cg"]["container"] + "&" + boundary["cg"]["pages"][0]

        def cg_frame():
            rows = events()[mark:]
            image_index = next((index for index, row in enumerate(rows)
                                if any(action == dict(kind="image", value=expected_visual)
                                       for action in row.get("room_actions", []))), None)
            if image_index is None:
                return None
            return next((row for row in rows[image_index + 1:]
                         if row.get("has_frame") and not row.get("native_ack")
                         and row.get("delay_ms") == -1 and not row.get("buttons")), None)

        rendered = p.wait("synthetic CG rendered after native image ACK", cg_frame, timeout=30)
        actions = [action for row in events()[mark:] for action in row.get("room_actions", [])]
        assert actions[:3] == [dict(kind="image", value="1 white"), dict(kind="commit", value=""),
                               dict(kind="image", value=expected_visual)], actions
        assert rendered["token"] == token
        report["cg_actions"] = actions
        report["cg_render_frame"] = rendered
        report["checks"].append("original ViewRoomCGPage ImageAction Commit ImageAction ACK Render displays generated pixels")
        mark = len(events())
        p.adb("shell", "input", "keyevent", "4")
        cg = state(mark, True)
        mark = tap(cg, control(cg, "SCENE"))
        scene = state(mark, True, control(cg, "SCENE")["name"].split(":")[0])
        assert item(scene, 2) is None, "fixture must only unlock the first original scene"
        report["checks"].append("CG viewer cancel returns to original CG page then switches to scene page")
        mark = tap(scene, item(scene, 1))
        replay = p.wait("original replay metadata boundary", lambda: next((row for row in events()[mark:]
                        if row.get("room_replay")), None), timeout=30)
        actions = [action for row in events()[mark:] for action in row.get("room_actions", [])]
        expected = [dict(kind="script_reset", value=""), dict(kind="script_reset", value=""),
                    dict(kind="assign_image", value=""),
                    dict(kind="replay", value=boundary["scene"]["name"])]
        assert [action for action in actions if action["kind"] in {"script_reset", "assign_image", "replay"}] == expected
        assert replay["token"] == token and replay["delay_ms"] == -1 and not replay["native_ack"]
        assert not p.obj("page-1.json"), "metadata-only boundary must not produce a replay dialogue"
        assert not p.read("error.txt")
        assert all(row["token"] == token for row in events() if row.get("command") == "room")
        report["replay_actions"] = actions
        report["replay_boundary"] = replay
        report["checks"].append("original scene selection reaches suspended ScriptEval metadata without replay resource loading")
        report.update(passed=True, events=events(), original_replay_executed=False)
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(dict(passed=True, run=run, checks=report["checks"])))


if __name__ == "__main__":
    main()
