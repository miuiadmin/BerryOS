#!/usr/bin/env python3
"""GUI F9 普通 app 边界静态守卫。

F6..F8 后，Settings/Files/Contacts/Browser/Photos/UITEST 全部走
Berry Room + BerryUI。本工具把路线文档的普通 Retained app 禁令机器化：

- manifest `type=app` 且 `wm=true` 的普通 GUI app 必须直接依赖
  `berry-room` 与 `berry-ui(guest-font-vmo)`；
- 数据 RPC 只能经 `berry-fsclient`，且仅限 manifest 授予 fs 能力的 app；
- app 源码不得直接调用 channel/vmar/vmo/process/handle 等 syscall 面，
  不得解析/编码 WM RPC，不得持 frame/字体固定映射、stride 或像素基元；
- `berry-nshfont` 是迁移期临时件，F9 后仓库内不得存在。

P0 CanvasSurface / immersive surface 未开放；未来授权 renderer 落地时，
必须先扩展本工具的授权面（manifest surface role/mode + 独立 allowlist），
不得删除守卫绕行。
"""

from __future__ import annotations

import argparse
import re
import sys
import tempfile
import tomllib
from pathlib import Path

KNOWN_GUI_APPS = {
    "SETTINGS",
    "FILES",
    "CONTACTS",
    "BROWSER",
    "PHOTOS",
    "UITEST",
}
REQUIRED_DEPENDENCIES = {"berry-room", "berry-ui"}
OPTIONAL_DEPENDENCIES = {"berry-gfx", "berry-fsclient"}
FORBIDDEN_SOURCE_PATTERNS: tuple[tuple[str, str], ...] = (
    (
        "直接 syscall 调用",
        r"\bberry_(?:channel|vmar|vmo|handle|process|object_wait|clock)"
        r"(?:_[A-Za-z0-9_]+)?\b",
    ),
    ("直接 syscall crate", r"\buse\s+berry_(?:sys|guiproto|nshfont)\b"),
    ("WM/input/present RPC 编解码", r"\b(?:encode|parse)_(?:wm|input|present)_"),
    ("GUI RPC 命名空间", r"\bgp::"),
    ("frame/字体固定映射", r"\b(?:FRAME_VA|FONT_VA)\b"),
    ("frame stride 计算", r"\bstride\b"),
    ("像素基元 put_px", r"\bput_px\s*\("),
    ("像素基元 blend_px", r"\bblend_px\s*\("),
    ("像素基元 round_rect", r"\b(?:round_rect|fill_round_rect)\s*\("),
    ("像素基元 circle", r"\b(?:circle|fill_circle)\s*\("),
    ("像素基元 shadow", r"\b(?:shadow|draw_shadow)\s*\("),
    ("直接光栅 surface 面", r"berry_gfx::surface\b"),
    ("直接 DisplayList Clip 命令", r"\.(?:push_clip|pop_clip)\s*\("),
    ("未授权 frame buffer", r"\.(?:frame_mut)\s*\("),
    ("unsafe", r"\bunsafe\b"),
    ("动态分配", r"\balloc::"),
)


def strip_rust_comments(source: str) -> str:
    """去掉 Rust 注释，避免文档里的历史词被当代码误报。

    - 行注释整行去掉；
    - 块注释按 Rust 规则嵌套计数；
    - 字符串/char 字面量保留给禁词扫描，注释标记在字符串内不生效；
    - lifetime（`'a`）不是 char，按普通 token 处理。
    """
    out = list(source)
    i = 0
    state = "code"
    block_depth = 0
    raw_hashes = 0
    while i < len(source):
        char = source[i]
        nxt = source[i + 1] if i + 1 < len(source) else ""
        if state == "code":
            if char == "/" and nxt == "/":
                state = "line"
                out[i] = out[i + 1] = " "
                i += 2
                continue
            if char == "/" and nxt == "*":
                state = "block"
                block_depth = 1
                out[i] = out[i + 1] = " "
                i += 2
                continue
            if char == '"':
                # Rust raw/byte raw string：r" / br" / r#" / r##"…。
                # 先向前数 `"` 前的 #，再确认 r/br 前缀；终止必须是
                # `"` 后跟同数 #。内容保留给禁词扫描。
                back = i - 1
                while back >= 0 and source[back] == "#":
                    back -= 1
                has_r = back >= 0 and source[back] == "r"
                has_br = back >= 1 and source[back - 1 : back + 1] == "br"
                if has_r or has_br:
                    raw_hashes = i - 1 - back
                    state = "raw-string"
                    i += 1
                    continue
                state = "string"
                i += 1
                continue
            # 只把真实 char 字面量（'x' / '\\x'）切进 char 态；lifetime
            # `'a` 没有近距闭合单引号，仍当代码。
            if char == "'" and re.match(r"'(\\.|[^\\'])'", source[i : i + 4]):
                state = "char"
                i += 1
                continue
            i += 1
            continue
        if state == "line":
            if char == "\n":
                state = "code"
            else:
                out[i] = " "
            i += 1
            continue
        if state == "block":
            if char == "/" and nxt == "*":
                block_depth += 1
                out[i] = out[i + 1] = " "
                i += 2
                continue
            if char == "*" and nxt == "/":
                block_depth -= 1
                out[i] = out[i + 1] = " "
                i += 2
                if block_depth == 0:
                    state = "code"
                continue
            if char != "\n":
                out[i] = " "
            i += 1
            continue
        if state == "raw-string":
            if char == '"':
                end = i + 1 + raw_hashes
                if (
                    source[i + 1 : end] == "#" * raw_hashes
                    and (end >= len(source) or source[end] != "#")
                ):
                    state = "code"
                    i = end
                    continue
            i += 1
            continue
        if state == "string":
            if char == "\\" and nxt:
                i += 2
                continue
            if char == '"':
                state = "code"
            i += 1
            continue
        if state == "char":
            if char == "\\" and nxt:
                i += 2
                continue
            if char == "'":
                state = "code"
            i += 1
            continue
    if state == "block" or block_depth != 0:
        raise ValueError("未闭合块注释")
    if state not in {"code", "string", "char", "raw-string"}:
        raise ValueError("注释/字面量状态机异常")
    return "".join(out)


def read_toml(path: Path) -> dict:
    with path.open("rb") as file:
        return tomllib.load(file)


AUTHORIZED_SURFACE_ESCAPE = {"UITEST"}


def gui_manifest_surface(manifest: dict) -> str | None:
    """返回 GUI app surface 形态；None = 非 GUI app。

    P1 起 manifest `[capabilities] surface_escape=true` 是授权真值源；
    只有显式 allowlist 内的 app 可以获得 Canvas/Immersive token。其他
    app 一律 fail closed，仍按 Retained 普通 BerryUI 面执法。
    """
    app = manifest.get("app", {})
    capabilities = manifest.get("capabilities", {})
    if app.get("type") != "app":
        return None
    if capabilities.get("surface_escape") is True:
        return "surface-escape"
    if capabilities.get("wm") is not True:
        return None
    return "retained"


def check_app(app_dir: Path) -> list[str]:
    errors: list[str] = []
    manifest_path = app_dir / "manifest.toml"
    cargo_path = app_dir / "Cargo.toml"
    if not manifest_path.is_file() or not cargo_path.is_file():
        return [f"{app_dir.name}: 缺 manifest.toml 或 Cargo.toml"]

    try:
        manifest = read_toml(manifest_path)
    except (OSError, tomllib.TOMLDecodeError) as error:
        return [f"{manifest_path}: manifest 解析失败：{error}"]
    try:
        cargo = read_toml(cargo_path)
    except (OSError, tomllib.TOMLDecodeError) as error:
        return [f"{cargo_path}: Cargo.toml 解析失败：{error}"]

    app_id = str(manifest.get("app", {}).get("id", app_dir.name.upper()))
    dependencies = cargo.get("dependencies", {})

    def collect_dependency_names(table: dict) -> set[str]:
        names: set[str] = set()
        for key, value in table.items():
            if key in {"dependencies", "build-dependencies", "dev-dependencies"}:
                if isinstance(value, dict):
                    names.update(value)
            elif isinstance(value, dict):
                names.update(collect_dependency_names(value))
        return names

    runtime_dependency_names = set(dependencies)
    dependency_names = collect_dependency_names(cargo)
    uses_fs = bool(manifest.get("capabilities", {}).get("fs"))

    missing = REQUIRED_DEPENDENCIES - runtime_dependency_names
    if missing:
        errors.append(f"{app_id}: 运行时缺普通 GUI 宿主依赖 {sorted(missing)}")

    allowed = REQUIRED_DEPENDENCIES | OPTIONAL_DEPENDENCIES
    unexpected = dependency_names - allowed
    if unexpected:
        errors.append(f"{app_id}: 普通 GUI app 出现非授权直接依赖 {sorted(unexpected)}")

    if "berry-fsclient" in dependency_names and not uses_fs:
        errors.append(f"{app_id}: 未授 fs 能力却直接依赖 berry-fsclient")
    if uses_fs and "berry-fsclient" not in runtime_dependency_names:
        errors.append(f"{app_id}: 已授 fs 能力但缺运行时 berry-fsclient 数据门面")

    ui_dependency = dependencies.get("berry-ui")
    features = ui_dependency.get("features", []) if isinstance(ui_dependency, dict) else []
    if "guest-font-vmo" not in features:
        errors.append(f"{app_id}: berry-ui 必须启用 guest-font-vmo")

    source_files = sorted((app_dir / "src").rglob("*.rs")) + [app_dir / "build.rs"]
    for source in source_files:
        if not source.is_file():
            continue
        try:
            code = strip_rust_comments(source.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, ValueError) as error:
            errors.append(f"{app_id}/{source.name}: 源码读取/注释解析失败：{error}")
            continue
        for label, pattern in FORBIDDEN_SOURCE_PATTERNS:
            if re.search(pattern, code):
                errors.append(f"{app_id}/{source.name}: 出现禁令「{label}」")

    return errors


def check_density_threshold(root: Path) -> list[str]:
    """LAYOUT-L5：视口密度阈值字面量只能住在 berry-ui/src/theme.rs。

    比较变量名不穷举（w/vp_w/display_w 等历史形态会漂移），直接禁
    `600/1000` 宽阈值与 `1800` 高阈值同文件共现；theme.rs 是唯一白名单。
    注释先经 strip_rust_comments 去除，历史说明不会误报。
    """
    errors: list[str] = []
    for source in sorted((root / "user").rglob("*.rs")):
        rel = source.relative_to(root)
        if str(rel) == "user/berry-ui/src/theme.rs" or "/tests/" in str(rel):
            continue
        try:
            code = strip_rust_comments(source.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, ValueError) as error:
            errors.append(f"{rel}: 源码读取/注释解析失败：{error}")
            continue
        has_width = re.search(r">=\s*(?:600|1000)\b", code) is not None
        has_height = re.search(r">=\s*1800\b", code) is not None
        if has_width and has_height:
            errors.append(f"{rel}: 私有视口密度阈值必须走 Density::for_viewport")
    return errors


def scan_root(root: Path) -> tuple[list[str], list[str]]:
    apps: list[Path] = []
    errors: list[str] = []
    seen_app_ids: set[str] = set()
    for manifest_path in sorted((root / "user" / "bins").glob("*/manifest.toml")):
        try:
            manifest = read_toml(manifest_path)
        except (OSError, tomllib.TOMLDecodeError) as error:
            errors.append(f"{manifest_path}: manifest 解析失败：{error}")
            continue
        surface = gui_manifest_surface(manifest)
        app_id = str(manifest.get("app", {}).get("id", manifest_path.parent.name.upper()))
        if app_id in seen_app_ids:
            errors.append(f"{manifest_path}: app id 重复/冒用 {app_id}")
            continue
        seen_app_ids.add(app_id)
        capabilities = manifest.get("capabilities", {})
        if capabilities.get("surface_escape") is True and capabilities.get("wm") is not True:
            errors.append(f"{manifest_path}: surface_escape=true 需要 wm=true")
            continue
        if surface == "retained":
            apps.append(manifest_path.parent)
        elif surface == "surface-escape":
            if app_id not in AUTHORIZED_SURFACE_ESCAPE:
                errors.append(
                    f"{manifest_path}: surface_escape 无 P1 授权 allowlist"
                )
            else:
                apps.append(manifest_path.parent)
        elif surface is not None:
            errors.append(f"{manifest_path}: 未知 surface 形态 {surface}")

    app_ids = {
        read_toml(app / "manifest.toml").get("app", {}).get("id", app.name.upper())
        for app in apps
    }
    missing_known = KNOWN_GUI_APPS - app_ids
    if missing_known:
        errors.append(f"GUI app 扫描漏掉已知普通 app：{sorted(missing_known)}")
    if len(apps) < len(KNOWN_GUI_APPS):
        errors.append(f"普通 GUI app 数量 {len(apps)} < {len(KNOWN_GUI_APPS)}")

    for app in apps:
        errors.extend(check_app(app))

    if (root / "user" / "berry-nshfont").exists():
        errors.append("user/berry-nshfont 仍存在：F9 后迁移期临时件必须删除")
    errors.extend(check_density_threshold(root))

    return [str(app) for app in apps], errors


def make_fixture(root: Path, *, valid: bool = True) -> None:
    for name, app_id in (
        ("settings", "SETTINGS"), ("files", "FILES"), ("contacts", "CONTACTS"),
        ("browser", "BROWSER"), ("photos", "PHOTOS"), ("uitest", "UITEST"),
    ):
        app = root / "user" / "bins" / name
        (app / "src").mkdir(parents=True)
        (app / "manifest.toml").write_text(
            f'[app]\nid = "{app_id}"\ntype = "app"\n\n'
            "[capabilities]\nconsole = true\nwm = true\n"
            + ("surface_escape = true\n" if app_id == "UITEST" else ""),
            encoding="utf-8",
        )
        deps = [
            'berry-gfx = { path = "../../berry-gfx" }',
            'berry-room = { path = "../../berry-room" }',
            'berry-ui = { path = "../../berry-ui", features = ["guest-font-vmo"] }',
        ]
        if not valid:
            deps[1] = 'berry-nshfont = { path = "../../berry-nshfont" }'
        (app / "Cargo.toml").write_text(
            "[workspace]\n[dependencies]\n" + "\n".join(deps) + "\n",
            encoding="utf-8",
        )
        source = "// 历史注释允许出现 berry_channel_send / stride / put_px.\nfn main() {}\n"
        (app / "src" / "main.rs").write_text(source, encoding="utf-8")


def self_test() -> int:
    failures = 0

    def expect_error(label: str, errors: list[str]) -> None:
        nonlocal failures
        if not errors:
            print(f"gui-guard self-test FAIL: {label} 未失败")
            failures += 1

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        # 覆盖嵌套块注释、字符串里的 //、char 引号与历史词：
        # 这些都必须被注释/字面量状态机正确忽略，不能误报。
        (root / "user" / "bins" / "settings" / "src" / "main.rs").write_text(
            """fn x() {
                let quote = '"';
                let url = b\"berry://x//y\";
                /* 历史 berry_vmar_map /* put_px */ stride 仍在注释 */
            }
            """,
            encoding="utf-8",
        )
        _, errors = scan_root(root)
        if errors:
            print(f"gui-guard self-test FAIL: 正样本误报 {errors}")
            failures += 1

    negative_cases = {
        "直接 syscall": "fn x() { let _ = berry_channel_send(1, core::ptr::null(), 0, core::ptr::null(), 0); }",
        "VMO 映射": "fn x() { let _ = berry_vmar_map(1, 0, 2, 0, &mut 0, 0, 0); }",
        "块注释绕行": "/* \" // */ fn x() { let _ = berry_vmar_map; }",
        "syscall 函数指针": "fn x() { let _f = berry_vmar_map; }",
        "stride": "fn x(stride: usize) { let _ = stride; }",
        "像素基元": "fn x() { put_px(core::ptr::null_mut(), 0, 0, 0, [0; 4]); }",
        "unsafe": "fn x() { unsafe { core::ptr::read_volatile(&0); } }",
        "直接 Clip 命令": "fn x(enc: &mut core::fmt::Formatter) { let _ = enc.push_clip(Default::default()); }",
        "未授权 frame buffer": "fn x(r: &mut berry_room::AppRoom) { let _ = r.frame_mut(); }",
        "raw string # 绕行": 'const S: &str = r#"a "// b"#; fn x() { let _ = berry_vmo_create(1, 0, 0, core::ptr::null_mut()); }',
        "raw string ## 绕行": 'const S: &str = r##"x "# y // z"##; fn x() { let _ = berry_clock_read(0, core::ptr::null_mut()); }',
    }
    for label, source in negative_cases.items():
        with tempfile.TemporaryDirectory(prefix="berry-gui-guard-bad.") as name:
            root = Path(name)
            make_fixture(root, valid=True)
            (root / "user" / "bins" / "settings" / "src" / "main.rs").write_text(
                source, encoding="utf-8"
            )
            _, errors = scan_root(root)
            expect_error(label, errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-dep.") as name:
        root = Path(name)
        make_fixture(root, valid=False)
        _, errors = scan_root(root)
        expect_error("nshfont 依赖", errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-crate.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        (root / "user" / "berry-nshfont").mkdir(parents=True)
        _, errors = scan_root(root)
        expect_error("nshfont crate 残留", errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-target.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        cargo = root / "user" / "bins" / "settings" / "Cargo.toml"
        cargo.write_text(
            cargo.read_text(encoding="utf-8")
            + "\n[target.'cfg(target_os = \"none\")'.dependencies]\n"
            + 'berry-sys = { path = "../../berry-sys" }\n',
            encoding="utf-8",
        )
        _, errors = scan_root(root)
        expect_error("target-specific berry-sys 依赖", errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-nested.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        nested = root / "user" / "bins" / "settings" / "src" / "widgets"
        nested.mkdir(parents=True)
        (nested / "evil.rs").write_text("fn x() { put_px(core::ptr::null_mut(), 0, 0, 0, [0; 4]); }", encoding="utf-8")
        _, errors = scan_root(root)
        expect_error("嵌套源码像素基元", errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-surface.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        renderer = root / "user" / "bins" / "renderer"
        renderer.mkdir(parents=True)
        (renderer / "manifest.toml").write_text(
            '[app]\nid = "RENDERER"\ntype = "app"\n\n'
            "[capabilities]\nwm = true\nsurface_escape = true\n",
            encoding="utf-8",
        )
        _, errors = scan_root(root)
        expect_error("未授权 immersive renderer", errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-surface-id.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        spoof = root / "user/bins/spoof"
        spoof.mkdir()
        (spoof / "manifest.toml").write_text(
            '[app]\nid = "UITEST"\ntype = "app"\n\n'
            "[capabilities]\nwm = true\nsurface_escape = true\n",
            encoding="utf-8",
        )
        _, errors = scan_root(root)
        expect_error("重复 app id 冒用 UITEST", errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-surface-wm.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        manifest = root / "user/bins/settings/manifest.toml"
        manifest.write_text(
            manifest.read_text(encoding="utf-8").replace(
                "console = true\nwm = true",
                "console = true\nsurface_escape = true",
            ),
            encoding="utf-8",
        )
        _, errors = scan_root(root)
        expect_error("surface_escape 缺 wm", errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-font.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        cargo = root / "user" / "bins" / "settings" / "Cargo.toml"
        cargo.write_text(
            cargo.read_text(encoding="utf-8").replace(
                'features = ["guest-font-vmo"]', "features = []"
            ),
            encoding="utf-8",
        )
        _, errors = scan_root(root)
        expect_error("缺 guest-font-vmo", errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-fs.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        manifest = root / "user" / "bins" / "settings" / "manifest.toml"
        manifest.write_text(
            manifest.read_text(encoding="utf-8").replace(
                "console = true\nwm = true", 'console = true\nfs = ["rootfs"]\nwm = true'
            ),
            encoding="utf-8",
        )
        _, errors = scan_root(root)
        expect_error("授 fs 但缺 fsclient", errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-buildroom.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        cargo = root / "user" / "bins" / "settings" / "Cargo.toml"
        cargo.write_text(
            cargo.read_text(encoding="utf-8").replace(
                'berry-room = { path = "../../berry-room" }\n', ""
            )
            + '[build-dependencies]\n'
            + 'berry-room = { path = "../../berry-room" }\n',
            encoding="utf-8",
        )
        _, errors = scan_root(root)
        expect_error("build 依赖冒充运行时 berry-room", errors)

    with tempfile.TemporaryDirectory(prefix="berry-gui-guard-density.") as name:
        root = Path(name)
        make_fixture(root, valid=True)
        theme = root / "user" / "bins" / "berry-launcher" / "src" / "desktop.rs"
        theme.parent.mkdir(parents=True, exist_ok=True)
        theme.write_text("fn x(w: u32, h: u32) { w >= 600 && h >= 1800 }", encoding="utf-8")
        _, errors = scan_root(root)
        expect_error("私有密度阈值", errors)

    if failures:
        return 1
    print("gui-guard self-test: 正样本 + 22 类负样本 fail closed 全绿")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parent.parent)
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args()
    if args.self_test:
        return self_test()

    apps, errors = scan_root(args.root)
    if errors:
        print("gui-guard: FAIL")
        for error in errors:
            print(f"  {error}")
        return 1
    print(f"gui-guard: PASS（{len(apps)} 个普通 GUI app 全部走 Berry Room + BerryUI；nshfont 已删除）")
    for app in apps:
        print(f"  {app}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
