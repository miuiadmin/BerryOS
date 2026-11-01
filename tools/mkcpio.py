#!/usr/bin/env python3
"""mkcpio——cpio newc 打包器（G-eco-c rootfs 产出端，配对消费端
`user/berry-loader` 的 cpio_walk；格式对读 GNU cpio(1) `newc`）。

用法：mkcpio.py <out.cpio> name=path [name=path ...] [--split <MiB>]
  - name 归档内路径（如 bin/hello），path 本地文件（ELF 本体，非平像）；
  - 条目按整条 `name=path` 字符串排序（确定性输出：同输入同字节——AI
    测试友好化。app-0 §4 钉板：这是 CLI main() 的行为；`build_archive`
    纯编码器**不排序**——排序归调用方，勿混）；
  - 字段全定值（mtime=0/uid=gid=0/mode 0755/nlink 1/ino 递增）：归档字节
    只随内容变，不随打包时刻变；
  - TRAILER!!! 自终止 + 尾垫 4（消费端 walk 到 trailer 即停）；
  - `--split <MiB>`：单卷帽分卷产链（容量工程前置-A1 §3.3）——归档照常
    单档落盘（既有消费面零扰动），另出 `<stem>0.cpio/<stem>1.cpio/…`
    编号卷（每卷 ≤MiB、末卷可小；**只切字节流不做条目对齐**，TRAILER!!!
    自然只在末卷；消费端 = 内核 VMO 逐页拼装重组单一视图，用户态零改动）。
  - `--selftest-split`：切分器纯函数面守卫（mkfat --check-fixture 同位
    纪律；A1-3b 挂 just test-host 前置，本刀可独立跑）。
"""
import os
import sys


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
    out += b"\0" * (-len(out) % 4)  # 头+名垫到 4
    out += data
    out += b"\0" * (-len(data) % 4)  # 数据垫到 4
    return out


def build_archive(items: list[tuple[bytes, bytes]]) -> bytes:
    """纯编码器（app-0 §4 单编码器纪律）：不排序、按传入序编码、ino =
    传入序号 + 1——**排序归调用方**。TRAILER + 尾垫 4 在内。

    mkapp（.bap 打包）`from mkcpio import build_archive` 复用——禁止任何
    第二份 newc 编码/trailer 逻辑（双编码器 = 漂移面）。CLI main() 的
    排序语义（整条 `name=path` 字符串）与其调用本函数的序保持不变。
    """
    blob = bytearray()
    for i, (name, data) in enumerate(items):
        blob += entry(name, data, i + 1)
    blob += entry(b"TRAILER!!!", b"", 0)
    blob += b"\0" * (-len(blob) % 4)
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


def selftest_split() -> int:
    """`--selftest-split`：切分面纯函数守卫（mkfat --check-fixture 同位纪律
    ——改本文件必跑；A1-3b 挂 just test-host 前置）。合成基准经**真实编码器**
    `build_archive` 产 blob（多条目、含非 4 对齐长度让数据垫生效——合成
    基准复刻真实产链形态，勿手造字节；elf2pe .reloc 教训的工具版）。
    """
    items = [
        (b"bin/aa", b"\x01" * 37),
        (b"bin/bb", b"\x02" * 1),
        (b"etc/cc", b"\x03" * 4096),
        (b"dd", b"\x04" * 123),
    ]
    blob = build_archive(items)
    assert len(blob) > 128, "selftest: 合成归档体量不足以覆盖多边界形态"
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
    print("mkcpio: selftest-split ok")
    return 0


def main() -> int:
    args = sys.argv[1:]
    split_mib: int | None = None
    pos: list[str] = []
    i = 0
    while i < len(args):
        if args[i] == "--selftest-split":
            return selftest_split()
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
    if len(pos) < 2:
        sys.stderr.write(__doc__)
        return 2
    out_path, items = pos[0], pos[1:]
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
