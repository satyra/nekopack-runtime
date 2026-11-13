"""Metadata-only original/PK02 startup regression on an explicitly selected device.
No screenshot or dialogue text is transferred. Requires the debug APK and SAF grant.
"""
import argparse, importlib.util, json, shlex, sys, uuid
from pathlib import Path
from datetime import datetime, timezone
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_menu_sounds import TITLE
spec = importlib.util.spec_from_file_location("metadata_probe", Path(__file__).with_name("verify-translated-speaker.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


def dialogue_digest(probe, number):
    # Extract and hash on device; only the digest leaves Android.
    pattern = r's/^.*"text":("(\\.|[^"\\])*"),"effects":.*$/\1/p'
    path = f"files/{probe.run}/page-{number}.json"
    command = f"sed -n -E {shlex.quote(pattern)} {shlex.quote(path)} | sha256sum"
    digest = probe.adb("shell", "run-as", "org.nekopack.probe", "sh", "-c", shlex.quote(command)).split()[0].decode()
    assert digest != "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855", "dialogue hash extraction failed"
    return digest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--serial", required=True)
    parser.add_argument("--adb", default="D:/codex/tools/android/sdk/platform-tools/adb.exe")
    parser.add_argument("--translated", action="store_true")
    parser.add_argument("--pages", type=int, default=3)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    run = "pk02-" + ("translated-" if args.translated else "original-") + uuid.uuid4().hex[:8]
    p = module.MetadataProbe(args.adb, args.serial, run, {})
    title = p.start_title("--ei", "title_game", "0", "--es", "library_entry", "pk02-validation",
                          "--ez", "library_original", str(not args.translated).lower(),
                          "--ez", "dialogue_probe", "true")
    assert title["chinese_scenario"] == args.translated
    if title.get("tick_ms", 0) > 0:
        title = p.wait("title animation complete", lambda: (s if (s := p.obj("title-presented.json")) and s.get("tick_ms", 0) <= 0 else None))
    apk_path = p.adb("shell", "pm", "path", "org.nekopack.probe").decode().strip().removeprefix("package:")
    apk_sha256 = p.adb("shell", "sha256sum", apk_path).split()[0].decode()
    result = dict(run=run, serial=args.serial, checked_at=datetime.now(timezone.utc).isoformat(),
                  apk_sha256=apk_sha256, translated=args.translated, title_ready=True,
                  width=title["width"], height=title["height"], pages=[])
    if args.pages == 0:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(result, indent=2), encoding="utf-8")
        print(json.dumps(result))
        return
    button = next(b for b in title["buttons"] if b["id"] == title["start_action"])
    assert button["enabled"]
    left, top, right, bottom = p.bounds(TITLE)
    width, height = title["width"], title["height"]
    x, y, w, h = button["bounds"]
    scale = min((right-left)/width, (bottom-top)/height)
    p.tap(round((left+right)/2 + (x+w/2-width/2)*scale),
          round((top+bottom)/2 + (y+h/2-height/2)*scale))
    for number in range(1, args.pages+1):
        page = p.wait("page metadata", lambda: p.obj(f"page-{number}.json"))
        p.wait("text complete", lambda: (s if (s := p.obj(f"text-{number}.json")) and s["complete"] else None))
        result["pages"].append(dict(sequence=number, source=page["source"], line=page["line"], speaker_chars=len(page["speaker"]), dialogue_sha256=dialogue_digest(p, number)))
        if number < args.pages:
            layout = p.obj("layout.json")
            p.tap(layout["stage_width"]//2, round(layout["stage_height"]*.4))
    assert not p.read("error.txt")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2), encoding="utf-8")
    print(json.dumps(result))

if __name__ == "__main__":
    main()
