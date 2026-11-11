//! berry-netproto host 单测：校验和向量/往返/wire 钉板/拒绝面/恰满/空 out。
//! 纪律与 berry-blkproto 对齐——「编不出就拒绝」的全路径覆盖。

use berry_netproto::*;

/// 独立实现的校验和（逐字节加权累加——与 crate 实现的 16 位字累加不同
/// 形状，互为交叉验证：字配对/偏移/折叠 bug 两边不同时犯）。
fn checksum_naive(data: &[u8]) -> u16 {
    let mut acc: u64 = 0;
    for (i, b) in data.iter().enumerate() {
        if i % 2 == 0 {
            acc += (*b as u64) << 8;
        } else {
            acc += *b as u64;
        }
    }
    while acc >> 16 != 0 {
        acc = (acc & 0xffff) + (acc >> 16);
    }
    !(acc as u16)
}

#[test]
fn checksum_hand_vectors() {
    // 手算向量（BE 字累加 + 折叠 + 取反）：
    // 空 → 0xFFFF；[00]（奇尾补零）→ 0xFFFF；[01,02] → ~0x0102 = 0xFEFD；
    // [01,02,03] → 0x0102+0x0300 = 0x0402 → 0xFBFD；
    // [ff,ff] → ~0xFFFF = 0x0000；[ff,ff,ff,ff] → 0xFFFF+0xFFFF = 0x1FFFE
    // → 折 0xFFFE+1 = 0xFFFF → ~0xFFFF = 0x0000。
    assert_eq!(checksum_rfc1071(&[]), 0xFFFF);
    assert_eq!(checksum_rfc1071(&[0x00]), 0xFFFF);
    assert_eq!(checksum_rfc1071(&[0x01, 0x02]), 0xFEFD);
    assert_eq!(checksum_rfc1071(&[0x01, 0x02, 0x03]), 0xFBFD);
    assert_eq!(checksum_rfc1071(&[0xff, 0xff]), 0x0000);
    assert_eq!(checksum_rfc1071(&[0xff, 0xff, 0xff, 0xff]), 0x0000);
}

#[test]
fn checksum_cross_impl_randomish() {
    // 与逐字节加权实现对拍（确定性伪随机流——同输入同输出）。
    let mut s = 0x1234_5678u32;
    for len in [0usize, 1, 2, 3, 7, 20, 41, 74] {
        let data: Vec<u8> = (0..len)
            .map(|_| {
                s = s.wrapping_mul(1664525).wrapping_add(1013904223);
                (s >> 24) as u8
            })
            .collect();
        assert_eq!(
            checksum_rfc1071(&data),
            checksum_naive(&data),
            "len={len}"
        );
    }
}

#[test]
fn eth_roundtrip_and_wire() {
    let dst = [0xffu8; 6];
    let src = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
    let mut out = [0u8; 14];
    assert!(encode_eth(&mut out, &dst, &src, ET_ARP).is_some());
    // wire 钉板：dst/src 原序 + ethertype BE（0x0806 → 08 06）。
    assert_eq!(
        &out,
        &[0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x52, 0x54, 0x00, 0x12,
            0x34, 0x56, 0x08, 0x06]
    );
    let e = parse_eth(&out).unwrap();
    assert_eq!(e.dst, dst);
    assert_eq!(e.src, src);
    assert_eq!(e.ethertype, ET_ARP);
    assert_eq!(e.payload, &[] as &[u8]);
    // payload 借用面：帧后字节原样可见。
    let mut frame = [0u8; 16];
    frame[..14].copy_from_slice(&out);
    frame[14] = 0xaa;
    frame[15] = 0xbb;
    assert_eq!(parse_eth(&frame).unwrap().payload, &[0xaa, 0xbb]);
}

#[test]
fn eth_rejects() {
    // 未知 ethertype / 截断 / 编码 out 不足 / 空 out / 编码面非 ARP|IPV4。
    let mut out = [0u8; 14];
    assert!(encode_eth(&mut out, &[0; 6], &[0; 6], 0x86DD).is_none()); // IPv6
    assert!(encode_eth(&mut out[..13], &[0; 6], &[0; 6], ET_ARP).is_none());
    assert!(encode_eth(&mut [], &[0; 6], &[0; 6], ET_ARP).is_none());
    let mut bad = [0u8; 14];
    bad[12..14].copy_from_slice(&0x86DDu16.to_be_bytes());
    assert!(parse_eth(&bad).is_none());
    assert!(parse_eth(&bad[..13]).is_none());
}

#[test]
fn arp_roundtrip_and_wire() {
    let sha = [0x52u8, 0x54, 0x00, 0x12, 0x34, 0x56];
    let spa = [10, 0, 2, 15];
    let tpa = [10, 0, 2, 2];
    let mut out = [0u8; 28];
    assert!(encode_arp(&mut out, ARP_OPER_REQ, &sha, spa, &[0; 6], tpa).is_some());
    // wire 钉板：htype 00 01 / ptype 08 00 / hlen 06 / plen 04 / oper 00 01
    // + 两端硬软地址。
    let want = [
        0x00, 0x01, 0x08, 0x00, 0x06, 0x04, 0x00, 0x01, //
        0x52, 0x54, 0x00, 0x12, 0x34, 0x56, 10, 0, 2, 15, //
        0, 0, 0, 0, 0, 0, 10, 0, 2, 2,
    ];
    assert_eq!(&out, &want);
    let a = parse_arp(&out).unwrap();
    assert_eq!(a.oper, ARP_OPER_REQ);
    assert_eq!(a.sha, sha);
    assert_eq!(a.spa, spa);
    assert_eq!(a.tha, [0; 6]);
    assert_eq!(a.tpa, tpa);
}

#[test]
fn arp_rejects() {
    let mut out = [0u8; 28];
    assert!(encode_arp(&mut out, 3, &[0; 6], [0; 4], &[0; 6], [0; 4]).is_none()); // oper 野值
    assert!(encode_arp(&mut out[..27], ARP_OPER_REQ, &[0; 6], [0; 4], &[0; 6], [0; 4]).is_none());
    assert!(encode_arp(&mut [], ARP_OPER_REQ, &[0; 6], [0; 4], &[0; 6], [0; 4]).is_none());
    let good = {
        let mut b = [0u8; 28];
        assert!(encode_arp(&mut b, ARP_OPER_REPLY, &[0; 6], [0; 4], &[0; 6], [0; 4]).is_some());
        b
    };
    // 截断拒（<28B）；尾随 padding 受（线上最小帧补零形态：42B ARP 帧
    // 被 SLIRP 补到 64B → eth payload 50B = 28 报文 + 22 零，A-7 pcap
    // 实证；字段仍取首 28B）。
    let padded = [good.as_slice(), &[0u8; 22]].concat();
    let a = parse_arp(&padded).unwrap();
    assert_eq!(a.oper, ARP_OPER_REPLY);
    assert!(parse_arp(&good[..27]).is_none());
    // 变体四前件逐坏：htype/ptype/hlen/plen（BE 字——低/高字节按实际非零位打）。
    let mut bad = good;
    bad[1] = 0; // htype BE [00,01] 的低位 1 → 0（打 bad[0] 是无操作）
    assert!(parse_arp(&bad).is_none());
    let mut bad = good;
    bad[2] = 0; // ptype BE [08,00] 的高位 08 → 0（打 bad[3] 是无操作）
    assert!(parse_arp(&bad).is_none());
    let mut bad = good;
    bad[4] = 8; // hlen
    assert!(parse_arp(&bad).is_none());
    let mut bad = good;
    bad[5] = 6; // plen
    assert!(parse_arp(&bad).is_none());
    let mut bad = good;
    bad[7] = 9; // oper 野值
    assert!(parse_arp(&bad).is_none());
}

#[test]
fn ipv4_roundtrip_and_fields() {
    let payload = [0xaau8; 32];
    let mut out = [0u8; 52];
    assert!(encode_ipv4(&mut out, 64, IP_PROTO_ICMP, [10, 0, 2, 15], [10, 0, 2, 2], &payload).is_some());
    // 0x45 / total_len BE=52 / ttl / proto + payload 原样。
    assert_eq!(out[0], 0x45);
    assert_eq!(u16::from_be_bytes([out[2], out[3]]), 52);
    assert_eq!(out[8], 64);
    assert_eq!(out[9], IP_PROTO_ICMP);
    assert_eq!(&out[20..], &payload);
    let ip = parse_ipv4(&out).unwrap();
    assert_eq!(ip.ttl, 64);
    assert_eq!(ip.proto, IP_PROTO_ICMP);
    assert_eq!(ip.src, [10, 0, 2, 15]);
    assert_eq!(ip.dst, [10, 0, 2, 2]);
    assert_eq!(ip.payload, &payload);
    // 头校验和独立复核（置零镜像重算相等）。
    let mut zeroed = out;
    zeroed[10] = 0;
    zeroed[11] = 0;
    assert_eq!(u16::from_be_bytes([out[10], out[11]]), checksum_naive(&zeroed[..20]));
}

#[test]
fn ipv4_rejects() {
    let mut out = [0u8; 52];
    assert!(encode_ipv4(&mut out, 64, IP_PROTO_ICMP, [0; 4], [0; 4], &[0xaau8; 32]).is_some());
    // 校验和坏一字节。
    let mut bad = out;
    bad[8] ^= 0xff;
    assert!(parse_ipv4(&bad).is_none());
    // total_len 权威长度：尾随以太网 padding 受（payload 按 total=52 截，
    // 不含 padding）；截断（total > buf.len()）拒；total < 头长拒。
    let padded = [out.as_slice(), &[0u8; 6]].concat();
    let ip = parse_ipv4(&padded).unwrap();
    assert_eq!(ip.payload.len(), 32);
    assert!(parse_ipv4(&out[..51]).is_none());
    let mut short_total = out;
    short_total[2..4].copy_from_slice(&19u16.to_be_bytes());
    short_total[10] = 0;
    short_total[11] = 0;
    let c = checksum_rfc1071(&short_total[..20]);
    short_total[10..12].copy_from_slice(&c.to_be_bytes());
    assert!(parse_ipv4(&short_total).is_none());
    // 分片/MF：byte6 bit5（MF）与 frag offset 域。
    let mut bad = out;
    bad[6] |= 0x20; // MF
    // 重算校验和让「分片拒」成为唯一拒绝因——否则先死在校验和。
    bad[10] = 0;
    bad[11] = 0;
    let c = checksum_rfc1071(&bad[..20]);
    bad[10..12].copy_from_slice(&c.to_be_bytes());
    assert!(parse_ipv4(&bad).is_none());
    let mut bad = out;
    bad[7] = 1; // frag offset 低字节
    bad[10] = 0;
    bad[11] = 0;
    let c = checksum_rfc1071(&bad[..20]);
    bad[10..12].copy_from_slice(&c.to_be_bytes());
    assert!(parse_ipv4(&bad).is_none());
    // 非 0x45 头。
    let mut bad = out;
    bad[0] = 0x47;
    bad[10] = 0;
    bad[11] = 0;
    let c = checksum_rfc1071(&bad[..20]);
    bad[10..12].copy_from_slice(&c.to_be_bytes());
    assert!(parse_ipv4(&bad).is_none());
    // ttl 0（同法重算）。
    let mut bad = out;
    bad[8] = 0;
    bad[10] = 0;
    bad[11] = 0;
    let c = checksum_rfc1071(&bad[..20]);
    bad[10..12].copy_from_slice(&c.to_be_bytes());
    assert!(parse_ipv4(&bad).is_none());
    // 编码面：out 不足 / 超 64KiB。
    assert!(encode_ipv4(&mut [0u8; 51], 64, 1, [0; 4], [0; 4], &[0xaau8; 32]).is_none());
    assert!(encode_ipv4(&mut [0u8; 20], 64, 1, [0; 4], [0; 4], &[0u8; 65516]).is_none());
    assert!(encode_ipv4(&mut [], 64, 1, [0; 4], [0; 4], &[]).is_none());
}

#[test]
fn icmp_roundtrip_and_wire() {
    let payload = b"berryos-vnet-ping-0123456789abcd"; // 32B
    let mut out = [0u8; 40];
    assert!(encode_icmp_echo(&mut out, ICMP_TY_ECHO_REQ, 0x6265, 1, payload).is_some());
    assert_eq!(out[0], ICMP_TY_ECHO_REQ);
    assert_eq!(out[1], 0); // code
    assert_eq!(u16::from_be_bytes([out[4], out[5]]), 0x6265); // id BE
    assert_eq!(u16::from_be_bytes([out[6], out[7]]), 1); // seq BE
    assert_eq!(&out[8..], &payload[..]);
    let e = parse_icmp_echo(&out).unwrap();
    assert_eq!(e.ty, ICMP_TY_ECHO_REQ);
    assert_eq!(e.id, 0x6265);
    assert_eq!(e.seq, 1);
    assert_eq!(e.payload, &payload[..]);
    // 校验和独立复核。
    let mut zeroed = out;
    zeroed[2] = 0;
    zeroed[3] = 0;
    assert_eq!(u16::from_be_bytes([out[2], out[3]]), checksum_naive(&zeroed));
    // reply 形同往返。
    let mut r = [0u8; 40];
    assert!(encode_icmp_echo(&mut r, ICMP_TY_ECHO_REPLY, 0x6265, 1, payload).is_some());
    assert_eq!(parse_icmp_echo(&r).unwrap().ty, ICMP_TY_ECHO_REPLY);
}

#[test]
fn icmp_rejects() {
    let payload = [0x5au8; 8];
    let mut out = [0u8; 16];
    assert!(encode_icmp_echo(&mut out, ICMP_TY_ECHO_REQ, 1, 1, &payload).is_some());
    let mut bad = out;
    bad[9] ^= 0x01; // payload 坏一字节 → 整包校验和拒
    assert!(parse_icmp_echo(&bad).is_none());
    let mut bad = out;
    bad[1] = 1; // code != 0（重算校验和使 code 成为唯一拒绝因）
    bad[2] = 0;
    bad[3] = 0;
    let c = checksum_rfc1071(&bad);
    bad[2..4].copy_from_slice(&c.to_be_bytes());
    assert!(parse_icmp_echo(&bad).is_none());
    let mut bad = out;
    bad[0] = 3; // ty 野值（目的地不可达）
    bad[2] = 0;
    bad[3] = 0;
    let c = checksum_rfc1071(&bad);
    bad[2..4].copy_from_slice(&c.to_be_bytes());
    assert!(parse_icmp_echo(&bad).is_none());
    assert!(parse_icmp_echo(&out[..7]).is_none());
    // 编码面：ty 野值 / out 不足 / 空 out。
    assert!(encode_icmp_echo(&mut out, 3, 1, 1, &payload).is_none());
    assert!(encode_icmp_echo(&mut [0u8; 15], ICMP_TY_ECHO_REQ, 1, 1, &payload).is_none());
    assert!(encode_icmp_echo(&mut [], ICMP_TY_ECHO_REQ, 1, 1, &[]).is_none());
    // 空 payload 合法（echo 最小面）。
    let mut m = [0u8; 8];
    assert!(encode_icmp_echo(&mut m, ICMP_TY_ECHO_REQ, 1, 1, &[]).is_some());
    assert_eq!(parse_icmp_echo(&m).unwrap().payload, &[] as &[u8]);
}

#[test]
fn exact_fit_out_accepted() {
    // 恰贴合容量（blkproto out_exact_fit / fsproto wire_name_exact_max
    // 同族）：不截断、不误拒。
    let mut eth = [0u8; 14];
    assert!(encode_eth(&mut eth, &[1; 6], &[2; 6], ET_IPV4).is_some());
    assert_eq!(parse_eth(&eth).unwrap().ethertype, ET_IPV4);
    let mut arp = [0u8; 28];
    assert!(encode_arp(&mut arp, ARP_OPER_REPLY, &[1; 6], [0; 4], &[2; 6], [0; 4]).is_some());
    assert!(parse_arp(&arp).is_some());
    let mut ip = [0u8; 20];
    assert!(encode_ipv4(&mut ip, 64, 1, [0; 4], [0; 4], &[]).is_some());
    assert!(parse_ipv4(&ip).is_some());
    let mut im = [0u8; 8];
    assert!(encode_icmp_echo(&mut im, ICMP_TY_ECHO_REPLY, 0, 0, &[]).is_some());
    assert!(parse_icmp_echo(&im).is_some());
}

// ------------------------------------------------------------- UDP / DNS

/// UDP 校验和独立复核：伪头（src/dst/0/proto/len）+ csum 置零的报文整段
/// 拼接后走逐字节加权实现——与 crate 侧「伪头/头/payload 三段 u32 累加」
/// 不同形状，互为交叉验证（分段可结合性因此也被钉板）。
fn udp_csum_naive(src: [u8; 4], dst: [u8; 4], len: u16, msg_zeroed: &[u8]) -> u16 {
    let mut v = Vec::new();
    v.extend_from_slice(&src);
    v.extend_from_slice(&dst);
    v.push(0);
    v.push(IP_PROTO_UDP);
    v.extend_from_slice(&len.to_be_bytes());
    v.extend_from_slice(msg_zeroed);
    checksum_naive(&v)
}

#[test]
fn udp_roundtrip_and_wire() {
    let payload = [0xaau8; 12];
    let src = [10, 0, 2, 15];
    let dst = [10, 0, 2, 3];
    let mut out = [0u8; 20];
    assert!(encode_udp(&mut out, src, dst, 0xC001, DNS_PORT, &payload).is_some());
    // wire 钉板：sport/dport/len 全 BE + payload 原样（csum 动态值不钉）。
    assert_eq!(u16::from_be_bytes([out[0], out[1]]), 0xC001);
    assert_eq!(u16::from_be_bytes([out[2], out[3]]), DNS_PORT);
    assert_eq!(u16::from_be_bytes([out[4], out[5]]), 20);
    assert_eq!(&out[8..], &payload);
    // 校验和独立复核（伪头 + csum 置零镜像整段拼接重算）。
    let mut zeroed = out;
    zeroed[6] = 0;
    zeroed[7] = 0;
    assert_eq!(
        u16::from_be_bytes([out[6], out[7]]),
        udp_csum_naive(src, dst, 20, &zeroed)
    );
    let u = parse_udp(&out, src, dst).unwrap();
    assert_eq!(u.sport, 0xC001);
    assert_eq!(u.dport, DNS_PORT);
    assert_eq!(u.payload, &payload);
    // 空 payload 合法（最小 8B 报文）。
    let mut m = [0u8; 8];
    assert!(encode_udp(&mut m, src, dst, 1, 2, &[]).is_some());
    assert_eq!(parse_udp(&m, src, dst).unwrap().payload, &[] as &[u8]);
    // 0 → 0xFFFF 补档角（RFC 768：0 保留「未算」）：搜一个 sport 使算得
    // 校验和为 0（one's-complement 可解、必存在），线上必须是 0xFFFF 且
    // 接收端复核仍过（0xFFFF 的反码是 0，加进和里恰使复核折叠为 0）。
    for sport in 0u16..=u16::MAX {
        let mut m9 = [0u8; 9];
        m9[0..2].copy_from_slice(&sport.to_be_bytes());
        m9[2..4].copy_from_slice(&DNS_PORT.to_be_bytes());
        m9[4..6].copy_from_slice(&9u16.to_be_bytes());
        m9[8] = 0xAB;
        if udp_csum_naive(src, dst, 9, &m9) == 0 {
            let mut p = [0u8; 9];
            assert!(encode_udp(&mut p, src, dst, sport, DNS_PORT, &[0xAB]).is_some());
            assert_eq!(u16::from_be_bytes([p[6], p[7]]), 0xFFFF);
            assert!(parse_udp(&p, src, dst).is_some());
            return;
        }
    }
    panic!("0-csum sport 未找到（数学上必存在——one's-complement 可解）");
}

#[test]
fn udp_csum_detects_corruption() {
    let payload = [0x5au8; 8];
    let src = [10, 0, 2, 15];
    let dst = [10, 0, 2, 3];
    let mut out = [0u8; 16];
    assert!(encode_udp(&mut out, src, dst, 5000, 53, &payload).is_some());
    // payload 坏一字节 → 伪头参与下的整包校验和拒。
    let mut bad = out;
    bad[9] ^= 0x01;
    assert!(parse_udp(&bad, src, dst).is_none());
    // 伪头 src 参与验证：解析侧 src 与编码不一致（IP 头被换读）→ 拒。
    assert!(parse_udp(&out, [10, 0, 2, 99], dst).is_none());
    // csum=0 = IPv4 域合法「未算」：置 0 后即使 payload 已坏也放行不校
    // （RFC 768：全零校验和 = 发送端没算）。
    let mut zero = out;
    zero[6] = 0;
    zero[7] = 0;
    let mut corrupted_unchecked = zero;
    corrupted_unchecked[9] ^= 0x01;
    assert!(parse_udp(&corrupted_unchecked, src, dst).is_some());
}

#[test]
fn udp_len_authority_and_rejects() {
    let payload = [0xaau8; 12];
    let src = [1, 2, 3, 4];
    let dst = [5, 6, 7, 8];
    let mut out = [0u8; 20];
    assert!(encode_udp(&mut out, src, dst, 1, 2, &payload).is_some());
    // 尾随 padding 受：payload 按 len=20 截，链路层补零不进 UDP 面。
    let padded = [out.as_slice(), &[0u8; 6]].concat();
    assert_eq!(parse_udp(&padded, src, dst).unwrap().payload.len(), 12);
    // len 字段两向拒：改 len 后重算校验和使 len 成为唯一拒绝因（len 同
    // 时在头与伪头里参与校验——不重算先死在校验和，ipv4_rejects 同法）。
    let relen = |len: u16| -> Vec<u8> {
        let mut b = out;
        b[4..6].copy_from_slice(&len.to_be_bytes());
        b[6] = 0;
        b[7] = 0;
        let c = udp_csum_naive(src, dst, len, &b);
        b[6..8].copy_from_slice(&c.to_be_bytes());
        b.to_vec()
    };
    assert!(parse_udp(&relen(30), src, dst).is_none()); // len > buf.len() 截断拒
    assert!(parse_udp(&relen(7), src, dst).is_none()); // len < 头长坏包拒
    assert!(parse_udp(&out[..7], src, dst).is_none()); // buf 本身截断
    // 编码面：out 不足 / 空 out / 超 u16 len 域（total > 65535）。
    assert!(encode_udp(&mut [0u8; 19], src, dst, 1, 2, &payload).is_none());
    assert!(encode_udp(&mut [], src, dst, 1, 2, &[]).is_none());
    assert!(encode_udp(&mut [0u8; 8], src, dst, 1, 2, &[0u8; 65528]).is_none());
    // 恰贴合（exact_fit 同族）。
    let mut fit = [0u8; 20];
    assert!(encode_udp(&mut fit, src, dst, 1, 2, &payload).is_some());
    assert!(parse_udp(&fit, src, dst).is_some());
}

fn dns_query_localhost() -> Vec<u8> {
    let mut q = [0u8; 64];
    let n = encode_dns_query_a(&mut q, 0x1234, "localhost").unwrap();
    q[..n].to_vec()
}

/// 从 query wire 搓应答：flags=0x8180（QR+RD+RA）、ancount 置数、拼 answers。
fn dns_reply(query: &[u8], ancount: u16, answers: &[u8]) -> Vec<u8> {
    let mut r = query.to_vec();
    r[2] = 0x81;
    r[3] = 0x80;
    r[6..8].copy_from_slice(&ancount.to_be_bytes());
    r.extend_from_slice(answers);
    r
}

#[test]
fn dns_query_wire_golden() {
    let mut out = [0u8; 64];
    let n = encode_dns_query_a(&mut out, 0x1234, "localhost").unwrap();
    assert_eq!(n, 27);
    // wire 钉板：id / flags=RD / qdcount=1 + QNAME(1+9+1，单 label 长 9)
    // + QTYPE/QCLASS。
    assert_eq!(
        &out[..n],
        &[
            0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x09, b'l', b'o', b'c', b'a', b'l', b'h', b'o', b's', b't', 0x00,
            0x00, 0x01, 0x00, 0x01,
        ][..]
    );
    // 多段名 a.berryos.dev：三 label 各带长度前缀 + 根终结 + 尾随 4B。
    let mut out = [0u8; 64];
    let n = encode_dns_query_a(&mut out, 7, "a.berryos.dev").unwrap();
    assert_eq!(n, 31);
    assert_eq!(out[12], 1);
    assert_eq!(&out[13..14], b"a");
    assert_eq!(out[14], 7);
    assert_eq!(&out[15..22], b"berryos");
    assert_eq!(out[22], 3);
    assert_eq!(&out[23..26], b"dev");
    assert_eq!(out[26], 0);
    // label 恰 63（上限含）：总长 = 12 + (1+63) + 1 + 4 = 81。
    let mut out = [0u8; 128];
    assert_eq!(encode_dns_query_a(&mut out, 1, &"x".repeat(63)), Some(81));
    // 拒面：空名 / 首点 / 尾点 / 连续点 / 64B label / label 内 NUL。
    let mut out = [0u8; 400];
    assert!(encode_dns_query_a(&mut out, 1, "").is_none());
    assert!(encode_dns_query_a(&mut out, 1, ".a").is_none());
    assert!(encode_dns_query_a(&mut out, 1, "a.").is_none());
    assert!(encode_dns_query_a(&mut out, 1, "a..b").is_none());
    assert!(encode_dns_query_a(&mut out, 1, &"x".repeat(64)).is_none());
    assert!(encode_dns_query_a(&mut out, 1, "a\0b").is_none());
    // wire 名 255 上限：四 label×63 = 257 拒；三 63 + 一 60 = 254 过（恰含）。
    let over = ["x".repeat(63), "y".repeat(63), "z".repeat(63), "w".repeat(63)].join(".");
    assert!(encode_dns_query_a(&mut out, 1, &over).is_none());
    let fit255 = ["x".repeat(63), "y".repeat(63), "z".repeat(63), "w".repeat(60)].join(".");
    assert!(encode_dns_query_a(&mut out, 1, &fit255).is_some());
    // out 不足 / 恰贴合 / 空 out。
    let mut small = [0u8; 26];
    assert!(encode_dns_query_a(&mut small, 0x1234, "localhost").is_none());
    let mut fit = [0u8; 27];
    assert!(encode_dns_query_a(&mut fit, 0x1234, "localhost").is_some());
    let mut empty = [0u8; 0];
    assert!(encode_dns_query_a(&mut empty, 1, "a").is_none());
}

#[test]
fn dns_reply_parse_and_rejects() {
    let q = dns_query_localhost();
    // 黄金应答：name=压缩指针 C0 0C（指回 question 的 "localhost"）+
    // A 记录 TTL 60 + RDATA 127.0.0.1。
    let ans_a = [
        0xC0, 0x0C, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3C, 0x00, 0x04,
        127, 0, 0, 1,
    ];
    let r = dns_reply(&q, 1, &ans_a);
    assert_eq!(parse_dns_a_reply(&r, 0x1234), Some([127, 0, 0, 1]));
    // id 不匹配。
    assert!(parse_dns_a_reply(&r, 0x1235).is_none());
    // 逐 flag 坏：QR=0（应答位没立）/ opcode=1 / TC=1 / rcode=1。
    let mut x = r.clone();
    x[2] = 0x01;
    x[3] = 0x80; // QR=0
    assert!(parse_dns_a_reply(&x, 0x1234).is_none());
    let mut x = r.clone();
    x[2] = 0x89; // opcode=1（0x8980 & 0x7800 = 0x0800）
    assert!(parse_dns_a_reply(&x, 0x1234).is_none());
    let mut x = r.clone();
    x[2] = 0x83; // TC=1（0x8380 & 0x0200 = 0x0200）
    assert!(parse_dns_a_reply(&x, 0x1234).is_none());
    let mut x = r.clone();
    x[3] = 0x81; // rcode=1
    assert!(parse_dns_a_reply(&x, 0x1234).is_none());
    // ancount=0（后面拼着记录也不认）。
    let x = dns_reply(&q, 0, &ans_a);
    assert!(parse_dns_a_reply(&x, 0x1234).is_none());
    // 截断：应答砍尾字节 / 头砍到 11B。
    assert!(parse_dns_a_reply(&r[..r.len() - 1], 0x1234).is_none());
    assert!(parse_dns_a_reply(&r[..11], 0x1234).is_none());
    // question 的 QTYPE 非 A（打 query 尾 4B 的 type 低位 = AAAA 0x001C）。
    let mut q6 = q.clone();
    q6[24] = 0x1C;
    let x = dns_reply(&q6, 1, &ans_a);
    assert!(parse_dns_a_reply(&x, 0x1234).is_none());
}

#[test]
fn dns_reply_cname_skip_and_pointer_guard() {
    let q = dns_query_localhost();
    // CNAME（rdata = "localhost" 平铺名，rdlen=11）+ A 两记录：跳 CNAME
    // 取首个 A 的 RDATA。
    let cname = [
        0xC0, 0x0C, 0x00, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0B,
        0x01, b'l', b'o', b'c', b'a', b'l', b'h', b'o', b's', b't', 0x00,
    ];
    let a2 = [
        0xC0, 0x0C, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3C, 0x00, 0x04,
        127, 0, 0, 2,
    ];
    let r = dns_reply(&q, 2, &[cname.as_slice(), a2.as_slice()].concat());
    assert_eq!(parse_dns_a_reply(&r, 0x1234), Some([127, 0, 0, 2]));
    // 平铺名应答（无压缩指针）：name = "a" 根 + RR + 4B rdata。
    let flat = [
        0x01, b'a', 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3C, 0x00, 0x04,
        1, 2, 3, 4,
    ];
    let r = dns_reply(&q, 1, &flat);
    assert_eq!(parse_dns_a_reply(&r, 0x1234), Some([1, 2, 3, 4]));
    // 前向指针（目标 ≥ 指针自身位）：环/越界同死状，一律拒。
    let fwd = [
        0xC0, 0x1E, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3C, 0x00, 0x04,
        127, 0, 0, 1,
    ];
    let r = dns_reply(&q, 1, &fwd);
    assert!(parse_dns_a_reply(&r, 0x1234).is_none());
    // 自指指针（ptr == pos）。
    let selfp = [
        0xC0, 0x1B, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3C, 0x00, 0x04,
        127, 0, 0, 1,
    ];
    let r = dns_reply(&q, 1, &selfp);
    assert!(parse_dns_a_reply(&r, 0x1234).is_none());
}

// ---------------------------------------------------------------- TCP

/// TCP 测试定值四元组（guest → 网关方向，vnet 同款）。
const TSRC: [u8; 4] = [10, 0, 2, 15];
const TDST: [u8; 4] = [10, 0, 2, 2];

/// TCP 校验和独立复核（udp_csum_naive 同构，proto=6）：伪头 len 字段 =
/// 段字节长（TCP 头自身无长度字段，len 直接取 msg.len()）。
fn tcp_csum_naive(src: [u8; 4], dst: [u8; 4], msg_zeroed: &[u8]) -> u16 {
    let mut v = Vec::new();
    v.extend_from_slice(&src);
    v.extend_from_slice(&dst);
    v.push(0);
    v.push(IP_PROTO_TCP);
    v.extend_from_slice(&(msg_zeroed.len() as u16).to_be_bytes());
    v.extend_from_slice(msg_zeroed);
    checksum_naive(&v)
}

/// 手工搓 TCP 段（encode_tcp 编不出的形态——PSH/URG 位、任意选项组合、
/// 坏偏移）：头 + opts 原样嵌入（长度须 4 对齐），csum 事后重算填入
/// （0→0xFFFF 补档同 crate 语义——手工段也按线上规范编）。
fn tcp_handmade(
    sport: u16,
    dport: u16,
    seq: u32,
    ack: u32,
    flags_byte: u8,
    window: u16,
    opts: &[u8],
    payload: &[u8],
) -> Vec<u8> {
    assert_eq!((TCP_HDR_SZ + opts.len()) % 4, 0, "opts 须 4 对齐");
    let mut m = vec![0u8; TCP_HDR_SZ + opts.len() + payload.len()];
    m[0..2].copy_from_slice(&sport.to_be_bytes());
    m[2..4].copy_from_slice(&dport.to_be_bytes());
    m[4..8].copy_from_slice(&seq.to_be_bytes());
    m[8..12].copy_from_slice(&ack.to_be_bytes());
    m[12] = (((TCP_HDR_SZ + opts.len()) / 4) as u8) << 4;
    m[13] = flags_byte;
    m[14..16].copy_from_slice(&window.to_be_bytes());
    m[20..20 + opts.len()].copy_from_slice(opts);
    m[TCP_HDR_SZ + opts.len()..].copy_from_slice(payload);
    let c = tcp_csum_naive(TSRC, TDST, &m);
    let wire = if c == 0 { 0xFFFF } else { c };
    m[16..18].copy_from_slice(&wire.to_be_bytes());
    m
}

#[test]
fn tcp_syn_wire_and_csum() {
    let mut out = [0u8; 24];
    let n = encode_tcp(
        &mut out,
        TSRC,
        TDST,
        0xC350,
        18080,
        1000,
        0,
        TcpFlags { syn: true, ..Default::default() },
        512,
        Some(128),
        &[],
    )
    .unwrap();
    assert_eq!(n, 24);
    // wire 钉板：sport/dport/seq/ack 全 BE + data offset 6（20+4 选项）
    // + flags 0x02（纯 SYN）+ MSS 选项 [2, 4, 0x00, 0x80]。
    assert_eq!(u16::from_be_bytes([out[0], out[1]]), 0xC350);
    assert_eq!(u16::from_be_bytes([out[2], out[3]]), 18080);
    assert_eq!(u32::from_be_bytes([out[4], out[5], out[6], out[7]]), 1000);
    assert_eq!(u32::from_be_bytes([out[8], out[9], out[10], out[11]]), 0);
    assert_eq!(out[12], 0x60);
    assert_eq!(out[13], 0x02);
    assert_eq!(u16::from_be_bytes([out[14], out[15]]), 512);
    assert_eq!(&out[20..24], &[TCP_OPT_MSS, TCP_OPT_MSS_SZ as u8, 0x00, 0x80]);
    // 校验和独立复核（伪头 proto=6/len=24 + csum 置零镜像整段）。
    let mut zeroed = out;
    zeroed[16] = 0;
    zeroed[17] = 0;
    let naive = tcp_csum_naive(TSRC, TDST, &zeroed);
    let wire = u16::from_be_bytes([out[16], out[17]]);
    assert!(wire == naive || (naive == 0 && wire == 0xFFFF)); // 0→0xFFFF 补档
    // 解析回读。
    let t = parse_tcp(&out, TSRC, TDST).unwrap();
    assert!(t.flags.syn && !t.flags.ack);
    assert_eq!(t.mss, Some(128));
    assert_eq!(t.payload, &[] as &[u8]);
}

#[test]
fn tcp_roundtrip_forms() {
    // 四形态全字段往返：纯 ACK / FIN+ACK / RST / 数据段（payload 借用验证）。
    let mut out = [0u8; 20 + 8];
    let n = encode_tcp(
        &mut out,
        TSRC,
        TDST,
        50000,
        80,
        1001,
        5001,
        TcpFlags { ack: true, ..Default::default() },
        512,
        None,
        &[],
    )
    .unwrap();
    assert_eq!(n, 20);
    let t = parse_tcp(&out[..n], TSRC, TDST).unwrap();
    assert_eq!(t.seq, 1001);
    assert_eq!(t.ack, 5001);
    assert!(t.flags.ack && !t.flags.fin);
    assert_eq!(t.mss, None);

    let n = encode_tcp(
        &mut out,
        TSRC,
        TDST,
        50000,
        80,
        1002,
        5002,
        TcpFlags { fin: true, ack: true, ..Default::default() },
        512,
        None,
        &[],
    )
    .unwrap();
    assert_eq!(n, 20);
    let t = parse_tcp(&out[..n], TSRC, TDST).unwrap();
    assert!(t.flags.fin && t.flags.ack);

    let n = encode_tcp(
        &mut out,
        TSRC,
        TDST,
        50000,
        80,
        0,
        0,
        TcpFlags { rst: true, ..Default::default() },
        0,
        None,
        &[],
    )
    .unwrap();
    assert_eq!(n, 20);
    let t = parse_tcp(&out[..n], TSRC, TDST).unwrap();
    assert!(t.flags.rst && !t.flags.ack);

    let payload = [0x5au8; 8];
    let n = encode_tcp(
        &mut out,
        TSRC,
        TDST,
        50000,
        80,
        1001,
        5001,
        TcpFlags { ack: true, ..Default::default() },
        512,
        None,
        &payload,
    )
    .unwrap();
    assert_eq!(n, 28);
    let t = parse_tcp(&out[..n], TSRC, TDST).unwrap();
    assert_eq!(t.payload, &payload);
    assert_eq!(t.window, 512);
}

#[test]
fn tcp_csum_detects_corruption() {
    let payload = [0xaau8; 8];
    let mut out = [0u8; 28];
    encode_tcp(
        &mut out,
        TSRC,
        TDST,
        1,
        2,
        100,
        200,
        TcpFlags { ack: true, ..Default::default() },
        512,
        None,
        &payload,
    )
    .unwrap();
    // payload 坏一字节 → 伪头参与下的整段校验和拒。
    let mut bad = out;
    bad[25] ^= 0x01;
    assert!(parse_tcp(&bad, TSRC, TDST).is_none());
    // 伪头 src 参与验证：解析侧 src 与编码不一致（IP 头被换读）→ 拒。
    assert!(parse_tcp(&out, [10, 0, 2, 99], TDST).is_none());
    // csum=0 拒（TCP 校验和强制——与 UDP 的「未算放行」相反）：
    // 置 0 后复核必不过，即使 payload 完好。
    let mut zero = out;
    zero[16] = 0;
    zero[17] = 0;
    assert!(parse_tcp(&zero, TSRC, TDST).is_none());
    // 0→0xFFFF 补档角：搜一个 sport 使算得校验和为 0（one's-complement
    // 可解、必存在），线上必须是 0xFFFF 且接收端复核仍过。
    for sport in 0u16..=u16::MAX {
        let mut m = [0u8; 28];
        m[0..2].copy_from_slice(&sport.to_be_bytes());
        m[2..4].copy_from_slice(&2u16.to_be_bytes());
        m[12] = 0x50;
        m[13] = 0x10;
        m[20..28].copy_from_slice(&payload);
        if tcp_csum_naive(TSRC, TDST, &m) == 0 {
            let mut p = [0u8; 28];
            assert!(
                encode_tcp(
                    &mut p,
                    TSRC,
                    TDST,
                    sport,
                    2,
                    0,
                    0,
                    TcpFlags { ack: true, ..Default::default() },
                    0,
                    None,
                    &payload,
                )
                .is_some()
            );
            assert_eq!(u16::from_be_bytes([p[16], p[17]]), 0xFFFF);
            assert!(parse_tcp(&p, TSRC, TDST).is_some());
            return;
        }
    }
    panic!("0-csum sport 未找到（数学上必存在——one's-complement 可解）");
}

#[test]
fn tcp_options_tolerated() {
    // 选项区混合步进：NOP×2 + MSS + 未知选项(len 6) + EOL + padding——
    // EOL 后余字节当 padding，MSS 提取、未知跳过。
    let opts: [u8; 16] = [
        TCP_OPT_NOP, TCP_OPT_NOP,
        TCP_OPT_MSS, 4, 0x05, 0xB4, // 1460
        253, 6, 0, 0, 0, 0, // 未知选项按 len 步进
        TCP_OPT_EOL, 0, 0, 0, // EOL 后 padding
    ];
    let m = tcp_handmade(1, 2, 100, 200, 0x10, 512, &opts, b"hi");
    let t = parse_tcp(&m, TSRC, TDST).unwrap();
    assert_eq!(t.mss, Some(1460));
    assert_eq!(t.payload, b"hi");
    // 首个有效 MSS 为准：两个 MSS 选项取第一个。
    let opts2: [u8; 8] = [TCP_OPT_MSS, 4, 0x04, 0x00, TCP_OPT_MSS, 4, 0x05, 0xB4];
    let m = tcp_handmade(1, 2, 0, 0, 0x02, 512, &opts2, &[]);
    assert_eq!(parse_tcp(&m, TSRC, TDST).unwrap().mss, Some(1024));
    // MSS len≠4 → 按普通选项跳过（不提取）。
    let opts3: [u8; 8] = [TCP_OPT_MSS, 5, 0x05, 0xB4, 0, TCP_OPT_NOP, TCP_OPT_NOP, TCP_OPT_NOP];
    let m = tcp_handmade(1, 2, 0, 0, 0x02, 512, &opts3, &[]);
    assert_eq!(parse_tcp(&m, TSRC, TDST).unwrap().mss, None);
    // MSS 值 0 → 无意义通告跳过（SYN 段也提不出）。
    let opts4: [u8; 4] = [TCP_OPT_MSS, 4, 0, 0];
    let m = tcp_handmade(1, 2, 0, 0, 0x02, 512, &opts4, &[]);
    assert_eq!(parse_tcp(&m, TSRC, TDST).unwrap().mss, None);
    // 非 SYN 段携带 MSS：解析侧照提（「无论段型」——调用方按需消费）。
    let m = tcp_handmade(1, 2, 0, 0, 0x10, 512, &[TCP_OPT_MSS, 4, 1, 2], &[]);
    assert_eq!(parse_tcp(&m, TSRC, TDST).unwrap().mss, Some(0x0102));
}

#[test]
fn tcp_parse_rejects() {
    let m = tcp_handmade(1, 2, 100, 200, 0x10, 512, &[], b"payload");
    // buf 截断（< 20 / 头在但越 data offset）。
    assert!(parse_tcp(&m[..19], TSRC, TDST).is_none());
    assert!(parse_tcp(&m[..22], TSRC, TDST).is_none()); // csum 对不上（截掉尾）
    // data offset 坏值（改字段后重算 csum 使 offset 成为唯一拒绝因）。
    let reoffset = |nib: u8| -> Vec<u8> {
        let mut b = m.clone();
        b[12] = (b[12] & 0x0f) | (nib << 4);
        b[16] = 0;
        b[17] = 0;
        let c = tcp_csum_naive(TSRC, TDST, &b);
        let wire = if c == 0 { 0xFFFF } else { c };
        b[16..18].copy_from_slice(&wire.to_be_bytes());
        b
    };
    assert!(parse_tcp(&reoffset(4), TSRC, TDST).is_none()); // offset 16 < 20
    assert!(parse_tcp(&reoffset(15), TSRC, TDST).is_none()); // offset 60 > buf.len()
    // 保留位非零（byte12 低 4 位 / byte13 高 2 位）——重算 csum 后仍拒。
    let mut r1 = m.clone();
    r1[12] |= 0x01;
    r1[16] = 0;
    r1[17] = 0;
    let c = tcp_csum_naive(TSRC, TDST, &r1);
    r1[16..18].copy_from_slice(&c.to_be_bytes());
    assert!(parse_tcp(&r1, TSRC, TDST).is_none());
    let mut r2 = m.clone();
    r2[13] |= 0x40;
    r2[16] = 0;
    r2[17] = 0;
    let c = tcp_csum_naive(TSRC, TDST, &r2);
    r2[16..18].copy_from_slice(&c.to_be_bytes());
    assert!(parse_tcp(&r2, TSRC, TDST).is_none());
    // 坏选项：olen<2 无法步进 / olen 越过 data offset 边界。
    let m2 = tcp_handmade(1, 2, 0, 0, 0x02, 512, &[253, 1, 0, 0], &[]);
    assert!(parse_tcp(&m2, TSRC, TDST).is_none());
    let m3 = tcp_handmade(1, 2, 0, 0, 0x02, 512, &[4, 12, 0, 0, 0, 0, 0, 0], &[]);
    assert!(parse_tcp(&m3, TSRC, TDST).is_none());
    // 防御界：段长 > u16::MAX（IP 面不可达，界检先行——csum 无关）。
    let big = vec![0u8; u16::MAX as usize + 1];
    assert!(parse_tcp(&big, TSRC, TDST).is_none());
}

#[test]
fn tcp_encode_rejects() {
    let fl = TcpFlags { ack: true, ..Default::default() };
    let syn = TcpFlags { syn: true, ..Default::default() };
    let payload = [0xaau8; 8];
    // 非 SYN 携带 MSS / SYN 携带 0 通告。
    let mut out = [0u8; 28];
    assert!(encode_tcp(&mut out, TSRC, TDST, 1, 2, 0, 0, fl, 512, Some(128), &[]).is_none());
    assert!(encode_tcp(&mut out, TSRC, TDST, 1, 2, 0, 0, syn, 512, Some(0), &[]).is_none());
    // out 不足 / 恰贴合 / 空 out。
    assert!(encode_tcp(&mut [0u8; 27], TSRC, TDST, 1, 2, 0, 0, fl, 512, None, &payload).is_none());
    let mut fit = [0u8; 28];
    assert!(encode_tcp(&mut fit, TSRC, TDST, 1, 2, 0, 0, fl, 512, None, &payload).is_some());
    assert!(parse_tcp(&fit, TSRC, TDST).is_some());
    assert!(encode_tcp(&mut [], TSRC, TDST, 1, 2, 0, 0, fl, 512, None, &[]).is_none());
    // total > u16 len 域。
    assert!(
        encode_tcp(&mut [0u8; 65535], TSRC, TDST, 1, 2, 0, 0, fl, 512, None, &[0u8; 65516])
            .is_none()
    );
}

#[test]
fn tcp_psh_urg_ignored() {
    // PSH/URG 位静默忽略（host 栈回显段常带 PSH——拒掉会把正常回显段
    // 当坏包）：手工段 flags=0x38（PSH|URG|ACK）解析过、TcpFlags 只出 ack。
    let m = tcp_handmade(50000, 18080, 5001, 1001, 0x38, 512, &[], b"echo-me");
    let t = parse_tcp(&m, TSRC, TDST).unwrap();
    assert!(t.flags.ack);
    assert!(!t.flags.fin && !t.flags.syn && !t.flags.rst);
    assert_eq!(t.payload, b"echo-me");
    // SYN+ACK 带 PSH 同理（SYN-ACK 形态）。
    let m = tcp_handmade(50000, 18080, 5000, 1001, 0x1A, 512, &[TCP_OPT_MSS, 4, 0x05, 0xB4], &[]);
    let t = parse_tcp(&m, TSRC, TDST).unwrap();
    assert!(t.flags.syn && t.flags.ack);
    assert_eq!(t.mss, Some(1460));
}
