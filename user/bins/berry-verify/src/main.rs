//! P3 案 c 最小形态——装载期验签**助手进程** berry-verify（rootfs
//! `bin/berry-verify` ELF）。ADR #77；设计母法 = `docs/设备侧验签与账本
//! 落盘设计-v1.0.md` §1.3（装载期执法点）/§1.3.3（验签序）；尺寸墙终裁
//! = research/厂商定制化深研/34 号 §98-2（svcmgr 余量恰 1 页 vs
//! berry-crypto 链入 +1772B 结构性不可行 → 案 c：守卫段驻 svcmgr 只出
//! spawn/wait 面，验签本体独立进程、各享 USER_CODE_MAX 128KiB 帽）。
//!
//! ## 调用合同（与 svcmgr `p3_sig_guard` 成对——改参数面须双侧同改）
//!
//! - 拉起 = `berry_exec::spawn_rootfs_elf`（hello 同链）；数据面 = env 块
//!   **gift[4] = rootfs VMO READ dup**（VMO 传数据的最小侵入形：bap/
//!   .sig/pk 全在 rootfs、零拷贝、svcmgr 不产第二数据面）+ argv 五个
//!   **hex8 定宽**几何：`<bap_off> <bap_len> <sig_off> <sig_len> <pk_off>`
//!   （argv_blob 192B 内恰容：argv0 16B + 5×9B = 61B）。
//! - `etc/trust-pk`（rootfs 32B raw 公钥）= 可信锚。**v1 测试场景件**——
//!   正式形 = 设计 §1.4 bootseed `etc/trust-ledger.toml`（账本 TOML 解析
//!   + 水位/TOFU 随 P3 完整策略刀，本件不预坐）。
//! - 语义：`admit::verify_signed` 验签序五步（lib 单源——P3-0 上移件）
//!   → **exit 0 = 过 / 1 = 拒（验签负）/ 2 = 用法·IO 面**（几何坏/读败/
//!   manifest 缺 [security]）。裁决不在此——dev 档 v1 失败 = 降级 + eco
//!   loud 不拒载（§1.3.2 表 dev 行，svcmgr 侧执法；product-locked 拒载
//!   随策略档刀）。
//!
//! ## 零双解析器纪律
//!
//! - manifest 语义单源 = berry-manifest（本 crate 零第二份 manifest 判断）；
//! - .bap 内层 cpio(newc) 步进单源 = berry-loader `cpio_step`（svcmgr
//!   `bap_scan` 同链——雷册 §A139 消费者清单纪律）；
//! - manifest 关键域字节（BSIG-MF 覆盖域）= berry-sign `key_fields_bytes`
//!   （main.rs）的**设备侧镜像**——派生点按设计归调用方 bin（admit.rs
//!   头注「manifest 语义不进 lib」），两处同改义务互指；漂移由
//!   sign→verify E2E 咬（berry-sign 产 .sig 本件验不过即拒——QEMU 真签
//!   场景 = 该对账的运行期执法）。
//!
//! 测试钥纪律：host 单测钥 = RFC 8032 §7.1 公开测试向量（永不真签——
//! 真签面 = 平台签发随 #77/P3）。
//!
//! host target（aarch64-apple-darwin）只编单测（`cargo test -p
//! berry-verify --target <host>`——justfile test-host 挂行随主会话收口，
//! berry-crypto 同款先例）；none target = 设备本体（std 门控手法 =
//! berry-crypto main.rs 同款）。
//! 尺寸终裁(主会话 2026-11-11):红线 10KB→32KB 重钉,现役 26729B 接受——128KiB 帽内余量 4 倍;瘦解析面(破文件域)另刀再议。

// none target（user workspace 默认 target）下 no_std/no_main；host 侧
// （单测）放开 std——实现体对两 target 同源（读面经 trait 抽象）。
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
// none 空壳无入口符号警告豁免——本件 none 下有真 _start，host 测试构建
// 由 test harness 供 main（ berry-crypto main.rs 同款手法）。
#![cfg_attr(target_os = "none", allow(linker_messages))]

use berry_crypto::admit::verify_signed;
use berry_crypto::envelope::{ENVELOPE_MAX, PK_LEN};
use berry_crypto::ledger::{Anchor, AnchorType};
use berry_crypto::sha512::{digest256, Sha512};
use berry_manifest::{BAP_MAX, MANIFEST_MAX};

// ---- 共通纯面（argv 解析 + 关键域派生 + 验签链——host 单测同源盖） ----

/// argv 定宽 hex8 解析（svcmgr `p3_sig_guard` 产侧镜像——双侧同改）。
fn hex8(s: &[u8]) -> Option<u64> {
    if s.len() != 8 {
        return None;
    }
    let mut v = 0u64;
    for &b in s {
        let d = match b {
            b'0'..=b'9' => b - b'0',
            b'a'..=b'f' => b - b'a' + 10,
            _ => return None,
        };
        v = (v << 4) | d as u64;
    }
    Some(v)
}

/// 装载面几何（argv[1..=5]——见模块头调用合同）。
struct Geom {
    bap_off: u64,
    bap_len: u64,
    sig_off: u64,
    sig_len: u64,
    pk_off: u64,
}

/// env 块 argv blob → 几何。argv0（cpio 名）跳过；恰 5×8 hex，多 token
/// 容忍（产侧单调截断契约下不出现，防御面不拒）。
fn parse_argv(blob: &[u8]) -> Option<Geom> {
    let mut it = blob.split(|&b| b == 0);
    it.next()?; // argv0
    Some(Geom {
        bap_off: hex8(it.next()?)?,
        bap_len: hex8(it.next()?)?,
        sig_off: hex8(it.next()?)?,
        sig_len: hex8(it.next()?)?,
        pk_off: hex8(it.next()?)?,
    })
}

/// 读面抽象：none = rootfs VMO gift（`berry_vmo_read`）；host 测试 =
/// 内存镜像。验签链对读面单源（syscall 面不进纯逻辑）。
trait Face {
    /// 读 [off, off+buf.len()) 进 buf；false = 读败（越界/IO）。
    fn read_at(&mut self, off: u64, buf: &mut [u8]) -> bool;
}

/// u8 长度前缀字符串追加（berry-sign `push_lstr` 镜像——同改义务见
/// 模块头）。
fn push_lstr(out: &mut [u8], n: &mut usize, s: &[u8]) -> Option<()> {
    if s.len() > 255 || out.len() < *n + 1 + s.len() {
        return None;
    }
    out[*n] = s.len() as u8;
    out[*n + 1..*n + 1 + s.len()].copy_from_slice(s);
    *n += 1 + s.len();
    Some(())
}

/// u16 十进制**变宽**发射（无前导零——`format!("{{}}")` Display 同文法）。
fn push_u16_dec(out: &mut [u8], n: &mut usize, v: u16) -> Option<()> {
    let mut b = [0u8; 5]; // u16 ≤ 65535 → ≤5 位
    let mut i = b.len();
    let mut x = v;
    loop {
        i -= 1;
        b[i] = b'0' + (x % 10) as u8;
        x /= 10;
        if x == 0 {
            break;
        }
    }
    if out.len() < *n + (b.len() - i) {
        return None;
    }
    out[*n..*n + (b.len() - i)].copy_from_slice(&b[i..]);
    *n += b.len() - i;
    Some(())
}

/// SdkVer → `maj.min.pat`（**变宽 + 点分隔**——berry-sign `ver_str` 的
/// no-fmt 镜像；漏点/定宽都会漂关键域字节 = 验签必拒，勿改。首轮 E2E
/// 实咬教训：唯 [compat] 在场的件（APPDEMO）拒、其余 8 件过——漏点
/// 只影响带版本串的件）。
fn push_ver(out: &mut [u8], n: &mut usize, v: &berry_manifest::SdkVer) -> Option<()> {    push_u16_dec(out, n, v.major)?;
    if out.len() < *n + 1 {
        return None;
    }
    out[*n] = b'.';
    *n += 1;
    push_u16_dec(out, n, v.minor)?;
    if out.len() < *n + 1 {
        return None;
    }
    out[*n] = b'.';
    *n += 1;
    push_u16_dec(out, n, v.patch)
}

/// manifest 关键域字节：`"BSIG-MF"` ‖ L‖id ‖ L‖version ‖ compat 标志
/// （0x00 缺席 / 0x01 在场‖L‖min‖L‖target‖L‖compile）‖ L‖profile ‖
/// L‖key_lineage ‖ L‖integrator——**berry-sign `key_fields_bytes`
/// （main.rs）的设备侧镜像**（24 号 §3.4-②；同改义务互指，模块头）。
/// 无 [security] = None（调用方按拒处理——空 lineage 恒回声拒同义）。
fn key_fields(m: &berry_manifest::Manifest, out: &mut [u8]) -> Option<usize> {
    let sec = m.security?;
    if out.len() < 7 {
        return None;
    }
    out[..7].copy_from_slice(b"BSIG-MF");
    let mut n = 7usize;
    push_lstr(out, &mut n, m.id.as_bytes())?;
    push_lstr(out, &mut n, m.version.as_bytes())?;
    match &m.compat {
        None => {
            if out.len() < n + 1 {
                return None;
            }
            out[n] = 0x00;
            n += 1;
        }
        Some(c) => {
            if out.len() < n + 1 {
                return None;
            }
            out[n] = 0x01;
            n += 1;
            for v in [&c.min, &c.target, &c.compile] {
                // 每版本串也是 lstr（berry-sign `push_lstr(ver_str(v))`
                // 镜像——向量钉板 key_fields_vec 执法：长度前缀不可缺）。
                let mut vb = [0u8; 17]; // "65535.65535.65535" 上界
                let mut vn = 0usize;
                push_ver(&mut vb, &mut vn, v)?;
                push_lstr(out, &mut n, &vb[..vn])?;
            }
        }
    }
    push_lstr(out, &mut n, sec.profile.as_bytes())?;
    push_lstr(out, &mut n, sec.key_lineage.as_bytes())?;
    push_lstr(out, &mut n, sec.integrator.as_bytes())?;
    Some(n)
}

/// 验签主链（几何 → pk 锚 → .sig → bap 全字节流式摘要 + manifest 走查
/// → 关键域摘要 → `admit::verify_signed` 五步序）。返回 false = 拒
/// （拒因分族归 lib Reason，本件出口码不细分——dev 档 v1 词面统一
/// `sig bad`，诊断细分随策略档刀）。
fn verify_chain(face: &mut dyn Face, g: &Geom) -> bool {
    if g.bap_len == 0 || g.bap_len > BAP_MAX as u64 || g.sig_len == 0 || g.sig_len as usize > ENVELOPE_MAX
    {
        return false; // 几何越域（用法面）
    }
    let mut pk = [0u8; PK_LEN];
    if !face.read_at(g.pk_off, &mut pk) {
        return false;
    }
    let mut sig = [0u8; ENVELOPE_MAX];
    if !face.read_at(g.sig_off, &mut sig[..g.sig_len as usize]) {
        return false;
    }
    // bap 全字节摘要：流式 1KiB 窗（BAP_MAX 4MiB 不整档驻留——设计
    // §1.3.1「装载期二次流式读」）。两遍走查（摘要线性一遍 + manifest
    // 步进一遍）防「跳跃步进漏摘要数据字节」面（cpio_step 的 next_off
    // 跳过数据区——单遍合流会漏 digested 区间）。
    let mut chunk = [0u8; 1024];
    let mut h = Sha512::new_256();
    let mut pos = 0u64;
    while pos < g.bap_len {
        let want = ((g.bap_len - pos).min(chunk.len() as u64)) as usize;
        if !face.read_at(g.bap_off + pos, &mut chunk[..want]) {
            return false;
        }
        h.update(&chunk[..want]);
        pos += want as u64;
    }
    let mut full = [0u8; 64];
    h.finalize(&mut full);
    let mut bd = [0u8; 32];
    bd.copy_from_slice(&full[..32]);
    // manifest 走查（cpio_step 单源——svcmgr bap_scan 同链，只找
    // manifest.toml；重复/超帽 = 拒，bap_scan「bap shape」同义）。
    let mut mf = [0u8; MANIFEST_MAX];
    let mut mf_len = 0usize;
    let mut got = false;
    let mut pos = 0u64;
    while pos < g.bap_len {
        let want = ((g.bap_len - pos).min(chunk.len() as u64)) as usize;
        if !face.read_at(g.bap_off + pos, &mut chunk[..want]) {
            return false;
        }
        match berry_loader::cpio_step(&chunk[..want], pos, g.bap_len) {
            Ok(berry_loader::StepOutcome::End) => break,
            Ok(berry_loader::StepOutcome::Entry(s)) => {
                if s.name == "manifest.toml" {
                    if got || s.data_len as usize > mf.len() {
                        return false;
                    }
                    if !face.read_at(g.bap_off + s.data_off, &mut mf[..s.data_len as usize]) {
                        return false;
                    }
                    mf_len = s.data_len as usize;
                    got = true;
                }
                pos = s.next_off;
            }
            Err(_) => return false,
        }
    }
    if !got {
        return false;
    }
    let Ok(man) = berry_manifest::parse(&mf[..mf_len]) else {
        return false;
    };
    let Some(sec) = man.security else {
        return false; // 无 [security]：无 lineage 可回声对账（恒拒面）
    };
    let mut kf = [0u8; 192]; // ≤ 7+2×25+1+3×13+3×17 ≈ 130B（值域执法下）
    let Some(kn) = key_fields(&man, &mut kf) else {
        return false;
    };
    let md = digest256(&kf[..kn]);
    let anchors = [Anchor { pk, ty: AnchorType::System }];
    verify_signed(&anchors, sec.key_lineage.as_bytes(), &bd, &md, &sig[..g.sig_len as usize]).is_ok()
}

// ---- 设备本体（none target）：env 块收面 + rootfs 读面 ----

#[cfg(target_os = "none")]
mod dev {
    use super::{parse_argv, verify_chain, Face};
    use berry_sys::{berry_channel_recv, berry_process_exit, berry_vmo_read, Handle};

    /// rootfs VMO 读面（env gift[4]——守卫 dup 的 READ 别名）。
    struct Rootfs(Handle);

    impl Face for Rootfs {
        fn read_at(&mut self, off: u64, buf: &mut [u8]) -> bool {
            berry_vmo_read(self.0, buf.as_mut_ptr(), off, buf.len()) == 0
        }
    }

    /// 入口 x0 = env 通道端（d-3a processargs v1——hello 同链）。gift 布局
    /// = spawn_rootfs_elf 契约前缀 [stdout, stdin(dummy feeder 端),
    /// self_proc, self_vmar, extras…]——extras[0]（= gift[4]）= rootfs
    /// dup。收窗容量 8 ≥ 5（chan 随行帽 6 + 富余——collect_eof 同口径）。
    #[link_section = ".text.entry"]
    #[no_mangle]
    pub extern "C" fn _start(env: Handle) -> ! {
        let mut blob = [0u8; 192];
        let mut nb = blob.len();
        let mut hs = [0 as Handle; 8];
        let mut nh = hs.len();
        if berry_channel_recv(env, blob.as_mut_ptr(), &mut nb, hs.as_mut_ptr(), &mut nh) != 0
            || nh < 5
        {
            berry_process_exit(2); // env 块坏/缺 rootfs gift（用法面）
        }
        let Some(g) = parse_argv(&blob[..nb]) else {
            berry_process_exit(2);
        };
        let mut face = Rootfs(hs[4]);
        if verify_chain(&mut face, &g) {
            berry_process_exit(0);
        }
        berry_process_exit(1);
    }
}

// none 面的 no_std 义务（宿主实编不携）。
#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

// 宿主误跑面（真宿主出口 = 单测 harness，不走此 main）。
#[cfg(not(target_os = "none"))]
fn main() -> std::process::ExitCode {
    eprintln!(
        "berry-verify: 设备侧验签助手（rootfs ELF）——宿主面仅单测：cargo test -p berry-verify --target <host>"
    );
    std::process::ExitCode::from(2)
}

// ---- host 单测（argv 解析 + 验签链纯函数面；测试钥 = RFC 8032 公开向量）----

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 8032 §7.1 TEST 1 官方密钥（公开测试向量——永不真签）。
    const SK_HEX: &str = "9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60";
    const PK_HEX: &str = "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";

    fn unhex32(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, b) in out.iter_mut().enumerate() {
            *b = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).unwrap();
        }
        out
    }

    // ---- argv 解析 ----

    fn blob_for(geom: &[u64; 5]) -> Vec<u8> {
        let mut b = b"bin/berry-verify\0".to_vec();
        for g in geom {
            b.extend_from_slice(format!("{g:08x}").as_bytes());
            b.push(0);
        }
        b
    }

    #[test]
    fn argv_pos() {
        let g = parse_argv(&blob_for(&[0x11, 0x40000, 0xdeadbeef, 120, 0x20])).unwrap();
        assert_eq!((g.bap_off, g.bap_len, g.sig_off, g.sig_len, g.pk_off),
            (0x11, 0x40000, 0xdeadbeef, 120, 0x20));
    }

    /// 关键域字节向量钉板（**点分隔回归锚**——首轮 E2E 实咬：push_ver 漏
    /// 点 → 唯 [compat] 件拒；此向量 = berry-sign `key_fields_bytes` 对
    /// 同 manifest 的逐字节镜像，漂即红）。
    #[test]
    fn key_fields_vec() {
        let m = berry_manifest::parse(MF).unwrap();
        let mut kf = [0u8; 192];
        let n = key_fields(&m, &mut kf).unwrap();
        // 期望 = "BSIG-MF" ‖L‖id ‖L‖ver ‖01 ‖L‖0.1.0×3 ‖L‖profile ‖L‖lineage ‖L‖integrator
        let mut want = Vec::new();
        want.extend_from_slice(b"BSIG-MF");
        for part in [
            m.id.as_bytes(),
            m.version.as_bytes(),
            &[0x01u8][..],
            b"0.1.0",
            b"0.1.0",
            b"0.1.0",
            b"ed25519-main",
            b"p3-verify",
            b"berryos-dev",
        ] {
            if part.len() == 1 && part[0] == 0x01 {
                want.push(0x01); // compat 标志位非 lstr
            } else {
                want.push(part.len() as u8);
                want.extend_from_slice(part);
            }
        }
        assert_eq!(&kf[..n], &want[..], "关键域字节镜像漂移（点分隔/长度前缀）");
    }

    #[test]
    fn argv_neg() {
        // 缺项（4 token）：完整 blob 产线裁尾。
        let mut b = b"bin/berry-verify\0".to_vec();
        for g in &[1u64, 2, 3, 4] {
            b.extend_from_slice(format!("{g:08x}").as_bytes());
            b.push(0);
        }
        assert!(parse_argv(&b).is_none());
        let mut b = b"bin/berry-verify\0abc\0".to_vec();
        assert!(parse_argv(&b).is_none());
        b = b"bin/berry-verify\0ABCDEF01\0".to_vec();
        assert!(parse_argv(&b).is_none());
        b = b"bin/berry-verify\0zzzzzzzz\0".to_vec();
        assert!(parse_argv(&b).is_none());
        assert!(parse_argv(b"\0").is_none());
    }

    // ---- 验签链（正/篡改/换锚/截断四件——berry-sign selftest 同族）----

    /// 测试 manifest（[security] 三键 + [compat] 三值——关键域全覆盖）。
    const MF: &[u8] = b"[app]\n\
                    id = \"P3VERIFY\"\n\
                    version = \"0.1.0\"\n\
                    entry = \"bin/p3verify\"\n\
                    type = \"app\"\n\
                    intent = \"utility\"\n\
                    \n[compat]\n\
                    min = \"0.1.0\"\n\
                    target = \"0.1.0\"\n\
                    compile = \"0.1.0\"\n\
                    \n[capabilities]\n\
                    console = true\n\
                    \n[security]\n\
                    profile = \"ed25519-main\"\n\
                    key_lineage = \"p3-verify\"\n\
                    integrator = \"berryos-dev\"\n";

    /// 单条目 manifest.toml cpio(newc) 容器（**测试夹具写侧**——几何 =
    /// 头 110B + 名垫 4096 + 数据垫 4096，berry-sign selftest `cpio_pack`
    /// 同款对拍件；§A139 消费面注记同款）。
    fn cpio_pack(payload: &[u8]) -> Vec<u8> {
        let name = b"manifest.toml\0";
        let mut out = Vec::new();
        out.extend_from_slice(b"070701");
        for i in 0..13 {
            let v = if i == 6 { payload.len() } else if i == 11 { name.len() } else { 0 };
            out.extend_from_slice(format!("{v:08x}").as_bytes());
        }
        out.extend_from_slice(name);
        while out.len() % 4096 != 0 {
            out.push(0);
        }
        out.extend_from_slice(payload);
        while out.len() % 4096 != 0 {
            out.push(0);
        }
        out
    }

    /// 信封构造（berry-sign `sign_envelope` 的测试镜像——关键域/lineage
    /// 经本 crate `key_fields` 单源派生，正样例即镜像对拍）。
    fn sign_envelope(sk: &[u8; 32], bap: &[u8], gen: u32) -> Vec<u8> {
        use berry_crypto::ed25519;
        use berry_crypto::envelope::{
            top_message, ChainNode, SigEnvelopeV1, ALG_ED25519_SHA512_256, CHAIN_NODES_MAX,
            LINEAGE_MAX, SIG_LEN, TOP_MSG_MAX,
        };
        let man = berry_manifest::parse(MF).unwrap();
        let sec = man.security.unwrap();
        let lineage = sec.key_lineage.as_bytes();
        let mut kf = [0u8; 192];
        let kn = key_fields(&man, &mut kf).unwrap();
        let md = digest256(&kf[..kn]);
        let bd = digest256(bap);
        let pk = ed25519::public_key(sk);
        let mut env = SigEnvelopeV1 {
            alg_id: ALG_ED25519_SHA512_256,
            lineage: [0; LINEAGE_MAX],
            lineage_len: lineage.len(),
            gen,
            pk,
            pending: None,
            chain: [ChainNode { new_pk: [0; 32], rot_sig: [0; SIG_LEN] }; CHAIN_NODES_MAX],
            chain_len: 0,
            top_sig: [0; SIG_LEN],
        };
        env.lineage[..lineage.len()].copy_from_slice(lineage);
        let mut buf = [0u8; ENVELOPE_MAX];
        let n = env.encode(&mut buf).unwrap();
        let mut msg = [0u8; TOP_MSG_MAX];
        let mn = top_message(&buf[..n - SIG_LEN], &bd, &md, &mut msg).unwrap();
        env.top_sig = ed25519::sign(sk, &msg[..mn]);
        let n2 = env.encode(&mut buf).unwrap();
        buf[..n2].to_vec()
    }

    /// 内存读面（[bap ‖ sig ‖ pk] 平铺——几何按布局派生）。
    struct Mem(Vec<u8>);

    impl Face for Mem {
        fn read_at(&mut self, off: u64, buf: &mut [u8]) -> bool {
            let o = off as usize;
            match o.checked_add(buf.len()) {
                Some(e) if e <= self.0.len() => {
                    buf.copy_from_slice(&self.0[o..e]);
                    true
                }
                _ => false,
            }
        }
    }

    /// 布景：bap + sig + pk 平铺 → (Mem, Geom)。
    fn stage(bap: &[u8], sig: &[u8], pk: &[u8; 32]) -> (Mem, Geom) {
        let mut m = Vec::new();
        let bap_off = 0u64;
        m.extend_from_slice(bap);
        let sig_off = m.len() as u64;
        m.extend_from_slice(sig);
        let pk_off = m.len() as u64;
        m.extend_from_slice(pk);
        (
            Mem(m),
            Geom {
                bap_off,
                bap_len: bap.len() as u64,
                sig_off,
                sig_len: sig.len() as u64,
                pk_off,
            },
        )
    }

    #[test]
    fn chain_pos() {
        let sk = unhex32(SK_HEX);
        let pk = unhex32(PK_HEX);
        let bap = cpio_pack(MF);
        let sig = sign_envelope(&sk, &bap, 1);
        let (mut face, g) = stage(&bap, &sig, &pk);
        assert!(verify_chain(&mut face, &g), "正样例：签→验链全过（含关键域镜像对拍）");
    }

    #[test]
    fn chain_neg_tamper() {
        let sk = unhex32(SK_HEX);
        let pk = unhex32(PK_HEX);
        let sig = sign_envelope(&sk, &cpio_pack(MF), 1);
        let mut bap = cpio_pack(MF);
        bap[100] ^= 0x01; // 载荷改一字节（BSIG-TOP 覆盖 bap 全字节摘要）
        let (mut face, g) = stage(&bap, &sig, &pk);
        assert!(!verify_chain(&mut face, &g), "篡改载荷必拒");
    }

    #[test]
    fn chain_neg_wrong_pk() {
        let sk = unhex32(SK_HEX);
        let mut wrong = unhex32(PK_HEX);
        wrong[0] ^= 0x01; // 锚不见证信封钥（KeyMismatch 面）
        let bap = cpio_pack(MF);
        let sig = sign_envelope(&sk, &bap, 1);
        let (mut face, g) = stage(&bap, &sig, &wrong);
        assert!(!verify_chain(&mut face, &g), "换锚必拒");
    }

    #[test]
    fn chain_neg_trunc_sig() {
        let sk = unhex32(SK_HEX);
        let pk = unhex32(PK_HEX);
        let bap = cpio_pack(MF);
        let sig = sign_envelope(&sk, &bap, 1);
        let (mut face, g) = stage(&bap, &sig[..sig.len() - 1], &pk);
        assert!(!verify_chain(&mut face, &g), ".sig 截断必拒（信封 BadLen 单一解释面）");
    }

    #[test]
    fn chain_neg_geom() {
        let sk = unhex32(SK_HEX);
        let pk = unhex32(PK_HEX);
        let bap = cpio_pack(MF);
        let sig = sign_envelope(&sk, &bap, 1);
        // 读面越界（pk_off 越档）与 bap_len 越帽（BAP_MAX）两用法面。
        let (mut face, mut g) = stage(&bap, &sig, &pk);
        g.pk_off = u32::MAX as u64;
        assert!(!verify_chain(&mut face, &g));
        let (mut face, mut g) = stage(&bap, &sig, &pk);
        g.bap_len = BAP_MAX as u64 + 1;
        assert!(!verify_chain(&mut face, &g));
    }

    /// 大数据面（设备实测量级）：380KB 级 .bap——流式摘要 372+ 窗 + 跨窗
    /// manifest 走查（QEMU APPDEMO 件同量级；E2E 首轮 APPDEMO 单件异常的
    /// 排除面——大窗跳步/摘要分片在此纯函数面钉板）。
    #[test]
    fn chain_pos_large_bap() {
        let sk = unhex32(SK_HEX);
        let pk = unhex32(PK_HEX);
        let mut bap = cpio_pack(MF);
        // manifest 条目后垫 ~380KB 填充（cpio_pad 条目：bin/pad 名几何同构）。
        let pad = vec![0xA5u8; 380 * 1024];
        let name = b"bin/pad\0";
        bap.extend_from_slice(b"070701");
        for i in 0..13 {
            let v = if i == 6 { pad.len() } else if i == 11 { name.len() } else { 0 };
            bap.extend_from_slice(format!("{v:08x}").as_bytes());
        }
        bap.extend_from_slice(name);
        while bap.len() % 4096 != 0 {
            bap.push(0);
        }
        bap.extend_from_slice(&pad);
        while bap.len() % 4096 != 0 {
            bap.push(0);
        }
        assert!(bap.len() > 380 * 1024);
        let sig = sign_envelope(&sk, &bap, 1);
        let (mut face, g) = stage(&bap, &sig, &pk);
        assert!(verify_chain(&mut face, &g), "380KB 级流式摘要 + 跨窗走查全过");
    }

    /// 真件 E2E（env 门控——构建树易失面不进默认测试面）：
    /// `BERRY_VERIFY_E2E_DIR=<dir>` 内 `<ID>.bap`/`<ID>.bap.sig`/`trust-pk`
    /// 逐件过链（sign-user 产线对账——QEMU APPDEMO 异常的宿主复现面）。
    #[test]
    fn chain_e2e_real_artifacts() {
        let Ok(dir) = std::env::var("BERRY_VERIFY_E2E_DIR") else {
            return; // 缺席 = 跳过（默认测试面零构建树依赖）
        };
        let pk: [u8; 32] = std::fs::read(format!("{dir}/trust-pk"))
            .expect("E2E: trust-pk 读败")
            .try_into()
            .expect("E2E: trust-pk 非 32B");
        let mut n = 0;
        for ent in std::fs::read_dir(&dir).expect("E2E: 目录读败") {
            let p = ent.unwrap().path();
            let Some(ext) = p.extension() else { continue };
            if ext != "bap" || !p.file_name().unwrap().to_str().unwrap().ends_with(".bap") {
                continue;
            }
            let bap = std::fs::read(&p).unwrap();
            let Ok(sig) = std::fs::read(format!("{}.sig", p.display())) else {
                continue; // selftest 夹具面（APPTOMB 无 [security] 被 sign 正拒——跳过）
            };
            let (mut face, g) = stage(&bap, &sig, &pk);
            assert!(
                verify_chain(&mut face, &g),
                "E2E: {} 验签拒（berry-sign 面应过——现场对账）",
                p.display()
            );
            n += 1;
        }
        assert!(n > 0, "E2E: 目录内零 .bap 件");
    }
}
