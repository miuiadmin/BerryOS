#!/usr/bin/env python3
"""fatcat —— FAT12 数据盘根文件提取（cbox 刀 2 盘面验收临时工具）。

用法：fatcat.py <镜像> <根文件名> [outfile]
BPB → 根目录 → 簇链直读。卷面发现：主路盘 LBA0 是 `BERRYOSVBLK!` 印章
保留区（vblk 对拍面，非 MBR 无分区表），FAT 卷真身在 LBA16（mkfat
vol@LBA16 铁律）；故先验 LBA0 是否合法 BPB（55aa + 几何健全），否则落
LBA16。刀 3 `berry crash-log` 落地后由正式工具接管。
"""
import sys
import struct


def bpb_ok(d: bytes, off: int) -> bool:
    if len(d) < off + 512:
        return False
    if d[off + 510:off + 512] != b"\x55\xaa":
        return False
    bps = struct.unpack_from("<H", d, off + 11)[0]
    nfats = d[off + 16]
    rents = struct.unpack_from("<H", d, off + 17)[0]
    spf = struct.unpack_from("<H", d, off + 22)[0]
    return (bps in (512, 1024, 2048, 4096) and 1 <= nfats <= 2
            and rents > 0 and spf > 0)


def main() -> int:
    if len(sys.argv) < 3:
        print("用法: fatcat.py <镜像> <根文件名> [outfile]", file=sys.stderr)
        return 2
    img_path, name = sys.argv[1], sys.argv[2].upper()
    with open(img_path, "rb") as f:
        data = f.read()
    # 卷面发现：LBA0 直盘（ EFI berryos.img 形）优先，印章保留区盘落 LBA16。
    vol = 0
    if not bpb_ok(data, 0):
        vol = 16 * 512
        if not bpb_ok(data, vol):
            print("fatcat: 找不到合法 FAT 卷 BPB（LBA0/LBA16 皆否）",
                  file=sys.stderr)
            return 1
    byts_per_sec = struct.unpack_from("<H", data, vol + 11)[0]
    spc = data[vol + 13]
    rsvd = struct.unpack_from("<H", data, vol + 14)[0]
    nfats = data[vol + 16]
    root_ents = struct.unpack_from("<H", data, vol + 17)[0]
    spf = struct.unpack_from("<H", data, vol + 22)[0]
    fat_off = vol + rsvd * byts_per_sec
    root_off = vol + (rsvd + nfats * spf) * byts_per_sec
    data_off = root_off + root_ents * 32
    ent_size = spc * byts_per_sec

    ent = None
    for i in range(root_ents):
        off = root_off + i * 32
        raw = data[off:off + 32]
        if raw[0] == 0x00:
            break
        if raw[11] & 0x08 or raw[0] == 0xE5:
            continue  # 卷标/已删项
        base = raw[0:8].rstrip(b" ").decode("ascii", "replace")
        ext = raw[8:11].rstrip(b" ").decode("ascii", "replace")
        full = base + ("." + ext if ext else "")
        if full.upper() == name:
            ent = raw
            break
    if ent is None:
        print(f"fatcat: 根目录无 {name}", file=sys.stderr)
        return 1
    first = struct.unpack_from("<H", ent, 26)[0]
    size = struct.unpack_from("<I", ent, 28)[0]

    def fat12(n: int) -> int:
        off = fat_off + n + n // 2
        v = struct.unpack_from("<H", data, off)[0]
        return (v >> 4) if (n & 1) else (v & 0xFFF)

    out = bytearray()
    clus = first
    seen = set()
    while 2 <= clus < 0xFF8:
        if clus in seen:
            print("fatcat: 簇链环", file=sys.stderr)
            return 1
        seen.add(clus)
        out += data[data_off + (clus - 2) * ent_size:
                   data_off + (clus - 1) * ent_size]
        clus = fat12(clus)
    out = out[:size]
    if len(sys.argv) > 3:
        with open(sys.argv[3], "wb") as f:
            f.write(out)
        print(f"fatcat: {name} {len(out)}B -> {sys.argv[3]}")
    else:
        sys.stdout.buffer.write(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
