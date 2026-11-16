#!/usr/bin/env python3
r"""tlsscan——rootfs TLS lint 门 + census 审计（T-1 §1，
docs/T-1实施设计草案.md；崩因背景见 docs/法证-TLS原位方案崩因归因.md）。

用法：
  tlsscan.py --check <rootfs.cpio>   FAIL 门（挂 justfile build-user 的
                                     mkcpio 步之后——smoke/check/run 全经
                                     build-user = CI 自然被护）
  tlsscan.py --audit [rootfs.cpio]   census 恒绿清单 + adrp 同页计数审计行
                                     （挂 just test-host；只审计不 FAIL；
                                     rootfs.cpio 缺席打跳过不 FAIL）
  tlsscan.py --selftest              合成 ELF 向量钉判定逻辑（挂 test-host，
                                     mkfat --selftest 同位先例）

--check 每 ELF 三条判据（解析 cpio 含递归 app/*.bap 内层；bin/* 条目必须
是 ELF64 LE、.bap 必须是可解析 cpio——畸形即 FAIL，loud not silent）：
  F1 放置可行性 = tls_place 的 python 逐行镜像（同 CAP 同算术）。
     FAIL ⟺ crt0 运行期 berry_tls_setup 的 placement None → brk #0x7C1
     会被引爆——无任意阈值。
  F2 .tdata 文件字节含指向 TLS 窗 [p_vaddr, p_vaddr+memsz) 的 u64 自指针
     （实测零基线 0/38——拷贝式迁移后镜像原位指针即悬垂直址，出现即 FAIL）。
  F3 ELF 头在映像内 = exec 族（链接基 ≥0x200000）须 ∃ PT_LOAD p_offset==0
     （X-4e 判例：手写 PHDRS 缺 FILEHDR PHDRS → 头排出所有段 →
     __ehdr_start=0 → find_ptls 胎死；0x1000 平像族豁免——装载不读
     PT_LOAD 且无 crt0 解引用，首日三误伤面非真雷）。

**python/rust 双份算术（义务注记）**：TLS_ARENA_CAP 与 tls_place 放置算术
逐行镜像 `user/berry-tls/src/lib.rs`（消费方 = berry-libc crt0 的
berry_tls_setup）——改任一侧后必须同步另一侧；两侧同向量钉板
（4080 恰合/4081 溢/align32 边界，见 --selftest 与 berry-tls
#[cfg(test)] mod tests）。镜像基准 base=0 = 真实 16 对齐 arena 的 worst
case（align ≥ 32 时 base=0 最大化 data−base；align ≤ 16 时两者同得 0x10）。

census 审计关键认知：拷贝式落地后 .tbss∩文件字节重叠**变无害**（主线程
TLS 窗已搬进 arena，镜像原位字节不再被读）——「污染即 FAIL」门会永久红
24 件，census 只配审计不配 FAIL。adrp 同页计数同样只审计（法证：9 处
同页全为窗外 .data.rel.ro/.data 假阳性）。
"""
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mkapp import Reject, _cpio_entries  # noqa: E402（解码单源：X-3c 4096 垫几何）
from mkcpio import build_archive  # noqa: E402（单编码器纪律：合成档复用，不长第二套 newc 编码）

# ---- python 侧镜像（对账锚点：user/berry-tls/src/lib.rs）----
# 改本常量/算术必须同步 rust 侧（义务注记双向点名）。
TLS_ARENA_CAP = 4096

PT_LOAD = 1
PT_TLS = 7
PF_X = 1


def tls_place(base: int, cap: int, filesz: int, memsz: int, align: int):
    """berry_tls::tls_place 的逐行镜像（同拒绝面/同算术）。"""
    if align == 0 or (align & (align - 1)) != 0:
        return None  # 非 2 幂（0 显式拒：幂检对 0 失明）
    if filesz > memsz:
        return None
    data = (base + 0x10 + align - 1) & ~(align - 1)
    if data + memsz > base + cap:
        return None
    return data


# ---- cpio newc 解码（单源 = mkapp._cpio_entries，宿主侧唯一解码器；
#      X-3c 起头+名/数据垫 4096。本地旧 4 垫步进 = 几何第二套，rootfs
#      垫页化后即撞（build-user 实证），已删）----


def parse_cpio(blob: bytes):
    """newc 归档 → [(name bytes, data bytes)]；walk 到 TRAILER!!! 即停。
    Reject → ValueError 翻译：iter_rootfs 的 .bap 内层面捕 ValueError。"""
    try:
        return _cpio_entries(blob)
    except Reject as e:
        raise ValueError(f"cpio: {e}") from e


# ---- ELF 面 ----


class Elf:
    """ELF64 LE 只读视图：phdrs + 供 F2/audit 的文件字节。"""

    def __init__(self, blob: bytes, name: str):
        self.name = name
        self.blob = blob
        if blob[:4] != b"\x7fELF":
            raise ValueError("非 ELF（magic）")
        if blob[4] != 2 or blob[5] != 1:
            raise ValueError("非 ELF64/LE")
        if len(blob) < 64:
            raise ValueError("ehdr 截断")
        (self.phoff,) = struct.unpack_from("<Q", blob, 0x20)
        (phentsize, phnum) = struct.unpack_from("<HH", blob, 0x36)
        self.phdrs = []
        for i in range(phnum):
            p = self.phoff + i * phentsize
            if p + 56 > len(blob):
                raise ValueError(f"phdr[{i}] 截断")
            (ptype, flags) = struct.unpack_from("<II", blob, p)
            (offset, vaddr) = struct.unpack_from("<QQ", blob, p + 8)
            (filesz, memsz, align) = struct.unpack_from("<QQQ", blob, p + 0x20)
            self.phdrs.append(
                {
                    "type": ptype,
                    "flags": flags,
                    "offset": offset,
                    "vaddr": vaddr,
                    "filesz": filesz,
                    "memsz": memsz,
                    "align": align,
                }
            )

    def ptls(self):
        for p in self.phdrs:
            if p["type"] == PT_TLS:
                return p
        return None

    def file_bytes(self, vaddr: int, n: int) -> bytes:
        """虚拟窗 [vaddr, vaddr+n) 的文件背书字节（无背书页返回 None）。"""
        for p in self.phdrs:
            if p["type"] != PT_LOAD:
                continue
            base, span = p["vaddr"], p["filesz"]
            if base <= vaddr and vaddr + n <= base + span:
                off = p["offset"] + (vaddr - base)
                if off + n > len(self.blob):
                    return None
                return self.blob[off:off + n]
        return None


def check_elf(name: str, blob: bytes) -> list:
    """F1/F2/F3 三判据 → FAIL 理由清单（空 = 过）。畸形 ELF 也是 FAIL。"""
    try:
        elf = Elf(blob, name)
    except ValueError as e:
        return [f"{name}: 非可解析 ELF64 LE（{e}）"]
    fails = []
    # F3：ELF 头在映像内（X-4e 判例）——仅对 PT_LOAD 驱动的 exec 族（链接基
    # ≥0x200000）生效。0x1000 = USER_CODE_VA 平像族（kernel/src/user.rs
    # UserProcess::build 约定 flat 输入，装载不读 PT_LOAD；且裸族无 crt0/
    # find_ptls 解引用链）双保险不踩杀链——F3 首日拦 hello/policydemo/
    # SETTINGS!settings 三件即此族误伤，非真雷。手写 PHDRS 缺 FILEHDR
    # PHDRS 时 lld 把头排出所有段 → __ehdr_start 解析为 0 → crt0 find_ptls
    # 读 [0+0x20]（e_phoff）胎死——find_ptls 无论有无 PT_TLS 都先走头。
    EXEC_LINK_BASE = 0x200000  # exec 族链接基（bsh/stdhello/dpgx 同值）
    loads = [p for p in elf.phdrs if p["type"] == PT_LOAD]
    if loads and min(p["vaddr"] for p in loads) >= EXEC_LINK_BASE and not any(
        p["offset"] == 0 for p in loads
    ):
        fails.append(
            f"{name}: F3 ELF 头不在任何 PT_LOAD（无 p_offset==0 段）——"
            "__ehdr_start 将解析为 0，crt0 find_ptls 胎死（X-4e）"
        )
    tls = elf.ptls()
    if tls is None:
        return fails  # 无 PT_TLS：crt0 空操作面，免疫
    vaddr, filesz, memsz, align = (
        tls["vaddr"], tls["filesz"], tls["memsz"], tls["align"],
    )
    # F1：放置可行性（python 镜像，base=0 = worst case，见模块头注）。
    if tls_place(0, TLS_ARENA_CAP, filesz, memsz, align) is None:
        fails.append(
            f"{name}: F1 放置溢出 memsz={memsz} align={align} "
            f"cap={TLS_ARENA_CAP}（crt0 brk #0x7C1 面实际可达）"
        )
    # F2：.tdata 文件字节里的 TLS 窗自指针（拷贝式下 = 悬垂直址）。
    if filesz > 0:
        tdata = elf.file_bytes(vaddr, filesz)
        if tdata is None:
            fails.append(f"{name}: .tdata 窗无文件背书（p_offset/filesz 畸形）")
        else:
            lo, hi = vaddr, vaddr + memsz
            for i in range(0, len(tdata) - 7):
                (v,) = struct.unpack_from("<Q", tdata, i)
                if lo <= v < hi:
                    fails.append(
                        f"{name}: F2 .tdata@+{i:#x} 自指针 {v:#x} "
                        f"落 TLS 窗 [{lo:#x},{hi:#x})"
                    )
                    break
    return fails


# ---- census / adrp 审计（--audit；只审计不 FAIL）----


def adrp_same_page_hits(elf: Elf) -> int:
    """执行段内 adrp 解算页与 TLS 窗相交的计数（AArch64 解码；假阳性
    （同页窗外 .data.rel.ro/.data）已知无害——审计行不设门）。"""
    tls = elf.ptls()
    if tls is None:
        return 0
    lo, hi = tls["vaddr"], tls["vaddr"] + tls["memsz"]
    hits = 0
    for p in elf.phdrs:
        if p["type"] != PT_LOAD or not (p["flags"] & PF_X):
            continue
        seg = elf.blob[p["offset"]:p["offset"] + p["filesz"]]
        for i in range(0, len(seg) - 3, 4):
            (insn,) = struct.unpack_from("<I", seg, i)
            if insn & 0x9F00_0000 != 0x9000_0000:
                continue
            immlo = (insn >> 29) & 3
            immhi = (insn >> 5) & 0x7FFF
            imm = (immhi << 2) | immlo
            if imm & (1 << 20):
                imm -= 1 << 21
            pc = p["vaddr"] + i
            page = pc & ~0xFFF
            target = page + (imm << 12)
            if target < hi and target + 0x1000 > lo:
                hits += 1
    return hits


def census_line(name: str, blob: bytes) -> str:
    """单 ELF census 审计行（恒绿）。"""
    try:
        elf = Elf(blob, name)
    except ValueError as e:
        return f"  {name}: 非 ELF（{e}）——不判"
    tls = elf.ptls()
    if tls is None:
        return f"  {name}: 无 PT_TLS（免疫面）"
    v, f, m, a = tls["vaddr"], tls["filesz"], tls["memsz"], tls["align"]
    # 法证判定法：.tbss 窗 [v+f, v+m) 落 PT_LOAD filesz 文件背书内且非零
    # → 拷贝式前「带毒形态」（现无害，census 只审计）。
    w0, w1 = v + f, v + m
    shape = "纯 .tbss 尾外零区（结构性干净形态）"
    if w1 > w0:
        win = elf.file_bytes(w0, w1 - w0)
        if win is not None:
            shape = (
                "旧带毒形态（.tbss∩文件背书非零——拷贝式后无害）"
                if any(win) else "文件背书零字节形态"
            )
    adrp = adrp_same_page_hits(elf)
    return (
        f"  {name}: PT_TLS vaddr={v:#x} filesz={f:#x} memsz={m:#x} "
        f"align={a:#x}；{shape}；adrp 同页入窗 {adrp} 处（假阳性已知）"
    )


# ---- 各入口 ----


def iter_rootfs(path: str):
    """rootfs.cpio → [(entry name str, ELF bytes or None)]；app/*.bap 递归。"""
    with open(path, "rb") as fh:
        blob = fh.read()
    for name, data in parse_cpio(blob):
        n = name.decode("ascii", "replace")
        if n.startswith("app/") and n.endswith(".bap"):
            try:
                inner = parse_cpio(data)
            except ValueError as e:
                yield (n, None, f"（.bap 解析失败：{e}）")
                continue
            for iname, idata in inner:
                if iname.startswith(b"bin/"):
                    yield (f"{n}!{iname.decode('ascii', 'replace')}", idata, None)
        else:
            yield (n, data, None)


def do_check(path: str) -> int:
    fails = []
    n_elf = 0
    for name, data, err in iter_rootfs(path):
        if err is not None:
            fails.append(f"{name}: {err}")
            continue
        if data[:4] == b"\x7fELF":
            n_elf += 1
            fails += check_elf(name, data)
        elif name.startswith("bin/"):
            fails.append(f"{name}: bin/ 条目非 ELF（头 4 字节 {data[:4]!r}）")
    if fails:
        for f in fails:
            print(f"tlsscan: FAIL {f}")
        print(f"tlsscan: {len(fails)} 处 FAIL（F1/F2/F3 门）")
        return 1
    print(f"tlsscan: {os.path.basename(path)} x{n_elf} ELF 全过（F1 放置/F2 自指针/F3 头在段）")
    return 0


def do_audit(path: str) -> int:
    if not os.path.isfile(path):
        print(f"tlsscan audit: 跳过（{path} 缺席——先 just build-user）")
        return 0
    print(f"tlsscan audit: census（恒绿——拷贝式后 .tbss∩文件字节无害，只审计）")
    for name, data, err in iter_rootfs(path):
        if err is not None or data[:4] != b"\x7fELF":
            print(f"  {name}: 非 ELF/畸形——不判")
            continue
        print(census_line(name, data))
    return 0


# ---- selftest：合成 ELF 向量钉判定逻辑（与 berry-tls host 测同向量）----


def mk_phdr(ptype: int, vaddr: int, filesz: int, memsz: int, align: int,
            offset: int = 0, flags: int = 5) -> bytes:
    p = bytearray(56)
    struct.pack_into("<II", p, 0, ptype, flags)
    struct.pack_into("<QQ", p, 8, offset, vaddr)
    struct.pack_into("<QQQ", p, 0x20, filesz, memsz, align)
    return bytes(p)


def mk_elf(phdrs) -> bytes:
    """ehdr(64) + 56B phdr 串（标准形态：文件 = 头 + phdrs + 段数据）。"""
    e = bytearray(64)
    e[:4] = b"\x7fELF"
    e[4], e[5] = 2, 1
    struct.pack_into("<Q", e, 0x20, 64)  # e_phoff
    struct.pack_into("<HH", e, 0x36, 56, len(phdrs))
    out = bytes(e) + b"".join(phdrs)
    return out


def tls_elf(vaddr: int, filesz: int, memsz: int, align: int,
            tdata: bytes = b"") -> bytes:
    """真实形态单 TLS ELF：PT_LOAD 盖全文件 + PT_TLS 窗背书 .tdata
    （.tdata 落 LOAD 文件尾，off = 头+双 phdr；真 ELF 同形——PT_TLS
    恒在 PT_LOAD 文件背书区内）。"""
    data_off = 64 + 2 * 56
    if len(tdata) < filesz:  # 向量自洽：filesz 必有等量文件背书（真 ELF 同形）
        tdata += b"\0" * (filesz - len(tdata))
    load = mk_phdr(PT_LOAD, vaddr - data_off, data_off + len(tdata),
                   data_off + len(tdata), 0x1000, offset=0, flags=6)
    tls = mk_phdr(PT_TLS, vaddr, filesz, memsz, align, offset=data_off)
    return mk_elf([load, tls]) + tdata


def expect(label: str, got, want) -> int:
    ok = got == want
    print(f"  selftest {'过（预期）' if ok else 'FAIL'}: {label} → {got!r}")
    return 0 if ok else 1


def selftest() -> int:
    bad = 0
    print("  —— tls_place python 镜像（对拍 berry_tls::tls_place 同向量）——")
    bad += expect("镜像家族形状 align=8", tls_place(0x5000, TLS_ARENA_CAP, 0x258, 0x301, 8), 0x5010)
    bad += expect("镜像 4080 恰合", tls_place(0, TLS_ARENA_CAP, 0, 4080, 8), 0x10)
    bad += expect("镜像 4081 溢", tls_place(0, TLS_ARENA_CAP, 0, 4081, 8), None)
    bad += expect("镜像 align32 边界 4064 恰合", tls_place(0, TLS_ARENA_CAP, 0, 4064, 32), 0x20)
    bad += expect("镜像 align32 边界 4065 溢", tls_place(0, TLS_ARENA_CAP, 0, 4065, 32), None)
    bad += expect("镜像非 2 幂拒", tls_place(0, TLS_ARENA_CAP, 0, 8, 3), None)
    bad += expect("镜像 align=0 拒", tls_place(0, TLS_ARENA_CAP, 0, 8, 0), None)
    bad += expect("镜像 filesz>memsz 拒", tls_place(0, TLS_ARENA_CAP, 100, 50, 8), None)

    print("  —— F1/F2 全链（check_elf 合成 ELF）——")
    bad += expect("F1 家族形状过", check_elf("t", tls_elf(0x300000, 0x258, 0x301, 8)), [])
    bad += expect("F1 4081 溢 FAIL", bool(check_elf("t", tls_elf(0x300000, 0, 4081, 8))),
                  True)
    bad += expect("F1 align32 4065 溢 FAIL",
                  bool(check_elf("t", tls_elf(0x300000, 0, 4065, 32))), True)
    bad += expect("F1 无 PT_TLS 免疫", check_elf("t", mk_elf([mk_phdr(PT_LOAD, 0x1000, 0x100, 0x100, 0x1000)])), [])
    # F3：ELF 头在映像内（X-4e——dpgx 断链脚本实雷同形：LOAD off=0x1000）。
    bad += expect("F3 头段外 FAIL",
                  bool(check_elf("t", mk_elf([mk_phdr(PT_LOAD, 0x200000, 0x100, 0x100,
                                                       0x1000, offset=0x1000)]))),
                  True)
    bad += expect("F3 头段内过",
                  check_elf("t", mk_elf([mk_phdr(PT_LOAD, 0x200000, 0x100, 0x100,
                                                0x1000, offset=0)])),
                  [])
    # F3 平像族豁免：0x1000=USER_CODE_VA 族 off=0x1000 同形不拦（首日三误伤面）。
    bad += expect("F3 平像族豁免",
                  check_elf("t", mk_elf([mk_phdr(PT_LOAD, 0x1000, 0x100, 0x100,
                                                0x1000, offset=0x1000)])),
                  [])
    # F2：.tdata 内嵌指向 TLS 窗的 u64 自指针（拷贝式下悬垂直址）。
    bad += expect("F2 自指针 FAIL",
                  bool(check_elf("t", tls_elf(0x300000, 16, 32, 8,
                                              tdata=struct.pack("<QQ", 0x300008, 0)))),
                  True)
    bad += expect(
        "F2 窗外指针过",
        check_elf("t", tls_elf(0x300000, 16, 32, 8,
                               tdata=struct.pack("<QQ", 0x400000, 0))),
        [],
    )

    print("  —— cpio 面（解码 + .bap 递归；编码复用 mkcpio 单源）——")
    good = tls_elf(0x300000, 0x258, 0x301, 8)
    over = tls_elf(0x300000, 0, 4081, 8)
    bap_bad = build_archive([(b"bin/inner", over), (b"manifest.toml", b"[app]\n")])
    bap_ok = build_archive([(b"bin/inner", good), (b"manifest.toml", b"[app]\n")])
    cases = [
        ("cpio F1 溢 FAIL", [(b"bin/good", good), (b"bin/over", over)], 1),
        ("cpio 全过", [(b"bin/good", good)], 0),
        (".bap 递归 F1 FAIL", [(b"app/T.bap", bap_bad), (b"bin/good", good)], 1),
        (".bap 递归全过", [(b"app/T.bap", bap_ok), (b"bin/good", good)], 0),
        ("bin/ 非 ELF FAIL", [(b"bin/notelf", b"#!/bin/sh\n")], 1),
    ]
    import contextlib
    import io
    import tempfile

    with tempfile.NamedTemporaryFile(suffix=".cpio", delete=False) as tf:
        tmp = tf.name
    try:
        for label, items, want in cases:
            with open(tmp, "wb") as fh:
                fh.write(build_archive(items))
            with contextlib.redirect_stdout(io.StringIO()):  # 静音内层门输出
                got = do_check(tmp)
            bad += expect(label, got, want)
    finally:
        os.unlink(tmp)

    if bad:
        print(f"tlsscan selftest: FAIL x{bad}")
        return 1
    print("tlsscan selftest: ok")
    return 0


def main() -> int:
    args = sys.argv[1:]
    if args[:1] == ["--check"] and len(args) == 2:
        return do_check(args[1])
    if args[:1] == ["--audit"] and len(args) <= 2:
        return do_audit(args[1] if len(args) == 2 else "rootfs.cpio")
    if args == ["--selftest"]:
        return selftest()
    sys.stderr.write(__doc__)
    return 2


if __name__ == "__main__":
    sys.exit(main())
