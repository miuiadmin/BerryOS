#!/usr/bin/env python3
"""mkfat——确定性 FAT12 造盘器（P6 A-5，配对消费端 `用户态/berry-fatfs`
解析 crate + fsmgr FAT 只读层；mkcpio 同纪律：同输入同字节，AI 测试友好化）。

用法：mkfat.py <out.img> [--fixture <path>]
  - <out.img>：整盘 2048 扇区 × 512B = 1MiB——布局 = [16 扇区 vblk 测试
    保留区][FAT12 卷 @ LBA16]（LBA0 印章 / LBA1 vblk 自验 / LBA5 BLKD 自验
    全落保留区，与卷数据零交集；盘容断言 EXPECT_SECTORS=2048 不变）；
  - --fixture：berry-fatfs host 测试的跨实现共识基准——卷前缀 22 扇区
    （boot + 双 FAT + 根目录 + cluster 2..5 数据），确定性字节进 git。
  - --check-fixture <path>：守卫模式（不写盘）——内存重产 fixture 字节与
    <path> 逐字节比对，不一致 exit 1。挂 just test-host 前置：改了 mkfat
    忘重产 --fixture，host 共识测试会对陈旧基准假绿（A-5 对抗审查实抓）。

确定性：无时刻输入（日期定值 2020-01-01 / 卷序号定值），字段全定宽——
归档字节只随内容变。 baked 文件（名序即簇序）：
  - HELLO.TXT    1 簇  （簇 2）
  - PATTERN.BIN  5000B = (i*31+7)%256，3 簇（簇 3→4→5——链走活证）
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


def build_volume() -> tuple[bytearray, dict]:
    """构建 FAT12 卷（卷相对 LBA 0 起）。返回 (卷字节, 几何摘要)。"""
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

    # —— FAT ×2（项 0=media 0xFF8 / 项 1=0xFFF 保留；文件链按名序分配簇）。
    fat = bytearray(fat_sz * SECTOR)
    fat12_set(fat, 0, 0xFF8)
    fat12_set(fat, 1, 0xFFF)
    next_clus = 2
    files = []  # (name11, 首簇, 尺寸)
    for name, data in sorted(ROOT_LISTING):  # 名序 = 簇序（确定性）
        nclus = (len(data) + SPC * SECTOR - 1) // (SPC * SECTOR)
        if nclus == 0:
            nclus = 1  # 空文件也占一簇（FAT 惯例；本卷无空文件，防御面）
        first = next_clus
        for i in range(nclus):
            fat12_set(fat, first + i, first + i + 1)
        fat12_set(fat, first + nclus - 1, 0xFFF)  # 链尾 EOC
        files.append((name_to_11(name), first, len(data)))
        next_clus += nclus
    for i in range(NUM_FATS):
        vol[(1 + i * fat_sz) * SECTOR:(1 + (i + 1) * fat_sz) * SECTOR] = fat

    # —— 根目录（16 项 × 32B）：卷标 + 文件项，余 0x00 = 目录终结。
    root = bytearray(SECTOR)
    root[0:32] = dir_entry(b"BERRYOS    ", 0x08, 0, 0)  # 卷标项（attr 0x08）
    for i, (name11, clus, size) in enumerate(files):
        root[(i + 1) * 32:(i + 2) * 32] = dir_entry(name11, 0x20, clus, size)
    vol[data_start * SECTOR - SECTOR:data_start * SECTOR] = root  # 根 @ 扇区 5

    # —— 数据区。
    for (name11, clus, size), (name, data) in zip(files, sorted(ROOT_LISTING)):
        pad = data + b"\0" * (clusters_pad(len(data), SPC * SECTOR) - len(data))
        off = (data_start + (clus - 2) * SPC) * SECTOR
        vol[off:off + len(pad)] = pad
        assert len(pad) % (SPC * SECTOR) == 0
        assert size == len(data)

    return vol, {"clusters": clusters, "fat_sz": fat_sz, "data_start": data_start}


def clusters_pad(size: int, clus_sz: int) -> int:
    """文件占用的整簇字节数（向上取整，至少 1 簇）。"""
    return max(clus_sz, (size + clus_sz - 1) // clus_sz * clus_sz)


def main() -> int:
    args = sys.argv[1:]
    # 守卫模式：不写盘，只比对 fixture（挂 test-host 前置——基准漂移即 FAIL）。
    if "--check-fixture" in args:
        try:
            fx = args[args.index("--check-fixture") + 1]
        except IndexError:
            sys.stderr.write("mkfat: --check-fixture 需要一个路径参数\n")
            return 2
        vol, geo = build_volume()
        n = geo["data_start"] + 4 * SPC
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
    vol, geo = build_volume()
    disk = bytearray(DISK_SECTORS * SECTOR)
    disk[RESERVED_ZONE * SECTOR:] = vol  # 保留区前缀全零（LBA0 印章由调用方 dd）
    with open(out_path, "wb") as f:
        f.write(disk)
    line = (f"mkfat: {out_path} {len(disk)}B vol@LBA{RESERVED_ZONE} "
            f"clusters={geo['clusters']} fat={geo['fat_sz']}x{NUM_FATS}")
    if "--fixture" in args:
        fx = args[args.index("--fixture") + 1]
        # 卷前缀 22 扇区 = boot(1) + FAT(4) + 根(1) + 簇 2..5 数据(16)
        n = geo["data_start"] + 4 * SPC
        with open(fx, "wb") as f:
            f.write(vol[:n * SECTOR])
        line += f" fixture={fx} {n * SECTOR}B"
    print(line)
    return 0


if __name__ == "__main__":
    sys.exit(main())
