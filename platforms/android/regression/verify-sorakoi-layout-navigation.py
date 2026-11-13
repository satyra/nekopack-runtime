"""Sorakoi layout and original NEXT/BACK regression; metadata and hashes only.

Requires the debug APK and the pk02-validation library entry. UI hierarchy XML
stays on Android: MetadataProbe.bounds transfers only matching numeric bounds.
"""
import argparse
import importlib.util
import json
import shlex
import uuid
from datetime import datetime, timezone
from pathlib import Path


spec = importlib.util.spec_from_file_location(
    "pk02_startup", Path(__file__).with_name("verify-pk02-startup.py"))
startup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(startup)
SIDE = "原版右侧悬浮菜单"


class NavigationProbe(startup.module.MetadataProbe):
    def obj(self, name):
        if not name.startswith("page-"):
            return super().obj(name)
        # Never transfer speaker or dialogue strings, including during polling.
        pattern = (r's/^\{"sequence":([0-9]+),"source":("(\\.|[^"\\])*"),'
                   r'"line":([0-9]+),.*$/{"sequence":\1,"source":\2,"line":\4}/p')
        path = f"files/{self.run}/{name}"
        command = (f"if test -f {shlex.quote(path)}; then "
                   f"sed -n -E {shlex.quote(pattern)} {shlex.quote(path)}; fi")
        raw = self.adb("shell", "run-as", "org.nekopack.probe", "sh", "-c",
                       shlex.quote(command)).strip()
        return json.loads(raw) if raw else None


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", default="D:/codex/tools/android/sdk/platform-tools/adb.exe")
    parser.add_argument("--translated", action="store_true")
    parser.add_argument("--pages", type=int, default=30,
                        help="additional dialogue pages after replaying the selection")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    assert args.pages >= 0
    run = "sorakoi-navigation-" + uuid.uuid4().hex[:8]
    p = NavigationProbe(args.adb, args.serial, run, {})
    result = dict(run=run, serial=args.serial, translated=args.translated,
                  checked_at=datetime.now(timezone.utc).isoformat(), pages=[],
                  selections=[], actions=[])
    current_sequence = 0

    def write_result():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2), encoding="utf-8")

    def press(label, state, bounds):
        left, top, right, bottom = p.bounds(label)
        width, height = state.get("size", [state.get("width"), state.get("height")])
        x, y, w, h = bounds
        scale = min((right-left)/width, (bottom-top)/height)
        p.tap(round((left+right)/2+(x+w/2-width/2)*scale),
              round((top+bottom)/2+(y+h/2-height/2)*scale))

    def complete(number, initial=False):
        nonlocal current_sequence
        page = p.wait("page metadata", lambda: p.obj(f"page-{number}.json"))
        p.wait("text complete", lambda: (s if (s := p.obj(f"text-{number}.json"))
                                         and s["complete"] else None))
        layout = p.wait("dialogue layout", lambda: p.obj(f"dialogue-{number}.json"))
        if initial:
            assert layout["text_size"] == 30, (number, "text_size", layout["text_size"])
            assert layout["line_span"] == 8, (number, "line_span", layout["line_span"])
            if layout["name_size"]:
                assert layout["name_size"] == 30, (number, "name_size", layout["name_size"])
                assert layout["name_position"] == [0, 0], (number, "name_position")
        result["pages"].append(dict(**page, layout=layout,
                                    dialogue_sha256=startup.dialogue_digest(p, number)))
        write_result()
        current_sequence = number
        return page

    def advance():
        layout = p.obj("layout.json")
        p.tap(layout["stage_width"]//2, round(layout["stage_height"]*.4))

    def side_action(action):
        # The host refreshes on each dialogue. Tap a button center rather than
        # a menu edge, where scaled rounding can send the touch to the scene.
        state = p.wait("enabled " + action, lambda: (s if (s := p.obj("side-menu-presented.json"))
                       and any(b["action"] == action and b["enabled"] for b in s["buttons"]) else None))
        button = next(b for b in state["buttons"] if b["action"] == action)
        assert button["enabled"] and button.get("supported", True), (action, "unavailable")
        if not state["keep_icon"] and not state["expanded"]:
            press(SIDE, state, button["bounds"])
            state = p.wait("revealed side menu", lambda: (s if (s := p.obj("side-menu-presented.json"))
                           and s["render_sequence"] > state["render_sequence"] and s["expanded"] else None))
            button = next(b for b in state["buttons"] if b["action"] == action)
        result["actions"].append(dict(action=action, render_sequence=state["render_sequence"],
                                      bounds=button["bounds"]))
        write_result()
        press(SIDE, state, button["bounds"])
        event = p.wait("dispatched " + action, lambda: next((event for event in reversed(p.events("side-menu-events.jsonl"))
                       if event["action"] == action and event["render_sequence"] > state["render_sequence"]), None))
        assert event["story_sequence"] == current_sequence, "shortcut dispatched on an unexpected page"

    def selection(after_token=None):
        value = p.wait("selection metadata", lambda: (s if (s := p.obj("selection-presented.json"))
                       and (after_token is None or s["token"] != after_token) else None), timeout=900)
        checkpoint = {key: value["checkpoint"][key] for key in ("source", "line", "scene", "slots")}
        result["selections"].append(dict(token=value["token"], checkpoint=checkpoint,
                                         options_count=value["options_count"], selected_id=1))
        return value, checkpoint

    def choose_first(state):
        count = state["options_count"]
        assert count > 0, "empty selection"
        left, top, right, bottom = p.bounds("android:id/select_dialog_listview", "resource-id")
        p.tap((left+right)//2, top+max(1, min(24, (bottom-top)//(2*count))))

    def next_event(number, previous_selection):
        def poll():
            if p.obj(f"page-{number}.json"):
                return "page", None
            state = p.obj("selection-presented.json")
            if state and state["token"] != previous_selection:
                return "selection", state
            return None
        return p.wait("next dialogue or selection", poll)

    try:
        title = p.start_title("--ei", "title_game", "0", "--es", "library_entry", "pk02-validation",
                              "--ez", "library_original", str(not args.translated).lower(),
                              "--ez", "dialogue_probe", "true")
        assert title["chinese_scenario"] == args.translated
        if title.get("tick_ms", 0) > 0:
            title = p.wait("title animation complete", lambda: (s if (s := p.obj("title-presented.json"))
                           and s.get("tick_ms", 0) <= 0 else None))
        apk = p.adb("shell", "pm", "path", "org.nekopack.probe").decode().strip().removeprefix("package:")
        result["apk_sha256"] = p.adb("shell", "sha256sum", apk).split()[0].decode()
        button = next(b for b in title["buttons"] if b["id"] == title["start_action"])
        assert button["enabled"]
        press(startup.TITLE, title, button["bounds"])
        for number in range(1, 4):
            complete(number, initial=True)
            if number < 3:
                advance()
        side_action("NEXT")
        p.control("确定")
        after_choice = current_sequence + 1
        first, original_checkpoint = selection()
        choose_first(first)
        complete(after_choice)
        side_action("BACK")
        p.control("取消")
        assert p.obj("selection-presented.json")["token"] == first["token"]
        assert p.obj(f"page-{after_choice+1}.json") is None
        result["back_cancel_preserves_page"] = True
        side_action("BACK")
        p.control("确定")
        restored, restored_checkpoint = selection(first["token"])
        assert restored_checkpoint == original_checkpoint, "BACK selection checkpoint changed"
        result["back_checkpoint_matches"] = True
        choose_first(restored)
        complete(after_choice + 1)
        previous_selection = restored["token"]
        for number in range(after_choice + 2, after_choice + 2 + args.pages):
            advance()
            while True:
                kind, state = next_event(number, previous_selection)
                if kind == "page":
                    break
                state, _ = selection(previous_selection)
                previous_selection = state["token"]
                choose_first(state)
            complete(number)
        assert not p.read("error.txt")
        result["passed"] = True
        write_result()
        print(json.dumps(dict(run=run, passed=True, pages=len(result["pages"]),
                              selections=len(result["selections"]), output=str(args.output))))
    finally:
        write_result()


if __name__ == "__main__":
    main()
