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
