#!/usr/bin/env python3
"""Fail if a crate crosses the GPUI / persistence / Objective-C boundaries.

The crate graph is supposed to keep domain crates free of GPUI and Diesel out of
the UI; this check encodes that so a new dependency cannot silently undo it.
Run from the workspace root. No cargo build is required.
"""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

GPUI = frozenset({"gpui", "gpui-kit", "gpui-pre"})
DIESEL = frozenset({"diesel", "diesel_migrations"})
OBJC = frozenset(
    {
        "block2",
        "objc2",
        "objc2-core-foundation",
        "objc2-core-graphics",
        "objc2-event-kit",
        "objc2-foundation",
    }
)

# Direct crates.io / workspace deps each package must not take.
FORBIDDEN_DEPS: dict[str, frozenset[str]] = {
    "crumble-calendar": GPUI | DIESEL | OBJC,
    "crumble-store": GPUI | OBJC,
    "crumble-eventkit": GPUI | DIESEL,
    "crumble-sync": frozenset({"gpui-kit"}) | DIESEL | OBJC,
    "crumble-ui": DIESEL | OBJC,
    "crumble-theme": DIESEL | OBJC,
    "crumble": DIESEL | OBJC,
}

# Source tokens that would smuggle a forbidden crate past Cargo.toml.
FORBIDDEN_SRC: dict[str, tuple[str, ...]] = {
    "crates/calendar": ("gpui::", "gpui_kit::", "diesel::"),
    "crates/store": ("gpui::", "gpui_kit::"),
    "crates/eventkit": ("gpui::", "gpui_kit::"),
    "crates/sync": ("gpui_kit::", "diesel::"),
    "crates/ui": ("diesel::", "diesel_migrations", "SqliteConnection"),
}


def member_manifests() -> list[Path]:
    return sorted(ROOT.glob("crates/*/Cargo.toml")) + sorted(ROOT.glob("apps/*/Cargo.toml"))


def walk_dep_tables(data: dict) -> list[tuple[str, dict]]:
    found: list[tuple[str, dict]] = []

    def walk(node: object, path: str) -> None:
        if not isinstance(node, dict):
            return
        for key, value in node.items():
            here = f"{path}.{key}" if path else key
            if key in {"dependencies", "dev-dependencies", "build-dependencies"} and isinstance(
                value, dict
            ):
                found.append((here, value))
            else:
                walk(value, here)

    walk(data, "")
    return found


def check_workspace_versions(path: Path, data: dict) -> list[str]:
    errors: list[str] = []
    for table, deps in walk_dep_tables(data):
        for name, spec in deps.items():
            if isinstance(spec, str):
                errors.append(
                    f"{path}: {table}.{name} pins version {spec!r}; "
                    "take it from [workspace.dependencies]"
                )
            elif isinstance(spec, dict) and spec.get("workspace") is not True:
                errors.append(
                    f"{path}: {table}.{name} must set workspace = true (got {spec})"
                )
    return errors


def check_lints(path: Path, data: dict) -> list[str]:
    name = data.get("package", {}).get("name")
    lints = data.get("lints", {})
    if name == "crumble-eventkit":
        rust = lints.get("rust") if isinstance(lints, dict) else None
        if not isinstance(rust, dict) or rust.get("unsafe_code") != "deny":
            return [
                f"{path}: crumble-eventkit must set lints.rust.unsafe_code = \"deny\""
            ]
        return []
    if lints.get("workspace") is not True:
        return [f"{path}: [lints] workspace = true is required"]
    return []


def check_forbidden_deps(path: Path, data: dict) -> list[str]:
    name = data.get("package", {}).get("name")
    forbidden = FORBIDDEN_DEPS.get(name)
    if not forbidden:
        return []
    errors: list[str] = []
    for table, deps in walk_dep_tables(data):
        for dep in deps:
            if dep in forbidden:
                errors.append(f"{path}: {name} must not depend on {dep} ({table})")
    return errors


def check_forbidden_src() -> list[str]:
    errors: list[str] = []
    for rel, tokens in FORBIDDEN_SRC.items():
        root = ROOT / rel
        for src in root.rglob("*.rs"):
            text = src.read_text()
            for token in tokens:
                if token in text:
                    errors.append(f"{src.relative_to(ROOT)}: {rel} must not mention {token!r}")
    return errors


def main() -> int:
    errors: list[str] = []
    manifests = member_manifests()
    if not manifests:
        print("no crate manifests found", file=sys.stderr)
        return 1
    for path in manifests:
        data = tomllib.loads(path.read_text())
        errors.extend(check_workspace_versions(path.relative_to(ROOT), data))
        errors.extend(check_lints(path.relative_to(ROOT), data))
        errors.extend(check_forbidden_deps(path.relative_to(ROOT), data))
    errors.extend(check_forbidden_src())
    if errors:
        print("crate boundary check failed:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    print(f"ok ({len(manifests)} manifests)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
