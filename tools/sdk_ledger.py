#!/usr/bin/env python3
"""BerryUI SDK API ledger guard (SDK-S0).

The machine-readable truth is tools/sdk_api.toml. This script:

1. validates schema, vocabularies, owner, spec links, and breaking history;
2. extracts the curated public names from user/berry-ui/src/sdk.rs;
3. requires 100% facade coverage in both directions;
4. generates the availability matrix inside
   docs/BerryUI-SDKS0ControlSpecAPIGovernance-v1.0.md.

Use `--check` in CI and `--write` after an intentional sdk.rs/API change.
"""

from __future__ import annotations

import argparse
import re
import sys
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LEDGER = ROOT / "tools/sdk_api.toml"
SDK = ROOT / "user/berry-ui/src/sdk.rs"
DOC = ROOT / "docs/BerryUI-SDKS0ControlSpecAPIGovernance-v1.0.md"
BEGIN = "<!-- BEGIN generated:sdk-availability -->"
END = "<!-- END generated:sdk-availability -->"


class LedgerError(Exception):
    pass


MODULES = {
    "desc", "diagnostic", "gesture", "layout", "metrics", "navigation", "nav_stack", "pull_refresh", "swipe",
    "theme", "text_input", "sem", "widgets", "crate", "sdk",
}


def exported_name(part: str) -> str | None:
    part = part.strip()
    if not part or part.startswith("//") or part.startswith("#"):
        return None
    # `path::Name as Exported` exports only the alias; without `as`, the last
    # path segment is the facade name.
    if " as " in part:
        name = part.rsplit(" as ", 1)[1].strip()
    else:
        name = part.rsplit("::", 1)[-1].strip()
    return name if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", name) else None


def public_facade_names() -> list[str]:
    text = SDK.read_text(encoding="utf-8")
    names: list[str] = []
    for match in re.finditer(r"^\s*pub\s+use\s+crate::(.+?);", text, re.M | re.S):
        statement = match.group(0)
        if "{" not in statement or not statement.rstrip(";").endswith("}"):
            names.append(exported_name(statement[statement.index("crate::") : -1]))
            continue
        payload = statement.split("{", 1)[1].rsplit("}", 1)[0]
        for part in payload.split(","):
            names.append(exported_name(part))
    for match in re.finditer(r"^\s*pub\s+use\s+(?!crate::)([^;]+);", text, re.M | re.S):
        names.append(exported_name(match.group(1).strip()))
    for match in re.finditer(
        r"^\s*pub\s+(?:const\s+)?(?:fn|struct|enum|type|const|trait)\s+"
        r"([A-Za-z_][A-Za-z0-9_]*)",
        text,
        re.M,
    ):
        names.append(match.group(1))
    out: list[str] = []
    for name in names:
        if name and name not in out:
            out.append(name)
    return out


def load() -> dict:
    with LEDGER.open("rb") as stream:
        return tomllib.load(stream)


def validate(data: dict) -> list[dict]:
    meta = data.get("meta")
    if not isinstance(meta, dict):
        raise LedgerError("missing [meta]")
    if meta.get("format") != 1:
        raise LedgerError("meta.format must be 1")
    channels = meta.get("channel_vocab", [])
    stability = meta.get("stability_vocab", [])
    capabilities = meta.get("capability_vocab", [])
    owner = meta.get("owner")
    if owner != "berry-ui":
        raise LedgerError("meta.owner must be berry-ui")

    required = {
        "id", "kind", "module", "since", "channel", "capability", "owner",
        "stability", "spec", "breaking",
    }
    items = data.get("item")
    if not isinstance(items, list) or not items:
        raise LedgerError("no [[item]] entries")
    ids: set[str] = set()
    for index, raw in enumerate(items):
        if not isinstance(raw, dict):
            raise LedgerError(f"item[{index}] is not a table")
        missing = required - raw.keys()
        if missing:
            raise LedgerError(f"item[{index}] missing: {', '.join(sorted(missing))}")
        item_id = raw["id"]
        if not isinstance(item_id, str) or not item_id.startswith("berry_ui::sdk::"):
            raise LedgerError(f"item[{index}].id invalid: {item_id!r}")
        if item_id in ids:
            raise LedgerError(f"duplicate id: {item_id}")
        ids.add(item_id)
        if raw["kind"] not in {"fn", "type", "const"}:
            raise LedgerError(f"{item_id}: kind must be fn/type/const")
        if raw["module"] not in MODULES:
            raise LedgerError(f"{item_id}: unknown module {raw['module']!r}")
        if not isinstance(raw["since"], str) or not raw["since"]:
            raise LedgerError(f"{item_id}: since empty")
        if raw["channel"] not in channels:
            raise LedgerError(f"{item_id}: unknown channel {raw['channel']!r}")
        if raw["capability"] not in capabilities:
            raise LedgerError(f"{item_id}: unknown capability {raw['capability']!r}")
        if raw["owner"] != owner:
            raise LedgerError(f"{item_id}: owner mismatch")
        if raw["stability"] not in stability:
            raise LedgerError(f"{item_id}: unknown stability {raw['stability']!r}")
        spec = raw["spec"]
        if not isinstance(spec, str) or not (ROOT / spec).is_file():
            raise LedgerError(f"{item_id}: spec missing or not a file: {spec}")
        breaking = raw["breaking"]
        if not isinstance(breaking, list):
            raise LedgerError(f"{item_id}: breaking must be an array")
        if raw["stability"] == "deprecated" and not breaking:
            raise LedgerError(f"{item_id}: deprecated item lacks successor")
        for change in breaking:
            if not isinstance(change, dict):
                raise LedgerError(f"{item_id}: breaking entry must be a table")
            if not change.get("successor"):
                raise LedgerError(f"{item_id}: breaking change lacks successor")
    return items


def compare_facade(items: list[dict]) -> None:
    ledger_names = {item["id"].removeprefix("berry_ui::sdk::") for item in items}
    source_names = set(public_facade_names())
    missing = sorted(source_names - ledger_names)
    stale = sorted(ledger_names - source_names)
    if missing or stale:
        details = []
        if missing:
            details.append("missing from ledger: " + ", ".join(missing))
        if stale:
            details.append("absent from sdk.rs facade: " + ", ".join(stale))
        raise LedgerError("; ".join(details))


def matrix(items: list[dict]) -> str:
    header = (
        "| API | kind | since | channel | stability | spec |\n"
        "|---|---|---|---|---|---|\n"
    )
    rows = []
    for item in sorted(items, key=lambda value: value["id"]):
        spec = item["spec"]
        rows.append(
            f"| `{item['id']}` | {item['kind']} | {item['since']} | "
            f"{item['channel']} | {item['stability']} | `{spec}` |\n"
        )
    return header + "".join(rows)


def write_doc(items: list[dict]) -> None:
    text = DOC.read_text(encoding="utf-8")
    if BEGIN not in text or END not in text:
        raise LedgerError(f"{DOC} lacks generated markers")
    before, rest = text.split(BEGIN, 1)
    _, after = rest.split(END, 1)
    updated = before + BEGIN + "\n\n" + matrix(items) + "\n" + END + after
    DOC.write_text(updated, encoding="utf-8")


def check_doc(items: list[dict]) -> None:
    text = DOC.read_text(encoding="utf-8")
    if BEGIN not in text or END not in text:
        raise LedgerError(f"{DOC} lacks generated markers")
    _, rest = text.split(BEGIN, 1)
    current, _ = rest.split(END, 1)
    expected = "\n\n" + matrix(items) + "\n"
    if current != expected:
        raise LedgerError("generated availability matrix is stale; run --write")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()
    if args.check == args.write:
        parser.error("choose exactly one of --check/--write")
    data = load()
    items = validate(data)
    compare_facade(items)
    if args.write:
        write_doc(items)
        print(f"sdk ledger: wrote {len(items)} entries")
    else:
        check_doc(items)
        print(f"sdk ledger: PASS ({len(items)} entries)")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (LedgerError, OSError, tomllib.TOMLDecodeError) as error:
        print(f"sdk ledger: FAIL: {error}", file=sys.stderr)
        raise SystemExit(1)
