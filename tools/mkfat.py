#!/usr/bin/env python3
"""mkfat——确定性 FAT12 造盘器（P6 A-5，配对消费端 `user/berry-fatfs`
解析 crate + fsmgr FAT 只读层；mkcpio 同纪律：同输入同字节，AI 测试友好化）。

用法：mkfat.py <out.img> [--fixture <path>] [--put <local>:<fat_path> ...]
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
  - --put <local>:<fat_path>（V-0a 刀 1，可重复）：EFI 引导盘模式——读
    local 文件注入为 fat_path（如 build/BERRYBOOT.EFI:/EFI/BOOT/
    BOOTAA64.EFI）。与默认/fixture 模式互斥。产物 = 可移介质引导卷：
    **不掺 ROOT_LISTING**（内容即 puts）、卷占全盘（无 vblk 保留区，
    hidden=0）、media 0xF0、任意层嵌套、子目录产 `.`/`..` 项（EDK2
    FatDxe 友好——build_volume 的扁平 dirs 不产，两者约定见各自头注）。
    **默认盘字节零漂移硬线不破**：--put 走独立构造体 build_volume_puts，
    与 build_volume 共享叶子助手但互不扰动。8.3 短名（A-Z0-9，LFN 留
    后续）——超限/空段/重复路径一律 loud FAIL。
  - --selftest：--put 面自检（不写盘）——确定性 + FAT 链回读对拍 +
    `.`/`..` 存在性 + 负样本（非 8.3 / 空段 / 重复）。挂 just test-host
    前置（与 mkapp --selftest 同位）。
  - --fat16（V-2，仅与 --put 连用，单独出现拒绝）：FAT16 编码显式旗——
    否决自动域分派（会把现役 FAT12 引导盘翻宽变字节，撞零漂移铁线）；
    簇数 <4085 的卷声明 FAT16 = 宽域不符，assert loud 拒。
  - --fixture16 <path>（V-2，单飞）：berry-fatfs FAT16 共识基准——16384
    扇区卷的前缀至末链簇（boot + FAT16×2 + 根 + 6 簇 = 29696B），只写
    <path> 不产整盘；内容与 FAT12 fixture 同串。
  - --check-fixture16 <path>（V-2）：FAT16 基准守卫（结构镜像
    --check-fixture）——内存重产逐字节比对，漂移 exit 1 + 重产提示。

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

# FAT16 域值常量（V-2 能力层）：与 berry-fatfs 双端同值钉死（MS FAT spec）。
# FAT[0]=0xFF00|media、FAT[1]=0xFFFF、EOC=0xFFF8..0xFFFF（编码取 0xFFFF）、
# BAD=0xFFF7；簇数域 <4085=FAT12、4085..65524=FAT16、>=65525=FAT32 地界拒。
FAT16_MIN_CLUSTERS = 4085
FAT16_MAX_CLUSTERS = 65524
FAT16_EOC = 0xFFFF
FAT16_BAD = 0xFFF7

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

# FAT16 共识基准专列（V-2）：--fixture16/--check-fixture16 面的 puts——内容
# 字节与 FAT12 fixture 现役同源同串（ROOT_LISTING/FIXTURE_DIRS 的生成式
# 照抄），仅卷几何翻宽（16384 扇区 / fat_sz=16 / data_start=34 / 4087 簇
# / spc=4——4087 恰贴 FAT12/16 分界）。走 build_volume_puts 构造体（子
# 目录产 `.`/`..`，与 FAT12 侧 build_volume 扁平 dirs 不同形——两端约定
# 见各自头注）；产物 = 卷前缀至末链簇（boot1 + FAT16×2 + 根1 + 6 簇×4
# 扇区 = 58 扇区 29696B）。
FIXTURE16_PUTS = [
    (b"/HELLO.TXT", b"berryos fat read-only hello\n"),
    (b"/PATTERN.BIN", bytes((i * 31 + 7) % 256 for i in range(5000))),
    (b"/DATA/NOTE.TXT", b"berryos fat subdir note\n"),
]
FIXTURE16_VOL_SECTORS = 16384


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


def fat16_set(fat: bytearray, n: int, val: int) -> None:
    """FAT16 表项写入（u16le@2n 定宽——V-2，fat12_set 的宽域对偶）。"""
    off = n * 2
    fat[off:off + 2] = u16(val)


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
                 "next_clus": next_clus, "spc": SPC}


def clusters_pad(size: int, clus_sz: int) -> int:
    """文件占用的整簇字节数（向上取整，至少 1 簇）。"""
    return max(clus_sz, (size + clus_sz - 1) // clus_sz * clus_sz)


# ---------------------------------------------------------------- --put 面。
# EFI 引导盘构造（V-0a 刀 1）：与 build_volume 分工——那边服务内核冒烟盘
# （默认字节零漂移硬线），这边服务可移介质引导卷。共享叶子助手
# （u16/u32/fat12_set/dir_entry/name_to_11/clusters_pad），构造体独立。

def fat_path_comps(path) -> list:
    """引导盘 FAT 路径 → 8.3 分量列表（bytes、已大写）。loud 拒面：
    非绝对 / 空段（`//`、尾 `/`）/ `.`/`..` 分量 / 非 8.3 / 非 [A-Z0-9]。"""
    if isinstance(path, (bytes, bytearray)):
        path = path.decode("latin-1")
    if not path.startswith("/"):
        raise SystemExit(f"mkfat: --put FAIL: 路径须绝对（/{path}）")
    raw = path[1:].split("/")
    if raw == [""]:
        raise SystemExit("mkfat: --put FAIL: 路径为根（无文件名）")
    comps = []
    for seg in raw:
        if not seg:
            raise SystemExit(f"mkfat: --put FAIL: 空路径段（{path}）")
        if seg in (".", ".."):
            raise SystemExit(f"mkfat: --put FAIL: `.`/`..` 段（{path}）")
        up = seg.upper().encode("ascii", "strict") if seg.isascii() else b""
        if not up:
            raise SystemExit(f"mkfat: --put FAIL: 非 ASCII 段（{seg}）")
        base, _, ext = up.partition(b".")
        ok = 1 <= len(base) <= 8 and len(ext) <= 3 and ext.count(b".") == 0
        ok = ok and all(0x30 <= c <= 0x39 or 0x41 <= c <= 0x5A for c in up.replace(b".", b""))
        if not ok:
            raise SystemExit(
                f"mkfat: --put FAIL: `{seg}` 非 8.3 短名（[A-Z0-9]、主名≤8、"
                f"扩展≤3、单点——LFN 留后续）")
        comps.append(up)
    return comps


def build_volume_puts(puts: list, vol_sectors: int = DISK_SECTORS,
                      fat16: bool = False, hidden: int = 0) -> tuple:
    """构建 EFI 引导卷（无保留区前缀）。puts = [(fat_path, 内容字节)]；
    vol_sectors = 卷扇区数（V-0b 刀 1 裁决 C：--size-mib 参数化——默认
    DISK_SECTORS=2048 与 V-0a 产物逐字节不变）；fat16=True 走 FAT16 编码
    （V-2，显式旗——自动域分派会把现役 FAT12 盘翻宽变字节，正面撞零漂移
    铁线；fat12 路径逐字节不动）；hidden = BPB 卷前隐藏扇区数（默认 0 =
    全盘卷；V-3 mkimage MBR 分区盘传分区起始 LBA——BPB 与分区表自洽）。
    返回 (盘字节, 几何摘要)。

    与 build_volume 的差异面：任意层嵌套；子目录产 `.`/`..` 首两项
    （attr 0x10，`..` 指父簇——根上指 0；EDK2 FatDxe 友好）；hidden
    （默认 0，无 vblk 保留区）；media 0xF0（可移介质语义，FAT[0] 同值）；
    内容即 puts（不掺 ROOT_LISTING）。
    确定性簇序 = 深度优先：每层先文件（名序）后子目录（名序），目录簇
    先于其子内容簇（与 build_volume 扁平 dirs 同款约定，嵌套递归展开）。
    单目录单簇（`.`/`..` + 子项 ≤ (2048/32 - 2) = 62 项）。"""
    tree: dict = {}
    for path, data in puts:
        comps = fat_path_comps(path)
        node = tree
        for c in comps[:-1]:
            nxt = node.get(c)
            if nxt is None:
                nxt = {}
                node[c] = nxt
            elif not isinstance(nxt, dict):
                raise SystemExit(f"mkfat: --put FAIL: {path} 中途撞文件 {c!r}")
            node = nxt
        leaf = comps[-1]
        if leaf in node:
            raise SystemExit(f"mkfat: --put FAIL: 重复路径 {path}")
        node[leaf] = data

    # total 域（C-2c 开启 total32，V-3 预留位兑现）：≥0x10000 扇区走
    # total16=0 + total32（@BPB+32，FAT 规范 32 位合成路径——消费端
    # EDK2 FatDxe / berry-fatfs parse_bpb 均支持，berry-fatfs host 测试
    # bpb_total32_path 在册）；域内仍 total16（V-2 全部产物字节零漂移）。
    assert vol_sectors >= 1, "total 域外（≥1 扇区）"
    assert vol_sectors < 0x1_0000_0000, "total32 域外（u32 扇区）"
    # SPC 自适应（V-1c）：簇数出 FAT12 域（≥4085）时 SPC 翻倍直至入域——
    # 确定性：spc 只随 vol_sectors 变（8MiB 引导盘 → SPC=8 → ~2046 簇）。
    # 默认 1MiB/4MiB 卷 SPC=4 域内不动 = V-0a/刀 1 产物字节零漂移。
    # fat16 分支（V-2）：翻倍条件上移到 >65524（FAT16 域上限），出环后
    # assert clusters>=4085——声明宽与簇数域不符 = loud 拒（写侧镜像
    # 「不静默按错宽解」）；簇数 <4085 的卷声明 FAT16 同样撞此 assert。
    spc = SPC
    while True:
        # FAT 尺寸迭代收敛（V-0b 刀 1 裁决 C）：表须装下 (clusters+2) 项
        # ——fat12 紧包 ×1.5B、fat16 定宽 ×2B。小卷稳定在 fat_sz=2（V-0a
        # 1MiB 盘字节不变——默认零漂移的构造性保证）；大卷逐轮放大至收敛
        # （fat_sz ↑ → 数据区 ↓ → 需求 ↓，单调必收敛；4MiB → 6 扇区）。
        fat_sz = 2
        while True:
            data_start = 1 + NUM_FATS * fat_sz + 1
            clusters = (vol_sectors - data_start) // spc
            if fat16:
                need_sectors = ((clusters + 2) * 2 + SECTOR - 1) // SECTOR
            else:
                need_sectors = (((clusters + 2) * 3 + 1) // 2 + SECTOR - 1) // SECTOR
            if need_sectors <= fat_sz:
                break
            fat_sz = need_sectors
        if fat16:
            if clusters <= FAT16_MAX_CLUSTERS:
                break
        elif clusters < FAT16_MIN_CLUSTERS:
            break
        spc *= 2
        if spc > 64:
            raise SystemExit(
                f"mkfat: --put FAIL: {vol_sectors} 扇区超 FAT{'16' if fat16 else '12'}"
                " 可表达域（SPC>64）")
    if fat16:
        assert clusters >= FAT16_MIN_CLUSTERS, \
            "FAT16 声明与簇数域不符（<4085 属 FAT12 域——换 FAT12 或加大卷）"
    assert clusters >= 1, "卷太小：装不下数据区"

    vol = bytearray(vol_sectors * SECTOR)
    # —— boot sector（BPB）——与 build_volume 同形，差异三处：total=全盘 /
    # hidden 参数化（默认 0）/ media 0xF0。
    b = bytearray(SECTOR)
    b[0:3] = b"\xEB\x3C\x90"
    b[3:11] = b"BERRYOS "
    b[11:13] = u16(SECTOR)
    b[13] = spc
    b[14:16] = u16(1)
    b[16] = NUM_FATS
    b[17:19] = u16(ROOT_ENTRIES)
    if vol_sectors < 0x10000:
        b[19:21] = u16(vol_sectors)     # total16（域内 16 位面——V-2 产物零漂移）
    else:
        b[19:21] = u16(0)               # total16=0 + total32 合成路径（C-2c）
        b[32:36] = u32(vol_sectors)
    b[21] = 0xF0                # media：可移介质
    b[22:24] = u16(fat_sz)
    b[24:26] = u16(63)
    b[26:28] = u16(255)
    b[28:32] = u32(hidden)      # 卷前隐藏扇区（默认 0 全盘卷；MBR 分区盘 =
                                # 分区起始 LBA——BPB 与分区表自洽，V-3）
    b[36] = 0x00                # drive number（可移介质惯例）
    b[38] = 0x29
    b[39:43] = u32(VOLUME_ID)
    b[43:54] = b"BERRYBOOT  "    # 卷标（11B——引导盘自证身份）
    b[54:62] = b"FAT16   " if fat16 else b"FAT12   "
    b[510:512] = b"\x55\xAA"
    vol[0:SECTOR] = b

    # 按宽分派（V-2）：fat16 = u16le 定宽 + EOC 0xFFFF；fat12 = 紧包 +
    # EOC 0xFFF（V-0a 约定原样）。fat12 路径语义逐字节不变。
    if fat16:
        fat_set, eoc = fat16_set, FAT16_EOC
    else:
        fat_set, eoc = fat12_set, 0xFFF
    fat = bytearray(fat_sz * SECTOR)
    fat_set(fat, 0, (0xFF00 if fat16 else 0xF00) | 0xF0)  # FAT[0] 低字节 = media
    fat_set(fat, 1, 0xFFFF if fat16 else 0xFFF)
    next_clus = 2
    placements = []
    root_entries = []

    def alloc_chain(nbytes: int) -> int:
        nonlocal next_clus
        nclus = max(1, (nbytes + spc * SECTOR - 1) // (spc * SECTOR))
        first = next_clus
        for i in range(nclus):
            fat_set(fat, first + i, first + i + 1)
        fat_set(fat, first + nclus - 1, eoc)
        next_clus += nclus
        return first

    def place(clus: int, data: bytes) -> None:
        pad = data + b"\0" * (clusters_pad(len(data), spc * SECTOR) - len(data))
        assert len(pad) % (spc * SECTOR) == 0
        placements.append((clus, pad))

    def emit_dir(node: dict, parent_clus: int) -> int:
        """分配目录簇并排放内容，返回首簇。序 = 自簇 → 文件链（名序）→
        子目录递归（名序，子目录簇先于其自身子内容）。"""
        nonlocal next_clus
        first = next_clus
        fat_set(fat, first, eoc)
        next_clus += 1
        entries = [dir_entry(b".          ", 0x10, first, 0),
                   dir_entry(b"..         ", 0x10, parent_clus, 0)]
        files = sorted((n, d) for n, d in node.items() if not isinstance(d, dict))
        dirs = sorted((n, d) for n, d in node.items() if isinstance(d, dict))
        for name, data in files:
            c = alloc_chain(len(data))
            entries.append(dir_entry(name_to_11(name), 0x20, c, len(data)))
            place(c, data)
        for name, sub in dirs:
            sub_first = emit_dir(sub, first)
            entries.append(dir_entry(name_to_11(name), 0x10, sub_first, 0))
        blob = b"".join(entries)
        if len(blob) > spc * SECTOR:
            raise SystemExit(
                f"mkfat: --put FAIL: 目录内容 {len(blob)}B 超单簇 {spc * SECTOR}B"
                "（v1 单簇目录——多簇目录留后续）")
        place(first, blob)
        return first

    root_files = sorted((n, d) for n, d in tree.items() if not isinstance(d, dict))
    root_dirs = sorted((n, d) for n, d in tree.items() if isinstance(d, dict))
    for name, data in root_files:
        c = alloc_chain(len(data))
        root_entries.append(dir_entry(name_to_11(name), 0x20, c, len(data)))
        place(c, data)
    for name, sub in root_dirs:
        sub_first = emit_dir(sub, 0)
        root_entries.append(dir_entry(name_to_11(name), 0x10, sub_first, 0))
    if 1 + len(root_entries) > ROOT_ENTRIES:
        raise SystemExit(f"mkfat: --put FAIL: 根项 {1 + len(root_entries)} 超帽 {ROOT_ENTRIES}")
    if next_clus - 2 > clusters:
        raise SystemExit(f"mkfat: --put FAIL: 簇 {next_clus - 2} 超帽 {clusters}")

    for i in range(NUM_FATS):
        vol[(1 + i * fat_sz) * SECTOR:(1 + (i + 1) * fat_sz) * SECTOR] = fat
    root = bytearray(SECTOR)
    root[0:32] = dir_entry(b"BERRYBOOT  ", 0x08, 0, 0)
    for i, e in enumerate(root_entries):
        root[(i + 1) * 32:(i + 2) * 32] = e
    vol[(data_start - 1) * SECTOR:data_start * SECTOR] = root
    for clus, pad in placements:
        off = (data_start + (clus - 2) * spc) * SECTOR
        vol[off:off + len(pad)] = pad

    return vol, {"clusters": clusters, "fat_sz": fat_sz, "data_start": data_start,
                 "next_clus": next_clus, "spc": spc}


def fat12_get(fat: bytes, n: int) -> int:
    """FAT12 表项读取（fat12_set 的逆——自检回读对拍用）。"""
    off = n * 3 // 2
    if n % 2 == 0:
        return fat[off] | ((fat[off + 1] & 0x0F) << 8)
    return (fat[off] >> 4) | (fat[off + 1] << 4)


def fat16_get(fat: bytes, n: int) -> int:
    """FAT16 表项读取（fat16_set 的逆——自检回读对拍用）。"""
    off = n * 2
    return fat[off] | (fat[off + 1] << 8)


def selftest_puts() -> int:
    """--put 面自检：确定性 + FAT 链回读对拍 + `.`/`..` 存在 + 负样本。"""
    puts = [(b"/EFI/BOOT/BOOTAA64.EFI", bytes((i * 7 + 3) % 256 for i in range(3000))),
            (b"/HELLO.TXT", b"berryboot efi hello\n")]
    vol, geo = build_volume_puts(puts)
    if build_volume_puts(puts)[0] != vol:
        print("mkfat: selftest FAIL: --put 不确定")
        return 1
    fat = bytes(vol[SECTOR:(1 + geo["fat_sz"]) * SECTOR])
    root = vol[(geo["data_start"] - 1) * SECTOR:geo["data_start"] * SECTOR]

    def entries_of(blob) -> dict:
        out = {}
        for i in range(len(blob) // 32):
            e = bytes(blob[i * 32:(i + 1) * 32])
            if e[0] == 0:
                break
            out[e[0:11]] = (e[11], int.from_bytes(e[26:28], "little"),
                            int.from_bytes(e[28:32], "little"))
        return out

    def chain_read(vol_x: bytes, fat_x: bytes, geo_x: dict,
                   first: int, size: int) -> bytes:
        data = b""
        c = first
        while 0x2 <= c < 0xFF8:
            off = (geo_x["data_start"] + (c - 2) * geo_x["spc"]) * SECTOR
            data += bytes(vol_x[off:off + geo_x["spc"] * SECTOR])
            c = fat12_get(fat_x, c)
        return data[:size]

    ents = entries_of(root)
    assert b"BERRYBOOT  " in ents, "卷标项缺失"
    efi = ents[b"EFI        "]
    assert efi[0] == 0x10, "EFI 非 attr 0x10"
    efi_dir = entries_of(chain_read(vol, fat, geo, efi[1], 2048))
    assert b".          " in efi_dir and b"..         " in efi_dir, "`.`/`..` 缺失"
    assert efi_dir[b".          "][1] == efi[1], "`.` 簇不自指"
    assert efi_dir[b"..         "][1] == 0, "`..` 应指根(0)"
    boot = efi_dir[b"BOOT       "]
    boot_dir = entries_of(chain_read(vol, fat, geo, boot[1], 2048))
    assert boot_dir[b"..         "][1] == efi[1], "BOOT 的 `..` 应指 EFI 簇"
    app = boot_dir[b"BOOTAA64EFI"]
    if chain_read(vol, fat, geo, app[1], app[2]) != puts[0][1]:
        print("mkfat: selftest FAIL: BOOTAA64.EFI 链回读不等于源")
        return 1
    hello = ents[b"HELLO   TXT"]
    if chain_read(vol, fat, geo, hello[1], hello[2]) != puts[1][1]:
        print("mkfat: selftest FAIL: HELLO.TXT 链回读不等于源")
        return 1

    for bad in [b"/TOOLONGNAME.EFI", b"/EFI//BOOT/X", b"/a.b.c", b"EFI/BOOT/X",
                b"/EFI/", b"/x/y\xe9.EFI", b"/BO OT.EFI"]:
        try:
            fat_path_comps(bad.decode("latin-1"))
        except SystemExit:
            continue
        print(f"mkfat: selftest FAIL: 坏路径 {bad!r} 未被拒绝")
        return 1
    try:
        build_volume_puts([(b"/A.TXT", b"x"), (b"/A.TXT", b"y")])
    except SystemExit:
        pass
    else:
        print("mkfat: selftest FAIL: 重复路径未被拒绝")
        return 1
    # 尺寸参数化面（V-0b 刀 1 裁决 C）：4MiB 卷 + 2.8MiB 级多簇大档——
    # fat_sz 收敛 >2、链回读仍等于源；显式 2048 扇区与默认调用产物逐字节
    # 一致（V-0a 引导盘零漂移的构造性证明）；过小卷 loud 拒。
    big = bytes((i * 11 + 5) % 256 for i in range(1_100_000))
    puts2 = puts + [(b"/KERNEL.ELF", big)]
    vol2, geo2 = build_volume_puts(puts2, 8192)
    if len(vol2) != 8192 * SECTOR or geo2["fat_sz"] <= 2:
        print(f"mkfat: selftest FAIL: 4MiB 卷几何异常 fat_sz={geo2['fat_sz']}")
        return 1
    fat2 = bytes(vol2[SECTOR:(1 + geo2["fat_sz"]) * SECTOR])
    root2 = vol2[(geo2["data_start"] - 1) * SECTOR:geo2["data_start"] * SECTOR]
    kern = entries_of(root2)[b"KERNEL  ELF"]
    if chain_read(vol2, fat2, geo2, kern[1], kern[2]) != big:
        print("mkfat: selftest FAIL: KERNEL.ELF 大档链回读不等于源")
        return 1
    if build_volume_puts(puts, DISK_SECTORS)[0] != vol:
        print("mkfat: selftest FAIL: 显式 2048 扇区与默认调用产物不一致")
        return 1
    # SPC 自适应面（V-1c）：8MiB 引导盘量级（debug ELF ~2.9MiB + INIT.CPIO
    # ~3.4MiB；草案 L91 曾误记 INIT.CPIO——扩展名 4 字符超 8.3 界，
    # 实名 INITRD.BIN，偏差记 阶段沿革）→ SPC 翻倍入 FAT12 域；链回读仍等于源；4MiB 及以下 SPC=4
    # 不动（V-0a/刀 1 产物零漂移守卫）。
    init = bytes((i * 13 + 9) % 256 for i in range(3_500_000))
    puts3 = puts2 + [(b"/INITRD.BIN", init)]
    vol3, geo3 = build_volume_puts(puts3, 16384)
    if geo3["spc"] != 8 or geo3["clusters"] >= 4085:
        print(f"mkfat: selftest FAIL: 8MiB 卷几何异常 "
              f"spc={geo3['spc']} clusters={geo3['clusters']}")
        return 1
    fat3 = bytes(vol3[SECTOR:(1 + geo3["fat_sz"]) * SECTOR])
    root3 = vol3[(geo3["data_start"] - 1) * SECTOR:geo3["data_start"] * SECTOR]
    icpio = entries_of(root3)[b"INITRD  BIN"]
    if chain_read(vol3, fat3, geo3, icpio[1], icpio[2]) != init:
        print("mkfat: selftest FAIL: INITRD.BIN 大档链回读不等于源")
        return 1
    if geo2.get("spc") != SPC:
        print(f"mkfat: selftest FAIL: 4MiB 卷 SPC 漂移 spc={geo2.get('spc')}")
        return 1
    # FAT16 面（V-2）：确定性双跑 + 16384 扇区卷（4087 簇恰贴 FAT12/16
    # 分界、fat_sz=16 收敛）+ fstype 串/FAT[0]/FAT[1]/EOC 保留项 + 链回读
    # 对拍源字节 + FAT12 域小卷拒 + total16 域顶合法产。
    puts16 = [(b"/HELLO.TXT", b"berryos fat read-only hello\n"),
              (b"/PATTERN.BIN", bytes((i * 31 + 7) % 256 for i in range(5000))),
              (b"/DATA/NOTE.TXT", b"berryos fat subdir note\n")]
    vol16, geo16 = build_volume_puts(puts16, FIXTURE16_VOL_SECTORS, fat16=True)
    if build_volume_puts(puts16, FIXTURE16_VOL_SECTORS, fat16=True)[0] != vol16:
        print("mkfat: selftest FAIL: FAT16 --put 不确定")
        return 1
    if not (FAT16_MIN_CLUSTERS <= geo16["clusters"] <= FAT16_MAX_CLUSTERS) \
            or geo16["spc"] != 4 or geo16["fat_sz"] != 16:
        print(f"mkfat: selftest FAIL: FAT16 卷几何异常 "
              f"clusters={geo16['clusters']} spc={geo16['spc']} "
              f"fat_sz={geo16['fat_sz']}")
        return 1
    if bytes(vol16[54:62]) != b"FAT16   ":
        print("mkfat: selftest FAIL: FAT16 fstype 串缺失")
        return 1
    fat16raw = bytes(vol16[SECTOR:(1 + geo16["fat_sz"]) * SECTOR])
    if fat16_get(fat16raw, 0) != 0xFFF0 or fat16_get(fat16raw, 1) != 0xFFFF:
        print("mkfat: selftest FAIL: FAT16 FAT[0]/FAT[1] 保留项值错")
        return 1
    root16 = vol16[(geo16["data_start"] - 1) * SECTOR:geo16["data_start"] * SECTOR]
    ents16 = entries_of(root16)

    def chain_read16(first: int, size: int) -> bytes:
        data = b""
        c = first
        while 0x2 <= c < 0xFFF8:
            off = (geo16["data_start"] + (c - 2) * geo16["spc"]) * SECTOR
            data += bytes(vol16[off:off + geo16["spc"] * SECTOR])
            c = fat16_get(fat16raw, c)
        return data[:size]

    hello16 = ents16[b"HELLO   TXT"]
    if chain_read16(hello16[1], hello16[2]) != puts16[0][1]:
        print("mkfat: selftest FAIL: FAT16 HELLO.TXT 链回读不等于源")
        return 1
    pat16 = ents16[b"PATTERN BIN"]
    if chain_read16(pat16[1], pat16[2]) != puts16[1][1]:
        print("mkfat: selftest FAIL: FAT16 PATTERN.BIN 链回读不等于源")
        return 1
    if fat16_get(fat16raw, 2) != FAT16_EOC or fat16_get(fat16raw, 5) != FAT16_EOC:
        print("mkfat: selftest FAIL: FAT16 链尾 EOC≠0xFFFF（HELLO@2/PATTERN@3..5）")
        return 1
    # FAT12 域小卷声明 fat16 = 宽域不符，出环 assert loud 拒。
    try:
        build_volume_puts([(b"/A.TXT", b"x")], 2048, fat16=True)
    except (SystemExit, AssertionError):
        pass
    else:
        print("mkfat: selftest FAIL: FAT12 域小卷 --fat16 未被拒绝")
        return 1
    # total 域（C-2c 开启 total32，V-3 预留位兑现）：域顶 65535 扇区
    # total16=0xFFFF 编码不变（域内 16 位面——V-2 产物零漂移）+ spc=4
    # 直落 FAT16 域不误翻倍；≥0x10000 扇区走 total16=0 + total32 合成
    # 路径——96256 扇区（47MiB，`just image --size-mib 48` 的 FAT16 分区
    # 真身）spc=4 不翻倍；300000 扇区（设计原案翻倍样本，曾为 total16
    # assert 结构性拦截）clusters>65524 → spc 4→8 翻倍入域——翻倍分支
    # 正向触发面在此兑现。拒面 = total32 域外（u32 扇区上限）。
    vol_top, geo_top = build_volume_puts([(b"/A.TXT", b"x")], 65535, fat16=True)
    if vol_top[19:21] != b"\xff\xff":
        print("mkfat: selftest FAIL: total16 域顶编码漂移（应 0xFFFF）")
        return 1
    if geo_top["spc"] != 4 or not (
            FAT16_MIN_CLUSTERS <= geo_top["clusters"] <= FAT16_MAX_CLUSTERS):
        print(f"mkfat: selftest FAIL: total16 域顶 FAT16 卷几何异常 "
              f"spc={geo_top['spc']} clusters={geo_top['clusters']}")
        return 1
    vol32, geo32 = build_volume_puts([(b"/A.TXT", b"x")], 96256, fat16=True)
    if vol32[19:21] != b"\x00\x00" or vol32[32:36] != u32(96256):
        print("mkfat: selftest FAIL: total32 路径 BPB 编码异常"
              "（total16 应=0 + total32@32 应=卷扇区数）")
        return 1
    if geo32["spc"] != 4 or not (
            FAT16_MIN_CLUSTERS <= geo32["clusters"] <= FAT16_MAX_CLUSTERS):
        print(f"mkfat: selftest FAIL: total32 路径 FAT16 卷几何异常 "
              f"spc={geo32['spc']} clusters={geo32['clusters']}")
        return 1
    _, geo_dbl = build_volume_puts([(b"/A.TXT", b"x")], 300000, fat16=True)
    if geo_dbl["spc"] != 8 or not (
            FAT16_MIN_CLUSTERS <= geo_dbl["clusters"] <= FAT16_MAX_CLUSTERS):
        print(f"mkfat: selftest FAIL: FAT16 翻倍分支未触发"
              f"（300000 扇区应 spc 4→8）spc={geo_dbl['spc']} "
              f"clusters={geo_dbl['clusters']}")
        return 1
    try:
        build_volume_puts([(b"/A.TXT", b"x")], 0x1_0000_0000, fat16=True)
    except (SystemExit, AssertionError):
        pass
    else:
        print("mkfat: selftest FAIL: total32 域外卷未被拒绝")
        return 1
    try:
        build_volume_puts([(b"/A.TXT", b"x")], 8)
    except (SystemExit, AssertionError):
        pass
    else:
        print("mkfat: selftest FAIL: 过小卷未被拒绝")
        return 1
    print(f"mkfat: selftest ok: --put 卷 {len(vol)}B clusters_used="
          f"{geo['next_clus'] - 2}/{geo['clusters']}（链回读/嵌套目录/负样本/"
          f"尺寸参数化全过）")
    return 0


def volume_for(args: list) -> tuple[bytearray, dict]:
    """按模式选卷：--fixture/--check-fixture 面带 FIXTURE_DIRS（C-0 刀 3
    子目录共识基准），默认裸调 = 纯 ROOT_LISTING（QEMU 冒烟盘零漂移）；
    --fixture16/--check-fixture16 面（V-2）= FAT16 共识基准卷。"""
    if "--fixture16" in args or "--check-fixture16" in args:
        return build_volume_puts(FIXTURE16_PUTS, FIXTURE16_VOL_SECTORS, fat16=True)
    if "--fixture" in args or "--check-fixture" in args:
        return build_volume(ROOT_LISTING, FIXTURE_DIRS)
    return build_volume()


def main() -> int:
    args = sys.argv[1:]
    if args[:1] == ["--selftest"]:
        return selftest_puts()
    # FAT16 守卫模式（V-2，结构镜像 --check-fixture）：不写盘，内存重产
    # fixture16 字节与 <path> 逐字节比对，不一致 exit 1——基准漂移即 FAIL。
    if "--check-fixture16" in args:
        clash = [a for a in ("--check-fixture", "--fixture", "--fixture16", "--put")
                 if a in args]
        if clash:
            sys.stderr.write(f"mkfat: --check-fixture16 与 {' '.join(clash)} 互斥\n")
            return 2
        try:
            fx = args[args.index("--check-fixture16") + 1]
        except IndexError:
            sys.stderr.write("mkfat: --check-fixture16 需要一个路径参数\n")
            return 2
        vol, geo = volume_for(args)
        n = geo["data_start"] + (geo["next_clus"] - 2) * geo["spc"]
        want = bytes(vol[: n * SECTOR])
        try:
            with open(fx, "rb") as f:
                got = f.read()
        except OSError as e:
            print(f"mkfat: check-fixture16 FAIL: 读不了 {fx}：{e}")
            return 1
        if got != want:
            print(f"mkfat: check-fixture16 FAIL: {fx} 与现行 mkfat 产物不一致"
                  f"（got {len(got)}B / want {len(want)}B）——"
                  f"改过 mkfat 就重产：mkfat.py --fixture16 {fx}")
            return 1
        print(f"mkfat: check-fixture16 ok: {fx} {len(want)}B")
        return 0
    # 守卫模式：不写盘，只比对 fixture（挂 test-host 前置——基准漂移即 FAIL）。
    if "--check-fixture" in args:
        if "--fixture16" in args:
            sys.stderr.write("mkfat: --check-fixture 与 --fixture16 互斥\n")
            return 2
        try:
            fx = args[args.index("--check-fixture") + 1]
        except IndexError:
            sys.stderr.write("mkfat: --check-fixture 需要一个路径参数\n")
            return 2
        vol, geo = volume_for(args)
        n = geo["data_start"] + (geo["next_clus"] - 2) * geo["spc"]
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
    # FAT16 基准产盘（V-2）：单飞——不产整盘只写卷前缀（boot + FAT16×2 +
    # 根 + 全部已分配簇），确定性字节进 git；与 --fixture/--put 互斥。
    if "--fixture16" in args:
        idx = args.index("--fixture16")
        try:
            fx = args[idx + 1]
        except IndexError:
            sys.stderr.write("mkfat: --fixture16 需要一个路径参数\n")
            return 2
        extra = args[:idx] + args[idx + 2:]
        if extra:
            sys.stderr.write(f"mkfat: --fixture16 单飞（多余参数 {extra}）\n")
            return 2
        vol, geo = volume_for(args)
        n = geo["data_start"] + (geo["next_clus"] - 2) * geo["spc"]
        with open(fx, "wb") as f:
            f.write(vol[: n * SECTOR])
        print(f"mkfat: {fx} {n * SECTOR}B (fat16 fixture) "
              f"clusters={geo['clusters']} fat={geo['fat_sz']}x{NUM_FATS} "
              f"spc={geo['spc']}")
        return 0
    if not args or args[0].startswith("-"):
        sys.stderr.write(__doc__)
        return 2
    out_path = args[0]
    # --put 模式（V-0a 刀 1）：EFI 引导盘——内容即 puts，独立构造体，与
    # 默认/fixture 模式互斥（混用 = 语义不明，loud）。取参约定与
    # --check-fixture 同形：`--put <local>:<fat_path>` 两 argv。
    # `--size-mib N`（V-0b 刀 1 裁决 C）：引导卷尺寸参数化——默认缺省 =
    # V-0a 1MiB 产物字节不变；4MiB 收 KERNEL.ELF（debug ELF ~2.9MiB）。
    put_specs = []
    size_mib = None
    fat16 = False
    rest = []
    i = 1
    while i < len(args):
        if args[i] == "--put":
            if i + 1 >= len(args):
                sys.stderr.write("mkfat: --put 需要一个 <local>:<fat_path> 参数\n")
                return 2
            put_specs.append(args[i + 1])
            i += 2
        elif args[i] == "--size-mib":
            if i + 1 >= len(args):
                sys.stderr.write("mkfat: --size-mib 需要一个整数参数\n")
                return 2
            try:
                size_mib = int(args[i + 1])
            except ValueError:
                sys.stderr.write(f"mkfat: --size-mib 非整数（got {args[i + 1]!r}）\n")
                return 2
            i += 2
        elif args[i] == "--fat16":
            # FAT16 旗（V-2）：显式声明卷宽——否决自动域分派（会把现役
            # FAT12 引导盘翻宽变字节，正面撞零漂移铁线）。
            fat16 = True
            i += 1
        else:
            rest.append(args[i])
            i += 1
    if put_specs:
        if "--fixture" in args:
            sys.stderr.write("mkfat: --put 与 --fixture 互斥\n")
            return 2
        pairs = []
        for spec in put_specs:
            local, sep, fat_path = spec.partition(":")
            if not sep or not local or not fat_path:
                sys.stderr.write(f"mkfat: --put 形如 <local>:<fat_path>（got {spec}）\n")
                return 2
            with open(local, "rb") as f:
                pairs.append((fat_path, f.read()))
        if size_mib is not None:
            vol, geo = build_volume_puts(pairs, size_mib * (1024 * 1024 // SECTOR),
                                         fat16=fat16)
        else:
            vol, geo = build_volume_puts(pairs, fat16=fat16)
        with open(out_path, "wb") as f:
            f.write(vol)
        names = " ".join(p for p, _ in pairs)
        size_note = f", {size_mib}MiB" if size_mib is not None else ""
        wide_note = ", fat16" if fat16 else ""
        print(f"mkfat: {out_path} {len(vol)}B (efi boot 盘{size_note}{wide_note}) "
              f"clusters_used={geo['next_clus'] - 2}/{geo['clusters']} "
              f"fat={geo['fat_sz']}x{NUM_FATS} puts=[{names}]")
        return 0
    if size_mib is not None:
        sys.stderr.write("mkfat: --size-mib 仅与 --put 连用（EFI 引导卷尺寸）\n")
        return 2
    if fat16:
        sys.stderr.write("mkfat: --fat16 仅与 --put 连用（FAT16 引导卷）\n")
        return 2
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
        # n 按几何摘要的 spc 折算（V-2：geo 补 spc 键——SPC≠4 面不再截错
        # 前缀；对现役 SPC=4 语义等价，cmp 对拍证明）。
        n = geo["data_start"] + (geo["next_clus"] - 2) * geo["spc"]
        with open(fx, "wb") as f:
            f.write(vol[:n * SECTOR])
        line += f" fixture={fx} {n * SECTOR}B"
    print(line)
    return 0


if __name__ == "__main__":
    sys.exit(main())
