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
    "desc", "diagnostic", "gesture", "layout", "metrics", "navigation", "nav_stack", "pull_refresh", "swipe", "text_area",
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


def selftest() -> int:
    """自检合成腿（design_ledger --selftest 同款风格；零真仓依赖——合成
    sdk.rs+TOML+doc tmp 树直调 validate/compare_facade/check_doc 全链）。

    覆盖面：
      1. exported_name 三形（裸路径/别名/非法名与注释跳过）；
      2. public_facade_names 提取（crate:: 路径/花括号组/非 crate re-export/
         fn/struct/enum/type/const/trait 声明面 + 去重保序）；
      3. validate 全链：合成最小合规 TOML 过检，单点变异（meta.format/
         owner/id 前缀/重复 id/kind/module/channel/capability/stability/
         spec 缺件/breaking 空/deprecated 无后继）逐项拒面文案钉板；
      4. compare_facade 双向对账（sdk.rs 多出 = missing、ledger 多出 =
         stale、恰等过）；
      5. matrix 黄金行形（id 定序 + 表头）；
      6. write_doc→check_doc 往返（fresh 过、漂移红、markers 缺失拒）。
    """
    import tempfile

    # ① exported_name 三形。
    assert exported_name("layout::Rect") == "Rect"
    assert exported_name("layout::Rect as UiRect") == "UiRect"
    assert exported_name("  Rect ") == "Rect"
    assert exported_name("// comment") is None
    assert exported_name("") is None
    assert exported_name("bad-name") is None
    assert exported_name("9start") is None
    assert exported_name("theme::Color as 9bad") is None

    # ② public_facade_names 提取（合成 sdk.rs 全形态）。
    sdk_src = (
        "pub use crate::layout::{Rect, Edge as UiEdge};\n"
        "pub use crate::theme::Palette;\n"
        "pub use external_crate::Ext;\n"
        "pub fn make_rect() -> Rect { Rect }\n"
        "pub const MAX: u32 = 4;\n"
        "pub struct Marker;\n"
        "pub enum Kind { A }\n"
        "pub type Alias = u8;\n"
        "pub trait Draw {}\n"
        "pub const fn helper() {}\n"
        "fn private_thing() {}\n"
        "pub use crate::layout::Rect;\n"  # 重复——去重保序
    )
    real_sdk = globals()["SDK"]
    globals()["SDK"] = _SyntheticSdk(sdk_src)
    try:
        names = public_facade_names()
    finally:
        globals()["SDK"] = real_sdk
    assert names == [
        "Rect", "UiEdge", "Palette", "Ext",
        "make_rect", "MAX", "Marker", "Kind", "Alias", "Draw", "helper",
    ], f"提取面漂移: {names}"

    # ③④ validate + compare_facade 全链（合成 tmp 树真跑）。
    spec_rel = "docs/spec.md"
    items = [
        {"id": "berry_ui::sdk::Draw", "kind": "fn", "module": "theme",
         "since": "F2", "channel": "in-tree", "capability": "none",
         "owner": "berry-ui", "stability": "stable", "spec": spec_rel,
         "breaking": []},
        {"id": "berry_ui::sdk::Rect", "kind": "type", "module": "layout",
         "since": "F2", "channel": "in-tree", "capability": "none",
         "owner": "berry-ui", "stability": "stable", "spec": spec_rel,
         "breaking": []},
    ]

    def item_toml(entries) -> str:
        lines = [
            "[meta]",
            "format = 1",
            'owner = "berry-ui"',
            'channel_vocab = ["in-tree", "sdk"]',
            'stability_vocab = ["stable", "deprecated"]',
            'capability_vocab = ["none", "gpu"]',
            "",
        ]
        for entry in entries:
            lines.append("[[item]]")
            lines.append(f'id = "{entry["id"]}"')
            lines.append(f'kind = "{entry["kind"]}"')
            lines.append(f'module = "{entry["module"]}"')
            lines.append(f'since = "{entry["since"]}"')
            lines.append(f'channel = "{entry["channel"]}"')
            lines.append(f'capability = "{entry["capability"]}"')
            lines.append('owner = "berry-ui"')
            lines.append(f'stability = "{entry["stability"]}"')
            lines.append(f'spec = "{entry["spec"]}"')
            lines.append("breaking = []")
        return "\n".join(lines) + "\n"

    import sdk_ledger as mod

    with tempfile.TemporaryDirectory() as tmp:
        tmp_path = Path(tmp)
        # spec 文件须在 ROOT 下真实存在（validate 查 is_file）。
        (tmp_path / spec_rel).parent.mkdir(parents=True, exist_ok=True)
        (tmp_path / spec_rel).write_text("spec\n", encoding="utf-8")
        ledger = tmp_path / "sdk_api.toml"
        ledger.write_text(item_toml(items), encoding="utf-8")
        sdk_file = tmp_path / "sdk.rs"
        sdk_file.write_text(
            "pub use crate::layout::Rect;\n"
            "pub use crate::theme::Draw;\n",
            encoding="utf-8",
        )
        doc = tmp_path / "doc.md"
        doc.write_text(
            "head\n" + BEGIN + "\nold\n" + END + "\ntail\n", encoding="utf-8",
        )
        real = {
            "LEDGER": mod.LEDGER, "SDK": mod.SDK, "DOC": mod.DOC,
            "ROOT": mod.ROOT,
        }
        mod.LEDGER = ledger
        mod.SDK = sdk_file
        mod.DOC = doc
        mod.ROOT = tmp_path
        try:
            # 正面：合成最小合规 2 条过检 + 双向对账恰等 + doc 往返。
            loaded = mod.validate(mod.load())
            assert len(loaded) == 2
            mod.compare_facade(loaded)
            mod.write_doc(loaded)
            mod.check_doc(loaded)
            text = doc.read_text(encoding="utf-8")
            assert text.startswith("head\n") and text.endswith("tail\n")

            # ⑤ matrix 黄金行形（id 定序 Draw < Rect 字典序）。
            matrix_text = mod.matrix(loaded)
            lines_out = matrix_text.splitlines()
            assert lines_out[0] == "| API | kind | since | channel | stability | spec |"
            assert lines_out[1] == "|---|---|---|---|---|---|"
            assert lines_out[2].startswith("| `berry_ui::sdk::Draw` | fn | F2 |")
            assert lines_out[3].startswith("| `berry_ui::sdk::Rect` | type | F2 |")

            # ④ compare_facade 双向：sdk.rs 多出 = missing。
            sdk_file.write_text(
                "pub use crate::layout::Rect;\n"
                "pub use crate::theme::Draw;\n"
                "pub use crate::extra::Ghost;\n",
                encoding="utf-8",
            )
            try:
                mod.compare_facade(loaded)
            except mod.LedgerError as error:
                assert "missing from ledger: Ghost" in str(error)
            else:
                raise AssertionError("facade 多出须 missing 拒")
            # ledger 多出 = stale。
            sdk_file.write_text(
                "pub use crate::layout::Rect;\n", encoding="utf-8",
            )
            try:
                mod.compare_facade(loaded)
            except mod.LedgerError as error:
                assert "absent from sdk.rs facade: Draw" in str(error)
            else:
                raise AssertionError("ledger 多出须 stale 拒")
            # 恢复恰等 + doc 漂移腿。
            sdk_file.write_text(
                "pub use crate::layout::Rect;\n"
                "pub use crate::theme::Draw;\n",
                encoding="utf-8",
            )
            mod.compare_facade(loaded)
            drifted = [dict(entry) for entry in items]
            drifted[0]["stability"] = "deprecated"
            try:
                mod.check_doc(drifted)
            except mod.LedgerError as error:
                assert "stale" in str(error)
            else:
                raise AssertionError("check_doc 漂移必须红")
            # markers 缺失拒。
            doc.write_text("no markers here\n", encoding="utf-8")
            try:
                mod.check_doc(loaded)
            except mod.LedgerError as error:
                assert "markers" in str(error)
            else:
                raise AssertionError("markers 缺失必须拒")

            # ③ validate 拒面族（单点变异逐项钉文案）。
            def expect_reject(toml_text: str, needle: str):
                ledger.write_text(toml_text, encoding="utf-8")
                try:
                    mod.validate(mod.load())
                except (mod.LedgerError, tomllib.TOMLDecodeError) as error:
                    assert needle in str(error), (
                        f"变异 {needle!r} 未咬: {error}"
                    )
                else:
                    raise AssertionError(f"变异 {needle!r} 未拒")

            good = item_toml(items)
            expect_reject(good.replace("format = 1", "format = 2", 1),
                          "meta.format must be 1")
            expect_reject(good.replace('owner = "berry-ui"', 'owner = "x"', 1),
                          "meta.owner must be berry-ui")
            expect_reject(
                good.replace('id = "berry_ui::sdk::Rect"',
                             'id = "core::Rect"', 1),
                "id invalid",
            )
            expect_reject(
                good.replace('kind = "type"', 'kind = "macro"', 1),
                "kind must be fn/type/const",
            )
            expect_reject(
                good.replace('module = "layout"', 'module = "ghost"', 1),
                "unknown module",
            )
            expect_reject(
                good.replace('channel = "in-tree"', 'channel = "udp"', 1),
                "unknown channel",
            )
            expect_reject(
                good.replace('capability = "none"', 'capability = "tpu"', 1),
                "unknown capability",
            )
            expect_reject(
                good.replace('stability = "stable"', 'stability = "beta"', 1),
                "unknown stability",
            )
            expect_reject(
                good.replace('spec = "docs/spec.md"', 'spec = "docs/none.md"', 1),
                "spec missing",
            )
            expect_reject(good.replace("breaking = []", "", 1),
                          "missing: breaking")
            # 重复 id。
            dup = good.replace(
                'id = "berry_ui::sdk::Rect"',
                'id = "berry_ui::sdk::Draw"', 1)
            expect_reject(dup, "duplicate id")
            # deprecated 无后继。
            dep = good.replace(
                'stability = "stable"', 'stability = "deprecated"', 1)
            expect_reject(dep, "deprecated item lacks successor")
            # 空 since。
            expect_reject(good.replace('since = "F2"', 'since = ""', 1),
                          "since empty")
        finally:
            mod.LEDGER = real["LEDGER"]
            mod.SDK = real["SDK"]
            mod.DOC = real["DOC"]
            mod.ROOT = real["ROOT"]

    print(
        "sdk_ledger selftest: 6 腿全绿"
        "（exported_name 三形/facade 提取/validate 拒面族/双向对账/matrix 黄金形/doc 往返）"
    )
    return 0


class _SyntheticSdk:
    """public_facade_names 的合成 SDK 文件替身（read_text 协议面）。"""

    def __init__(self, text: str):
        self._text = text

    def read_text(self, encoding: str = "utf-8") -> str:
        return self._text


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
    parser.add_argument("--selftest", action="store_true",
                        help="合成自检腿（零真仓依赖）")
    args = parser.parse_args()
    if args.selftest:
        return selftest()
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
