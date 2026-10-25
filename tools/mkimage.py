#!/usr/bin/env python3
"""mkimage——确定性整盘镜像打包（V-3：MBR + 单 FAT16 分区，VM 草案 §V-3）。

用法：mkimage.py <out.img> --efi <BERRYBOOT.EFI> --hello <hello.txt>
              --kernel <kernel.elf> --initrd <rootfs.cpio> [--size-mib N]
      mkimage.py --selftest

盘布局 = [LBA0 MBR][LBA2048.. FAT16 卷（四件套）]：
  - MBR：现代 LBA 假几何（255 头/63 扇每磁道——protective CHS 值自洽
    即可，草案 §V-3 原话），单分区 type 0x0E（FAT16 LBA），起始 LBA
    2048（1MiB 对齐惯例）；引导代码区全零（UEFI 只解析表 + 55AA 签章）。
  - FAT16 卷：编码单源 = mkfat.build_volume_puts（单编码器纪律——FAT
    写法只在 mkfat 一处，本工具零 FAT 逻辑）；hidden = 分区起始 LBA
    （BPB 与分区表自洽，V-3 起 build_volume_puts 参数化）。
  - 四件套：\\EFI\\BOOT\\BOOTAA64.EFI / \\HELLO.TXT / \\KERNEL.ELF /
    \\INITRD.BIN（8.3 全合法，LFN 零需求——V-2 §）。

接受面（V-2 FatDxe 实验消雷 2026-10-25）：EDK2 PartitionDxe（MBR 表）
+ FatDxe（FAT16 卷）+ BdsDxe（可移介质默认路径 \\EFI\\BOOT\\BOOTAA64.EFI）
——BerryBoot SFS 面零改动。盘容默认 24MiB：C-2a 后 rootfs.cpio 14.4MB
已超 V-1c 8MiB FAT12 盘约定（存量断裂，本刀修复——build-boot 8MiB 造盘
已被本工具吸收，`just image` 单口）。

确定性：与 mkcpio/mkfat 同纪律——无时刻输入、同输入同字节（MBR 字段
全定宽定值；卷 VOLUME_ID/日期沿 mkfat 定值）。
"""
import argparse
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import mkfat  # noqa: E402（同目录导入——FAT 编码单源，见模块头注）

SECTOR = 512
PART_LBA = 2048        # 分区起始 LBA（1MiB 对齐——现代盘惯例，EDK2/UTM 皆认）
HEADS, SPT = 255, 63   # LBA 假几何（protective CHS 自洽用——非寻址依据）
PART_TYPE_FAT16_LBA = 0x0E


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
    """MBR：引导代码区全零 + 表项 0（其余表项全零）+ 55AA。"""
    mbr = bytearray(SECTOR)
    e = bytearray(16)
    e[0] = 0x00                    # 非 active（UEFI 引导不看活动位）
    e[1:4] = lba_to_chs(part_lba)
    e[4] = PART_TYPE_FAT16_LBA
    e[5:8] = lba_to_chs(part_lba + part_sectors - 1)
    e[8:12] = part_lba.to_bytes(4, "little")
    e[12:16] = part_sectors.to_bytes(4, "little")
    mbr[446:462] = e
    mbr[510:512] = b"\x55\xAA"
    return bytes(mbr)


def selftest() -> int:
    """MBR 薄面自检（不写盘）：CHS 编码向量 + 表项布局钉板 + 签章/零区。
    卷编码面归 mkfat --selftest（单编码器纪律——不在此长第二套）。"""
    assert lba_to_chs(0) == bytes([0, 1, 0]), "LBA0 → CHS(0,0,1)"
    assert lba_to_chs(2048) == bytes([32, 33, 0]), "LBA2048 → CHS(0,32,33)"
    assert lba_to_chs(49151) == bytes([15, 12, 3]), "LBA49151 → CHS(3,15,12)"
    assert lba_to_chs(1024 * HEADS * SPT) == bytes([254, 63, 255]), "超顶钳 (1023,254,63)"
    m = build_mbr(PART_LBA, 47104)
    assert m[446] == 0x00, "status 非 active"
    assert m[450] == PART_TYPE_FAT16_LBA, "分区类型 0x0E"
    assert int.from_bytes(m[454:458], "little") == PART_LBA, "起始 LBA 域"
    assert int.from_bytes(m[458:462], "little") == 47104, "扇区数域"
    assert m[510:512] == b"\x55\xAA", "签章"
    assert all(b == 0 for b in m[:446]), "引导代码区全零"
    assert all(b == 0 for b in m[462:510]), "表项 1-3 全零"
    print("mkimage: selftest ok")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description="BerryOS 整盘镜像打包（MBR + 单 FAT16 分区）")
    ap.add_argument("out", nargs="?", help="输出镜像路径（如 out/berryos.img）")
    ap.add_argument("--efi", help="BERRYBOOT.EFI（PE32+，进 \\EFI\\BOOT\\BOOTAA64.EFI）")
    ap.add_argument("--hello", help="hello.txt（BerryBoot fs: 自验基准，进 \\HELLO.TXT）")
    ap.add_argument("--kernel", help="内核 debug ELF（进 \\KERNEL.ELF）")
    ap.add_argument("--initrd", help="rootfs.cpio（进 \\INITRD.BIN）")
    ap.add_argument("--size-mib", type=int, default=24, help="整盘容量 MiB（默认 24）")
    ap.add_argument("--selftest", action="store_true", help="MBR 薄面自检（不写盘）")
    args = ap.parse_args()

    if args.selftest:
        return selftest()
    if not (args.out and args.efi and args.hello and args.kernel and args.initrd):
        ap.error("造盘需 out + --efi/--hello/--kernel/--initrd 四件套（缺一即 loud）")

    total = args.size_mib * 1024 * 1024 // SECTOR
    if total <= PART_LBA + 64:
        raise SystemExit(f"mkimage: FAIL: {args.size_mib}MiB 装不下分区起始 {PART_LBA} + 卷")
    vol_sectors = total - PART_LBA

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

    vol, geo = mkfat.build_volume_puts(puts, vol_sectors=vol_sectors,
                                       fat16=True, hidden=PART_LBA)
    img = bytearray(total * SECTOR)
    img[0:SECTOR] = build_mbr(PART_LBA, vol_sectors)
    img[PART_LBA * SECTOR:PART_LBA * SECTOR + len(vol)] = vol
    Path(args.out).write_bytes(img)
    print(f"mkimage: ok {args.out} {len(img)}B (mbr+fat16, {args.size_mib}MiB) "
          f"part=lba{PART_LBA}+{vol_sectors}s clusters={geo['clusters']} "
          f"used={geo['next_clus'] - 2} fat={geo['fat_sz']}x{mkfat.NUM_FATS} "
          f"spc={geo['spc']} files=4 bytes={nbytes}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
