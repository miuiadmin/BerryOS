//! host 钉板（W3-Q140）：boot-params 纯函数面表驱动测试——ktest
//! `test_bootparams`（QEMU 面）同源覆盖在 host 复现 + host-only 钉板
//! （hex8 全域/几何恰界/魔数与 namesize 拒面/循环防护）。语义锚 =
//! kernel 侧原实现（抽离即唯一事实源，本测试钉「抽离未漂移」）。

use berry_bpface::{
    find_in, hex8, intent_code, parse, parse_u32, trim, ByteSource, INTENT_WORDS, NAME, NAME_MAX,
};
use berry_intface::{
    INTENT_BACKGROUND, INTENT_INTERACTIVE, INTENT_LATENCY_CRITICAL, INTENT_MAINTENANCE,
    INTENT_UNSPECIFIED, INTENT_UTILITY,
};

// ---- 测试装备：合成 newc 归档（ktest bp_entry 同构——mkcpio entry()
// 垫 4096 契约镜像）。----

fn hex8_mut(v: usize, out: &mut [u8; 8]) {
    for i in 0..8 {
        let d = (v >> ((7 - i) * 4)) & 0xF;
        out[i] = if d < 10 { b'0' + d as u8 } else { b'A' + d as u8 - 10 };
    }
}

fn entry(name: &[u8], data: &[u8], ino: usize, out: &mut Vec<u8>) {
    let mut e = Vec::new();
    e.extend_from_slice(b"070701");
    let mut f = [0u8; 8];
    for v in [ino, 0o100755, 0, 0, 1, 0, data.len(), 0, 0, 0, 0, name.len() + 1, 0] {
        hex8_mut(v, &mut f);
        e.extend_from_slice(&f);
    }
    e.extend_from_slice(name);
    e.push(0);
    while e.len() % 4096 != 0 {
        e.push(0);
    }
    e.extend_from_slice(data);
    while e.len() % 4096 != 0 {
        e.push(0);
    }
    out.extend_from_slice(&e);
}

const PARAMS_TXT: &[u8] = b"schema = 1\nsched_slice_hz = 250\nintent_default = \"interactive\"\ntimer_slack = 50000000\n";

fn archive() -> Vec<u8> {
    let mut arc = Vec::new();
    entry(b"bin/aa", b"XX", 1, &mut arc);
    entry(NAME, PARAMS_TXT, 2, &mut arc);
    entry(b"zz", b"", 3, &mut arc);
    entry(b"TRAILER!!!", b"", 4, &mut arc);
    arc
}

// ---- hex8：大小写/非 hex/域面 ----

#[test]
fn hex8_roundtrip_both_cases() {
    assert_eq!(hex8(b"00000000"), Some(0));
    assert_eq!(hex8(b"0000000A"), Some(10));
    assert_eq!(hex8(b"0000000a"), Some(10));
    assert_eq!(hex8(b"FFFFFFFF"), Some(0xFFFF_FFFF));
    assert_eq!(hex8(b"ffffffff"), Some(0xFFFF_FFFF));
    assert_eq!(hex8(b"00001000"), Some(4096));
    // mkcpio 域锚：namesize 21（etc/boot-params.toml + NUL）。
    assert_eq!(hex8(b"00000015"), Some(21));
}

#[test]
fn hex8_rejects_non_hex() {
    for bad in [b"g".as_ref(), b"0000000g".as_ref(), b"0000000G".as_ref(), b"/".as_ref(), b" ".as_ref(), b"0000000:".as_ref()] {
        if bad.len() == 8 {
            assert_eq!(hex8(bad), None, "hex8 拒 {:?}", bad);
        }
    }
    // 边界字符恰在闭域外。
    assert_eq!(hex8(b"0000000/"), None); // '0'-1
    assert_eq!(hex8(b"0000000:"), None); // '9'+1
    assert_eq!(hex8(b"0000000`"), None); // 'a'-1
    assert_eq!(hex8(b"0000000g"), None); // 'f'+1
    assert_eq!(hex8(b"0000000@"), None); // 'A'-1
    assert_eq!(hex8(b"0000000G"), None); // 'F'+1
    // 长度面：非 8 恒 None。
    assert_eq!(hex8(b""), None);
    assert_eq!(hex8(b"1234567"), None);
    assert_eq!(hex8(b"123456789"), None);
}

// ---- parse：正例/词表/域执法/schema 门/容忍面/帽 ----

#[test]
fn parse_positive_two_and_three_keys() {
    let ok = parse(b"schema = 1\nintent_default = \"interactive\"\nsched_slice_hz = 250\n");
    assert_eq!(ok.slice_hz, Some(250));
    assert_eq!(ok.intent, Some(INTENT_INTERACTIVE));
    assert_eq!(ok.timer_slack, None);
    assert!(ok.diag.is_none());

    let ok3 = parse(b"schema = 1\nintent_default = \"utility\"\nsched_slice_hz = 64\ntimer_slack = 50000000\n");
    assert_eq!(ok3.slice_hz, Some(64));
    assert_eq!(ok3.intent, Some(INTENT_UTILITY));
    assert_eq!(ok3.timer_slack, Some(50_000_000));
    assert!(ok3.diag.is_none());
}

#[test]
fn parse_intent_six_word_vocabulary() {
    let expect = [
        (b"unspecified".as_ref(), INTENT_UNSPECIFIED),
        (b"latency_critical".as_ref(), INTENT_LATENCY_CRITICAL),
        (b"interactive".as_ref(), INTENT_INTERACTIVE),
        (b"utility".as_ref(), INTENT_UTILITY),
        (b"background".as_ref(), INTENT_BACKGROUND),
        (b"maintenance".as_ref(), INTENT_MAINTENANCE),
    ];
    // 词表常量自锚：六档全在、码恰为 intface 冻结值。
    assert_eq!(INTENT_WORDS.len(), 6);
    for (i, (w, c)) in expect.iter().enumerate() {
        assert_eq!(&INTENT_WORDS[i].0, w, "词表序 {} 词面", i);
        assert_eq!(INTENT_WORDS[i].1, *c, "词表序 {} 码值", i);
        let c2 = format!("schema = 1\nintent_default = \"{}\"\n", String::from_utf8(w.to_vec()).unwrap());
        let o = parse(c2.as_bytes());
        assert_eq!(o.intent, Some(*c), "档 {} 解析", i);
        assert!(o.diag.is_none());
    }
    // 词 → 码 → 词全往返（eco 行反查同源）。
    for (w, c) in INTENT_WORDS {
        assert_eq!(intent_code(&format!("\"{}\"", String::from_utf8(w.to_vec()).unwrap()).into_bytes()), Some(c));
    }
}

#[test]
fn parse_intent_reject_shapes() {
    // 无引号 / 单引号 / 半引号 / 六档外词 / 空串。
    for bad in [
        &b"schema = 1\nintent_default = interactive\n"[..],
        b"schema = 1\nintent_default = 'interactive'\n",
        b"schema = 1\nintent_default = \"interactive\n",
        b"schema = 1\nintent_default = interactive\"\n",
        b"schema = 1\nintent_default = \"turbo\"\n",
        b"schema = 1\nintent_default = \"\"\n",
    ] {
        let o = parse(bad);
        assert_eq!(o.intent, None, "拒 {:?}", String::from_utf8_lossy(bad));
        assert!(o.diag.is_some(), "loud {:?}", String::from_utf8_lossy(bad));
    }
    assert_eq!(intent_code(b"\""), None);
    assert_eq!(intent_code(b"\"\""), None, "引号内空词不在词表");
}

#[test]
fn parse_slice_hz_domain() {
    for good in ["16", "1024"] {
        let c = format!("schema = 1\nsched_slice_hz = {}\n", good);
        let o = parse(c.as_bytes());
        assert_eq!(o.slice_hz, Some(good.parse().unwrap()), "域界 {} 过", good);
        assert!(o.diag.is_none());
    }
    for bad in ["15", "1025", "0", "999999", "abc", "-1", "1e3", "0x10"] {
        let c = format!("schema = 1\nsched_slice_hz = {}\n", bad);
        let o = parse(c.as_bytes());
        assert_eq!(o.slice_hz, None, "坏值 {} 拒", bad);
        assert!(o.diag.is_some(), "坏值 {} loud", bad);
    }
}

#[test]
fn parse_timer_slack_domain() {
    // 0（显式无松弛）/ 50000000 / 上界 999999999 过。
    for good in ["0", "50000000", "999999999"] {
        let c = format!("schema = 1\ntimer_slack = {}\n", good);
        let o = parse(c.as_bytes());
        assert_eq!(o.timer_slack, Some(good.parse().unwrap()), "slack {} 过", good);
        assert!(o.diag.is_none());
    }
    // 越九位（10 亿）落 parse_u32 None → 「range」。
    for bad in ["abc", "-1", "1000000000"] {
        let c = format!("schema = 1\ntimer_slack = {}\n", bad);
        let o = parse(c.as_bytes());
        assert_eq!(o.timer_slack, None, "slack {} 拒", bad);
        assert!(o.diag.is_some(), "slack {} loud", bad);
    }
}

#[test]
fn parse_schema_gate_discards_segment() {
    let no_schema = parse(b"sched_slice_hz = 250\n");
    assert!(no_schema.slice_hz.is_none() && no_schema.diag == Some("schema"));
    let v2 = parse(b"schema = 2\nsched_slice_hz = 250\n");
    assert!(v2.slice_hz.is_none() && v2.diag == Some("schema"));
    // 坏值 schema 同弃（非数字）。
    let bad = parse(b"schema = one\nsched_slice_hz = 250\n");
    assert!(bad.slice_hz.is_none() && bad.diag == Some("schema"));
    // 已拾键后到 schema 拒也全弃（判序锚：schema 终判在循环后）。
    let later = parse(b"sched_slice_hz = 250\nschema = 2\n");
    assert!(later.slice_hz.is_none() && later.diag == Some("schema"));
}

#[test]
fn parse_unknown_key_skips_but_loud() {
    let mixed = parse(
        b"schema = 1\nthermal_cap_c = 1\nsched_slice_hz = 250\nintent_default = \"turbo\"\n",
    );
    assert_eq!(mixed.slice_hz, Some(250), "未知键不连坐");
    assert_eq!(mixed.intent, None, "六档外词须拒");
    assert!(mixed.diag.is_some(), "坏键须 loud");
}

#[test]
fn parse_tolerates_comments_blank_crlf() {
    let messy = parse(b"# oem override\r\nschema = 1\r\nsched_slice_hz = 250\r\n");
    assert_eq!(messy.slice_hz, Some(250));
    assert!(messy.diag.is_none());
    // 空行 + 多注释 + 键内空白。
    let spaced = parse(b"\n\n# a\n# b\n  schema = 1  \n  sched_slice_hz = 64  \n");
    assert_eq!(spaced.slice_hz, Some(64));
    assert!(spaced.diag.is_none());
    // Tab 空白同剥。
    let tabbed = parse(b"\tschema = 1\t\n\tsched_slice_hz = 64\t\n");
    assert_eq!(tabbed.slice_hz, Some(64));
}

#[test]
fn parse_line_caps() {
    // 行数帽 64：超帽弃 + loud（首错 = lines）。
    let mut many = b"schema = 1\nsched_slice_hz = 250\n".to_vec();
    for _ in 0..80 {
        many.push(b'\n');
    }
    let capped = parse(&many);
    assert_eq!(capped.diag, Some("lines"));

    // 恰 64 元素（63 个换行）不触帽——split 尾元素也计行。
    let mut exact = b"schema = 1\nsched_slice_hz = 250\n".to_vec();
    while exact.iter().filter(|&&b| b == b'\n').count() < 63 {
        exact.push(b'\n');
    }
    let ok = parse(&exact);
    assert_eq!(ok.slice_hz, Some(250), "恰 64 行零帽");
    assert!(ok.diag.is_none());

    // 单行超 128B = syntax loud，好键并存不连坐。
    let mut longline = b"schema = 1\nsched_slice_hz = 250\n# ".to_vec();
    for _ in 0..200 {
        longline.push(b'x');
    }
    longline.extend_from_slice(b"\n");
    let o = parse(&longline);
    assert_eq!(o.slice_hz, Some(250), "超长行不连坐好键");
    assert_eq!(o.diag, Some("syntax"));
}

#[test]
fn parse_u32_pure_digit_law() {
    assert_eq!(parse_u32(b"0"), Some(0));
    assert_eq!(parse_u32(b"999999999"), Some(999_999_999));
    assert_eq!(parse_u32(b"1234567890"), None, "十位拒（九位帽）");
    assert_eq!(parse_u32(b""), None);
    assert_eq!(parse_u32(b"12a4"), None);
    assert_eq!(parse_u32(b"+1"), None);
    assert_eq!(parse_u32(b"-1"), None);
    assert_eq!(parse_u32(b"1_0"), None);
    assert_eq!(parse_u32(b"1.0"), None);
    assert_eq!(parse_u32(b" 1"), None, "空白不剥（trim 已在行面）");
}

#[test]
fn parse_repeat_key_last_wins() {
    let rep = parse(b"schema = 1\nsched_slice_hz = 64\nsched_slice_hz = 250\n");
    assert_eq!(rep.slice_hz, Some(250), "后者胜（投放面键序确定性兜底）");
}

#[test]
fn trim_law() {
    assert_eq!(trim(b"  a  "), b"a");
    assert_eq!(trim(b"\ta\t"), b"a");
    assert_eq!(trim(b"  "), b"");
    assert_eq!(trim(b""), b"");
    assert_eq!(trim(b"a\r"), b"a", "尾 CR 剥");
    assert_eq!(trim(b"a \r"), b"a ", "CR 停空白剥离（kernel 同序——不回环）");
    assert_eq!(trim(b"\ra"), b"\ra", "首 CR 不剥（产链不产此形——kernel 侧同形钉板）");
}

// ---- find_in：定址查找状态机 ----

#[test]
fn find_hits_target_with_byte_exact_data() {
    let arc = archive();
    let src = arc.as_slice();
    let full = find_in(src, NAME).expect("命中定址件");
    assert_eq!(full.1, PARAMS_TXT.len(), "filesize 恰数据长");
    let mut back = vec![0u8; full.1];
    assert!(src.read_at(full.0, &mut back));
    assert_eq!(&back[..], PARAMS_TXT, "数据区逐字节对账");
    // 他名不命中。
    assert!(find_in(src, b"etc/other.toml").is_none());
    // 前缀/后缀名不误命中（bin/aa vs bin/a）。
    assert!(find_in(src, b"bin/a").is_none());
    assert!(find_in(src, b"a").is_none());
}

#[test]
fn find_empty_archive_and_trailer_only() {
    assert!(find_in([].as_slice(), NAME).is_none(), "空档 None");
    let mut empty = Vec::new();
    entry(b"TRAILER!!!", b"", 1, &mut empty);
    assert!(find_in(empty.as_slice(), NAME).is_none(), "件缺 None（TRAILER 自终止）");
}

#[test]
fn find_rejects_bad_magic_and_geometry() {
    // 首四字节坏魔数 = 停走 None。
    let mut bad = archive();
    bad[0] = b'9';
    assert!(find_in(bad.as_slice(), NAME).is_none());
    // 070703 拒（第三字节域外——只收 1/2）。
    let mut bad2 = archive();
    bad2[5] = b'3';
    assert!(find_in(bad2.as_slice(), NAME).is_none());
    // 截断（尾部不足一头）= None。
    let arc = archive();
    assert!(find_in(&arc[..50], NAME).is_none());
    assert!(find_in(&arc[..110], NAME).is_none());
    // 敌意巨 filesize（hex 全 F）→ next > total 拒。
    let mut hostile = Vec::new();
    entry(b"a", b"data", 1, &mut hostile);
    // 伪造第二条头：pos 恰在 entry a 的 next（4096*2 处），写魔数 + 全 F filesize。
    let next = 4096 * 2;
    hostile.resize(next + 110 + 21 + 64, 0);
    hostile[next..next + 6].copy_from_slice(b"070701");
    let mut f = [0u8; 8];
    hex8_mut(usize::MAX, &mut f);
    hostile[next + 6..next + 14].copy_from_slice(&f); // filesize = 全 F
    hex8_mut(21, &mut f);
    hostile[next + 6 + 8 * 11..next + 6 + 8 * 11 + 8].copy_from_slice(&f); // namesize = 21
    hostile[next + 110..next + 110 + 21].copy_from_slice(b"etc/boot-params.toml\0");
    assert!(find_in(hostile.as_slice(), NAME).is_none(), "敌意巨 filesize 拒");
}

#[test]
fn find_rejects_bad_namesize() {
    // namesize = 0 → None。
    let mut z = Vec::new();
    z.extend_from_slice(b"070701");
    let mut f = [0u8; 8];
    for v in [1usize, 0o100755, 0, 0, 1, 0, 4, 0, 0, 0, 0, 0, 0] {
        hex8_mut(v, &mut f);
        z.extend_from_slice(&f);
    }
    z.resize(4096, 0);
    z.extend_from_slice(b"data");
    z.resize(8192, 0);
    assert!(find_in(z.as_slice(), b"a").is_none(), "namesize=0 拒");

    // namesize > NAME_MAX（301）→ None。
    let mut big = Vec::new();
    big.extend_from_slice(b"070701");
    for v in [1usize, 0o100755, 0, 0, 1, 0, 4, 0, 0, 0, 0, NAME_MAX + 1, 0] {
        hex8_mut(v, &mut f);
        big.extend_from_slice(&f);
    }
    big.resize(4096, 0);
    big.extend_from_slice(b"data");
    big.resize(8192, 0);
    assert!(find_in(big.as_slice(), b"a").is_none(), "namesize 超帽拒");

    // 名尾非 NUL → None。
    let mut nonul = Vec::new();
    nonul.extend_from_slice(b"070701");
    for v in [1usize, 0o100755, 0, 0, 1, 0, 4, 0, 0, 0, 0, 3, 0] {
        hex8_mut(v, &mut f);
        nonul.extend_from_slice(&f);
    }
    nonul.resize(4096, 0);
    nonul.extend_from_slice(b"abX"); // 第三字节应为 NUL
    nonul.resize(8192, 0);
    assert!(find_in(nonul.as_slice(), b"ab").is_none(), "非 NUL 名尾拒");
}

#[test]
fn find_zero_length_entry_loop_guard() {
    // 零长条目 + 零 namesize 已拒；构造 next==0 需 pos 回卷——垫几何下
    // 不可达，防御面钉「pos==0 → None」常量行为（敌意全零域 + 魔数于
    // pos=0、filesize=0、namesize 最小时 next 至少 8192，不可达 = 钉
    // 防御分支存在性以回归计数器替代：此处钉非 reachable 说明）。
    // —— 用 070702 魔数变体过门（070702 同收）。
    let mut alt = Vec::new();
    alt.extend_from_slice(b"070702");
    let mut f = [0u8; 8];
    for v in [1usize, 0o100755, 0, 0, 1, 0, 2, 0, 0, 0, 0, 3, 0] {
        hex8_mut(v, &mut f);
        alt.extend_from_slice(&f);
    }
    alt.resize(8192, 0);
    alt[110..113].copy_from_slice(b"hi\0");
    // 首条目名非目标 → 继续推进，尾部不足一头 → None。
    assert!(find_in(alt.as_slice(), b"zz").is_none());
    // 070702 魔数合法路径：目标可命中。
    assert_eq!(find_in(alt.as_slice(), b"hi"), Some((4096, 2)), "070702 同收");
}

#[test]
fn geometry_double_pad_law() {
    // 垫 4096 契约：名字段后与数据段后均垫页；跨多条目推进恰落在页界。
    let arc = archive();
    let src = arc.as_slice();
    let (off, len) = find_in(src, NAME).unwrap();
    assert_eq!(off % 4096, 0, "data_off 页对齐（垫几何）");
    assert!(len > 0 && len <= 4096);
    // 逐条目推进核验：三个真条目 + TRAILER 的 next 链全页对齐。
    // （entry() 合成器自身即契约镜像——此测钉「walker 消费同一几何」。）
    for (name, expect_hit) in [(b"bin/aa".as_ref(), true), (b"zz".as_ref(), true), (b"TRAILER!!!".as_ref(), false)] {
        let r = find_in(src, name);
        if expect_hit {
            let (o, l) = r.unwrap();
            assert_eq!(o % 4096, 0);
            if name == b"zz" {
                assert_eq!(l, 0, "空数据条目 filesize=0 合法");
            }
        } else {
            assert!(r.is_none(), "TRAILER 自终止");
        }
    }
}

/// 归一 `&[u8]` 直读臂（lib 已出 `impl ByteSource for &[u8]`；本别名
/// 便于 `&Vec<u8>` 直接传参——kernel `Src::Bytes` 同构）。
fn b<'a>(v: &'a Vec<u8>) -> &'a [u8] {
    v.as_slice()
}
