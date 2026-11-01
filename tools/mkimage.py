#!/usr/bin/env python3
"""mkimage——确定性整盘镜像打包（V-3：MBR + 单 FAT16 分区，VM 草案 §V-3；
SEC-02 A1 产链缝三件——research/SEC-01 §4.5 件 3/4/5，2026-10-30）。

用法：mkimage.py <out.img> --efi <BERRYBOOT.EFI> --hello <hello.txt>
              --kernel <kernel.elf> --initrd <rootfs.cpio> [--size-mib N]
              [--key-hash <64hex>]
      mkimage.py --selftest

盘布局 = [LBA0 MBR][LBA2048.. FAT16 主分区（四件套）][尾部 16MiB 密钥
材料预留区（全零，第二分区项覆盖）]：
  - MBR：现代 LBA 假几何（255 头/63 扇每磁道——protective CHS 值自洽
    即可，草案 §V-3 原话），表项 0 = type 0x0E（FAT16 LBA），起始 LBA
    2048（1MiB 对齐惯例）；引导代码区全零（UEFI 只解析表 + 55AA 签章）。
  - FAT16 卷：编码单源 = mkfat.build_volume_puts（单编码器纪律——FAT
    写法只在 mkfat 一处，本工具零 FAT 逻辑）；hidden = 分区起始 LBA
    （BPB 与分区表自洽，V-3 起 build_volume_puts 参数化）。
  - 四件套：\\EFI\\BOOT\\BOOTAA64.EFI / \\HELLO.TXT / \\KERNEL.ELF /
    \\INITRD.BIN（8.3 全合法，LFN 零需求——V-2 §）。

SEC-02 A1 三件（day-one 留缝——纯宿主侧、零运行时影响；今天只冻结
格式与槽位，不接任何验签/度量消费端，v2 真机窗口才兑现语义）：
  - 件 3【SEC-1】产 <out.img> 同目录 manifest.txt（侧产物，不进盘）：
    每件 SHA-256 + 尺寸 + rollback_index 恒 0 = 未来 vbmeta 数据源，
    格式 v0 冻结于 2026-10-30（定宽列/路径字节序/无时刻输入——逐字节
    规约见 build_manifest 头注；改格式 = 破冻，须升 v1 并另立对拍基准）。
  - 件 4【SEC-7】MBR 表项 1 = 密钥材料分区预留槽（16MiB 全零、type
    0xDA Non-FS data——Android /metadata 16MiB 独立分区同型：密钥/
    防回卷计数等安全元数据住盘上独立分区，不与系统卷混放，v1 只占槽
    不写内容）。FAT16 主分区字节零漂移：第一分区起始/尺寸/卷面全
    不动，预留区纯尾部追加（总盘 = --size-mib MiB + 16MiB）。
  - 件 5【SEC-3/4】--key-hash <64hex> 参数位：默认缺席 = QEMU 路不烧
    （manifest 落 key_hash=none）；提供时只落 manifest 的 key_hash
    字段、盘字节不动（数据流结构保证——key_hash 只进 build_manifest，
    不进 build_image）。产链尾打 `img: sha256=<整盘 hex>`（产链日志
    对拍/审计锚——任何输入漂移在整盘哈希上一目了然）。

接受面（V-2 FatDxe 实验消雷 2026-10-25）：EDK2 PartitionDxe（MBR 表）
+ FatDxe（FAT16 卷）+ BdsDxe（可移介质默认路径 \\EFI\\BOOT\\BOOTAA64.EFI）
——BerryBoot SFS 面零改动。第二分区全零无 FAT 签名 → FatDxe 不绑定 →
无文件系统可挂（预留语义自洽；Bds 对它无默认路径可走）。盘容默认
96MiB + 16MiB 预留（A1-3b：rootfs 40MB+ 级验收形态同盘容；C-2c 史
48MiB = text 批后 ≈21.5MB 留 ~2x；V-3 史：默认 24MiB 修 C-2a 后
14.4MB 超 V-1c 8MiB FAT12 盘约定的存量断裂（build-boot 8MiB 造盘
已被本工具吸收，`just image` 单口）。

确定性：与 mkcpio/mkfat 同纪律——无时刻输入、同输入同字节（MBR 字段
全定宽定值；卷 VOLUME_ID/日期沿 mkfat 定值；manifest 同输入同字节）。
"""
import argparse
import hashlib
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mkfat  # noqa: E402（同目录导入——FAT 编码单源，见模块头注）

SECTOR = 512
PART_LBA = 2048        # 分区起始 LBA（1MiB 对齐——现代盘惯例，EDK2/UTM 皆认）
HEADS, SPT = 255, 63   # LBA 假几何（protective CHS 自洽用——非寻址依据）
PART_TYPE_FAT16_LBA = 0x0E
DEFAULT_SIZE_MIB = 96  # FAT 段默认容量（A1-3b：rootfs 40MB+ 验收形态同盘容——
                       # 恰传 justfile --size-mib 96；C-2c 史 48MiB = text 批后 ≈2x）

# 件 4【SEC-7】：密钥材料分区预留槽——16MiB（Android /metadata 同型量级）。
# type 0xDA = MBR「Non-FS data」：向消费方声明「非文件系统、勿自动挂载/
# 勿格式化探测」——预留语义比 0x83（Linux fs，暗示有文件系统）更自洽；
# 分区内容 v1 恒全零（今天只占槽，真机窗口才落密钥/防回卷计数真身）。
META_SECTORS = 16 * 1024 * 1024 // SECTOR  # 32768 扇区 = 16MiB
PART_TYPE_KEYMETA = 0xDA

# 件 3【SEC-1】：manifest v0 格式冻结常量（列宽是契约的一部分——改即破冻）。
MANIFEST_NAME = "manifest.txt"
MANIFEST_FROZEN = "2026-10-30"
MANIFEST_PATH_W = 24   # path 列宽（最长 /EFI/BOOT/BOOTAA64.EFI = 22 + 余量）
MANIFEST_SIZE_W = 12   # size 零垫宽（4TB 域——盘件量级恒有余）
MANIFEST_RB_W = 20     # rollback_index 零垫宽（u64 全域——AVB 同宽，真值
                       # 启用不破列宽）


def volume_sectors(size_mib: int) -> int:
    """整盘 MiB → 卷扇区数（= FAT 段扇区 − 分区起始 LBA）。造盘与自检共用
    同一推导——扩容只动 DEFAULT_SIZE_MIB / --size-mib，期望值随动不滞留。"""
    return size_mib * 1024 * 1024 // SECTOR - PART_LBA


def lba_to_chs(lba: int) -> bytes:
    """LBA → 假几何 CHS 三字节（MBR 表项 5/6/9 位布局：H | S|C 高 2 位 |
    C 低 8 位）。超 1023 柱面钳顶（protective 语义——LBA 盘上 CHS 仅装饰，
    草案：自洽即可）。"""
    cyl = lba // (HEADS * SPT)
    r = lba % (HEADS * SPT)
    h, s = r // SPT, r % SPT + 1
    if cyl > 1023:
        cyl, h, s = 1023, HEADS - 1, SPT
    return bytes([h, s | ((cyl >> 8) & 0x03), cyl & 0xFF])


def build_mbr(part_lba: int, part_sectors: int) -> bytes:
    """MBR：引导代码区全零 + 表项 0（FAT16 主分区）+ 表项 1（密钥材料预留
    槽，件 4）+ 55AA。预留槽起始 = 主分区尾（part_lba + part_sectors——
    单一事实源由调用几何推导，不另传参防错位）；表项 2-3 全零。"""
    mbr = bytearray(SECTOR)
    e = bytearray(16)
    e[0] = 0x00                    # 非 active（UEFI 引导不看活动位）
    e[1:4] = lba_to_chs(part_lba)
    e[4] = PART_TYPE_FAT16_LBA
    e[5:8] = lba_to_chs(part_lba + part_sectors - 1)
    e[8:12] = part_lba.to_bytes(4, "little")
    e[12:16] = part_sectors.to_bytes(4, "little")
    mbr[446:462] = e
    meta_lba = part_lba + part_sectors
    m = bytearray(16)
    m[0] = 0x00                    # 同非 active
    m[1:4] = lba_to_chs(meta_lba)
    m[4] = PART_TYPE_KEYMETA       # Non-FS data——预留不挂载语义（见常量注）
    m[5:8] = lba_to_chs(meta_lba + META_SECTORS - 1)
    m[8:12] = meta_lba.to_bytes(4, "little")
    m[12:16] = META_SECTORS.to_bytes(4, "little")
    mbr[462:478] = m
    mbr[510:512] = b"\x55\xAA"
    return bytes(mbr)


def sha256_hex(data: bytes) -> str:
    """SHA-256 小写 hex（64 位定宽）——manifest 件哈希（件 3）与整盘哈希
    （件 5 产链尾打印）共源。"""
    return hashlib.sha256(data).hexdigest()


def parse_key_hash(s: str) -> str:
    """件 5：--key-hash 校验——恰 64 位 hex（SHA-256 锚），归一小写。坏值
    loud 拒：真机产链烧错长度的公钥锚 = 未来验签面全线静默错，宁在注入
    口炸。"""
    v = s.strip().lower()
    if len(v) != 64 or any(c not in "0123456789abcdef" for c in v):
        raise SystemExit(f"mkimage: FAIL: --key-hash 须 64 位 hex（SHA-256），got {s!r}")
    return v


def build_manifest(puts: list, key_hash=None) -> bytes:
    """件 3：manifest 字节（v0 格式冻结于 2026-10-30——未来 vbmeta 数据源；
    改任何列宽/行序/字段 = 破冻，须升 v1 并另立黄金字节对拍基准）。

    格式规约（逐字节冻结——列宽/行序/填充全是契约）：
      行 1   头：`berryos-manifest v0 format_frozen=2026-10-30 entries=NN`
             （NN = 条目数两位零垫——头部计数即廉价完整性哨兵）。
      条目行：`item path=<24 左对齐右补空> sha256=<64 小写 hex>
             size=<12 零垫十进制> rollback_index=<20 零垫十进制，v1 恒 0>`
             —— rollback_index 列宽取 u64 全域（AVB 同宽）：冻结后真值
             启用不破格式。防回卷语义（件 14/#56 三值版本域）v2 才接。
      尾行   ：`key_hash=none`（默认——QEMU 路不烧）或 `key_hash=<64hex>`
             （件 5 --key-hash 注入；恒在行——显式缺席优于隐式缺席）。
    行序 = FAT 路径字节序（与传参次序无关——数据自定序，「同输入同字节」
    的必要条件）；ASCII、LF 行尾、末行有换行；无时刻输入。

    路径超 MANIFEST_PATH_W 即 loud 拒（对抗审查实抓：f-string 定宽对超长
    输入只是不加垫、静默加宽列仍产「合法」manifest——冻结格式的执法面不能
    只剩固定输入的黄金字节；今日四件套最长 /EFI/BOOT/BOOTAA64.EFI = 22，
    守卫面当前不可达但换名即达）。"""
    lines = [f"berryos-manifest v0 format_frozen={MANIFEST_FROZEN} "
             f"entries={len(puts):02d}"]
    for path, data in sorted(puts, key=lambda p: p[0]):
        if len(path) > MANIFEST_PATH_W:
            raise SystemExit(f"mkimage: FAIL: manifest path 超冻结列宽 "
                             f"{MANIFEST_PATH_W}（{path!r} = {len(path)} 字符）"
                             f"——破 v0 格式冻结；缩短路径或升 manifest v1")
        lines.append(f"item path={path:<{MANIFEST_PATH_W}} "
                     f"sha256={sha256_hex(data)} "
                     f"size={len(data):0{MANIFEST_SIZE_W}d} "
                     f"rollback_index={0:0{MANIFEST_RB_W}d}")
    lines.append(f"key_hash={key_hash if key_hash else 'none'}")
    return ("\n".join(lines) + "\n").encode("ascii")


def build_image(puts: list, size_mib: int) -> tuple:
    """整盘装配（MBR + FAT16 主分区 + 尾部密钥材料全零区）。返回
    (盘字节, 几何摘要)。件 4 起总盘 = size_mib MiB + 16MiB——FAT 段
    （第一分区起始/尺寸/卷面）与 V-3/A1-3b 形制零漂移，预留区纯尾部
    追加（bytearray 出生全零即预留区内容，不另写）。key_hash 有意
    不入参（件 5「不动盘字节」的数据流保证）。"""
    vol_sectors = volume_sectors(size_mib)
    if vol_sectors <= 64:
        raise SystemExit(f"mkimage: FAIL: {size_mib}MiB 装不下分区起始 {PART_LBA} + 卷")
    vol, geo = mkfat.build_volume_puts(puts, vol_sectors=vol_sectors,
                                       fat16=True, hidden=PART_LBA)
    total = PART_LBA + vol_sectors + META_SECTORS
    img = bytearray(total * SECTOR)
    img[0:SECTOR] = build_mbr(PART_LBA, vol_sectors)
    img[PART_LBA * SECTOR:PART_LBA * SECTOR + len(vol)] = vol
    return bytes(img), geo


def selftest() -> int:
    """MBR 薄面自检（不写盘）：CHS 编码向量 + 两分区表项钉板（黄金字节）+
    manifest v0 黄金字节 + 整盘装配确定性。卷编码面归 mkfat --selftest
    （单编码器纪律——不在此长第二套）。"""
    assert lba_to_chs(0) == bytes([0, 1, 0]), "LBA0 → CHS(0,0,1)"
    assert lba_to_chs(2048) == bytes([32, 33, 0]), "LBA2048 → CHS(0,32,33)"
    assert lba_to_chs(49151) == bytes([15, 12, 3]), "LBA49151 → CHS(3,15,12)"
    assert lba_to_chs(1024 * HEADS * SPT) == bytes([254, 63, 255]), "超顶钳 (1023,254,63)"
    # 表项 0 布局钉板：卷扇区随 DEFAULT_SIZE_MIB 推导（A1-3b 96MiB → 194560）——
    # V-3 史：硬编 47104（24MiB 派生值）在 C-2c 扩容时滞留一轮，参数化断根。
    vol = volume_sectors(DEFAULT_SIZE_MIB)
    m = build_mbr(PART_LBA, vol)
    assert m[446] == 0x00, "status 非 active"
    assert m[450] == PART_TYPE_FAT16_LBA, "分区类型 0x0E"
    assert int.from_bytes(m[454:458], "little") == PART_LBA, "起始 LBA 域"
    assert int.from_bytes(m[458:462], "little") == vol, "扇区数域"
    assert m[510:512] == b"\x55\xAA", "签章"
    assert all(b == 0 for b in m[:446]), "引导代码区全零"
    # 件 4：表项 1（密钥材料预留槽）钉板——字段面 + 96MiB 派生黄金字节
    # （meta@LBA196608 + 32768s；CHS (12,60,49)→(14,70,56) 沿假几何自洽）。
    meta_lba = PART_LBA + vol
    assert m[462] == 0x00, "meta status 非 active"
    assert m[466] == PART_TYPE_KEYMETA, "meta 分区类型 0xDA"
    assert int.from_bytes(m[470:474], "little") == meta_lba, "meta 起 LBA 域"
    assert int.from_bytes(m[474:478], "little") == META_SECTORS, "meta 扇区数 = 16MiB"
    assert m[463:466] == lba_to_chs(meta_lba), "meta 起 CHS 与 LBA 自洽"
    assert m[467:470] == lba_to_chs(meta_lba + META_SECTORS - 1), "meta 止 CHS"
    assert m[462:478] == bytes.fromhex("003c310cda46380e0000030000800000"), \
        "表项 1 黄金字节（96MiB 派生定值）"
    assert m[478:510] == b"\x00" * 32, "表项 2-3 全零"

    # 件 3：manifest v0 黄金字节（格式冻结的执法点——改列宽/行序/填充/
    # 哈希算法 = 此处 FAIL）。内容为确定性生成式（mkfat selftest 同族）。
    puts = [("/EFI/BOOT/BOOTAA64.EFI", bytes((i * 7 + 3) % 256 for i in range(3000))),
            ("/HELLO.TXT", b"berryboot efi hello\n"),
            ("/KERNEL.ELF", bytes((i * 31 + 7) % 256 for i in range(5000))),
            ("/INITRD.BIN", b"berryos initrd stub\n")]
    golden = (b"berryos-manifest v0 format_frozen=2026-10-30 entries=04\n"
              b"item path=/EFI/BOOT/BOOTAA64.EFI   "
              b"sha256=f541874101876255b4baf3a739778d04cb9cba25ffa38b30bc1fb8b0701f2a45 "
              b"size=000000003000 rollback_index=00000000000000000000\n"
              b"item path=/HELLO.TXT               "
              b"sha256=785594f64f783cfaac356d9f75ba42d369e414e316ad1e8649b41090e6787526 "
              b"size=000000000020 rollback_index=00000000000000000000\n"
              b"item path=/INITRD.BIN              "
              b"sha256=6c150fd292a1c404285bddf840b1a77cd9d479555b4e50c3dd98030779de29ba "
              b"size=000000000020 rollback_index=00000000000000000000\n"
              b"item path=/KERNEL.ELF              "
              b"sha256=1e92fd98f113aba0a78e0830ca06e2775912370feab112dfc57bf3258b810595 "
              b"size=000000005000 rollback_index=00000000000000000000\n"
              b"key_hash=none\n")
    got = build_manifest(puts)
    assert got == golden, "manifest v0 黄金字节漂移（破冻——格式改动须升 v1）"
    # 行序冻结面：传参次序无关（数据自定序 = 路径字节序）。
    assert build_manifest(list(reversed(puts))) == golden, "manifest 行序随传参漂移"
    # path 列宽 loud 守卫（对抗审查补）：超宽路径曾静默加宽列仍报 ok——恰 24
    # 字符边界正例（无垫片、单空格分隔）不误拒，25+ 一律 SystemExit。
    p24 = "/EFI/BOOT/BOOTAA64.EFI" + "XX"  # 22 + 2 = 恰列宽上界
    assert len(p24) == MANIFEST_PATH_W
    m24 = build_manifest([(p24, b"x")]).decode()
    assert f"path={p24} sha256=" in m24, "24 字符边界路径应原样成列（单空格分隔）"
    for bad in ("/" + "a" * MANIFEST_PATH_W, "/EFI/BOOT/BERRYBOOTAA64.EFI"):
        try:
            build_manifest([(bad, b"x")])
        except SystemExit:
            continue
        assert False, f"超宽 manifest path 未被拒绝：{bad!r}"
    # 件 5：key_hash 注入面——差集恰尾行一行（其余字段零扰动）+ 坏值 loud 拒。
    kh = parse_key_hash("0123456789ABCDEF" * 4)
    assert build_manifest(puts, key_hash=kh) == \
        golden.replace(b"key_hash=none", b"key_hash=" + kh.encode()), \
        "key_hash 注入面差集非恰一行"
    for bad in ("0x" + "ab" * 32, "ab" * 31, "ab" * 33, "zz" * 32):
        try:
            parse_key_hash(bad)
        except SystemExit:
            continue
        assert False, f"坏 key-hash 未被拒绝：{bad!r}"

    # 整盘装配确定性（9MiB FAT 段 = FAT16 最小量级卷：16384 扇区 → 4087 簇
    # 恰入域，mkfat selftest 同几何）：双跑逐字节恒等 + FAT 段与 mkfat 直产
    # 零漂移（件 4「卷面全不动」构造性钉板）+ 预留区恒全零。
    img, _ = build_image(puts, 9)
    assert img == build_image(puts, 9)[0], "整盘装配不确定"
    vol_direct, _ = mkfat.build_volume_puts(puts, vol_sectors=volume_sectors(9),
                                            fat16=True, hidden=PART_LBA)
    meta_off = (PART_LBA + volume_sectors(9)) * SECTOR
    assert img[PART_LBA * SECTOR:meta_off] == bytes(vol_direct), "FAT 段漂移"
    assert img[meta_off:] == b"\x00" * (META_SECTORS * SECTOR), "预留区非全零"
    assert len(img) == 9 * 1024 * 1024 + META_SECTORS * SECTOR, "总盘 = 9MiB + 16MiB"
    print("mkimage: selftest ok")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="BerryOS 整盘镜像打包（MBR + 单 FAT16 分区 + 密钥材料预留槽）")
    ap.add_argument("out", nargs="?", help="输出镜像路径（如 out/berryos.img）")
    ap.add_argument("--efi", help="BERRYBOOT.EFI（PE32+，进 \\EFI\\BOOT\\BOOTAA64.EFI）")
    ap.add_argument("--hello", help="hello.txt（BerryBoot fs: 自验基准，进 \\HELLO.TXT）")
    ap.add_argument("--kernel", help="内核 debug ELF（进 \\KERNEL.ELF）")
    ap.add_argument("--initrd", help="rootfs.cpio（进 \\INITRD.BIN）")
    ap.add_argument("--size-mib", type=int, default=DEFAULT_SIZE_MIB,
                    help=f"FAT 段容量 MiB（默认 {DEFAULT_SIZE_MIB}；总盘另加 16MiB 密钥材料预留区）")
    ap.add_argument("--key-hash", help="OEM 公钥 SHA-256（64 位 hex——件 5：只落 manifest "
                                       "key_hash 字段不动盘字节；缺省 none = QEMU 路不烧）")
    ap.add_argument("--selftest", action="store_true", help="MBR/manifest 薄面自检（不写盘）")
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    if not (args.out and args.efi and args.hello and args.kernel and args.initrd):
        ap.error("造盘需 out + --efi/--hello/--kernel/--initrd 四件套（缺一即 loud）")

    key_hash = parse_key_hash(args.key_hash) if args.key_hash else None

    puts = []
    nbytes = 0
    for local, fat_path in ((args.efi, "/EFI/BOOT/BOOTAA64.EFI"),
                            (args.hello, "/HELLO.TXT"),
                            (args.kernel, "/KERNEL.ELF"),
                            (args.initrd, "/INITRD.BIN")):
        try:
            data = Path(local).read_bytes()
        except OSError as e:
            raise SystemExit(f"mkimage: FAIL: 读 {local}：{e}")
        puts.append((fat_path, data))
        nbytes += len(data)

    img, geo = build_image(puts, args.size_mib)
    Path(args.out).write_bytes(img)
    # 件 3：manifest = <out.img> 同目录侧产物（不进盘——vbmeta 签名产物
    # 未来另刀，manifest 只是其数据源）。
    manifest = build_manifest(puts, key_hash)
    manifest_path = Path(args.out).with_name(MANIFEST_NAME)
    manifest_path.write_bytes(manifest)
    vol_sectors = volume_sectors(args.size_mib)
    meta_lba = PART_LBA + vol_sectors
    print(f"mkimage: ok {args.out} {len(img)}B (mbr+fat16+keymeta, "
          f"{args.size_mib}+16MiB) part=lba{PART_LBA}+{vol_sectors}s "
          f"meta=lba{meta_lba}+{META_SECTORS}s clusters={geo['clusters']} "
          f"used={geo['next_clus'] - 2} fat={geo['fat_sz']}x{mkfat.NUM_FATS} "
          f"spc={geo['spc']} files=4 bytes={nbytes}")
    print(f"mkimage: manifest {manifest_path} {len(manifest)}B "
          f"entries={len(puts)} key_hash={key_hash if key_hash else 'none'}")
    # 件 5：产链尾整盘哈希行——供 CI/审计对拍（黄金格式 img: sha256=<64hex>）。
    print(f"img: sha256={sha256_hex(img)}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
