//! 最小以太网/ARP/IPv4/ICMP-echo/UDP/TCP/DNS 编解码（P6 A-7 + A-8 +
//! TCP 栈 T-a，RFC 894/826/791/792/768/793/1035）。
//!
//! 纯函数纪律（berry-loader/berry-fsproto/berry-blkproto 同款）：零分配、
//! 零 unsafe、零依赖；输入只借用，输出写调用方切片——**编不出就拒绝
//! （None），不截断**（含空 out 切片）。host 单测全盖（tests/net.rs）；
//! 消费者 vnet（用户态网络驱动）经 path 依赖双端共用。
//!
//! 范围 = v1 网络自验面（ARP 解网关 MAC + ICMP echo 往返对拍 + UDP/DNS
//! 名字解析对拍）最小集：
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
//! - UDP（8B 头 + payload；校验和含 12B IPv4 伪头；len 字段是报文权威
//!   长度——<8 拒、>buf.len() 截断拒、≤buf.len() 时尾随字节容忍；csum=0
//!   = IPv4 合法「未算」放行不校，见 [`parse_udp`]）
//! - TCP（20B 头 + 选项；校验和含伪头且**强制**——与 UDP 的 csum=0 放行
//!   不同；data offset 是头长权威；SYN 段 MSS 选项可编可解、其余选项按
//!   长度字段步进跳过容忍、PSH/URG 位静默忽略——host 栈回显段常带
//!   PSH，见 [`encode_tcp`]/[`parse_tcp`]；**段长 = buf.len()**——长度
//!   权威在上游 IP total_len，TCP 头自身无长度字段）
//! - DNS（A 查询编码 + A 应答解析，RFC 1035 子集——flags 恒 RD、QDCOUNT
//!   恒 1、应答跳 CNAME 取首个 A；名字支持 0xC0 压缩指针，环防护 =
//!   指针目标必须严格小于当前位置，见 [`parse_dns_a_reply`]）
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

/// 协议号：UDP。
pub const IP_PROTO_UDP: u8 = 17;
/// UDP 头长（sport/dport/len/csum）。
pub const UDP_HDR_SZ: usize = 8;
/// IPv4 伪头长（src 4 + dst 4 + zero 1 + proto 1 + len 2）——UDP 校验和
/// 的覆盖面前缀。
pub const UDP_PSEUDO_SZ: usize = 12;

/// 协议号：TCP。
pub const IP_PROTO_TCP: u8 = 6;
/// TCP 头长（无选项——data offset 5）。
pub const TCP_HDR_SZ: usize = 20;
/// TCP 选项 kind：EOL（选项列表终结，其后余字节当 padding 忽略）。
pub const TCP_OPT_EOL: u8 = 0;
/// TCP 选项 kind：NOP（1B 填充对齐，跳过）。
pub const TCP_OPT_NOP: u8 = 1;
/// TCP 选项 kind：MSS（最大段长通告，SYN 段 4B 定长）。
pub const TCP_OPT_MSS: u8 = 2;
/// MSS 选项总长（kind 1 + len 1 + 值 2）。
pub const TCP_OPT_MSS_SZ: usize = 4;

/// DNS 头长（id/flags/qd/an/ns/ar 各 2B）。
pub const DNS_HDR_SZ: usize = 12;
/// DNS 端口（RFC 1035）。
pub const DNS_PORT: u16 = 53;
/// QTYPE：A（IPv4 地址）。
pub const DNS_QTYPE_A: u16 = 1;
/// QCLASS：IN。
pub const DNS_QCLASS_IN: u16 = 1;

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

/// 解析出的 UDP 报文（payload 按 len 字段截，借用原缓冲）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Udp<'a> {
    pub sport: u16,
    pub dport: u16,
    pub csum: u16,
    pub payload: &'a [u8],
}

/// TCP 段标志（本 crate 消费的四位；PSH/URG 解析侧静默忽略——host 栈
/// 回显段常带 PSH，拒掉会把正常回显段当坏包；编码侧不产出）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TcpFlags {
    pub fin: bool,
    pub syn: bool,
    pub rst: bool,
    pub ack: bool,
}

/// 解析出的 TCP 段（payload = 头+选项后剩余，借用原缓冲）。MSS 只在
/// 选项里提取（无论段型——首个有效 MSS 选项为准，调用方按需消费）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tcp<'a> {
    pub sport: u16,
    pub dport: u16,
    pub seq: u32,
    pub ack: u32,
    pub flags: TcpFlags,
    pub window: u16,
    pub mss: Option<u16>,
    pub payload: &'a [u8],
}

/// RFC1071 累加（不折叠）：BE 16 位字累加、奇尾字节进高半字（BE 语义 =
/// 尾字节后补零）。u32 wrapping 加法精确可结合——**分段累加后折叠 == 整段
/// 折叠**（UDP 校验和 = 伪头 + 头(置零镜像) + payload 三段分开喂的数学
/// 前提）。
fn rfc1071_sum(data: &[u8]) -> u32 {
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
    sum
}

/// 折叠到 16 位 + 取反（RFC1071 收尾）。
fn fold_invert(sum: u32) -> u16 {
    let mut sum = sum;
    while sum >> 16 != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

/// RFC1071 one's-complement 校验和：BE 16 位字累加、奇尾字节进高半字
/// （BE 语义 = 尾字节后补零）、进位折叠到 16 位、取反。编码侧在
/// checksum 字段置零的字节串上算出后写入；解析侧对含 checksum 的
/// 原样字节串复核 ==0（正确报文恒折叠为 0xFFFF → 取反 0）。
pub fn checksum_rfc1071(data: &[u8]) -> u16 {
    fold_invert(rfc1071_sum(data))
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

// ---------------------------------------------------------------- UDP

/// 编码 UDP 报文（8B 头 + payload；校验和含 12B IPv4 伪头——src/dst
/// 参与，RFC 768 的覆盖面 = 伪头 + 头 + payload）。**算得 0 发 0xFFFF**
/// （0 在线路上保留「未算」语义）；out.len() 须 ≥ 8+payload.len()。
pub fn encode_udp(
    out: &mut [u8],
    src: [u8; 4],
    dst: [u8; 4],
    sport: u16,
    dport: u16,
    payload: &[u8],
) -> Option<()> {
    let total = UDP_HDR_SZ.checked_add(payload.len())?;
    if total > u16::MAX as usize {
        return None;
    }
    let m = out.get_mut(..total)?;
    m[..UDP_HDR_SZ].fill(0);
    m[0..2].copy_from_slice(&sport.to_be_bytes());
    m[2..4].copy_from_slice(&dport.to_be_bytes());
    m[4..6].copy_from_slice(&(total as u16).to_be_bytes());
    m[UDP_HDR_SZ..].copy_from_slice(payload);
    // 三段分开累加（伪头 / 头的 csum 置零镜像 / payload）——rfc1071_sum
    // 的 u32 wrapping 加法精确可结合，等价于拼接整算（零拼缓冲）。
    let pseudo = [
        src[0], src[1], src[2], src[3],
        dst[0], dst[1], dst[2], dst[3],
        0,
        IP_PROTO_UDP,
        (total >> 8) as u8,
        total as u8,
    ];
    let sum = rfc1071_sum(&pseudo).wrapping_add(rfc1071_sum(m));
    let csum = fold_invert(sum);
    // 0 保留为「未算」——算出全零时发补码形态（接收端对 0xFFFF 复核同过：
    // 0xFFFF 的反码是 0，加进和里恰使折叠和取反为 0）。
    let wire = if csum == 0 { 0xFFFF } else { csum };
    m[6..8].copy_from_slice(&wire.to_be_bytes());
    Some(())
}

/// 解析 UDP 报文：**len 字段是报文权威长度**（RFC 768）——`< 8` 坏包拒、
/// `> buf.len()` 截断拒、`≤ buf.len()` 时 payload 按 len 截（尾随字节是
/// 以太网 padding 的链路层现象，不进 UDP 面）；csum ≠ 0 时对伪头 + 报文
/// 复核 ==0（调用方传 IP 头的 src/dst——伪头参与校验）；**csum == 0 =
/// IPv4 合法「未算」放行不校**（RFC 768：全零校验和 = 发送端没算）。
pub fn parse_udp(buf: &[u8], src: [u8; 4], dst: [u8; 4]) -> Option<Udp<'_>> {
    let h = buf.get(..UDP_HDR_SZ)?;
    let len = u16::from_be_bytes([h[4], h[5]]) as usize;
    if len < UDP_HDR_SZ || len > buf.len() {
        return None;
    }
    let m = buf.get(..len)?;
    let csum = u16::from_be_bytes([m[6], m[7]]);
    if csum != 0 {
        let pseudo = [
            src[0], src[1], src[2], src[3],
            dst[0], dst[1], dst[2], dst[3],
            0,
            IP_PROTO_UDP,
            h[4],
            h[5],
        ];
        let sum = rfc1071_sum(&pseudo).wrapping_add(rfc1071_sum(m));
        if fold_invert(sum) != 0 {
            return None;
        }
    }
    Some(Udp {
        sport: u16::from_be_bytes([m[0], m[1]]),
        dport: u16::from_be_bytes([m[2], m[3]]),
        csum,
        payload: m.get(UDP_HDR_SZ..)?,
    })
}

// ---------------------------------------------------------------- TCP

/// 编码 TCP 段（20B 头 + SYN 段可带 4B MSS 选项 + payload；校验和含
/// 12B IPv4 伪头——src/dst 参与，同 UDP 先例，proto=6）。**算得 0 发
/// 0xFFFF**（TCP 校验和强制，0 在线路上 = 坏段——接收端复核对 0xFFFF
/// 恒过、对 0 恒拒，无 UDP 的「未算」豁免）。MSS 选项只在 SYN 段编
/// （非 SYN 携带是调用方错误 → None）；mss=0 无意义通告 → None。返回
/// 编码总长（选项长度可变——encode_dns_query_a 先例）。urg/保留位恒 0。
///
/// 序号语义（比较/推进）不进本 crate——归 berry-tcp 状态机。
pub fn encode_tcp(
    out: &mut [u8],
    src: [u8; 4],
    dst: [u8; 4],
    sport: u16,
    dport: u16,
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    window: u16,
    mss: Option<u16>,
    payload: &[u8],
) -> Option<usize> {
    let opt_len = match (flags.syn, mss) {
        (false, Some(_)) => return None, // MSS 选项只属 SYN 段
        (true, Some(0)) => return None,  // 0 通告无意义
        (true, Some(_)) => TCP_OPT_MSS_SZ,
        (_, None) => 0,
    };
    let total = TCP_HDR_SZ.checked_add(opt_len)?.checked_add(payload.len())?;
    if total > u16::MAX as usize {
        return None;
    }
    let m = out.get_mut(..total)?;
    m[..TCP_HDR_SZ + opt_len].fill(0);
    m[0..2].copy_from_slice(&sport.to_be_bytes());
    m[2..4].copy_from_slice(&dport.to_be_bytes());
    m[4..8].copy_from_slice(&seq.to_be_bytes());
    m[8..12].copy_from_slice(&ack.to_be_bytes());
    m[12] = (((TCP_HDR_SZ + opt_len) / 4) as u8) << 4; // data offset
    let mut f = 0u8;
    if flags.fin {
        f |= 0x01;
    }
    if flags.syn {
        f |= 0x02;
    }
    if flags.rst {
        f |= 0x04;
    }
    if flags.ack {
        f |= 0x10;
    }
    m[13] = f;
    m[14..16].copy_from_slice(&window.to_be_bytes());
    if opt_len == TCP_OPT_MSS_SZ {
        m[20] = TCP_OPT_MSS;
        m[21] = TCP_OPT_MSS_SZ as u8;
        m[22..24].copy_from_slice(&mss.unwrap().to_be_bytes());
    }
    m[TCP_HDR_SZ + opt_len..].copy_from_slice(payload);
    // 三段分开累加（伪头 / 头+选项的 csum 置零镜像 / payload）——
    // rfc1071_sum 的 u32 wrapping 加法精确可结合，等价拼接整算。
    let pseudo = [
        src[0], src[1], src[2], src[3],
        dst[0], dst[1], dst[2], dst[3],
        0,
        IP_PROTO_TCP,
        (total >> 8) as u8,
        total as u8,
    ];
    let sum = rfc1071_sum(&pseudo).wrapping_add(rfc1071_sum(m));
    let csum = fold_invert(sum);
    let wire = if csum == 0 { 0xFFFF } else { csum };
    m[16..18].copy_from_slice(&wire.to_be_bytes());
    Some(total)
}

/// 解析 TCP 段：**段长 = buf.len()**（长度权威在上游 IP total_len——TCP
/// 头自身无长度字段，调用方应传 parse_ipv4 已按 total 截好的 payload）；
/// data offset（byte12 高 4 位 ×4）是头长权威——`< 20` 坏段拒、
/// `> buf.len()` 截断拒；保留位（byte12 低 4 位 + byte13 高 2 位）非零
/// 拒；**伪头校验和复核 ==0 强制**（csum=0 也过不了复核——与 UDP 的
/// 「未算放行」不同，TCP 校验和必算）。选项区遍历：EOL 后余字节当
/// padding 忽略、NOP 步进 1、MSS（len=4 且值非 0）提取**首个**、其余
/// 选项按 len 字段步进跳过容忍（len<2 无法步进 = 坏段拒；步进越过
/// data offset 边界 = 坏段拒）；PSH/URG 位静默忽略。payload 借用
/// 原缓冲。
pub fn parse_tcp(buf: &[u8], src: [u8; 4], dst: [u8; 4]) -> Option<Tcp<'_>> {
    let h = buf.get(..TCP_HDR_SZ)?;
    if buf.len() > u16::MAX as usize {
        return None; // IP 面不可能到达（u16 total_len），防御性界检
    }
    // 保留位：byte12 低 4 位（data offset 占高 4 位）+ byte13 高 2 位
    // （flags 占低 6 位）。
    if h[12] & 0x0f != 0 || h[13] & 0xc0 != 0 {
        return None;
    }
    let offset = (h[12] >> 4) as usize * 4;
    if offset < TCP_HDR_SZ || offset > buf.len() {
        return None;
    }
    // 伪头校验和复核（强制）：伪头 + 整段原样（含 csum 字段）折叠
    // 取反 == 0。伪头 len = 段字节长。
    let pseudo = [
        src[0], src[1], src[2], src[3],
        dst[0], dst[1], dst[2], dst[3],
        0,
        IP_PROTO_TCP,
        (buf.len() >> 8) as u8,
        buf.len() as u8,
    ];
    let sum = rfc1071_sum(&pseudo).wrapping_add(rfc1071_sum(buf));
    if fold_invert(sum) != 0 {
        return None;
    }
    // 选项区 [TCP_HDR_SZ, offset)：提取首个有效 MSS，其余步进跳过。
    let mut mss = None;
    let mut p = TCP_HDR_SZ;
    while p < offset {
        let kind = buf[p];
        if kind == TCP_OPT_EOL {
            break; // 其后当 padding
        }
        if kind == TCP_OPT_NOP {
            p += 1;
            continue;
        }
        let olen = *buf.get(p + 1)? as usize;
        if olen < 2 {
            return None; // 无法步进的坏选项
        }
        if kind == TCP_OPT_MSS && olen == TCP_OPT_MSS_SZ {
            let m = u16::from_be_bytes([
                *buf.get(p + 2)?,
                *buf.get(p + 3)?,
            ]);
            if m != 0 && mss.is_none() {
                mss = Some(m); // 首个有效 MSS 为准；len≠4/值 0 按普通选项跳过
            }
        }
        p += olen;
        if p > offset {
            return None; // 选项长度越过 data offset 边界 = 坏段
        }
    }
    let f = h[13] & 0x3f;
    Some(Tcp {
        sport: u16::from_be_bytes([h[0], h[1]]),
        dport: u16::from_be_bytes([h[2], h[3]]),
        seq: u32::from_be_bytes([h[4], h[5], h[6], h[7]]),
        ack: u32::from_be_bytes([h[8], h[9], h[10], h[11]]),
        flags: TcpFlags {
            fin: f & 0x01 != 0,
            syn: f & 0x02 != 0,
            rst: f & 0x04 != 0,
            ack: f & 0x10 != 0,
        },
        window: u16::from_be_bytes([h[14], h[15]]),
        mss,
        payload: buf.get(offset..)?,
    })
}

// ---------------------------------------------------------------- DNS

/// 编码 DNS A 查询（12B 头 + 单问题；flags 恒 0x0100 = RD；返回编码
/// 总长）。name 非法（空前/空尾/连续点、label 0 或 >63、label 内 NUL、
/// wire 名超 255B）一律 None；out 不足 None 不截断。两遍结构：先整名
/// 验证 + 算 wire 长度，再写——写循环无需再处理错误面。
pub fn encode_dns_query_a(out: &mut [u8], id: u16, name: &str) -> Option<usize> {
    let bytes = name.as_bytes();
    // 第一遍：验证 + wire 长度（每 label 1 前缀 + len，末尾根终结 1）。
    let mut wire = 1usize; // 根终结符
    let mut label_start = 0usize;
    let mut i = 0usize;
    while i <= bytes.len() {
        let at_end = i == bytes.len();
        if at_end || bytes[i] == b'.' {
            let llen = i - label_start;
            if llen == 0 || llen > 63 {
                return None; // 空前/空尾/连续点 + label 上限（RFC 1035）
            }
            if bytes[label_start..i].contains(&0) {
                return None; // NUL 进 label 会伪装成 wire 终结符
            }
            wire += 1 + llen;
            if wire > 255 {
                return None;
            }
            label_start = i + 1;
        }
        i += 1;
    }
    let total = DNS_HDR_SZ + wire + 4;
    let m = out.get_mut(..total)?;
    m[..DNS_HDR_SZ].fill(0);
    m[0..2].copy_from_slice(&id.to_be_bytes());
    m[2..4].copy_from_slice(&0x0100u16.to_be_bytes()); // flags：RD=1
    m[4..6].copy_from_slice(&1u16.to_be_bytes()); // QDCOUNT=1
    // 第二遍：写 QNAME（第一遍已拒尾点——末 label 必在）。
    let mut p = DNS_HDR_SZ;
    let mut label_start = 0usize;
    for i in 0..bytes.len() {
        if bytes[i] == b'.' {
            let llen = i - label_start;
            m[p] = llen as u8;
            m[p + 1..p + 1 + llen].copy_from_slice(&bytes[label_start..i]);
            p += 1 + llen;
            label_start = i + 1;
        }
    }
    let llen = bytes.len() - label_start;
    m[p] = llen as u8;
    m[p + 1..p + 1 + llen].copy_from_slice(&bytes[label_start..]);
    p += 1 + llen;
    m[p] = 0; // 根终结
    p += 1;
    m[p..p + 2].copy_from_slice(&DNS_QTYPE_A.to_be_bytes());
    m[p + 2..p + 4].copy_from_slice(&DNS_QCLASS_IN.to_be_bytes());
    Some(total)
}

/// 跳过应答里的一个 name 字段，返回**下一字段的偏移**。支持 0xC0 压缩
/// 指针；环防护 = 指针目标必须严格小于当前读位（前向指针一律拒——自指
/// /互指环与越界同死状）。指针位点的结束偏移 = 指针处 + 2（指针即本
/// 记录内 name 的终点；终结符属于被指向的 name 主体，不再计入）。
fn skip_dns_name(buf: &[u8], mut pos: usize) -> Option<usize> {
    let mut end: Option<usize> = None;
    let mut jumped = false;
    while pos < buf.len() {
        let b = buf[pos];
        if b == 0 {
            return Some(end.unwrap_or(pos + 1));
        }
        if b & 0xC0 == 0xC0 {
            let ptr = (((b & 0x3F) as usize) << 8) | *buf.get(pos + 1)? as usize;
            if !jumped {
                end = Some(pos + 2); // 只锚首个指针位（指针链终点同此处）
            }
            if ptr >= pos {
                return None; // 前向指针 = 环或越界
            }
            jumped = true;
            pos = ptr;
        } else if b & 0xC0 != 0 {
            return None; // 保留位形（0b01/0b10）非 label 非指针
        } else {
            pos += 1 + b as usize; // label：长度前缀 + 本体
        }
    }
    None // 走到越界 = 截断坏包
}

/// 解析 DNS A 应答：id 匹配、QR=1、opcode=0、TC=0、rcode=0、QDCOUNT
/// 恰 1（跳过 question 但校验 QTYPE/QCLASS = A/IN——name 本体不逐字节
/// 比，压缩指针等价形态都认）、ANCOUNT ≥ 1，遍历应答**跳 CNAME 取首个
/// A**（RDATA 4B）。截断坏包一律 None。
pub fn parse_dns_a_reply(buf: &[u8], want_id: u16) -> Option<[u8; 4]> {
    let h = buf.get(..DNS_HDR_SZ)?;
    if u16::from_be_bytes([h[0], h[1]]) != want_id {
        return None;
    }
    let flags = u16::from_be_bytes([h[2], h[3]]);
    if flags & 0x8000 == 0 {
        return None; // QR=0：还是查询不是应答
    }
    if flags & 0x7800 != 0 {
        return None; // opcode ≠ 标准 QUERY
    }
    if flags & 0x0200 != 0 {
        return None; // TC=1：截断应答
    }
    if flags & 0x000F != 0 {
        return None; // rcode ≠ NOERROR
    }
    if u16::from_be_bytes([h[4], h[5]]) != 1 {
        return None; // QDCOUNT ≠ 1（v1 只发单问题查询）
    }
    let ancount = u16::from_be_bytes([h[6], h[7]]) as usize;
    if ancount == 0 {
        return None;
    }
    // 跳 question：name + 4B（QTYPE/QCLASS 校验 A/IN）。
    let mut p = skip_dns_name(buf, DNS_HDR_SZ)?;
    let q = buf.get(p..p + 4)?;
    if u16::from_be_bytes([q[0], q[1]]) != DNS_QTYPE_A
        || u16::from_be_bytes([q[2], q[3]]) != DNS_QCLASS_IN
    {
        return None;
    }
    p += 4;
    // 遍历应答：非 A 记录（CNAME 等）整段跳过，取首个 A 的 RDATA。
    for _ in 0..ancount {
        p = skip_dns_name(buf, p)?;
        let rr = buf.get(p..p + 10)?; // type/class/ttl/rdlen
        let rtype = u16::from_be_bytes([rr[0], rr[1]]);
        let rdlen = u16::from_be_bytes([rr[8], rr[9]]) as usize;
        p += 10;
        let rdata = buf.get(p..p + rdlen)?;
        if rtype == DNS_QTYPE_A {
            if rdlen != 4 {
                return None; // A 记录 RDATA 恒 4B
            }
            return Some(rdata.try_into().ok()?);
        }
        p += rdlen;
    }
    None // 应答里没有 A 记录（全 CNAME）
}
