//! berry-dtbface——最小 FDT（flattened device tree）walker 纯函数面
//!（W3-Q18，自 `kernel/src/dtb.rs` 外迁）。
//!
//! 单源纪律（同 berry-hofface/vslotface）：**判定律唯一事实源在此**，
//! `kernel/src/dtb.rs` 机械壳消费（`boot_probe` 物理臂——`from_raw_parts`
//! 直映切片 + RAM 界检——留内核侧）。
//!
//! 纪律（自 dtb.rs 逐字继承）：
//! - **纯函数、零 panic**——畸形输入（坏魔数/越界偏移/截断/未知 token）
//!   一律 `None`，boot 侧降级为「无 initrd」存活（ABI §8.0「永不 panic」
//!   同源精神）。所有指针算术走 `checked_add`——溢出即畸形。
//! - FDT 大端字段全 `from_be`；结构块偏移由推进逻辑保证 4 对齐。
//! - 迭代 walk 不递归——恶意深嵌套只耗 cycle 不炸栈，自然终止
//!   （每 token 至少前进 4 字节，越界即 `None` 出口）。

#![cfg_attr(not(test), no_std)]

const FDT_MAGIC: u32 = 0xD00D_FEED;
const FDT_BEGIN_NODE: u32 = 0x1;
const FDT_END_NODE: u32 = 0x2;
const FDT_PROP: u32 = 0x3;
const FDT_NOP: u32 = 0x4;
const FDT_END: u32 = 0x9;
/// FDT 头固定 10 个 u32（magic/totalsize/off_mem_rsvmap/off_dt_struct/
/// off_dt_strings/version/last_comp/boot_cpuid/size_strings/size_struct）。
const FDT_HEADER_LEN: usize = 40;

/// 界检大端 u32 读取（`off + 4` 越界即 `None`）。
pub fn rd32(b: &[u8], off: usize) -> Option<u32> {
    let end = off.checked_add(4)?;
    if end > b.len() { return None; }
    Some(u32::from_be_bytes([b[off], b[off + 1], b[off + 2], b[off + 3]]))
}

/// NUL 结尾字符串定位（返回字节串与**含 NUL** 的长度；越界无 NUL 即 `None`）。
pub fn cstr_at(b: &[u8], off: usize) -> Option<(&[u8], usize)> {
    if off >= b.len() { return None; }
    let rest = &b[off..];
    let nul = rest.iter().position(|&c| c == 0)?;
    Some((&rest[..nul], nul + 1))
}

/// 属性值按 cell 数转 u64（1 cell = u32 / 2 cell = u64，大端；其他长度畸形）。
pub fn prop_u64(dt: &[u8], off: usize, len: usize) -> Option<u64> {
    match len {
        4 => Some(rd32(dt, off)? as u64),
        8 => Some(((rd32(dt, off)? as u64) << 32) | rd32(dt, off.checked_add(4)?)? as u64),
        _ => None,
    }
}

/// 4 对齐步进（FDT 名字/属性值段长度向上取整到 4 的幂次边界）。
const fn align4(len: usize) -> usize {
    (len + 3) & !3
}

/// 在合法 FDT 里找 `/chosen`（**根的直接子节点**）的 initrd 区间。
///
/// 返回 `(start, end)` 物理地址（`end > start`；start==0 视无效——QEMU 不会
/// 把 initrd 放 0，0 用作「无」哨兵）。
pub fn find_initrd(dtb: &[u8]) -> Option<(u64, u64)> {
    if rd32(dtb, 0)? != FDT_MAGIC { return None; }
    let totalsize = rd32(dtb, 4)? as usize;
    let off_struct = rd32(dtb, 12)? as usize;
    let off_strings = rd32(dtb, 16)? as usize;
    // 声明超实给 = 截断，视畸形（不悄悄缩水解析）。
    if totalsize < FDT_HEADER_LEN || totalsize > dtb.len() { return None; }
    let dt = &dtb[..totalsize];
    if off_struct >= dt.len() || off_strings >= dt.len() { return None; }
    let strings = &dt[off_strings..];

    let mut off = off_struct;
    let mut depth = 0usize; // 根 BEGIN 后为 1；根的直接子节点为 2
    let mut in_chosen = false;
    let mut start: Option<u64> = None;
    let mut end: Option<u64> = None;
    loop {
        match rd32(dt, off)? {
            FDT_BEGIN_NODE => {
                off = off.checked_add(4)?;
                let (name, len) = cstr_at(dt, off)?;
                // 名字段（含 NUL）向上 4 对齐。
                off = off.checked_add(align4(len))?;
                depth += 1;
                in_chosen = depth == 2 && name == b"chosen";
            }
            FDT_PROP => {
                let len = rd32(dt, off.checked_add(4)?)? as usize;
                let nameoff = rd32(dt, off.checked_add(8)?)? as usize;
                let data_off = off.checked_add(12)?;
                off = data_off.checked_add(align4(len))?;
                if in_chosen {
                    let (name, _) = cstr_at(strings, nameoff)?;
                    if name == b"linux,initrd-start" {
                        start = Some(prop_u64(dt, data_off, len)?);
                    } else if name == b"linux,initrd-end" {
                        end = Some(prop_u64(dt, data_off, len)?);
                    }
                }
            }
            FDT_END_NODE => {
                off = off.checked_add(4)?;
                depth = depth.checked_sub(1)?;
                if depth <= 1 { in_chosen = false; }
            }
            FDT_NOP => { off = off.checked_add(4)?; }
            FDT_END => break,
            _ => return None, // 未知 token = 畸形
        }
    }
    match (start, end) {
        (Some(s), Some(e)) if s != 0 && e > s => Some((s, e)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- 测试脚手架：手工构 DTB（大端头 10×u32 + 结构块 + 字符串块）----

    fn be32(v: u32) -> [u8; 4] {
        v.to_be_bytes()
    }

    fn be64(v: u64) -> [u8; 8] {
        v.to_be_bytes()
    }

    /// 构一个最小合法 FDT：根节点含子节点 `chosen`（带 start/end 属性）。
    /// `props` = chosen 内属性序列（name, 值字节）。
    fn build_dtb(chosen_props: &[(&str, Vec<u8>)], extra_strings: &[&str]) -> Vec<u8> {
        let mut strings: Vec<u8> = Vec::new();
        let mut name_off = std::collections::HashMap::<&str, usize>::new();
        for (name, _) in chosen_props {
            if !name_off.contains_key(name) {
                name_off.insert(name, strings.len());
                strings.extend_from_slice(name.as_bytes());
                strings.push(0);
            }
        }
        for name in extra_strings {
            if !name_off.contains_key(name) {
                name_off.insert(name, strings.len());
                strings.extend_from_slice(name.as_bytes());
                strings.push(0);
            }
        }

        let mut dt = Vec::new();
        // 头 10 字段先占位（最后回填）。
        dt.extend_from_slice(&be32(0)); // magic
        dt.extend_from_slice(&be32(0)); // totalsize
        dt.extend_from_slice(&be32(48)); // off_mem_rsvmap（头后 8B 保留项×0——仅偏移合法即可）
        let off_struct_pos = dt.len();
        dt.extend_from_slice(&be32(0)); // off_dt_struct
        let off_strings_pos = dt.len();
        dt.extend_from_slice(&be32(0)); // off_dt_strings
        dt.extend_from_slice(&be32(17)); // version
        dt.extend_from_slice(&be32(16)); // last_comp
        dt.extend_from_slice(&be32(0)); // boot_cpuid
        dt.extend_from_slice(&be32(0)); // size_strings（本 walker 不读）
        dt.extend_from_slice(&be32(0)); // size_struct
        assert_eq!(dt.len(), FDT_HEADER_LEN);

        // 结构块。
        let off_struct = dt.len();
        dt.extend_from_slice(&be32(FDT_BEGIN_NODE));
        dt.push(0); // 根名 = 空（只有 NUL）
        dt.extend_from_slice(&[0; 3]); // 对齐补齐（1→4）

        dt.extend_from_slice(&be32(FDT_BEGIN_NODE));
        dt.extend_from_slice(b"chosen\0");
        dt.extend_from_slice(&[0; 1]); // 7→8 对齐

        for (name, val) in chosen_props {
            dt.extend_from_slice(&be32(FDT_PROP));
            dt.extend_from_slice(&be32(val.len() as u32));
            dt.extend_from_slice(&be32(name_off[name] as u32));
            dt.extend_from_slice(val);
            let pad = (4 - (val.len() & 3)) & 3;
            dt.extend(std::iter::repeat(0u8).take(pad));
        }

        dt.extend_from_slice(&be32(FDT_END_NODE)); // /chosen
        dt.extend_from_slice(&be32(FDT_END_NODE)); // /
        dt.extend_from_slice(&be32(FDT_END));
        // 结构块 4 对齐（凑到 4 倍数再放 strings）。
        while dt.len() & 3 != 0 {
            dt.push(0);
        }
        let off_strings = dt.len();
        dt.extend_from_slice(&strings);

        // 回填头。
        dt[0..4].copy_from_slice(&be32(FDT_MAGIC));
        let total = dt.len() as u32;
        dt[4..8].copy_from_slice(&be32(total));
        dt[off_struct_pos..off_struct_pos + 4].copy_from_slice(&be32(off_struct as u32));
        dt[off_strings_pos..off_strings_pos + 4].copy_from_slice(&be32(off_strings as u32));
        dt
    }

    fn good_dtb() -> Vec<u8> {
        build_dtb(
            &[
                ("linux,initrd-start", be32(0x4200_0000).to_vec()),
                ("linux,initrd-end", be32(0x4280_0000).to_vec()),
            ],
            &[],
        )
    }

    // ---- rd32 ----

    #[test]
    fn rd32_be_and_bounds() {
        let b = [0x12, 0x34, 0x56, 0x78, 0x9a];
        assert_eq!(rd32(&b, 0), Some(0x1234_5678)); // 大端
        assert_eq!(rd32(&b, 1), Some(0x3456_789a)); // 非对齐偏移可读
        assert_eq!(rd32(&b, 2), None); // 越界（2+4 > 5）
        assert_eq!(rd32(&b, 5), None); // 恰端点
        assert_eq!(rd32(&b, usize::MAX), None); // checked_add 溢出 None（零 panic）
        assert_eq!(rd32(&[], 0), None); // 空切片
    }

    // ---- cstr_at ----

    #[test]
    fn cstr_at_finds_nul_and_len_includes_it() {
        let b = b"chosen\0other\0";
        let (s, len) = cstr_at(b, 0).unwrap();
        assert_eq!(s, b"chosen");
        assert_eq!(len, 7); // 含 NUL
        let (s2, len2) = cstr_at(b, 7).unwrap();
        assert_eq!(s2, b"other");
        assert_eq!(len2, 6);
        assert_eq!(cstr_at(b, 12), Some((&b""[..], 1))); // 末位 NUL = 空串合法
        assert_eq!(cstr_at(b, 13), None); // 恰端点（越界）
        assert_eq!(cstr_at(b"no-nul", 0), None); // 无 NUL = 畸形
    }

    // ---- prop_u64 ----

    #[test]
    fn prop_u64_cell_kinds() {
        let b4 = be32(0x0102_0304);
        assert_eq!(prop_u64(&b4, 0, 4), Some(0x0102_0304)); // 1 cell
        let b8 = be64(0x0102_0304_0506_0708);
        assert_eq!(prop_u64(&b8, 0, 8), Some(0x0102_0304_0506_0708)); // 2 cell
        assert_eq!(prop_u64(&b8, 0, 0), None); // 空
        assert_eq!(prop_u64(&b8, 0, 1), None); // 1 字节畸形
        assert_eq!(prop_u64(&b8, 0, 16), None); // 4 cell 畸形
        assert_eq!(prop_u64(&b8, 6, 4), None); // 8 字节缓冲读尾 4 越界
    }

    // ---- find_initrd：正路 ----

    #[test]
    fn find_initrd_happy_path() {
        let dtb = good_dtb();
        assert_eq!(find_initrd(&dtb), Some((0x4200_0000, 0x4280_0000)));
    }

    #[test]
    fn find_initrd_two_cell_addrs() {
        let dtb = build_dtb(
            &[
                ("linux,initrd-start", be64(0x0000_0004_2000_0000).to_vec()),
                ("linux,initrd-end", be64(0x0000_0004_2800_0000).to_vec()),
            ],
            &[],
        );
        assert_eq!(find_initrd(&dtb), Some((0x4_2000_0000, 0x4_2800_0000)));
    }

    #[test]
    fn find_initrd_ignores_siblings_and_nested() {
        // chosen 的兄弟节点带同款属性——不误收（深度 ≠ 2 的直接子节点）。
        let mut dtb = good_dtb();
        // 尾部追加没意义（结构块长度由 totalsize 钉）；改法 = 重建带杂项字符串。
        let dtb2 = build_dtb(
            &[
                ("linux,initrd-start", be32(0x4200_0000).to_vec()),
                ("linux,initrd-end", be32(0x4280_0000).to_vec()),
                ("bootargs", b"console=ttyAMA0\0".to_vec()),
            ],
            &["some-other-prop"],
        );
        assert_eq!(find_initrd(&dtb), Some((0x4200_0000, 0x4280_0000)));
        assert_eq!(find_initrd(&dtb2), Some((0x4200_0000, 0x4280_0000)));
    }

    // ---- find_initrd：畸形全 None ----

    #[test]
    fn find_initrd_rejects_bad_magic() {
        let mut dtb = good_dtb();
        dtb[0..4].copy_from_slice(&be32(0xDEAD_BEEF));
        assert_eq!(find_initrd(&dtb), None);
    }

    #[test]
    fn find_initrd_rejects_truncated_totalsize() {
        let mut dtb = good_dtb();
        let total = u32::from_be_bytes([dtb[4], dtb[5], dtb[6], dtb[7]]);
        // 声明超实给 = 畸形（不悄悄缩水解析）。
        dtb[4..8].copy_from_slice(&be32(total + 4));
        assert_eq!(find_initrd(&dtb), None);
        // 头长以下同拒。
        dtb[4..8].copy_from_slice(&be32(FDT_HEADER_LEN as u32 - 4));
        assert_eq!(find_initrd(&dtb), None);
    }

    #[test]
    fn find_initrd_rejects_oob_struct_or_strings_off() {
        let mut dtb = good_dtb();
        // off_dt_struct 越界（≥ totalsize）。
        let oob = dtb.len() as u32;
        dtb[12..16].copy_from_slice(&be32(oob));
        assert_eq!(find_initrd(&dtb), None);
        let mut dtb2 = good_dtb();
        // off_dt_strings 越界。
        let oob2 = dtb2.len() as u32;
        dtb2[16..20].copy_from_slice(&be32(oob2));
        assert_eq!(find_initrd(&dtb2), None);
    }

    #[test]
    fn find_initrd_rejects_missing_or_bad_props() {
        // 只有 start 无 end → None。
        let only_start = build_dtb(&[("linux,initrd-start", be32(0x4200_0000).to_vec())], &[]);
        assert_eq!(find_initrd(&only_start), None);
        // end ≤ start → None。
        let inverted = build_dtb(
            &[
                ("linux,initrd-start", be32(0x4280_0000).to_vec()),
                ("linux,initrd-end", be32(0x4200_0000).to_vec()),
            ],
            &[],
        );
        assert_eq!(find_initrd(&inverted), None);
        // start == 0（哨兵「无」）→ None——即使 end > 0。
        let zero = build_dtb(
            &[
                ("linux,initrd-start", be32(0).to_vec()),
                ("linux,initrd-end", be32(0x1000).to_vec()),
            ],
            &[],
        );
        assert_eq!(find_initrd(&zero), None);
    }

    #[test]
    fn find_initrd_rejects_unknown_token() {
        let mut dtb = good_dtb();
        let off_struct = u32::from_be_bytes([dtb[12], dtb[13], dtb[14], dtb[15]]) as usize;
        // 首 token 换成未知值（如 0x7）。
        dtb[off_struct..off_struct + 4].copy_from_slice(&be32(0x7));
        assert_eq!(find_initrd(&dtb), None);
    }

    #[test]
    fn find_initrd_rejects_truncation_mid_walk() {
        // 把 totalsize 截到结构块中间——walk 中 rd32 越界即 None。
        let mut dtb = good_dtb();
        let off_struct = u32::from_be_bytes([dtb[12], dtb[13], dtb[14], dtb[15]]) as usize;
        let cut = off_struct + 20; // 根 BEGIN+名 之后、属性序列中途
        dtb.truncate(cut);
        dtb[4..8].copy_from_slice(&be32(cut as u32));
        assert_eq!(find_initrd(&dtb), None);
    }

    #[test]
    fn find_initrd_handles_nop_padding() {
        // NOP token = 4 字节直进——插在结构块头不破坏解析。
        let mut dtb = good_dtb();
        let off_struct = u32::from_be_bytes([dtb[12], dtb[13], dtb[14], dtb[15]]) as usize;
        let mut padded = dtb.clone();
        // 在结构块前插 2 个 NOP（重排 = 复制旧块内容前移）。
        let mut out = padded[..off_struct].to_vec();
        out.extend_from_slice(&be32(FDT_NOP));
        out.extend_from_slice(&be32(FDT_NOP));
        out.extend_from_slice(&padded[off_struct..]);
        // off_dt_struct 不变（NOP 插在指向位置）——但 off_dt_strings 平移 +8。
        let off_strings = u32::from_be_bytes([out[16], out[17], out[18], out[19]]) as usize + 8;
        out[16..20].copy_from_slice(&be32(off_strings as u32));
        let total = out.len() as u32;
        out[4..8].copy_from_slice(&be32(total));
        assert_eq!(find_initrd(&out), Some((0x4200_0000, 0x4280_0000)));
    }

    #[test]
    fn find_initrd_empty_and_tiny_inputs() {
        assert_eq!(find_initrd(&[]), None);
        assert_eq!(find_initrd(&[0; 8]), None); // 不足头长
        let mut hdr_only = vec![0u8; FDT_HEADER_LEN];
        hdr_only[0..4].copy_from_slice(&be32(FDT_MAGIC));
        hdr_only[4..8].copy_from_slice(&be32(FDT_HEADER_LEN as u32));
        hdr_only[12..16].copy_from_slice(&be32(FDT_HEADER_LEN as u32)); // off_struct = 恰 totalsize → 拒
        assert_eq!(find_initrd(&hdr_only), None);
    }
}
