#!/usr/bin/env python3
"""mkfat——确定性 FAT12 造盘器（P6 A-5，配对消费端 `用户态/berry-fatfs`
解析 crate + fsmgr FAT 只读层；mkcpio 同纪律：同输入同字节，AI 测试友好化）。

用法：mkfat.py <out.img> [--fixture <path>]
  - <out.img>：整盘 2048 扇区 × 512B = 1MiB——布局 = [16 扇区 vblk 测试
    保留区][FAT12 卷 @ LBA16]（LBA0 印章 / LBA1 vblk 自验 / LBA5 BLKD 自验
    全落保留区，与卷数据零交集；盘容断言 EXPECT_SECTORS=2048 不变）；
  - --fixture：berry-fatfs host 测试的跨实现共识基准——含 fixture 专列
    子目录（见 FIXTURE_DIRS；随产 x.img 同源含目录，默认盘**不含**——
    QEMU 冒烟盘字节零漂移）。fixture = 卷前缀至末链簇（boot + 双 FAT +
    根目录 + 全部已分配簇数据），确定性字节进 git。
  - --check-fixture <path>：守卫模式（不写盘）——内存重产 fixture 字节与
    <path> 逐字节比对，不一致 exit 1。挂 just test-host 前置：改了 mkfat
    忘重产 --fixture，host 共识测试会对陈旧基准假绿（A-5 对抗审查实抓）。

确定性：无时刻输入（日期定值 2020-01-01 / 卷序号定值），字段全定宽——
归档字节只随内容变。 baked 文件（簇分配序 = 根文件名序 → 逐目录名序，
目录簇先于其子文件簇）：
  - HELLO.TXT    1 簇  （簇 2）
  - PATTERN.BIN  5000B = (i*31+7)%256，3 簇（簇 3→4→5——链走活证）
  - fixture 专列（FIXTURE_DIRS，默认盘不含）：
    - DATA        目录 1 簇（簇 6，attr 0x10——C-0 刀 3 子目录读面基准）
    - DATA/NOTE.TXT  1 簇（簇 7）
卷几何：簇 = 4 扇区（2KiB）、保留 1、双 FAT × 2 扇区、根 16 项（1 扇区）
→ 数据 2026 扇区 = 506 簇 < 4085（FAT12 合法域）。
"""
import sys

SECTOR = 512
DISK_SECTORS = 2048          # 1MiB（vblk EXPECT_SECTORS 对账）
RESERVED_ZONE = 16           # vblk 测试保留区（卷前缀，LBA 0..15）
VOL_SECTORS = DISK_SECTORS - RESERVED_ZONE  # 2032
SPC = 4                      # sectors per cluster
NUM_FATS = 2
ROOT_ENTRIES = 16
VOLUME_ID = 0x42525953       # "BRS" 定值——确定性
DATE = 0x5021                # 2020-01-01（(40<<9)|(1<<5)|1）
TIME = 0x0000

# 卷内布局（扇区，卷相对）：boot=0, FAT×2 @1, 根目录 @5, 数据 @6。
ROOT_LISTING = [
    # (8.3 名, 内容字节)
    (b"HELLO.TXT", b"berryos fat read-only hello\n"),
    (b"PATTERN.BIN", bytes((i * 31 + 7) % 256 for i in range(5000))),
]

# fixture 专列子目录（C-0 刀 3）：默认卷 build_volume() **不含**——QEMU
# 冒烟盘（mkfat <out.img> 裸调）字节零漂移是硬线，子目录只进共识基准。
# `DATA` 名序在 PATTERN.BIN 后 = 簇分配不扰 HELLO@2 / PATTERN@3..5 的
# 既有共识锚（目录簇 6、子文件簇 7 都是全新分配）。目录项 attr 0x10 /
# size 0；目录簇内容 = 子文件项（32B/项）+ 0x00 终结填充——mkfat 不产
# `.`/`..` 项与多簇目录（v1 读面几何，两端同约定）。
FIXTURE_DIRS = [
    (b"DATA", [(b"NOTE.TXT", b"berryos fat subdir note\n")]),
]


def u16(v: int) -> bytes:
    return v.to_bytes(2, "little")


def u32(v: int) -> bytes:
    return v.to_bytes(4, "little")


def fat12_set(fat: bytearray, n: int, val: int) -> None:
    """FAT12 表项写入（12 位紧包：偶项低 12 位 / 奇项高 12 位）。"""
    off = n * 3 // 2
    if n % 2 == 0:
        fat[off] = val & 0xFF
        fat[off + 1] = (fat[off + 1] & 0xF0) | ((val >> 8) & 0x0F)
    else:
        fat[off] = (fat[off] & 0x0F) | ((val & 0x0F) << 4)
        fat[off + 1] = (val >> 4) & 0xFF


def dir_entry(name11: bytes, attr: int, clus: int, size: int) -> bytes:
    e = bytearray(32)
    e[0:11] = name11
    e[11] = attr
    e[22:24] = u16(TIME)   # 簇写时刻（定值）
    e[24:26] = u16(DATE)
    e[26:28] = u16(clus)   # 首簇
    e[28:32] = u32(size)
    return bytes(e)


def name_to_11(n: bytes) -> bytes:
    base, _, ext = n.partition(b".")
    return base.ljust(8, b" ") + ext.ljust(3, b" ")


def build_volume(listing=None, dirs=()) -> tuple[bytearray, dict]:
    """构建 FAT12 卷（卷相对 LBA 0 起）。返回 (卷字节, 几何摘要)。

    listing = [(名, 内容)] 根文件；dirs = [(目录名, [(子文件名, 内容)])]
    子目录（attr 0x10 单簇）。簇分配序 = 根文件名序 → 逐目录名序（目录簇
    先于其子文件簇）——确定性，与旧版（纯文件名序）对默认卷逐字节同产。
    """
    listing = ROOT_LISTING if listing is None else listing
    fat_sz = 2  # (506+2)*1.5=762B → 2 扇区
    data_start = 1 + NUM_FATS * fat_sz + 1  # 6（根目录恰 1 扇区）
    clusters = (VOL_SECTORS - data_start) // SPC  # 506
    assert clusters < 4085, "FAT12 域外"

    vol = bytearray(VOL_SECTORS * SECTOR)
    # —— boot sector（BPB）。
    b = bytearray(SECTOR)
    b[0:3] = b"\xEB\x3C\x90"            # jmp short + nop
    b[3:11] = b"BERRYOS "               # OEM 名（8B）
    b[11:13] = u16(SECTOR)
    b[13] = SPC
    b[14:16] = u16(1)                   # 保留扇区
    b[16] = NUM_FATS
    b[17:19] = u16(ROOT_ENTRIES)
    b[19:21] = u16(VOL_SECTORS)         # total16（2032 < 65536 用 16 位面）
    b[21] = 0xF8                        # media（硬盘）
    b[22:24] = u16(fat_sz)
    b[24:26] = u16(63)                  # 每磁道扇区（CHS 遗物，定值）
    b[26:28] = u16(255)                 # 磁头数（同上）
    b[28:32] = u32(RESERVED_ZONE)       # hidden = 卷前缀扇区数（消费端据此
                                        # 折算盘绝对 LBA——不硬编码 16）
    b[36] = 0x80                        # drive number
    b[38] = 0x29                        # boot sig（扩展引导签名的起点）
    b[39:43] = u32(VOLUME_ID)
    b[43:54] = b"BERRYOS    "           # 卷标（11B，与根目录 label 项一致）
    b[54:62] = b"FAT12   "
    b[510:512] = b"\x55\xAA"
    vol[0:SECTOR] = b

    # —— FAT ×2（项 0=media 0xFF8 / 项 1=0xFFF 保留；链尾 EOC）。
    fat = bytearray(fat_sz * SECTOR)
    fat12_set(fat, 0, 0xFF8)
    fat12_set(fat, 1, 0xFFF)
    next_clus = 2

    def alloc_chain(nbytes: int) -> int:
        """nbytes 的簇链（至少 1 簇），返回首簇。"""
        nonlocal next_clus
        nclus = max(1, (nbytes + SPC * SECTOR - 1) // (SPC * SECTOR))
        first = next_clus
        for i in range(nclus):
            fat12_set(fat, first + i, first + i + 1)
        fat12_set(fat, first + nclus - 1, 0xFFF)  # 链尾 EOC
        next_clus += nclus
        return first

    root_entries = []  # (name11, attr, 首簇, size)
    placements = []    # (簇号, 已补零到簇倍数的数据)

    def place(clus: int, data: bytes) -> None:
        pad = data + b"\0" * (clusters_pad(len(data), SPC * SECTOR) - len(data))
        assert len(pad) % (SPC * SECTOR) == 0
        placements.append((clus, pad))

    for name, data in sorted(listing):
        first = alloc_chain(len(data))
        root_entries.append((name_to_11(name), 0x20, first, len(data)))
        place(first, data)
    for dname, children in sorted(dirs):
        dfirst = next_clus  # 目录单簇：先占簇，再排子文件簇
        fat12_set(fat, dfirst, 0xFFF)
        next_clus += 1
        child_entries = []
        for cname, cdata in sorted(children):
            cfirst = alloc_chain(len(cdata))
            child_entries.append(dir_entry(name_to_11(cname), 0x20, cfirst, len(cdata)))
            place(cfirst, cdata)
        root_entries.append((name_to_11(dname), 0x10, dfirst, 0))  # 目录 size 恒 0
        place(dfirst, b"".join(child_entries))  # 尾随 0x00 = 目录终结
    for i in range(NUM_FATS):
        vol[(1 + i * fat_sz) * SECTOR:(1 + (i + 1) * fat_sz) * SECTOR] = fat

    # —— 根目录（16 项 × 32B）：卷标 + 文件/目录项，余 0x00 = 目录终结。
    root = bytearray(SECTOR)
    root[0:32] = dir_entry(b"BERRYOS    ", 0x08, 0, 0)  # 卷标项（attr 0x08）
    for i, (name11, attr, clus, size) in enumerate(root_entries):
        root[(i + 1) * 32:(i + 2) * 32] = dir_entry(name11, attr, clus, size)
    vol[data_start * SECTOR - SECTOR:data_start * SECTOR] = root  # 根 @ 扇区 5

    # —— 数据区（文件内容 / 目录簇的子项表）。
    for clus, pad in placements:
        off = (data_start + (clus - 2) * SPC) * SECTOR
        vol[off:off + len(pad)] = pad

    return vol, {"clusters": clusters, "fat_sz": fat_sz, "data_start": data_start,
                 "next_clus": next_clus}


def clusters_pad(size: int, clus_sz: int) -> int:
    """文件占用的整簇字节数（向上取整，至少 1 簇）。"""
    return max(clus_sz, (size + clus_sz - 1) // clus_sz * clus_sz)


def volume_for(args: list) -> tuple[bytearray, dict]:
    """按模式选卷：--fixture/--check-fixture 面带 FIXTURE_DIRS（C-0 刀 3
    子目录共识基准），默认裸调 = 纯 ROOT_LISTING（QEMU 冒烟盘零漂移）。"""
    if "--fixture" in args or "--check-fixture" in args:
        return build_volume(ROOT_LISTING, FIXTURE_DIRS)
    return build_volume()


def main() -> int:
    args = sys.argv[1:]
    # 守卫模式：不写盘，只比对 fixture（挂 test-host 前置——基准漂移即 FAIL）。
    if "--check-fixture" in args:
        try:
            fx = args[args.index("--check-fixture") + 1]
        except IndexError:
            sys.stderr.write("mkfat: --check-fixture 需要一个路径参数\n")
            return 2
        vol, geo = volume_for(args)
        n = geo["data_start"] + (geo["next_clus"] - 2) * SPC
        want = bytes(vol[: n * SECTOR])
        try:
            with open(fx, "rb") as f:
                got = f.read()
        except OSError as e:
            print(f"mkfat: check-fixture FAIL: 读不了 {fx}：{e}")
            return 1
        if got != want:
            print(f"mkfat: check-fixture FAIL: {fx} 与现行 mkfat 产物不一致"
                  f"（got {len(got)}B / want {len(want)}B）——"
                  f"改过 mkfat 就重产：mkfat.py x.img --fixture {fx}")
            return 1
        print(f"mkfat: check-fixture ok: {fx} {len(want)}B")
        return 0
    if not args or args[0].startswith("-"):
        sys.stderr.write(__doc__)
        return 2
    out_path = args[0]
    vol, geo = volume_for(args)
    disk = bytearray(DISK_SECTORS * SECTOR)
    disk[RESERVED_ZONE * SECTOR:] = vol  # 保留区前缀全零（LBA0 印章由调用方 dd）
    with open(out_path, "wb") as f:
        f.write(disk)
    line = (f"mkfat: {out_path} {len(disk)}B vol@LBA{RESERVED_ZONE} "
            f"clusters={geo['clusters']} fat={geo['fat_sz']}x{NUM_FATS}")
    if "--fixture" in args:
        fx = args[args.index("--fixture") + 1]
        # 卷前缀至末链簇（fixture 专列：boot(1) + FAT(4) + 根(1) + 簇 2..7）。
        n = geo["data_start"] + (geo["next_clus"] - 2) * SPC
        with open(fx, "wb") as f:
            f.write(vol[:n * SECTOR])
        line += f" fixture={fx} {n * SECTOR}B"
    print(line)
    return 0


if __name__ == "__main__":
    sys.exit(main())
