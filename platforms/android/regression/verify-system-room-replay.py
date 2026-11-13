"""Verify production Android replay start/EOF/return using only generated script.

Requires a deployed --boundaries --synthetic-replay fixture and installed APK.
No room_metadata_only flag, screenshots, original replay text, or artwork reads.
This script does not deploy or install anything.
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
    parser.add_argument("--remote-name", default="NekoPack-Nested-Room-Replay-Probe")
    parser.add_argument("--fixture", type=Path, default=ROOT / ".local/system-integration/nested-room-replay-fixture")
    args = parser.parse_args()
    manifest = json.loads((args.fixture / "manifest.json").read_text(encoding="utf-8"))
    boundary = manifest["boundaries"]
    replay_fixture = manifest["synthetic_replay"]
    script = b".native_replay_probe\n"
    assert replay_fixture["sha256"] == hashlib.sha256(script).hexdigest()
    assert replay_fixture["bytes"] == len(script)
    assert replay_fixture["original_replay_resource_packaged"] is False
    assert replay_fixture["original_script_archive_packaged"] is False
    assert boundary["replay_resource_packaged"] is True
    assert boundary["original_cg_extracted"] is False
    assert manifest["original_replay_text_loaded"] is False
    expected_files = {"system.dat", "sorakoi.exe", "nested_room_probe.dat"}
    assert {file["name"] for file in manifest["files"]} == expected_files
    assert {entry["name"] for entry in manifest["packaged_script_resources"]} == {
        "start.txt", "titlelogo.txt", "title.txt", "story1.txt", boundary["scene"]["name"] + ".txt"}
    # Read back only our known generated resource, never the source game's script archive.
    builder_spec = importlib.util.spec_from_file_location("room_builder", Path(__file__).with_name("build-system-room-fixture.py"))
    builder = importlib.util.module_from_spec(builder_spec)
    builder_spec.loader.exec_module(builder)
    packed, evidence = builder.packs.extract([str(args.fixture / "nested_room_probe.dat")], "script", boundary["scene"]["name"] + ".txt")
    assert packed == script
    tree = "content://com.android.providers.downloads.documents/tree/raw%3A%2Fstorage%2Femulated%2F0%2FDownload%2FNekoPack-Sorakoi"
    remote = "/sdcard/Download/NekoPack-Sorakoi/" + args.remote_name
    directory = "raw:/storage/emulated/0/Download/NekoPack-Sorakoi/" + args.remote_name
    run = "system-room-replay-" + uuid.uuid4().hex[:8]
    p = nav.NavigationProbe(args.adb, args.serial, run, {})
    report = dict(run=run, passed=False, checks=[], fixture=replay_fixture, synthetic_resource=evidence,
                  original_replay_executed=False, room_metadata_only=False)

    def events():
        return p.events("system-command-events.jsonl")

    def control(state, name):
        return next((button for button in state.get("buttons", [])
                     if button["name"].split(":")[1] == name), None)

    def item(state, ident):
        return next((button for button in state.get("buttons", [])
                     if button["name"].split(":")[1] == "" and int(button["name"].split(":")[-1]) == ident), None)

    def room_state(after=0, scene=False, token=None):
        return p.wait("original room controls", lambda: next((row for row in reversed(events()[after:])
                      if row.get("command") == "room" and row.get("has_frame") and control(row, "SCENE")
                      and (not scene or (item(row, 1) and not control(row, "PLAY")))
                      and (token is None or row["token"] == token)), None), timeout=30)

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
        remote_files = set(p.adb("shell", "ls", "-1", shlex.quote(remote)).decode().splitlines())
        assert remote_files == expected_files | {"manifest.json"}, "unexpected fixture files could introduce original replay resources"
        for resource in manifest["files"]:
            local_digest = hashlib.sha256((args.fixture / resource["name"]).read_bytes()).hexdigest()
            assert local_digest == resource["sha256"], resource["name"]
            digest = p.adb("shell", "sha256sum", shlex.quote(remote + "/" + resource["name"])).split()[0].decode()
            assert digest == local_digest, resource["name"]
        installed = p.adb("shell", "pm", "path", "org.nekopack.probe").decode().strip().removeprefix("package:")
        digest = p.adb("shell", "sha256sum", shlex.quote(installed)).split()[0].decode()
        assert digest == hashlib.sha256(args.apk.read_bytes()).hexdigest(), "installed APK differs"
        report["apk_sha256"] = digest
        report["checks"].append("deployed archive hashes and exact inventory bind the generated replay bytes; no original script archive")
        title = p.start_title("--ei", "title_game", "0", "--es", "library_tree", shlex.quote(tree),
                              "--es", "library_directory", shlex.quote(directory), "--ez", "library_original", "true",
                              "--ez", "dialogue_probe", "true", "--es", "save_probe", run, "--es", "read_history_probe", run)
        start = next(button for button in title["buttons"] if button["id"] == title["start_action"])
        left, top, right, bottom = p.bounds(nav.startup.TITLE)
        x, y, w, h = start["bounds"]
        scale = min((right-left)/title["width"], (bottom-top)/title["height"])
        p.tap(round((left+right)/2+(x+w/2-title["width"]/2)*scale), round((top+bottom)/2+(y+h/2-title["height"]/2)*scale))
        music = room_state()
        token = music["token"]
        assert control(music, "PLAY")
        mark = tap(music, control(music, "SCENE"))
        scene = room_state(mark, scene=True, token=token)
        assert item(scene, 2) is None, "only the first original scene may be unlocked"
        mark = tap(scene, item(scene, 1))
        replay = p.wait("generated replay command through production Android operation 82", lambda: next((row for row in events()[mark:]
                        if row.get("command") == "native_replay_probe" and row.get("has_frame")
                        and row.get("delay_ms") == 250), None), timeout=30)
        assert replay["token"] != token, "replay must execute as a separate command, not reuse the retained room token"
        assert replay["platform_replaying"] is True, "platform save controls must enter replay mode"
        rows = events()
        replay_index = next(index for index in range(mark, len(rows)) if rows[index] == replay)
        boundary_rows = [row for row in rows[mark:replay_index] if row.get("room_replay")]
        assert boundary_rows and boundary_rows[-1]["token"] == token
        assert any(action == dict(kind="replay", value=boundary["scene"]["name"])
                   for row in rows[mark:replay_index] for action in row.get("room_actions", []))
        report["replay_command"] = replay
        report["checks"].append("original music to scene selection starts the generated command with a new token through the production replay path")
        returned = room_state(replay_index + 1, scene=True, token=token)
        assert returned["buttons"] == scene["buttons"], "EOF must restore the retained original scene controls"
        assert returned["platform_replaying"] is False, "platform must restore the caller's replay mode"
        assert not p.obj("page-1.json"), "replay contains no dialogue and must return to the suspended original room first"
        report["retained_room"] = returned
        report["checks"].append("generated command EOF returns to the original room token and identical original controls")
        tap(returned, control(returned, "CLOSE"))
        page = p.wait("original room returns to synthetic caller", lambda: p.obj("page-1.json"), timeout=30)
        assert (page["source"], page["line"]) == ("story1", 4), page
        assert not p.read("error.txt")
        assert all(row["token"] == token for row in events() if row.get("command") == "room")
        final_digest = p.adb("shell", "sha256sum", shlex.quote(installed)).split()[0].decode()
        assert final_digest == digest
        report["return_page"] = page
        report["checks"].append("closing the original room returns to story1:4 without errors under the same APK hash")
        report.update(passed=True, events=events(), synthetic_replay_executed=True)
    finally:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(dict(passed=True, run=run, checks=report["checks"])))


if __name__ == "__main__":
    main()
