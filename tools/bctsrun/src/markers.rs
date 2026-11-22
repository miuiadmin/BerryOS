//! bctsrun 九刀：步④ 标记对拍 + 步⑤ 结果包文本面镜像（现役单源 =
//! `tools/bcts-run.sh` :655-660 `assert_marker` + :756-776 对账循环 +
//! :797-840 结果包 echo 块——彻底 Rust 化轨道 b 层 #7 刀序末档九刀）。
//!
//! 纯面拆分原则：QEMU 回放（步④镜像腿/标准流 boot）与文件 IO 不在此刀
//! ——`assert_marker` 的输入是**已采到的语料**（`$CORPUS` 文件内容），
//! 对拍只看「语料里有没有」，故语料读入后即纯函数；结果包 echo 块只拼
//! 字符串，落盘由调用方做。两腿 oracle 对拍均用合成语料驱动 bash 原段
//! （循环体逐字提取——八刀先例），零 QEMU 零真回放。
//!
//! 行为面逐字镜像：
//! - `assert_marker "$1"`：`DEMO ok` 前邻非 `I`（防 `IDEMO ok` 子串假阳性
//!   ——smoke.sh assert_marker 注）；`TAB ok` 前邻非 `G`（防 `GTAB ok`）；
//!   其余 `grep -qF` 定串；三形均**行内任意位置命中即过**（`^|[^I]` = 行首
//!   或非 I 字符后随——ERE 同形）；
//! - 对账循环：逐 `MARK` 行，镜像腿（`image = Some`）只回放 `bare` 相，
//!   其余 `continue`；过 = `PASS_N + 1`，缺 = `MISSING` 空格连接 id；
//!   `FACE_N` = 镜像腿 `N_BARE` / 标准流 `N_MARKERS`；
//! - verdict：`MISSING` 空 = PASS 否则 FAIL；判定式行两形态（镜像腿带
//!   〔镜像腿〕后缀）；
//! - 结果包文本：`mode`/`artifact`（live 恒 live 行；frozen 带 sha256 行）
//!   /frozen 四键（platform_revision/frozen_at/abi_snapshot/abi_snapshot_hash
//!   自 META 域）/boot 面双形态/`host_gates`/`smoke`·`image_boot` 行/
//!   `verdict`/`markers`/`serial_logs`——行序与字面逐字镜像（awk print
//!   两行 → 同序两行；`printf '%02d'` → `{n:02}`）。
//!
//! 钉序发现：bash `MISSING="${MISSING:+$MISSING }$mid"` 空格分隔首项无
//! 前导——`join(" ")` 同形；`markers = ${PASS_N}/${FACE_N} 过；missing =
//! ${MISSING:-（无）}` 全角分号 + 全角括号「（无）」是字面。

use crate::load::LoadOutcome;

/// `assert_marker` 单条判定（语料 = 已读入文本；grep -F/-E 行内任意位置）。
///
/// 语料末尾无换行的尾行视同一条完整行（grep 同形：尾行照匹配）。
pub fn assert_marker(expect: &str, corpus: &str) -> bool {
    match expect {
        "DEMO ok" => corpus.lines().any(|l| prev_char_not(l, "DEMO ok", 'I')),
        "TAB ok" => corpus.lines().any(|l| prev_char_not(l, "TAB ok", 'G')),
        // 空模式（grep -F ""）：每行恒匹配——空文件零行无匹配（rc 1）。发射器
        // 不可达（expect 非门前置）；纯函数面按 grep 真值镜像。
        "" => corpus.lines().next().is_some(),
        _ => corpus.contains(expect),
    }
}

/// `(^|[^X])needle` ERE 语义：needle 出现在行首，或出现处前一字符 ≠ X。
fn prev_char_not(line: &str, needle: &str, x: char) -> bool {
    let b = line.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || b.len() < n.len() {
        return false;
    }
    for i in 0..=(b.len() - n.len()) {
        if &b[i..i + n.len()] == n {
            match b.get(i.wrapping_sub(1)) {
                // i = 0：行首命中（`^` 分支）。
                None => return true,
                Some(&p) => {
                    // 前邻字节须构成完整字符且 ≠ X：needle 为 ASCII，X 为
                    // ASCII 单字节，非 ASCII 前邻（多字节 UTF-8 序列的尾
                    // 字节 ≥0x80）必然 ≠ X → 过（ERE `[^I]` 在 UTF-8 locale
                    // 下按字符比较，非 ASCII 字符同样满足「非 I」）。
                    if p != x as u8 {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// 步④对账出口（对账循环变量面）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReconcileOutcome {
    /// 过件数（`PASS_N`）。
    pub pass_n: usize,
    /// 缺件 id 空格连接串（`MISSING`；全过 = 空串）。
    pub missing: String,
    /// 判定面分母（`FACE_N`：镜像腿 = `n_bare`；标准流 = `n_markers`）。
    pub face_n: usize,
    /// verdict 字面：`"PASS"` / `"FAIL"`。
    pub verdict: &'static str,
}

/// 步④对账循环（:756-776）。
///
/// `image` = 镜像腿标志（`-n "$IMAGE_FILE"`）：只回放 `bare` 相；语料 =
/// `$CORPUS` 全文。MARK 行短行（域数 < 4）跳过——bash `read -r tag phase
/// mid expect` 四域，短行 mid/expect 为空：expect 空 → `grep -qF ""` 恒过
/// （空模式匹配任意行）——镜像同形严拒：空 expect 判为**不过**并记缺，
/// 与 t1_check/markers.sh「expect 非空」前置不变量一致（发射器不可达，
/// 合成语料面钉板诚实处理）。
pub fn reconcile(o: &LoadOutcome, corpus: &str, image: bool) -> ReconcileOutcome {
    let mut pass_n = 0usize;
    let mut missing: Vec<&str> = Vec::new();
    for e in &o.entries {
        let f: Vec<&str> = e.split('\t').collect();
        if f.first() != Some(&"MARK") {
            continue;
        }
        let phase = f.get(1).copied().unwrap_or("");
        let mid = f.get(2).copied().unwrap_or("");
        let expect = f.get(3).copied().unwrap_or("");
        if image && phase != "bare" {
            continue;
        }
        if !expect.is_empty() && assert_marker(expect, corpus) {
            pass_n += 1;
        } else {
            missing.push(mid);
        }
    }
    let face_n = if image { o.n_bare } else { o.n_markers };
    let verdict = if missing.is_empty() { "PASS" } else { "FAIL" };
    ReconcileOutcome {
        pass_n,
        missing: missing.join(" "),
        face_n,
        verdict,
    }
}

/// 判定式行（`VERDICT_LINE`，:805-809）。
pub fn verdict_line(verdict: &str, image: bool) -> String {
    if image {
        format!("{verdict}（判定式 = 观测标记 ⊇ bare 相清单〔镜像腿〕，一条不少；多打不算 fail）")
    } else {
        format!("{verdict}（判定式 = 观测标记 ⊇ 清单全集，一条不少；多打不算 fail）")
    }
}

/// 结果包输入（步⑤ echo 块消费面；全部已解析值——零 IO）。
#[derive(Debug, Clone, PartialEq)]
pub struct ResultInput<'a> {
    pub mode: &'a str,
    /// live 恒 `None`；frozen = (冻结件路径, sha256 hex)。
    pub frozen: Option<(&'a str, &'a str)>,
    /// META 四域（frozen 态发射；live 态 None = 不发射四键）。
    pub meta: Option<(&'a str, &'a str, &'a str, &'a str)>,
    /// 镜像腿：Some((镜像路径, 镜像 sha256, IMG_PR, IMG_PR_ANCHOR, N_BARE))。
    pub image: Option<(&'a str, &'a str, &'a str, &'a str, usize)>,
    /// 标准流：(内核路径, 内核 sha256)。
    pub kernel: Option<(&'a str, &'a str)>,
    /// `GATE_SUMMARY`（八刀 `GateOutcome.summary`）。
    pub gate_summary: &'a str,
    /// 步④对账出口。
    pub rc: &'a ReconcileOutcome,
    /// 串口日志份数（镜像腿恒 1；标准流 = 保留日志数）。
    pub n_logs: usize,
}

/// 步⑤结果包文本（`bcts-result.txt` 全文，含尾换行；行序逐字镜像
/// :811-840）。
pub fn result_text(r: &ResultInput) -> String {
    let mut v: Vec<String> = Vec::new();
    v.push("# BCTS v0 结果包（hlkx 式 submission package 雏形——设计稿 §4.2-⑤/§5.2）".into());
    v.push(format!("mode                = {}", r.mode));
    match r.frozen {
        Some((path, sha)) => {
            v.push(format!("artifact            = {path}"));
            v.push(format!("artifact_sha256     = sha256:{sha}"));
        }
        None => v.push("artifact            = live（tools/markers.sh 单源）".into()),
    }
    if let Some((pr, at, snap, hash)) = r.meta {
        v.push(format!("platform_revision   = {pr}"));
        v.push(format!("frozen_at           = {at}"));
        v.push(format!("abi_snapshot        = {snap}"));
        v.push(format!("abi_snapshot_hash   = {hash}"));
    }
    match r.image {
        Some((img, isha, pr, anchor, n_bare)) => {
            v.push(format!("boot_image          = {img}"));
            v.push(format!("boot_image_sha256   = sha256:{isha}"));
            v.push(format!("manifest_platform_revision = {pr}（== {anchor}；版本声称位对账 ok——G3 镜像腿）"));
            v.push("replay              = EFI 镜像流（AAVMF + img boot，零 -kernel/零 fw_cfg——smoke-image.sh 同款拓扑）".into());
            v.push(format!("image_face          = boot 相（EFI 十段 + 接续 src=handoff + bare 相 {n_bare} 件 + banner 形态/ident 非 nogit）；rootfs/RX 相留标准流——EFI 路无 virtio/simdev 面"));
        }
        None => {
            let (k, ksha) = r.kernel.expect("标准流须带内核路径与 sha256");
            v.push(format!("kernel_image        = {k}"));
            v.push(format!("kernel_sha256       = sha256:{ksha}（v0 镜像 = QEMU -kernel ELF 直载形态；组装镜像 hash 随 V13 宿主迁入）"));
        }
    }
    v.push(format!("host_gates          = {}", r.gate_summary));
    if r.image.is_some() {
        v.push("image_boot          = PASS（EFI 镜像流 boot 相到齐 + banner/ident 门过）".into());
    } else {
        v.push("smoke               = PASS（八跑全相，退出码 0）".into());
    }
    v.push(format!("verdict             = {}", verdict_line(r.rc.verdict, r.image.is_some())));
    v.push(format!("markers             = {}/{} 过；missing = {}", r.rc.pass_n, r.rc.face_n, missing_disp(&r.rc.missing)));
    if r.image.is_some() {
        v.push("serial_logs         = logs/boot-01.log（镜像腿单 boot）".into());
    } else {
        v.push(format!("serial_logs         = logs/boot-01.log … boot-{:02}.log（共 {} 份，八跑轮转）", r.n_logs, r.n_logs));
    }
    let mut s = v.join("\n");
    s.push('\n');
    s
}

/// 结果包 `markers = … missing = ${MISSING:-（无）}` 展开形（:837——空串 →
/// `（无）` 全角括号；bash `:-` 对未定义/空同判）。
fn missing_disp(missing: &str) -> String {
    if missing.is_empty() {
        "（无）".into()
    } else {
        missing.to_string()
    }
}

/// 尾段 stdout `missing = ${MISSING:-无}` 展开形（:851——**无括号**，与结果包
/// 行不同形——两处字面各自镜像，首跑咬出）。
fn missing_disp_tail(missing: &str) -> String {
    if missing.is_empty() {
        "无".into()
    } else {
        missing.to_string()
    }
}

/// 步⑤尾段 stdout 两行（:851-852）。
pub fn tail_lines_verdict(rc: &ReconcileOutcome, result_dir: &str) -> Vec<String> {
    vec![
        format!("bcts: verdict = {}（{}/{}；missing = {}）", rc.verdict, rc.pass_n, rc.face_n, missing_disp_tail(&rc.missing)),
        format!("bcts: 结果包：{result_dir}/bcts-result.txt（per-boot 日志同目录 logs/）"),
    ]
}

/// 步⑤出口码映射（`[ "$VERDICT" = PASS ] || exit 1` → PASS 0 / FAIL 1）。
pub fn exit_code(rc: &ReconcileOutcome) -> i32 {
    if rc.verdict == "PASS" {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lo(markers: &[(&str, &str, &str)], n_bare: usize) -> LoadOutcome {
        let entries: Vec<String> = markers
            .iter()
            .map(|(ph, id, ex)| format!("MARK\t{ph}\t{id}\t{ex}"))
            .collect();
        let n_markers = markers.len();
        LoadOutcome {
            entries,
            n_markers,
            n_gates: 0,
            floor: 91,
            n_bare,
            img_pr: None,
            img_pr_anchor: None,
            pre_stdout: vec![],
        }
    }

    // ---- assert_marker 三形 ----

    #[test]
    fn assert_fixed_string_anywhere_in_line() {
        let c = "foo\nROOTFS ok vmo=1\nbar\n";
        assert!(assert_marker("ROOTFS ok", c));
        assert!(assert_marker("vmo=1", c)); // 行内任意位置
        assert!(!assert_marker("ROOTFS okk", c));
        assert!(!assert_marker("", "")); // 空语料零行——grep 空模式无行可匹配（rc 1）
        assert!(assert_marker("", "\n")); // 有行（空行）即匹配
        assert!(!assert_marker("X", "")); // 空语料无非空命中
    }

    #[test]
    fn assert_demo_tab_prefix_guard() {
        // DEMO ok 前邻 I 拒（IDEMO ok 子串假阳性面）。
        assert!(!assert_marker("DEMO ok", "IDEMO ok n=1\n"));
        assert!(assert_marker("DEMO ok", "DEMO ok n=1\n")); // 行首
        assert!(assert_marker("DEMO ok", "x DEMO ok\n")); // 空格前邻
        assert!(assert_marker("DEMO ok", "[DEMO ok\n"));
        // TAB ok 前邻 G 拒（GTAB ok）。
        assert!(!assert_marker("TAB ok", "GTAB ok\n"));
        assert!(assert_marker("TAB ok", "TAB ok\n"));
        assert!(assert_marker("TAB ok", "ITAB ok\n")); // I 不是 G——过
        // 非 ASCII 前邻按字符比较 ≠ I/G → 过。
        assert!(assert_marker("DEMO ok", "中DEMO ok\n"));
        // 同行多候选：IDEMO 拒后 DEMO 仍可在别处命中。
        assert!(assert_marker("DEMO ok", "IDEMO ok DEMO ok\n"));
        // 尾行无换行照匹配。
        assert!(assert_marker("DEMO ok", "x\nDEMO ok"));
    }

    #[test]
    fn assert_partial_overlap_scan() {
        // 重叠候选：needle 自身首字符 = 排除字符 X 的邻接形。
        // "DEDEMO ok"：位置 0 失配（后随 DE 而非 D…），位置 2 前邻 'E' ≠ I → 过。
        assert!(assert_marker("DEMO ok", "DEDEMO ok\n"));
        // "IIDEMO ok"：位置 2 前邻 'I' 拒——不漏到位置 1（失配）。
        assert!(!assert_marker("DEMO ok", "IIDEMO ok\n"));
    }

    // ---- 对账循环 ----

    #[test]
    fn reconcile_all_pass_standard_stream() {
        let o = lo(&[("bare", "b-01", "A ok"), ("rootfs", "r-01", "B ok"), ("rx", "x-01", "C ok")], 0);
        let r = reconcile(&o, "A ok\nB ok\nC ok\n", false);
        assert_eq!(r, ReconcileOutcome { pass_n: 3, missing: String::new(), face_n: 3, verdict: "PASS" });
        assert_eq!(exit_code(&r), 0);
    }

    #[test]
    fn reconcile_missing_joins_with_single_spaces() {
        let o = lo(&[("bare", "b-01", "A ok"), ("bare", "b-02", "B ok"), ("rx", "x-01", "C ok")], 0);
        let r = reconcile(&o, "A ok\n", false);
        assert_eq!(r.pass_n, 1);
        assert_eq!(r.missing, "b-02 x-01"); // 首项无前导空格——${MISSING:+$MISSING } 同形
        assert_eq!(r.verdict, "FAIL");
        assert_eq!(exit_code(&r), 1);
    }

    #[test]
    fn reconcile_image_leg_bare_only_face_n_bare() {
        let o = lo(&[("bare", "b-01", "A ok"), ("rootfs", "r-01", "B ok"), ("rx", "x-01", "C ok")], 1);
        // 语料无 B/C：标准流会缺 2，镜像腿只看 bare → PASS 1/1。
        let img = reconcile(&o, "A ok\n", true);
        assert_eq!(img, ReconcileOutcome { pass_n: 1, missing: String::new(), face_n: 1, verdict: "PASS" });
        let std = reconcile(&o, "A ok\n", false);
        assert_eq!(std.missing, "r-01 x-01");
        assert_eq!(std.face_n, 3);
    }

    #[test]
    fn reconcile_marker_semantics_demo_prefix_not_counted() {
        // DEMO ok 子串假阳性面：语料 IDEMO ok 不算 DEMO 到齐。
        let o = lo(&[("bare", "demo", "DEMO ok")], 1);
        assert_eq!(reconcile(&o, "IDEMO ok\n", true).verdict, "FAIL");
        assert_eq!(reconcile(&o, "DEMO ok\n", true).verdict, "PASS");
    }

    #[test]
    fn reconcile_short_or_empty_expect_rows_are_missing() {
        // 域数不足 / expect 空：bash grep -qF "" 恒过 = 假绿面；镜像严拒记缺
        // （发射器不可达——t1_check expect 非门前置不变量；合成语料钉板）。
        let mut o = lo(&[], 0);
        o.entries = vec!["MARK\tbare\tb-01\t".into(), "MARK\tbare".into(), "MARK\tbare\tb-03\tX ok".into()];
        o.n_markers = 3;
        o.n_bare = 3;
        let r = reconcile(&o, "whatever\n", true);
        assert_eq!(r.pass_n, 0);
        // 行序：空 expect 行（mid=b-01）→ 短行（mid=空串）→ b-03；join(" ") =
        // "b-01" + " " + "" + " " + "b-03" = 双空格（bash ${MISSING:+ } 同形）。
        assert_eq!(r.missing, "b-01  b-03");
        let r2 = reconcile(&o, "X ok\n", true);
        assert_eq!(r2.missing, "b-01 "); // b-03 过；尾空槽带尾随空格（bash 同形）
    }

    #[test]
    fn reconcile_zero_markers_image_leg_is_vacuous_pass() {
        // 0/0 假绿面：步②/镜像腿空集守卫在装载段先发（七刀钉序），此处
        // 结构性不可达——纯函数面如实镜像 bash（MISSING 空 → PASS）。
        let o = lo(&[], 0);
        let r = reconcile(&o, "", true);
        assert_eq!(r, ReconcileOutcome { pass_n: 0, missing: String::new(), face_n: 0, verdict: "PASS" });
    }

    #[test]
    fn reconcile_ignores_gate_and_meta_rows() {
        let mut o = lo(&[("bare", "b-01", "A ok")], 1);
        o.entries.insert(0, "META\t6\t91\t91\t1\tabc\ttools/abi.toml\tsha256:def".into());
        o.entries.push("GATE\tabi-check\tjust abi-check".into());
        let r = reconcile(&o, "A ok\n", true);
        assert_eq!(r.pass_n, 1);
        assert_eq!(r.face_n, 1);
    }

    // ---- 结果包文本 ----

    fn rc(pass: usize, face: usize, missing: &str) -> ReconcileOutcome {
        ReconcileOutcome { pass_n: pass, missing: missing.into(), face_n: face, verdict: if missing.is_empty() { "PASS" } else { "FAIL" } }
    }

    #[test]
    fn result_text_live_standard_stream() {
        let r = rc(91, 91, "");
        let t = result_text(&ResultInput {
            mode: "live",
            frozen: None,
            meta: None,
            image: None,
            kernel: Some(("/k/kernel.elf", "abc123")),
            gate_summary: "abi-check: PASS",
            rc: &r,
            n_logs: 8,
        });
        let lines: Vec<&str> = t.lines().collect();
        assert!(t.ends_with('\n'));
        assert_eq!(lines[0], "# BCTS v0 结果包（hlkx 式 submission package 雏形——设计稿 §4.2-⑤/§5.2）");
        assert_eq!(lines[1], "mode                = live");
        assert_eq!(lines[2], "artifact            = live（tools/markers.sh 单源）");
        assert_eq!(lines[3], "kernel_image        = /k/kernel.elf");
        assert_eq!(lines[4], "kernel_sha256       = sha256:abc123（v0 镜像 = QEMU -kernel ELF 直载形态；组装镜像 hash 随 V13 宿主迁入）");
        assert_eq!(lines[5], "host_gates          = abi-check: PASS");
        assert_eq!(lines[6], "smoke               = PASS（八跑全相，退出码 0）");
        assert_eq!(lines[7], "verdict             = PASS（判定式 = 观测标记 ⊇ 清单全集，一条不少；多打不算 fail）");
        assert_eq!(lines[8], "markers             = 91/91 过；missing = （无）");
        assert_eq!(lines[9], "serial_logs         = logs/boot-01.log … boot-08.log（共 8 份，八跑轮转）");
        assert_eq!(lines.len(), 10);
    }

    #[test]
    fn result_text_frozen_image_leg_with_meta_and_missing() {
        let r = rc(2, 3, "b-03");
        let t = result_text(&ResultInput {
            mode: "frozen",
            frozen: Some(("tools/bcts/frozen/bcts@6.toml", "f00d")),
            meta: Some(("6", "51932bff", "tools/abi.toml", "sha256:d47c")),
            image: Some(("/img/a.img", "beef", "6", "冻结件 platform_revision（tools/bcts/frozen/bcts@6.toml）", 3)),
            kernel: None,
            gate_summary: "g1: PASS；g2: FAIL",
            rc: &r,
            n_logs: 1,
        });
        let lines: Vec<&str> = t.lines().collect();
        assert_eq!(lines[1], "mode                = frozen");
        assert_eq!(lines[2], "artifact            = tools/bcts/frozen/bcts@6.toml");
        assert_eq!(lines[3], "artifact_sha256     = sha256:f00d");
        assert_eq!(lines[4], "platform_revision   = 6");
        assert_eq!(lines[5], "frozen_at           = 51932bff");
        assert_eq!(lines[6], "abi_snapshot        = tools/abi.toml");
        assert_eq!(lines[7], "abi_snapshot_hash   = sha256:d47c");
        assert_eq!(lines[8], "boot_image          = /img/a.img");
        assert_eq!(lines[9], "boot_image_sha256   = sha256:beef");
        assert_eq!(lines[10], "manifest_platform_revision = 6（== 冻结件 platform_revision（tools/bcts/frozen/bcts@6.toml）；版本声称位对账 ok——G3 镜像腿）");
        assert_eq!(lines[11], "replay              = EFI 镜像流（AAVMF + img boot，零 -kernel/零 fw_cfg——smoke-image.sh 同款拓扑）");
        assert_eq!(lines[12], "image_face          = boot 相（EFI 十段 + 接续 src=handoff + bare 相 3 件 + banner 形态/ident 非 nogit）；rootfs/RX 相留标准流——EFI 路无 virtio/simdev 面");
        assert_eq!(lines[13], "host_gates          = g1: PASS；g2: FAIL");
        assert_eq!(lines[14], "image_boot          = PASS（EFI 镜像流 boot 相到齐 + banner/ident 门过）");
        assert_eq!(lines[15], "verdict             = FAIL（判定式 = 观测标记 ⊇ bare 相清单〔镜像腿〕，一条不少；多打不算 fail）");
        assert_eq!(lines[16], "markers             = 2/3 过；missing = b-03");
        assert_eq!(lines[17], "serial_logs         = logs/boot-01.log（镜像腿单 boot）");
        assert_eq!(lines.len(), 18);
    }

    #[test]
    fn verdict_line_two_forms() {
        assert_eq!(verdict_line("PASS", false), "PASS（判定式 = 观测标记 ⊇ 清单全集，一条不少；多打不算 fail）");
        assert_eq!(verdict_line("FAIL", true), "FAIL（判定式 = 观测标记 ⊇ bare 相清单〔镜像腿〕，一条不少；多打不算 fail）");
    }

    #[test]
    fn tail_and_exit_mapping() {
        let ok = rc(3, 3, "");
        let bad = rc(1, 3, "a b");
        // 尾段 missing 缺省 = 裸「无」（:851）——与结果包行「（无）」（:837）不同形。
        assert_eq!(tail_lines_verdict(&ok, "/tmp/r"), vec![
            "bcts: verdict = PASS（3/3；missing = 无）".to_string(),
            "bcts: 结果包：/tmp/r/bcts-result.txt（per-boot 日志同目录 logs/）".to_string(),
        ]);
        assert_eq!(tail_lines_verdict(&bad, "/tmp/r")[0], "bcts: verdict = FAIL（1/3；missing = a b）");
        assert_eq!((exit_code(&ok), exit_code(&bad)), (0, 1));
    }

    #[test]
    fn result_text_logs_zero_padding_beyond_99() {
        // printf '%02d'：≥100 自然十进制（bash 同形）。
        let r = rc(1, 1, "");
        let t = result_text(&ResultInput { mode: "live", frozen: None, meta: None, image: None, kernel: Some(("k", "s")), gate_summary: "", rc: &r, n_logs: 123 });
        assert!(t.contains("boot-123.log（共 123 份，八跑轮转）"));
        let t2 = result_text(&ResultInput { mode: "live", frozen: None, meta: None, image: None, kernel: Some(("k", "s")), gate_summary: "", rc: &r, n_logs: 7 });
        assert!(t2.contains("boot-07.log（共 7 份"));
    }
}
