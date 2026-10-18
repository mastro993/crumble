#!/usr/bin/env python3
"""Fail if a crate crosses the GPUI / persistence / Objective-C boundaries.

The crate graph is supposed to keep domain crates free of GPUI and Diesel out of
the UI; this check encodes that so a new dependency cannot silently undo it.
Run from the workspace root. No cargo build is required.
"""

from __future__ import annotations

import sys
from pathlib import Path

if sys.version_info < (3, 11):
    sys.exit(f"needs Python 3.11+ for tomllib; this is {sys.version.split()[0]}")

import tomllib

ROOT = Path(__file__).resolve().parents[1]

GPUI = frozenset({"gpui", "gpui-pre"})
GPUI_KIT = frozenset({"gpui-kit"})
DIESEL = frozenset({"diesel", "diesel_migrations"})

# Who may take each group; every other package, including a new one, may not.
GPUI_USERS = frozenset({"crumble", "crumble-ui", "crumble-theme", "crumble-sync"})
GPUI_KIT_USERS = GPUI_USERS - {"crumble-sync"}
DIESEL_USERS = frozenset({"crumble-db", "crumble-store", "crumble-notes"})
OBJC_USERS = frozenset({"crumble-eventkit", "crumble-audio", "crumble-notify"})

# Crates that call `unsafe` under their own lint table (AGENTS.md lists the ADRs).
UNSAFE_CRATES = frozenset(
    {
        "crumble-eventkit",
        "crumble-audio",
        "crumble-transcribe",
        "crumble-summarize",
        "crumble-notify",
    }
)


def is_objc(dep: str) -> bool:
    return dep == "block2" or dep.startswith("objc2")


def forbidden(package: str, dep: str) -> bool:
    # A GPUI user brings GPUI with it, so depending on one counts as depending on GPUI.
    # ponytail: only GPUI is followed through workspace crates; `crumble-ui` reaches Diesel and
    # objc2 through `crumble-store` and `crumble-eventkit` by design. Use `cargo metadata` if a
    # transitive Diesel or objc2 rule is ever wanted.
    return (
        (dep in GPUI | GPUI_USERS and package not in GPUI_USERS)
        or (dep in GPUI_KIT | GPUI_KIT_USERS and package not in GPUI_KIT_USERS)
        or (dep in DIESEL and package not in DIESEL_USERS)
        or (is_objc(dep) and package not in OBJC_USERS)
    )


# Source tokens that would smuggle a forbidden crate past Cargo.toml.
def forbidden_tokens(package: str) -> tuple[str, ...]:
    tokens: list[str] = []
    if package not in GPUI_USERS:
        tokens.append("gpui::")
    if package not in GPUI_KIT_USERS:
        tokens.append("gpui_kit::")
    if package not in DIESEL_USERS:
        tokens += ["diesel::", "diesel_migrations", "SqliteConnection"]
    return tuple(tokens)


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
    if name in UNSAFE_CRATES:
        rust = lints.get("rust") if isinstance(lints, dict) else None
        if not isinstance(rust, dict) or rust.get("unsafe_code") != "deny":
            return [f"{path}: {name} must set lints.rust.unsafe_code = \"deny\""]
        return []
    if lints.get("workspace") is not True:
        return [f"{path}: [lints] workspace = true is required"]
    return []


def package_name(key: str, spec: object, workspace_deps: dict) -> str:
    """The crate a dependency entry pulls in, through `package =` renames here or in the root."""
    if isinstance(spec, dict) and spec.get("workspace") is True:
        spec = workspace_deps.get(key, {})
    if isinstance(spec, dict) and isinstance(spec.get("package"), str):
        return spec["package"]
    return key


def check_forbidden_deps(path: Path, data: dict, workspace_deps: dict) -> list[str]:
    name = data.get("package", {}).get("name")
    errors: list[str] = []
    for table, deps in walk_dep_tables(data):
        for key, spec in deps.items():
            dep = package_name(key, spec, workspace_deps)
            if forbidden(name, dep):
                errors.append(f"{path}: {name} must not depend on {dep} as {key} ({table})")
    return errors


def check_forbidden_src(path: Path, data: dict) -> list[str]:
    name = data.get("package", {}).get("name")
    errors: list[str] = []
    for src in (ROOT / path.parent).rglob("*.rs"):
        text = src.read_text()
        for token in forbidden_tokens(name):
            if token in text:
                errors.append(f"{src.relative_to(ROOT)}: {name} must not mention {token!r}")
    return errors


def main() -> int:
    errors: list[str] = []
    manifests = member_manifests()
    if not manifests:
        print("no crate manifests found", file=sys.stderr)
        return 1
    root = tomllib.loads((ROOT / "Cargo.toml").read_text())
    workspace_deps = root.get("workspace", {}).get("dependencies", {})
    for path in manifests:
        data = tomllib.loads(path.read_text())
        errors.extend(check_workspace_versions(path.relative_to(ROOT), data))
        errors.extend(check_lints(path.relative_to(ROOT), data))
        errors.extend(check_forbidden_deps(path.relative_to(ROOT), data, workspace_deps))
        errors.extend(check_forbidden_src(path.relative_to(ROOT), data))
    if errors:
        print("crate boundary check failed:", file=sys.stderr)
        for error in errors:
            print(f"  {error}", file=sys.stderr)
        return 1
    print(f"ok ({len(manifests)} manifests)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
