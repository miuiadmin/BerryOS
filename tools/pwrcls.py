#!/usr/bin/env python3
"""pwrcls.py —— B4 掉电安全首刀的盘面撕裂态分类/注入工具（诊断级）。

对象 = `tools/mkfat.py` 默认产的 smoke 盘（2048 扇区 1MiB：[16 扇区保留区]
[FAT12 卷 @ LBA16]）。两种模式：

  --classify <img>   读盘分类 RW.TXT 的簇链状态（零写入）：
                     `PWR cls=<state> first=<n> size=<n> chain=<k>`
                     state ∈ clean / poison / midchain / malformed / loop /
                     noent / empty。smoke 相 6 的 A/B/C 三 boot 采样与
                     恢复不变量断言（Boot C 后必 clean）都吃这行。
  --poison <img>     确定性注入毒环态：RW.TXT 整链在 **双 FAT 副本** 置
                     Free、根目录原样不动——等价于覆写 commit 撕裂窗
                     「FAT 已写、根未写」的盘面形状（fsmgr 恢复 commit 的
                     ④ 守卫正是为此窗而设，见 user/bins/fsmgr/src/fat.rs）。
                     注入后自检 FAT0/FAT1[first] 均 Free 才退出 0。

几何从 BPB 动态解析（bps/spc/reserved/nfats/root_entries/fat_sz/hidden），
卷基 LBA=16 与 hidden 互检不符即 loud 拒——本工具只服务 smoke 平盘，
不猜 MBR/分区盘。12 位紧包解码/编码与 mkfat.py 同式（跨实现写读共识面）。

B4 设计依据：docs/FAT写路径原子性评估-2026-10.md（13 写序列与撕裂前缀表）。

自检：pwrcls.py --selftest——合成 smoke 平盘（mkfat 裸调同形：单编码器
复用 import，非第二套几何）+ RW.TXT 链注入，classify 七态钉板
（noent/empty/malformed×2/clean/midchain/poison/loop——FAT12 项 6/7/8
奇偶双侧跨）+ poison_rw 全链注入（双 FAT 置 Free/根目录原样/freed 对账）
+ 五拒面（非平盘 hidden/坏几何 spc/55AA 缺席/注入遇 noent/empty）。
挂 just test-host 前置（elf2flat --selftest 同位纪律：smoke 掉电相三
boot 的采样断言与毒环注入都吃本件，门禁自身坏 = 掉电相静默劣化）。
"""

import argparse
import sys

SECTOR = 512
VOL_LBA = 16          # mkfat RESERVED_ZONE（保留区 = vblk 测试面，与卷零交集）
RW_NAME = b"RW      TXT"   # FAT11 短名（FATW 自验文件——smoke 每轮覆写 3000B）

EOC_MIN12, EOC_MAX12 = 0xFF8, 0xFFF
BAD12 = 0xFF7


def rd_u16(b, o):
    return b[o] | (b[o + 1] << 8)


def rd_u32(b, o):
    return b[o] | (b[o + 1] << 8) | (b[o + 2] << 16) | (b[o + 3] << 24)


def fat12_get(fat, clus):
    """读 12 位表项——mkfat fat12_entry 同式（返回 int；切片界内恒可达）。"""
    off = (clus * 3) // 2
    if clus % 2 == 0:
        return fat[off] | ((fat[off + 1] & 0x0F) << 8)
    return (fat[off] >> 4) | (fat[off + 1] << 4)


def fat12_put(fat, clus, v):
    """写 12 位表项——mkfat fat12_set 同式（半 nibble 邻位保序）。"""
    off = (clus * 3) // 2
    if clus % 2 == 0:
        fat[off] = v & 0xFF
        fat[off + 1] = (fat[off + 1] & 0xF0) | ((v >> 8) & 0x0F)
    else:
        fat[off] = (fat[off] & 0x0F) | ((v & 0x0F) << 4)
        fat[off + 1] = (v >> 4) & 0xFF


class Geom:
    """盘几何（BPB 派生 + 平盘互检）。"""

    def __init__(self, disk):
        if len(disk) < (VOL_LBA + 1) * SECTOR:
            sys.exit("pwrcls: 盘太小（< 卷首扇区）")
        bpb = disk[VOL_LBA * SECTOR:(VOL_LBA + 1) * SECTOR]
        self.bps = rd_u16(bpb, 11)
        self.spc = bpb[13]
        self.reserved = rd_u16(bpb, 14)
        self.nfats = bpb[16]
        self.root_entries = rd_u16(bpb, 17)
        total16 = rd_u16(bpb, 19)
        self.fat_sz = rd_u16(bpb, 22)
        hidden = rd_u32(bpb, 28)
        total32 = rd_u32(bpb, 32)
        # loud 拒面：本工具只认 smoke 平盘几何，不符不猜。
        if hidden != VOL_LBA:
            sys.exit(f"pwrcls: hidden={hidden} != 卷基 {VOL_LBA}（非 smoke 平盘？拒）")
        if self.bps != SECTOR or self.spc != 4 or self.reserved != 1:
            sys.exit(f"pwrcls: 非预期几何 bps={self.bps} spc={self.spc} reserved={self.reserved}")
        if self.nfats != 2 or self.fat_sz != 2:
            sys.exit(f"pwrcls: 非预期 nfats={self.nfats} fat_sz={self.fat_sz}")
        if bpb[510] != 0x55 or bpb[511] != 0xAA:
            sys.exit("pwrcls: BPB 签名 55AA 缺席")
        total = total16 if total16 else total32
        root_secs = (self.root_entries * 32 + SECTOR - 1) // SECTOR
        data_lba = VOL_LBA + self.reserved + self.nfats * self.fat_sz + root_secs
        # total16/total32 = 卷相对扇区数（mkfat 写 VOL_SECTORS；与 berry-fatfs
        # lib.rs `total - data_sector` 同口径）——data_lba-VOL_LBA 已折卷相对，
        # 再减 VOL_LBA = 多减 16 扇区 = 4 簇（502≠506，域检查上界潜伏错）。
        self.cluster_count = (total - (data_lba - VOL_LBA)) // self.spc
        if not 2 <= self.cluster_count <= 4084:
            sys.exit(f"pwrcls: 簇数 {self.cluster_count} 出 FAT12 域（非预期盘）")
        # 盘绝对 LBA（消费端按 hidden 折算口径——不硬编码）。
        self.fat0_lba = VOL_LBA + self.reserved
        self.fat1_lba = self.fat0_lba + self.fat_sz
        self.root_lba = self.fat0_lba + self.nfats * self.fat_sz
        self.data_lba = data_lba
        self.root_secs = root_secs

    def clus_lba(self, c):
        return self.data_lba + (c - 2) * self.spc


def find_rw(disk, g):
    """根目录寻 RW.TXT 项——返回 (first_clus, size) 或 None（noent）。"""
    for i in range(g.root_entries):
        off = g.root_lba * SECTOR + i * 32
        e = disk[off:off + 32]
        if e[0] in (0x00, 0xE5):
            if e[0] == 0x00:
                break
            continue
        if e[:11] == RW_NAME:
            return rd_u16(e, 26), rd_u32(e, 28)
    return None


def classify(disk, g):
    """分类 RW.TXT 簇链状态——返回 (state, first, size)。"""
    ent = find_rw(disk, g)
    if ent is None:
        return "noent", 0, 0
    first, size = ent
    if first == 0:
        return ("empty" if size == 0 else "malformed"), first, size
    fat0 = disk[g.fat0_lba * SECTOR:(g.fat0_lba + g.fat_sz) * SECTOR]
    c, chain = first, 0
    while True:
        v = fat12_get(fat0, c)
        if v == 0:
            return ("poison" if c == first else "midchain"), first, size
        if v == 1 or v == BAD12:
            return "malformed", first, size
        if EOC_MIN12 <= v <= EOC_MAX12:
            return "clean", first, size
        chain += 1
        if chain > g.cluster_count or not 2 <= v < 2 + g.cluster_count:
            return "loop", first, size
        c = v


def poison_rw(disk: bytearray, g: Geom) -> tuple[int, int]:
    """纯注入核：RW.TXT 整链双 FAT 置 Free、根目录原样。返回 (first, freed)。

    拒面一律 SystemExit（与 CLI 语义同源）；main 与 --selftest 共用本函数
    （elf2flat build_flat 同款纪律——注入逻辑单源，自检即测真路径）。
    """
    ent = find_rw(disk, g)
    if ent is None:
        sys.exit("pwrcls: RW.TXT 不在根目录——先跑一轮 FATW 再注入")
    first, _ = ent
    if first == 0:
        sys.exit("pwrcls: RW.TXT 空文件无链可毒——先跑一轮 FATW")
    # 先走链收集（读 FAT0；bytearray 切片是拷贝——写回须显式）。
    fat0 = bytes(disk[g.fat0_lba * SECTOR:(g.fat0_lba + g.fat_sz) * SECTOR])
    chain, c = [first], first
    while True:
        v = fat12_get(fat0, c)
        if v == 0 or v == 1 or v == BAD12 or EOC_MIN12 <= v <= EOC_MAX12:
            break
        if len(chain) > g.cluster_count or not 2 <= v < 2 + g.cluster_count:
            sys.exit("pwrcls: 链走查出域——盘面畸形，拒注入")
        chain.append(v)
        c = v
    for base in (g.fat0_lba, g.fat1_lba):
        fat = bytearray(disk[base * SECTOR:(base + g.fat_sz) * SECTOR])
        for c in chain:
            fat12_put(fat, c, 0)
        disk[base * SECTOR:(base + g.fat_sz) * SECTOR] = fat
    # 自检：双副本链头均 Free、根未动（重读分类 = poison）。
    for base in (g.fat0_lba, g.fat1_lba):
        fat = disk[base * SECTOR:(base + g.fat_sz) * SECTOR]
        if fat12_get(fat, first) != 0:
            sys.exit("pwrcls: 注入自检失败（链头未 Free）")
    state, first2, size2 = classify(disk, g)
    if state != "poison":
        sys.exit(f"pwrcls: 注入后分类={state}（应 poison）")
    _ = first2, size2
    return first, len(chain)


def selftest() -> int:
    # 合成 smoke 平盘：mkfat 裸调同形（import 单编码器，非第二套几何）——
    # 保留区 16 扇区 + 默认 ROOT_LISTING 卷（HELLO@2 / PATTERN@3..5，next_clus=6）。
    import mkfat

    vol, geo = mkfat.build_volume()
    disk = bytearray(mkfat.DISK_SECTORS * SECTOR)
    disk[mkfat.RESERVED_ZONE * SECTOR:] = vol
    g = Geom(bytes(disk))
    assert g.cluster_count == geo["clusters"] == 506, "几何互检漂移"
    assert (g.fat0_lba, g.fat1_lba, g.root_lba, g.data_lba) == (17, 19, 21, 22), \
        "LBA 派生钉板"

    # —— RW.TXT 落盘：2 簇链 @6..7（奇偶双簇跨 12 位紧包双侧），EOC@7。
    assert classify(disk, g)[0] == "noent"
    root = disk[g.root_lba * SECTOR:(g.root_lba + g.root_secs) * SECTOR]
    ent_off = g.root_lba * SECTOR + 3 * 32          # 卷标/HELLO/PATTERN 后首空闲槽
    assert disk[ent_off:ent_off + 11] == b"\x00" * 11, "根目录槽 3 应空闲"
    _ = root
    disk[ent_off:ent_off + 11] = RW_NAME
    disk[ent_off + 26:ent_off + 28] = (6).to_bytes(2, "little")
    disk[ent_off + 28:ent_off + 32] = (5000).to_bytes(4, "little")
    for base in (g.fat0_lba, g.fat1_lba):
        fat = bytearray(disk[base * SECTOR:(base + g.fat_sz) * SECTOR])
        fat12_put(fat, 6, 7)
        fat12_put(fat, 7, 0xFFF)
        disk[base * SECTOR:(base + g.fat_sz) * SECTOR] = fat
    state, first, size = classify(disk, g)
    assert (state, first, size) == ("clean", 6, 5000), (state, first, size)

    # —— 七态钉板：noent/empty/malformed×2/clean 已证，注毒后 poison/再注
    #    midchain/loop 各证一。
    first_p, freed = poison_rw(disk, g)
    assert (first_p, freed) == (6, 2), (first_p, freed)
    assert classify(disk, g)[0] == "poison"

    # 恢复链后证 midchain（链中 Free）与 loop（指回自身）。
    for base in (g.fat0_lba, g.fat1_lba):
        fat = bytearray(disk[base * SECTOR:(base + g.fat_sz) * SECTOR])
        fat12_put(fat, 6, 7)
        fat12_put(fat, 7, 0xFFF)
        disk[base * SECTOR:(base + g.fat_sz) * SECTOR] = fat
    fat = bytearray(disk[g.fat0_lba * SECTOR:(g.fat0_lba + g.fat_sz) * SECTOR])
    fat12_put(fat, 7, 0)                             # 尾簇 Free → midchain
    disk[g.fat0_lba * SECTOR:(g.fat0_lba + g.fat_sz) * SECTOR] = fat
    assert classify(disk, g)[0] == "midchain"
    fat = bytearray(disk[g.fat0_lba * SECTOR:(g.fat0_lba + g.fat_sz) * SECTOR])
    fat12_put(fat, 6, 6)                             # 自指 → loop
    disk[g.fat0_lba * SECTOR:(g.fat0_lba + g.fat_sz) * SECTOR] = fat
    assert classify(disk, g)[0] == "loop"
    # empty / malformed(v==1)：首簇项坏值面。
    for v, want in ((0, "empty"), (1, "malformed")):
        d2 = bytearray(disk)
        e2 = Geom(bytes(d2))
        ent = find_rw(d2, e2)
        f2, _ = ent
        fat2 = bytearray(d2[e2.fat0_lba * SECTOR:(e2.fat0_lba + e2.fat_sz) * SECTOR])
        fat12_put(fat2, f2, v)
        d2[e2.fat0_lba * SECTOR:(e2.fat0_lba + e2.fat_sz) * SECTOR] = fat2
        if v == 0:                                   # empty 需 size=0 配 first=0
            d2[ent_off + 26:ent_off + 28] = (0).to_bytes(2, "little")
            d2[ent_off + 28:ent_off + 32] = (0).to_bytes(4, "little")
        assert classify(d2, e2)[0] == want, (v, classify(d2, e2)[0])

    # —— 五拒面：几何族 ×3 + 注入遇 noent/empty。
    def expect_reject(fn, why):
        try:
            fn()
        except SystemExit:
            return
        print(f"pwrcls: selftest FAIL: {why} 未被拒绝")
        raise SystemExit(1)

    disk2 = bytearray(disk)
    bpb = VOL_LBA * SECTOR
    disk2[bpb + 28:bpb + 32] = (4).to_bytes(4, "little")      # hidden≠16
    expect_reject(lambda: Geom(bytes(disk2)), "非平盘 hidden")
    disk2 = bytearray(disk); disk2[bpb + 13] = 8              # spc≠4
    expect_reject(lambda: Geom(bytes(disk2)), "坏几何 spc")
    disk2 = bytearray(disk); disk2[bpb + 510] = 0             # 55AA 缺席
    expect_reject(lambda: Geom(bytes(disk2)), "BPB 签名缺席")

    noent_disk = bytearray(disk)
    noent_disk[ent_off] = 0xE5                                # 抹 RW.TXT 项
    expect_reject(lambda: poison_rw(noent_disk, Geom(bytes(noent_disk))), "注入遇 noent")
    empty_disk = bytearray(noent_disk)
    empty_disk[ent_off:ent_off + 11] = RW_NAME              # 复活完整 11B 名字
    empty_disk[ent_off + 26:ent_off + 28] = (0).to_bytes(2, "little")  # first=0 → empty
    expect_reject(lambda: poison_rw(empty_disk, Geom(bytes(empty_disk))), "注入遇 empty")
    print("pwrcls: selftest ok (七态钉板 + 双 FAT 注入对账 + 5 拒面)")
    return 0


def main() -> int:
    if sys.argv[1:2] == ["--selftest"]:
        return selftest()
    ap = argparse.ArgumentParser(description="RW.TXT 撕裂态分类/毒环注入（smoke 平盘）")
    ap.add_argument("img")
    mx = ap.add_mutually_exclusive_group(required=True)
    mx.add_argument("--classify", action="store_true", help="读盘分类（零写入）")
    mx.add_argument("--poison", action="store_true", help="双 FAT 注入毒环态")
    args = ap.parse_args()

    with open(args.img, "rb+") as f:
        disk = bytearray(f.read())
        g = Geom(disk)
        if args.classify:
            state, first, size = classify(disk, g)
            print(f"PWR cls={state} first={first} size={size}")
            return 0
        # --poison：根指旧链、双 FAT 全链 Free（「FAT 已写、根未写」窗形状）。
        first, freed = poison_rw(disk, g)
        _, _, size2 = classify(disk, g)
        f.seek(0)
        f.write(disk)
        f.truncate()
    print(f"PWR poison ok first={first} freed={freed} size={size2}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
