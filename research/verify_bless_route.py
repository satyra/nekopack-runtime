"""Verify BLESS route trace metadata without reporting dialogue or artwork."""

import argparse
import hashlib
import json
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("trace", type=Path)
    parser.add_argument("--ending", required=True, choices=("early", "full"))
    args = parser.parse_args()

    root = args.trace
    boundary = json.loads((root / "boundary.json").read_text(encoding="utf8"))
    message = (root / "boundary.txt").read_text(encoding="utf8")
    assert boundary["source"] == "title" and boundary["line"] == 9
    assert message.endswith(": unsupported story preview command map"), message

    pages = sorted(
        (p for p in root.glob("*.json") if p.stem.isdecimal()),
        key=lambda p: int(p.stem),
    )
    assert pages
    digest = hashlib.sha256()
    sources = set()
    last_steps = 0
    credits = []
    epilogue = {}
    for index, path in enumerate(pages):
        page = json.loads(path.read_text(encoding="utf8"))
        assert path.stem == str(index) and page["sequence"] == index + 1
        assert last_steps < page["steps"] < boundary["steps"]
        last_steps = page["steps"]
        sources.add(page["source"])
        if page["source"] in {"EP01", "EP02"}:
            epilogue.setdefault(page["source"], index)
        if "presentation staffroll" in page["effects"]:
            credits.append(index)
        digest.update(json.dumps([page["source"], page["line"], last_steps, page["slots"]]).encode())

    slots = {(bank, slot): value for bank, slot, value in boundary["slots"]}
    assert slots[76, 100] == 1  # First selection, F013203.
    if args.ending == "full":
        assert credits == [epilogue["EP02"]] and epilogue["EP01"] < credits[0]
        assert all(slots[71, slot] == 1 for slot in range(900, 905))
        assert slots[71, 804] == 1
    else:
        assert "024101D" in sources and "EP02" not in sources
        assert slots[71, 904] == 0

    print(json.dumps({
        "ending": args.ending,
        "pages": len(pages),
        "steps": boundary["steps"],
        "title_line": boundary["line"],
        "chapter_clear": [slots[71, slot] for slot in range(900, 904)],
        "all_clear": slots[71, 904],
        "control_flow_sha256": digest.hexdigest(),
    }, indent=2))


if __name__ == "__main__":
    main()
