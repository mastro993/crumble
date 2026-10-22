#!/usr/bin/env python3
"""Copy Handy's model list from its latest release, for `.github/workflows/handy-catalog.yml`.

Crumble's Local Models list is Handy's `catalog.json`, copied unchanged (ADR 0010). This reads
Handy's latest release tag, and when that release's list differs from the copy in
`crates/local-models/handy/`, writes it there with `HANDY_SOURCE` naming the tag and commit,
and writes a Markdown summary of what changed for the pull request. Set GH_TOKEN to raise
GitHub's rate limit. Prints `changed=true|false` and `tag=<tag>` for GitHub Actions.

    python3 scripts/sync-handy-catalog.py --summary /tmp/summary.md
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FOLDER = ROOT / "crates/local-models/handy"
REPO = "cjpais/Handy"
CATALOG = "src-tauri/src/catalog/catalog.json"


def get(url: str) -> bytes:
    request = urllib.request.Request(url, headers={"User-Agent": "crumble-catalog-sync"})
    token = os.environ.get("GH_TOKEN")
    if token and url.startswith("https://api.github.com/"):
        request.add_header("Authorization", f"Bearer {token}")
    with urllib.request.urlopen(request, timeout=60) as response:
        return response.read()


def api(path: str) -> dict:
    return json.loads(get(f"https://api.github.com/repos/{REPO}/{path}"))


def default_file(model: dict) -> dict:
    return next(
        (file for file in model.get("files", []) if file.get("quant") == model.get("default_quant")),
        {},
    )


def summary(old: dict, new: dict, tag: str, commit: str) -> str:
    """What changed between two lists, by slug; a new revision or sha256 is called out, as it
    decides which weights users download."""
    before = {model["slug"]: model for model in old.get("models", [])}
    after = {model["slug"]: model for model in new.get("models", [])}
    lines = [
        f"Copies Handy's model list from [{tag}](https://github.com/{REPO}/tree/{commit}) "
        f"(`{commit[:7]}`), as ADR 0010 describes.",
        "",
    ]
    if old.get("catalog_version") != new.get("catalog_version"):
        lines += [
            f"**List format changed** from {old.get('catalog_version')} to "
            f"{new.get('catalog_version')}: the catalog test fails until the parser reads it.",
            "",
        ]
    added = sorted(after.keys() - before.keys())
    removed = sorted(before.keys() - after.keys())
    changed = sorted(slug for slug in before.keys() & after.keys() if before[slug] != after[slug])
    if added:
        lines += ["**Added**", *(f"- {after[slug]['name']} (`{slug}`)" for slug in added), ""]
    if removed:
        lines += [
            "**Removed** (downloaded copies stay usable as \"No longer offered\")",
            *(f"- {before[slug]['name']} (`{slug}`)" for slug in removed),
            "",
        ]
    if changed:
        lines.append("**Changed**")
        for slug in changed:
            fields = sorted(
                key
                for key in before[slug].keys() | after[slug].keys()
                if before[slug].get(key) != after[slug].get(key)
            )
            weights = (
                before[slug].get("revision") != after[slug].get("revision")
                or default_file(before[slug]).get("sha256") != default_file(after[slug]).get("sha256")
            )
            flag = " **new weights: check the revision and sha256**" if weights else ""
            lines.append(f"- {after[slug]['name']} (`{slug}`): {', '.join(fields)}{flag}")
        lines.append("")
    if not (added or removed or changed):
        lines += ["Only the list's metadata changed.", ""]
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--summary", type=Path, required=True)
    args = parser.parse_args()

    tag = api("releases/latest")["tag_name"]
    commit = api(f"commits/{tag}")["sha"]
    fresh = get(f"https://raw.githubusercontent.com/{REPO}/{commit}/{CATALOG}")
    copy = FOLDER / "catalog.json"
    changed = fresh != copy.read_bytes()
    if changed:
        old, new = json.loads(copy.read_bytes()), json.loads(fresh)
        args.summary.write_text(summary(old, new, tag, commit))
        copy.write_bytes(fresh)
        (FOLDER / "HANDY_SOURCE").write_text(f"{tag} {commit}\n")
    print(f"changed={'true' if changed else 'false'}")
    print(f"tag={tag}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
