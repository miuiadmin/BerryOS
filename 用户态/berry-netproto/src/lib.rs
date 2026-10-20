//! 最小以太网/ARP/IPv4/ICMP-echo 编解码（P6 A-7，RFC 894/826/791/792）。
//!
//! 纯函数纪律（berry-loader/berry-fsproto/berry-blkproto 同款）：零分配、
//! 零 unsafe、零依赖；输入只借用，输出写调用方切片——**编不出就拒绝
//! （None），不截断**（含空 out 切片）。host 单测全盖（tests/net.rs）；
//! 消费者 vnet（用户态网络驱动）经 path 依赖双端共用。
//!
//! 范围 = v1 网络自验面（ARP 解网关 MAC + ICMP echo 往返对拍）最小集：
//! - 以太网帧头（14B；ethertype 只认 ARP/IPV4——其余 None，调用方当
//!   「跳过杂散帧」处理）
//! - ARP（28B Ethernet/IPv4 变体——htype/ptype/hlen/plen 四前件解析侧
//!   执法，编码侧恒写规范值；尾随字节容忍 = 以太网最小帧 padding，见
//!   [`parse_arp`]）
//! - IPv4 头（20B 无选项——0x45；RFC1071 one's-complement 校验和：编码
//!   侧在 checksum 字段置零的镜像上算，解析侧对含 checksum 的原样头
//!   复核 ==0；分片/MF 拒、total_len 上界执法 ≤ buf.len()——payload 按
//!   total 截，链路 padding 不进 IP 面，见 [`parse_ipv4`]）
//! - ICMP echo（8B 头 + 任意 payload；校验和整报文执法、code=0 执法）
//!
//! 一切多字节字段网络字节序（BE）；TTL=64/ICMP code=0 等 v1 定值由编码
//! 侧内部写死，参数面只收调用方可变项（地址/操作码/id/seq/payload）。

// berry-virtio 同款：非 test 构建无 std（workspace none-target 成员 + musl
// 依赖面），host 测试构建放开 std（tests/net.rs 独立 crate 本就带 std）。
#![cfg_attr(not(test), no_std)]

/// 以太网帧头（dst 6 + src 6 + ethertype 2）。
pub const ETH_HDR_SZ: usize = 14;
/// ethertype：ARP。
pub const ET_ARP: u16 = 0x0806;
/// ethertype：IPv4。
pub const ET_IPV4: u16 = 0x0800;

/// ARP 报文长（Ethernet/IPv4 变体：htype/ptype/hlen/plen/oper 8 + 两端
/// 硬软地址 20）。
pub const ARP_LEN: usize = 28;
/// htype = 1（Ethernet）。
pub const ARP_HTYPE_ETHERNET: u16 = 1;
pub const ARP_OPER_REQ: u16 = 1;
pub const ARP_OPER_REPLY: u16 = 2;

/// IPv4 头长（无选项——0x45 定长头）。
pub const IPV4_HDR_SZ: usize = 20;
/// 协议号：ICMP。
pub const IP_PROTO_ICMP: u8 = 1;

/// ICMP echo 头长（ty/code/csum/id/seq）。
pub const ICMP_HDR_SZ: usize = 8;
pub const ICMP_TY_ECHO_REQ: u8 = 8;
pub const ICMP_TY_ECHO_REPLY: u8 = 0;

/// 解析出的以太网帧（payload = 头后剩余字节，借用原缓冲）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Eth<'a> {
    pub dst: [u8; 6],
    pub src: [u8; 6],
    pub ethertype: u16,
    pub payload: &'a [u8],
}

/// ARP 报文（Ethernet/IPv4 变体的语义面）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Arp {
    pub oper: u16,
    pub sha: [u8; 6],
    pub spa: [u8; 4],
    pub tha: [u8; 6],
    pub tpa: [u8; 4],
}

/// 解析出的 IPv4 包（payload = 头后剩余 = total_len - 20，借用原缓冲）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ipv4<'a> {
    pub ttl: u8,
    pub proto: u8,
    pub src: [u8; 4],
    pub dst: [u8; 4],
    pub payload: &'a [u8],
}

/// 解析出的 ICMP echo 报文（req/reply 同形，ty 区分）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IcmpEcho<'a> {
    pub ty: u8,
    pub id: u16,
    pub seq: u16,
    pub payload: &'a [u8],
}

/// RFC1071 one's-complement 校验和：BE 16 位字累加、奇尾字节进高半字
/// （BE 语义 = 尾字节后补零）、进位折叠到 16 位、取反。编码侧在
/// checksum 字段置零的字节串上算出后写入；解析侧对含 checksum 的
/// 原样字节串复核 ==0（正确报文恒折叠为 0xFFFF → 取反 0）。
pub fn checksum_rfc1071(data: &[u8]) -> u16 {
    let mut sum: u32 = 0;
    let mut i = 0;
    while i + 2 <= data.len() {
        sum = sum
            .wrapping_add(u16::from_be_bytes([data[i], data[i + 1]]) as u32);
        i += 2;
    }
    if i < data.len() {
        // 奇尾字节：BE 字序下占据下一字的高半（低位补零）。
        sum = sum.wrapping_add((data[i] as u32) << 8);
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// 编码以太网帧头（写 out[0..14]；ethertype 只收 ARP/IPV4——其余 None，
/// v1 无其它承载）。payload 由调用方自行续写。
pub fn encode_eth(out: &mut [u8], dst: &[u8; 6], src: &[u8; 6], ethertype: u16) -> Option<()> {
    if ethertype != ET_ARP && ethertype != ET_IPV4 {
        return None;
    }
    let b = out.get_mut(..ETH_HDR_SZ)?;
    b[..6].copy_from_slice(dst);
    b[6..12].copy_from_slice(src);
    b[12..14].copy_from_slice(&ethertype.to_be_bytes());
    Some(())
}

/// 解析以太网帧头。buf < 14B → None；ethertype 不在 ARP/IPV4 → None
/// （杂散帧面：调用方跳过，不当错误——None 即「不是本 crate 的帧」）。
pub fn parse_eth(buf: &[u8]) -> Option<Eth<'_>> {
    let h = buf.get(..ETH_HDR_SZ)?;
    let ethertype = u16::from_be_bytes([h[12], h[13]]);
    if ethertype != ET_ARP && ethertype != ET_IPV4 {
        return None;
    }
    Some(Eth {
        dst: h[..6].try_into().ok()?,
        src: h[6..12].try_into().ok()?,
        ethertype,
        payload: buf.get(ETH_HDR_SZ..)?,
    })
}

/// 编码 ARP 报文（写 out[0..28]，规范 Ethernet/IPv4 变体；oper 只收
/// REQ/REPLY）。编码侧恒写 htype=1/ptype=0x0800/hlen=6/plen=4。
pub fn encode_arp(
    out: &mut [u8],
    oper: u16,
    sha: &[u8; 6],
    spa: [u8; 4],
    tha: &[u8; 6],
    tpa: [u8; 4],
) -> Option<()> {
    if oper != ARP_OPER_REQ && oper != ARP_OPER_REPLY {
        return None;
    }
    let b = out.get_mut(..ARP_LEN)?;
    b.copy_from_slice(&[0u8; ARP_LEN]);
    b[0..2].copy_from_slice(&ARP_HTYPE_ETHERNET.to_be_bytes());
    b[2..4].copy_from_slice(&ET_IPV4.to_be_bytes()); // ptype
    b[4] = 6; // hlen
    b[5] = 4; // plen
    b[6..8].copy_from_slice(&oper.to_be_bytes());
    b[8..14].copy_from_slice(sha);
    b[14..18].copy_from_slice(&spa);
    b[18..24].copy_from_slice(tha);
    b[24..28].copy_from_slice(&tpa);
    Some(())
}

/// 解析 ARP 报文（**≥28B**——首 28B 为报文本体，尾随字节容忍：发送端把
/// 不足 60B 的帧补零到以太网最小线宽（A-7 pcap 实证：SLIRP 的 42B ARP
/// 应答以 64B 帧/50B eth payload 到达），长度权威在 hlen/plen 不在线宽）；
/// htype/ptype/hlen/plen 四前件任一非规范 Ethernet/IPv4 变体 → None；
/// oper 只收 REQ/REPLY。
pub fn parse_arp(buf: &[u8]) -> Option<Arp> {
    let b = buf.get(..ARP_LEN)?;
    if u16::from_be_bytes([b[0], b[1]]) != ARP_HTYPE_ETHERNET {
        return None;
    }
    if u16::from_be_bytes([b[2], b[3]]) != ET_IPV4 {
        return None;
    }
    if b[4] != 6 || b[5] != 4 {
        return None;
    }
    let oper = u16::from_be_bytes([b[6], b[7]]);
    if oper != ARP_OPER_REQ && oper != ARP_OPER_REPLY {
        return None;
    }
    Some(Arp {
        oper,
        sha: b[8..14].try_into().ok()?,
        spa: b[14..18].try_into().ok()?,
        tha: b[18..24].try_into().ok()?,
        tpa: b[24..28].try_into().ok()?,
    })
}

/// 编码 IPv4 包（头 20B + payload 续写进同一 out——out.len() 须 ≥
/// 20+payload.len()）。v1 定值：tos=0/id=0/frag=0/ttl 由参数（调用方
/// 传 64）；checksum 在置零镜像上算出后写入。
pub fn encode_ipv4(
    out: &mut [u8],
    ttl: u8,
    proto: u8,
    src: [u8; 4],
    dst: [u8; 4],
    payload: &[u8],
) -> Option<()> {
    let total = IPV4_HDR_SZ.checked_add(payload.len())?;
    if total > u16::MAX as usize {
        return None;
    }
    if out.len() < total {
        return None;
    }
    let h = out.get_mut(..IPV4_HDR_SZ)?;
    h.copy_from_slice(&[0u8; IPV4_HDR_SZ]);
    h[0] = 0x45; // ver 4 / IHL 5（无选项）
    h[2..4].copy_from_slice(&(total as u16).to_be_bytes());
    h[8] = ttl;
    h[9] = proto;
    h[12..16].copy_from_slice(&src);
    h[16..20].copy_from_slice(&dst);
    let csum = checksum_rfc1071(h);
    h[10..12].copy_from_slice(&csum.to_be_bytes());
    out.get_mut(IPV4_HDR_SZ..total)?.copy_from_slice(payload);
    Some(())
}

/// 解析 IPv4 包：0x45 头执法、**头校验和复核 ==0**、total_len 是包的权威
/// 长度（RFC 791）——`< 头长` 坏包拒、`> buf.len()` 截断拒、`≤ buf.len()`
/// 时 payload 按 total 截（尾随字节是以太网最小帧 padding 的链路层现象，
/// 不进 IP payload——下游 ICMP 整报文校验和/payload 对拍因此不受 padding
/// 影响）、分片/MF 拒（frag 位域 + MF 任一非零 → None——v1 无重组面）、
/// ttl=0 拒。
pub fn parse_ipv4(buf: &[u8]) -> Option<Ipv4<'_>> {
    let h = buf.get(..IPV4_HDR_SZ)?;
    if h[0] != 0x45 {
        return None;
    }
    if checksum_rfc1071(h) != 0 {
        return None;
    }
    let total = u16::from_be_bytes([h[2], h[3]]) as usize;
    if total < IPV4_HDR_SZ || total > buf.len() {
        return None;
    }
    // byte6 低 6 位 = MF(bit5) + frag offset 高 5 位；byte7 = frag offset
    // 低 8 位——任一分片痕迹即拒（DF bit6 允许）。
    if h[6] & 0x3f != 0 || h[7] != 0 {
        return None;
    }
    if h[8] == 0 {
        return None;
    }
    Some(Ipv4 {
        ttl: h[8],
        proto: h[9],
        src: [h[12], h[13], h[14], h[15]],
        dst: [h[16], h[17], h[18], h[19]],
        payload: buf.get(IPV4_HDR_SZ..total)?,
    })
}

/// 编码 ICMP echo 报文（8B 头 + payload；ty 只收 REQ/REPLY，code 恒 0；
/// checksum 在置零镜像上算出后写入）。
pub fn encode_icmp_echo(
    out: &mut [u8],
    ty: u8,
    id: u16,
    seq: u16,
    payload: &[u8],
) -> Option<()> {
    if ty != ICMP_TY_ECHO_REQ && ty != ICMP_TY_ECHO_REPLY {
        return None;
    }
    let total = ICMP_HDR_SZ.checked_add(payload.len())?;
    if out.len() < total {
        return None;
    }
    let m = out.get_mut(..total)?;
    // 头区清零（code/校验和位从零起算）；payload 区随后整段覆写。
    m[..ICMP_HDR_SZ].fill(0);
    m[0] = ty;
    m[4..6].copy_from_slice(&id.to_be_bytes());
    m[6..8].copy_from_slice(&seq.to_be_bytes());
    m[ICMP_HDR_SZ..].copy_from_slice(payload);
    let csum = checksum_rfc1071(m);
    m[2..4].copy_from_slice(&csum.to_be_bytes());
    Some(())
}

/// 解析 ICMP echo 报文：**整报文校验和复核 ==0**、code=0 执法、ty 只收
/// REQ/REPLY。payload 借用原缓冲（恰长语义：payload 即头后全部）。
pub fn parse_icmp_echo(buf: &[u8]) -> Option<IcmpEcho<'_>> {
    let h = buf.get(..ICMP_HDR_SZ)?;
    if checksum_rfc1071(buf) != 0 {
        return None;
    }
    if h[1] != 0 {
        return None;
    }
    let ty = h[0];
    if ty != ICMP_TY_ECHO_REQ && ty != ICMP_TY_ECHO_REPLY {
        return None;
    }
    Some(IcmpEcho {
        ty,
        id: u16::from_be_bytes([h[4], h[5]]),
        seq: u16::from_be_bytes([h[6], h[7]]),
        payload: buf.get(ICMP_HDR_SZ..)?,
    })
}
