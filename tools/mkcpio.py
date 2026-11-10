#!/usr/bin/env python3
"""mkcpio——cpio newc 打包器（G-eco-c rootfs 产出端，配对消费端
`user/berry-loader` 的 cpio_walk；头/字段面对读 GNU cpio(1) `newc`——
垫料面 X-3c 起垫 4096（见 `entry()`，GNU cpio 的 4 垫步进不再兼容；
宿主侧对读 = mkapp `_cpio_entries` 单源）。

用法：mkcpio.py <out.cpio> name=path [name=path ...] [--split <MiB>]
              [--manifest <rootfs-manifest.toml>]
  - name 归档内路径（如 bin/hello），path 本地文件（ELF 本体，非平像）；
  - 条目按整条 `name=path` 字符串排序（确定性输出：同输入同字节——AI
    测试友好化。app-0 §4 钉板：这是 CLI main() 的行为；`build_archive`
    纯编码器**不排序**——排序归调用方，勿混）；
  - `--manifest <toml>`（V13 预埋刀，ADR #82「组装不 fork」）：条目映射
    改读数据文件（tools/rootfs-manifest.toml——justfile build-user 58 条
    硬编外化，13 号 §5.3-③ 现场重数）。与位置参数 name=path 可混用
    （合并后照常整条排序——同输入同字节不破）；双源撞名 loud 拒。
    schema：`version = 1` + `[entries]` 表（见 load_manifest 头注）；
    未知顶层键/坏条目一律拒（berry-manifest 未知键硬拒同纪律）。
    **缺席 = 现行为零漂移**（位置参数路不 imports tomllib——Python ≥3.11
    依赖只落 --manifest 路，gen_abi 同先例）；未知条目（不在表内）行为
    不变：本工具不持白名单，条目合法性只看 name/path 形。
  - 字段全定值（mtime=0/uid=gid=0/mode 0755/nlink 1/ino 递增）：归档字节
    只随内容变，不随打包时刻变；
  - TRAILER!!! 自终止 + 尾垫页（消费端 walk 到 trailer 即停）；
  - `--split <MiB>`：单卷帽分卷产链（容量工程前置-A1 §3.3）——归档照常
    单档落盘（既有消费面零扰动），另出 `<stem>0.cpio/<stem>1.cpio/…`
    编号卷（每卷 ≤MiB、末卷可小；**只切字节流不做条目对齐**，TRAILER!!!
    自然只在末卷；消费端 = 内核 VMO 逐页拼装重组单一视图，用户态零改动）。
  - `--selftest-split`：切分器纯函数面守卫（mkfat --check-fixture 同位
    纪律；A1-3b 挂 just test-host 前置，本刀可独立跑）+ V13 manifest
    加载面守卫（selftest_manifest 同刀串跑——test-host 既有行零改）。
"""
import os
import sys

# 垫料粒度（X-3c）：头+名与数据都垫到 4096——条目长恒页倍数 → 每条
# data_off 页对齐，0x23 pager_supply_mapped 的 delta 页对齐门（delta %
# PAGE != 0 → EINVAL）在产链侧直接兑现，berry-exec congruent 检查无需
# 自垫。消费端同口径四点：berry-loader cpio_walk/cpio_step + mkapp
# _cpio_entries + 本文件（单编码器纪律的伴生义务——改此值四点同步）。
CPIO_PAD = 4096


def hex8(v: int) -> bytes:
    return f"{v:08X}".encode()


def entry(name: bytes, data: bytes, ino: int) -> bytes:
    namesize = len(name) + 1  # 含 NUL
    # magic + ino mode uid gid nlink mtime filesize devmaj devmin
    #        rdevmaj rdevmin namesize check（13 × hex8）
    out = b"070701" + b"".join(
        hex8(v)
        for v in (ino, 0o100755, 0, 0, 1, 0, len(data), 0, 0, 0, 0, namesize, 0)
    )
    out += name + b"\0"
    out += b"\0" * (-len(out) % CPIO_PAD)  # 头+名垫到页（entry 内相对）
    out += data
    out += b"\0" * (-len(data) % CPIO_PAD)  # 数据垫到页（data_off 起点页对齐）
    return out


def build_archive(items: list[tuple[bytes, bytes]]) -> bytes:
    """纯编码器（app-0 §4 单编码器纪律）：不排序、按传入序编码、ino =
    传入序号 + 1——**排序归调用方**。TRAILER + 尾垫页在内。条目自垫
    恒页倍数（见 `entry()`）→ 拼接后每条起点页对齐，data_off 亦然。

    mkapp（.bap 打包）`from mkcpio import build_archive` 复用——禁止任何
    第二份 newc 编码/trailer 逻辑（双编码器 = 漂移面）。CLI main() 的
    排序语义（整条 `name=path` 字符串）与其调用本函数的序保持不变。
    """
    blob = bytearray()
    for i, (name, data) in enumerate(items):
        blob += entry(name, data, i + 1)
    blob += entry(b"TRAILER!!!", b"", 0)
    blob += b"\0" * (-len(blob) % CPIO_PAD)
    return bytes(blob)


VOL_SUFFIX = ".cpio"

# 内核 fw_cfg 连续前缀发现域（A1 §3.2）：opt/berry/rootfs0..9——陈卷清理
# 的执法范围与之同口径（第 10 卷起内核本就不读，清了也无害、不清也不劫持）。
VOL_SCAN_MAX = 10


def split_volumes(blob: bytes, vol_bytes: int) -> list[bytes]:
    """纯切分器（容量工程前置-A1 §3.3）：只切字节流、不做条目对齐——cpio
    newc 是流式格式（消费端 berry-exec `cpio_walk` 走到 TRAILER!!! 即停），
    卷边界落在条目中间无碍；TRAILER!!! 自然只在末卷。重装恒等 =
    `b"".join(volumes) == blob`（内核 fw_cfg/handoff 逐卷拼装 VMO 的核心
    不变量，selftest 钉板）。恰整数倍长度无空尾卷。
    """
    if vol_bytes <= 0:
        raise ValueError("vol_bytes must be positive")
    return [blob[i : i + vol_bytes] for i in range(0, len(blob), vol_bytes)]


def volume_paths(out_path: str, n: int) -> list[str]:
    """编号卷路径：`rootfs.cpio` → `rootfs0.cpio/rootfs1.cpio/…`（剥 .cpio
    后缀接序号再接回）；无后缀直拼（`out` → `out0.cpio`）。目录分量保留
    （`build/rootfs.cpio` → `build/rootfs0.cpio`）。与 fw_cfg 具名文件
    `opt/berry/rootfs0..N`（A1 §3.2）同一序号口径。
    """
    stem = out_path[: -len(VOL_SUFFIX)] if out_path.endswith(VOL_SUFFIX) else out_path
    return [f"{stem}{i}{VOL_SUFFIX}" for i in range(n)]


def clean_stale_volumes(out_path: str, keep: int) -> int:
    """清陈卷（A1-3b 产链卫生）：旧构建分了 N 卷、新构建只出 M<N 卷时，
    残留的 <stem><i>.cpio（i≥M）会被内核 fw_cfg 连续前缀发现序
    （opt/berry/rootfs0..9）拼进 VMO = **静默错字节**（rootfs 尾段来自
    陈卷，ktest/装载面全不自知）。写卷前 unlink 序号 ≥keep 的卷；
    `--split` 缺席路径 keep=0 全清——防旧名单单卷路（rootfs.cpio 直挂
    opt/berry/rootfs）被陈 rootfs0.cpio 劫持为多卷路。ENOENT 静默
    （常态即不存在）；返回实删数（打点用）。"""
    stem = out_path[: -len(VOL_SUFFIX)] if out_path.endswith(VOL_SUFFIX) else out_path
    removed = 0
    for idx in range(keep, VOL_SCAN_MAX):
        try:
            os.unlink(f"{stem}{idx}{VOL_SUFFIX}")
            removed += 1
        except FileNotFoundError:
            pass
    return removed


# ─── V13（ADR #82 预埋刀）：条目映射数据文件加载面 ─────────────────────
#
# justfile build-user 的 58 条 mkcpio 硬编映射外化为 tools/rootfs-manifest.toml
# （13 号 §5.1-V13 载体 / 08 号 §3.2 改造清单行 1 的过渡形态）。加载器只做
# 「数据 → 条目」翻译，不持白名单、不做条目存在性检查（path 打开失败在
# main() 既有 loud 路爆炸——未知条目行为不变）。


def load_manifest(path: str) -> dict:
    """rootfs-manifest.toml → {name: path} 条目表（V13）。

    schema（文件头注同源）：
      version = 1          # u32，add-only（V12 schema=1 同先例）；非 1 拒
      [entries]            # 必有非空表；键 = 归档内名、值 = 本地路径
      "app/APPDEMO.bap" = "userbins/app/APPDEMO.bap"

    拒面（一律 SystemExit loud——berry-manifest 未知键硬拒同纪律）：
    未知顶层键 / version ≠ 1 / 缺 [entries] / 空表 / 坏条目名（空、
    绝对路径、含 `=`——与 CLI `name=path` 可达名集同口径）/ 坏路径
    （非字符串、空）/ 坏 TOML / 读失败。TOML 表键天然去重（tomllib
    重复键即解析错）——撞名只剩 manifest↔CLI 双源面，归 merge_items。
    """
    try:
        import tomllib  # Python ≥3.11（gen_abi 同依赖先例；仅本函数导入）
    except ImportError:
        raise SystemExit("mkcpio: FAIL: --manifest 需 Python ≥3.11（tomllib,"
                         "gen_abi 同依赖先例）")
    try:
        with open(path, "rb") as f:
            data = tomllib.load(f)
    except OSError as e:
        raise SystemExit(f"mkcpio: FAIL: 读 {path}：{e}")
    except tomllib.TOMLDecodeError as e:
        raise SystemExit(f"mkcpio: FAIL: 坏 TOML {path}：{e}")
    unknown = sorted(set(data) - {"version", "entries"})
    if unknown:
        raise SystemExit(f"mkcpio: FAIL: {path} 未知顶层键 {unknown}"
                         f"（收 version/entries——berry-manifest 未知键硬拒同纪律）")
    if data.get("version") != 1:
        raise SystemExit(f"mkcpio: FAIL: {path} version 须 = 1"
                         f"（got {data.get('version')!r}）")
    entries = data.get("entries")
    if not isinstance(entries, dict) or not entries:
        raise SystemExit(f"mkcpio: FAIL: {path} 缺非空 [entries] 条目表")
    for name, src in entries.items():
        if not isinstance(name, str) or not name or name.startswith("/") or "=" in name:
            raise SystemExit(f"mkcpio: FAIL: {path} 坏条目名 {name!r}"
                             f"（空/绝对路径/含 = 拒——CLI name=path 同口径）")
        if not isinstance(src, str) or not src:
            raise SystemExit(f"mkcpio: FAIL: {path} 条目 {name!r} 坏路径"
                             f" {src!r}（须非空字符串）")
    return dict(entries)


def merge_items(positional: list, manifest: dict) -> list:
    """--manifest 条目表与位置参数 name=path 合并（V13）。返回未排序合并
    列表（排序仍归 main() 既有 `sorted(items)`——CLI 确定性语义单点）。
    双源撞名 loud 拒：归档内同名双条目 = 消费端 cpio_find 命中首个的
    静默错字节面（berry-loader/svcmgr 名序敏感，justfile :45 在册），宁
    在注入口炸。纯位置参数路**不经本函数**（旧路径行为逐字节不动）。"""
    items = list(positional) + [f"{n}={p}" for n, p in manifest.items()]
    names = [i.partition("=")[0] for i in items]
    dups = sorted({n for n in names if names.count(n) > 1})
    if dups:
        raise SystemExit(f"mkcpio: FAIL: 条目撞名 {dups}"
                         f"（--manifest 与 CLI 双源同名——归档内名须唯一）")
    return items


def selftest_manifest() -> int:
    """V13 manifest 加载面守卫（selftest_split 同刀串跑——test-host 既有
    `--selftest-split` 行零改即覆盖）。合成 TOML 经**真实加载器**
    load_manifest 产条目表（手造字典不算数）；拒面逐型独立钉。"""
    import tempfile
    from pathlib import Path

    good_toml = 'version = 1\n[entries]\n"bin/aa" = "x/aa"\n"bb" = "y/bb"\n'
    with tempfile.TemporaryDirectory() as td:
        def write(text, name):
            p = Path(td) / name
            p.write_text(text, encoding="utf-8")
            return str(p)

        good = write(good_toml, "good.toml")
        assert load_manifest(good) == {"bin/aa": "x/aa", "bb": "y/bb"}, "正例加载"
        # 拒面九型：坏 TOML/未知顶层键/version 块/缺 entries/空表/坏名三型
        # /坏路径两型——每型独立文件 + 独立断言（一型一咬，拒面不是计数
        # 游戏；不复写正例文件——写同名曾把 good 覆写成末例坏档）。
        for k, (bad, why) in enumerate([
            ("[entries\n", "坏 TOML 语法"),
            ('extra = 1\nversion = 1\n[entries]\n"a" = "b"\n', "未知顶层键"),
            ("version = 2\n[entries]\n\"a\" = \"b\"\n", "version ≠ 1"),
            ('version = "1"\n[entries]\n"a" = "b"\n', "version 非整数"),
            ("[entries]\n", "缺 version"),
            ("version = 1\n", "缺 entries"),
            ("version = 1\n[entries]\n", "空 entries"),
            ('version = 1\n[entries]\n"" = "b"\n', "空条目名"),
            ('version = 1\n[entries]\n"/abs" = "b"\n', "绝对路径名"),
            ('version = 1\n[entries]\n"a=b" = "c"\n', "名含 ="),
            ('version = 1\n[entries]\n"a" = ""\n', "空路径"),
            ('version = 1\n[entries]\n"a" = 3\n', "路径非字符串"),
        ]):
            try:
                load_manifest(write(bad, f"bad{k}.toml"))
            except SystemExit:
                continue
            assert False, f"拒面未咬：{why}"
        try:
            load_manifest(str(Path(td) / "absent.toml"))
            assert False, "缺文件未拒"
        except SystemExit:
            pass
        # 合并面：无撞名 = 位置 + manifest 全量保序并入（排序归 main）；
        # 撞名 = SystemExit loud。
        m = load_manifest(good)
        assert merge_items(["bin/cc=z/cc"], m) == ["bin/cc=z/cc", "bin/aa=x/aa", "bb=y/bb"], \
            "合并保序"
        try:
            merge_items(["bin/aa=other/aa"], m)
            assert False, "撞名未拒"
        except SystemExit:
            pass
        # 真件格式腐烂守卫：tools/rootfs-manifest.toml（在席时）必过加载器
        # + 核心常驻件锚（bin/bsh——svcmgr 走查名序依赖在册，justfile :45
        # 「mkcpio 名序 bsh 首位」两源）；不钉总数（活文件，13 号 §5.3-③
        # 保质期教训——数漂移是本表职责非守卫职责）。
        real = Path(__file__).resolve().with_name("rootfs-manifest.toml")
        if real.exists():
            r = load_manifest(str(real))
            assert "bin/bsh" in r, "真件缺核心常驻 bin/bsh"
        else:
            raise SystemExit("mkcpio: FAIL: tools/rootfs-manifest.toml 缺席"
                             "（V13 数据源——本刀同笔落库）")
    print("mkcpio: selftest-manifest ok")
    return 0


def selftest_split() -> int:
    """`--selftest-split`：切分面纯函数守卫（mkfat --check-fixture 同位纪律
    ——改本文件必跑；A1-3b 挂 just test-host 前置）+ V13 manifest 加载面
    （selftest_manifest 串跑——test-host 既有行零改双覆盖）。合成基准经
    **真实编码器** `build_archive` 产 blob（多条目、含非 4 对齐长度让数据垫
    生效——合成基准复刻真实产链形态，勿手造字节；elf2pe .reloc 教训的
    工具版）。
    """
    items = [
        (b"bin/aa", b"\x01" * 37),
        (b"bin/bb", b"\x02" * 1),
        (b"etc/cc", b"\x03" * 4096),
        (b"dd", b"\x04" * 123),
    ]
    blob = build_archive(items)
    assert len(blob) > 128, "selftest: 合成归档体量不足以覆盖多边界形态"
    # ⑦ X-3c 垫页几何钉：预测边界步进全档走查——每条头魔数落在预测起点、
    #    data_off 恒页对齐、数据逐字节在位、步进终点 == 归档总长（产链侧
    #    兑现 0x23 delta % PAGE == 0 门；走查算术与消费端 cpio_step/
    #    _cpio_entries 同口径，改 CPIO_PAD 此锚必炸）。
    off = 0
    for nm, dt in [*items, (b"TRAILER!!!", b"")]:
        assert blob[off : off + 6] == b"070701", f"selftest: 头边界漂 @ {off}"
        data_off = (off + 110 + len(nm) + 1 + CPIO_PAD - 1) & ~(CPIO_PAD - 1)
        assert data_off % CPIO_PAD == 0, f"selftest: data_off 非页对齐 @ {off}"
        if dt:
            assert blob[data_off : data_off + len(dt)] == dt, f"selftest: 数据漂 @ {data_off}"
        off = (data_off + len(dt) + CPIO_PAD - 1) & ~(CPIO_PAD - 1)
    assert off == len(blob), f"selftest: 步进终点 {off} != 归档总长 {len(blob)}"
    # ① 重装恒等 + 卷帽 + 无空尾卷：任意卷宽（含 1B/全长-1/恰全长/超全长）。
    for vol in (1, 7, 64, len(blob) - 1, len(blob), len(blob) + 100):
        vols = split_volumes(blob, vol)
        assert b"".join(vols) == blob, f"selftest: 重装恒等破 @vol={vol}"
        assert all(len(v) <= vol for v in vols), f"selftest: 卷越界 @vol={vol}"
        assert vols and len(vols[-1]) > 0, f"selftest: 空尾卷 @vol={vol}"
    # ② 恰整数倍长度：卷数恰商、每卷满宽（无空尾卷的边界形态）。
    n = (len(blob) // 64) * 64
    vols = split_volumes(blob[:n], 64)
    assert len(vols) == n // 64, "selftest: 恰整数倍卷数"
    assert all(len(v) == 64 for v in vols), "selftest: 恰整数倍每卷满宽"
    assert b"".join(vols) == blob[:n], "selftest: 恰整数倍重装恒等"
    # ③ 单卷：卷宽 ≥ 总长 → 恰 1 卷、逐字节同（--split 但归档未超帽的形态）。
    vols = split_volumes(blob, len(blob))
    assert len(vols) == 1 and vols[0] == blob, "selftest: 单卷形态"
    # ④ 编号卷命名：剥接 .cpio / 目录分量保留 / 无后缀直拼。
    assert volume_paths("rootfs.cpio", 3) == [
        "rootfs0.cpio",
        "rootfs1.cpio",
        "rootfs2.cpio",
    ], "selftest: 卷命名 .cpio 剥接"
    assert volume_paths("build/rootfs.cpio", 2) == [
        "build/rootfs0.cpio",
        "build/rootfs1.cpio",
    ], "selftest: 卷命名目录分量保留"
    assert volume_paths("out", 1) == ["out0.cpio"], "selftest: 卷命名无后缀"
    # ⑤ 非正卷宽拒（ValueError，不静默）。
    try:
        split_volumes(blob, 0)
        raise AssertionError("selftest: vol=0 未拒")
    except ValueError:
        pass
    # ⑥ 陈卷清理（A1-3b 产链卫生）：多卷→少卷收缩时 ≥keep 序号卷被清、
    # <keep 存活；keep=0 全清（--split 缺席路径防旧名单路劫持）。
    import tempfile

    with tempfile.TemporaryDirectory() as td:
        vol0 = os.path.join(td, "rootfs0.cpio")
        vol1 = os.path.join(td, "rootfs1.cpio")
        vol2 = os.path.join(td, "rootfs2.cpio")
        for p, payload in ((vol0, b"a"), (vol1, b"b"), (vol2, b"c")):
            with open(p, "wb") as f:
                f.write(payload)
        root = os.path.join(td, "rootfs.cpio")
        assert clean_stale_volumes(root, 2) == 1, "selftest: 收缩清 1 卷"
        assert os.path.exists(vol0) and os.path.exists(vol1), "selftest: <keep 存活"
        assert not os.path.exists(vol2), "selftest: ≥keep 已清"
        assert clean_stale_volumes(root, 0) == 2, "selftest: 全清恰余量"
        assert not (os.path.exists(vol0) or os.path.exists(vol1)), "selftest: 全清"
        assert clean_stale_volumes(root, 0) == 0, "selftest: 空集幂等返 0"
    # V13：manifest 加载面守卫串跑（本入口是 test-host 既有门——justfile
    # 零改即覆盖；将来单列配方无必要，双面一门是收敛面）。
    selftest_manifest()
    print("mkcpio: selftest-split ok")
    return 0


def main() -> int:
    args = sys.argv[1:]
    split_mib: int | None = None
    manifest_path: str | None = None
    pos: list[str] = []
    i = 0
    while i < len(args):
        if args[i] == "--selftest-split":
            return selftest_split()
        if args[i] == "--manifest":
            # V13：条目映射数据文件（tools/rootfs-manifest.toml——加载/
            # 拒面见 load_manifest 头注；与位置参数可混用，撞名拒）。
            if i + 1 >= len(args):
                sys.stderr.write("mkcpio: --manifest 需要 <toml> 参数\n")
                return 2
            manifest_path = args[i + 1]
            i += 2
            continue
        if args[i] == "--split":
            if i + 1 >= len(args):
                sys.stderr.write("mkcpio: --split 需要 <MiB> 参数\n")
                return 2
            try:
                split_mib = int(args[i + 1])
            except ValueError:
                sys.stderr.write(f"mkcpio: 坏 --split 值 {args[i + 1]!r}\n")
                return 2
            if split_mib <= 0:
                sys.stderr.write("mkcpio: --split 须为正整数（MiB）\n")
                return 2
            i += 2
            continue
        pos.append(args[i])
        i += 1
    # V13：--manifest 在席时允许零位置条目（out + 纯表驱动——justfile 换装
    # 后的形态）；缺席路径维持「out + ≥1 位置条目」旧检查不动。
    if len(pos) < 2 and manifest_path is None:
        sys.stderr.write(__doc__)
        return 2
    out_path = pos[0] if pos else None
    if out_path is None:
        sys.stderr.write("mkcpio: --manifest 路径仍需 <out.cpio> 位置参数\n")
        return 2
    items = pos[1:]
    if manifest_path is not None:
        items = merge_items(items, load_manifest(manifest_path))
    parsed = []
    for item in sorted(items):
        name, _, path = item.partition("=")
        if not name or not _ or "/" in name and name.startswith("/"):
            sys.stderr.write(f"mkcpio: 坏条目 {item!r}\n")
            return 2
        with open(path, "rb") as f:
            parsed.append((name.encode(), f.read()))
    blob = build_archive(parsed)
    # 陈卷清理先于写卷（keep 由本次产出卷数定；--split 缺席 = 单档产链，
    # 编号卷一律不该在——全清防内核多卷发现序被陈卷劫持）。
    n_vols = 0
    if split_mib is not None:
        n_vols = len(split_volumes(blob, split_mib * 1024 * 1024))
    stale = clean_stale_volumes(out_path, n_vols)
    if stale:
        print(f"mkcpio: 清陈卷 {stale} 个（序号 ≥{n_vols}——旧构建残留）")
    with open(out_path, "wb") as f:
        f.write(blob)
    print(f"mkcpio: {out_path} {len(blob)}B x{len(items)} 条目")
    if split_mib is not None:
        # 单档照常落盘（既有消费面零扰动）；编号卷旁路产出（A1 §3.3——
        # 卷帽 = 单卷暂存缓冲帽，§3.1 单卷 32MiB 帽同口径的消费端铺垫）。
        vol_bytes = split_mib * 1024 * 1024
        vols = split_volumes(blob, vol_bytes)
        vpaths = volume_paths(out_path, len(vols))
        for path, vol in zip(vpaths, vols):
            with open(path, "wb") as f:
                f.write(vol)
        print(
            f"mkcpio: split {len(vols)} 卷 ≤{split_mib}MiB"
            f"（{vpaths[0]}..{vpaths[-1]}，重装 {sum(len(v) for v in vols)}B）"
        )
    return 0


if __name__ == "__main__":
    sys.exit(main())
