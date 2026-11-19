//! BerryOS app 打包门禁 Rust 件（mkapp.py 首刀——彻底 Rust 化轨道 b 层
//! #4 刀序第四位）。本刀范围 = **canonical 规范形重发射面**：过检后的
//! manifest 视图（`M`）+ `M::canonical()` 定序重发射 + Python 现役件
//! 黄金向量字节级对拍。产线零触碰——tools/mkapp.py 现役不动，后续刀
//! （check_manifest_text 门禁/BYI1/信封/签名）依 §4 刀序推进。
//!
//! canonical 规约（SDK-1a §3.1 定序 + GUI-7/同意面/ADR #82 尾增）：
//! `[app]` → `[compat]?` → `[capabilities]` → `[runtime]?` →
//! `[security]?` → `[ui]?` → `[agent-actions]?` → `[blob]?`；节间恰一
//! 空行；旧形（全缺省）输出与 v0 逐字节同（尾增居末 = 既有形态字节零
//! 漂移）。与 Python `M.canonical()`（tools/mkapp.py :292）单源对齐——
//! 改任一侧必须同步另一侧并重钉黄金向量。

/// manifest 帽常量（对账锚点：tools/mkapp.py :158-215 常量段镜像；SDK 级
/// 单一事实源 = user/berry-manifest，双侧同步义务）。
pub const ID_MAX: usize = 24;
pub const VERSION_MAX: usize = 16;
pub const ENTRY_MAX: usize = 32;
pub const STACK_PAGES_MIN: u32 = 1;
pub const STACK_PAGES_MAX: u32 = 64;
pub const UI_PATH_MAX: usize = 32;
pub const ICON_FILES_MAX: usize = 4;
pub const VIEWS_FILES_MAX: usize = 4;
pub const LABEL_MAX: usize = 32;
pub const ACTION_NAME_MAX: usize = 24;
pub const AGENT_ACTIONS_MAX: usize = 16;
pub const BLOB_FIELD_MAX: usize = 64;
pub const ABI_REVISION_V1: u32 = 1;
pub const ABI_REVISION_MAX: u32 = 65535;

/// intent 六词表（对账锚点：berry-manifest `Intent`——mkapp.py :163 镜像）。
pub const INTENTS: [&str; 6] = [
    "unspecified",
    "latency_critical",
    "interactive",
    "utility",
    "background",
    "maintenance",
];

/// 风险档三词（berry-manifest `RiskTier` 镜像）。
pub const RISK_TIERS: [&str; 3] = ["routine", "sensitive", "destructive"];

/// [blob] 五字段定序（BLOB_KEYS 镜像——ADR #82 V6 最低披露）。
pub const BLOB_KEYS: [&str; 5] = ["license", "origin", "sha256", "size", "usage"];

/// [security] 三键（ADR #82 V3——齐备或齐缺）。
pub const SEC_KEYS: [&str; 3] = ["profile", "key_lineage", "integrator"];

/// SDK 现值（对账锚点：berry-manifest `SdkVer`——mkapp.py :171 镜像）。
pub const SDK_CURRENT: (u32, u32, u32) = (0, 1, 0);

/// fs/net 词表（caps 单元素数组执法后此处只存合法值）。
pub const FS_SCOPES: [&str; 1] = ["rootfs"];
pub const NET_SCOPES: [&str; 1] = ["client"];

/// int 三元组 → `x.y.z`（canonical 重发射/诊断行用——文法禁前导零 ⇒
/// 「收下 → 重发射」恒等归一，§3.1 要点①；Python `_ver` 镜像）。
pub fn ver_str(v: (u32, u32, u32)) -> String {
    format!("{}.{}.{}", v.0, v.1, v.2)
}

/// W1-Q27 二刀：`check_manifest_text` 门禁面（Python mkapp.py :375 镜像）。
pub mod gate;

/// W1-Q27 三刀：BYI1 图标编码面（`_png_chunks`/`_paeth`/`png_to_byi`/
/// `_byi_validate` 镜像 + 自持 inflate）。
pub mod byi;

/// W1-Q27 四刀：cpio(newc) 走查器 + SHA256SUMS 复验（`_cpio_entries`/
/// `_verify_sums` 镜像 + 自持 SHA-256）。
pub mod cpio;

/// W1-Q27 六刀：`--check` 五步复验面（`check_bap` 镜像）。
pub mod check;

/// W1-Q27 五刀：build 编排面（Python `build()` 镜像——staging 走查 →
/// 门禁 → abi 打戳 → SHA256SUMS → cpio 落盘 + 摘要行）+ 产链换轨件。
pub mod build;

/// W1-Q27 七刀：`--json` 机读单行信封面（Python `_json_envelope`/
/// `_json_main` 镜像——build/check/new 三 op）。
pub mod json;

/// W1-Q27 八刀：`--new` 应用脚手架四件（Python `do_new` 镜像——SDK-1a
/// §4.5；模板 = include_str! 资产与 Python `SCAFFOLD_*` 同字节源）。
pub mod new;

/// [security] 三键视图（profile/key_lineage/integrator——齐备或齐缺）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sec<'a> {
    pub profile: &'a str,
    pub key_lineage: &'a str,
    pub integrator: &'a str,
}

/// GUI-7 [ui] 视图——三槽各自独立可选。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ui<'a> {
    pub icon: Option<&'a str>,
    pub label: Option<&'a str>,
    pub views: Option<&'a str>,
}

/// ADR #82 V6 [blob] 视图——五字段齐备（BLOB_KEYS 定序）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob<'a> {
    pub license: &'a str,
    pub origin: &'a str,
    pub sha256: &'a str,
    pub size: &'a str,
    pub usage: &'a str,
}

/// 过检后的 manifest 视图（规范形重序列化的数据源——Python `M` 镜像）。
/// 全部字段已过门禁（值域/词表/字符集），canonical 只做定序重发射。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestView<'a> {
    pub id: &'a str,
    pub version: &'a str,
    pub entry: &'a str,
    pub intent: &'a str,
    pub console: bool,
    pub fs: Option<&'a str>,
    pub net: Option<&'a str>,
    pub has_runtime: bool,
    pub stack_pages: u32,
    pub compat: Option<(SdkTriple, SdkTriple, SdkTriple)>,
    pub min_level: Option<u32>,
    pub abi_revision: Option<u32>,
    pub sec: Option<Sec<'a>>,
    pub ui: Option<Ui<'a>>,
    pub agent_actions: Vec<(&'a str, &'a str)>,
    pub blob: Option<Blob<'a>>,
}

/// SDK 三元组（min/target/compile 共用形）。
pub type SdkTriple = (u32, u32, u32);

impl<'a> ManifestView<'a> {
    /// 全缺省旧形（v0）：console=false、无可选节——canonical 输出与 v0
    /// 逐字节同。测试与后续门禁构造用。
    pub fn legacy(id: &'a str, version: &'a str, entry: &'a str, intent: &'a str) -> Self {
        Self {
            id,
            version,
            entry,
            intent,
            console: false,
            fs: None,
            net: None,
            has_runtime: false,
            stack_pages: 8,
            compat: None,
            min_level: None,
            abi_revision: None,
            sec: None,
            ui: None,
            agent_actions: Vec::new(),
            blob: None,
        }
    }

    /// 规范形重发射（Python `M.canonical()` 逐分支对齐——见头注定序）。
    pub fn canonical(&self) -> String {
        let mut out = String::new();
        out.push_str("[app]\n");
        out.push_str(&format!("id = \"{}\"\n", self.id));
        out.push_str(&format!("version = \"{}\"\n", self.version));
        out.push_str(&format!("entry = \"{}\"\n", self.entry));
        out.push_str("type = \"app\"\n");
        out.push_str(&format!("intent = \"{}\"\n", self.intent));
        if self.compat.is_some() || self.abi_revision.is_some() {
            out.push_str("\n[compat]\n");
            if let Some((mn, tg, cp)) = self.compat {
                out.push_str(&format!(
                    "min = \"{}\"\ntarget = \"{}\"\ncompile = \"{}\"\n",
                    ver_str(mn),
                    ver_str(tg),
                    ver_str(cp)
                ));
                if let Some(ml) = self.min_level {
                    out.push_str(&format!("min_level = {}\n", ml));
                }
            }
            if let Some(ar) = self.abi_revision {
                out.push_str(&format!("abi_revision = {}\n", ar));
            }
        }
        out.push_str("\n[capabilities]\n");
        out.push_str(&format!("console = {}\n", if self.console { "true" } else { "false" }));
        if let Some(fs) = self.fs {
            out.push_str(&format!("fs = [\"{}\"]\n", fs));
        }
        if let Some(net) = self.net {
            out.push_str(&format!("net = [\"{}\"]\n", net));
        }
        if self.has_runtime {
            out.push_str(&format!("\n[runtime]\nstack_pages = {}\n", self.stack_pages));
        }
        if let Some(sec) = &self.sec {
            out.push_str(&format!(
                "\n[security]\nprofile = \"{}\"\nkey_lineage = \"{}\"\nintegrator = \"{}\"\n",
                sec.profile, sec.key_lineage, sec.integrator
            ));
        }
        if let Some(ui) = &self.ui {
            out.push_str("\n[ui]\n");
            if let Some(icon) = ui.icon {
                out.push_str(&format!("icon = \"{}\"\n", icon));
            }
            if let Some(label) = ui.label {
                out.push_str(&format!("label = \"{}\"\n", label));
            }
            if let Some(views) = ui.views {
                out.push_str(&format!("views = \"{}\"\n", views));
            }
        }
        if !self.agent_actions.is_empty() {
            out.push_str("\n[agent-actions]\n");
            for (k, v) in &self.agent_actions {
                out.push_str(&format!("{} = \"{}\"\n", k, v));
            }
        }
        if let Some(blob) = &self.blob {
            out.push_str(&format!(
                "\n[blob]\nlicense = \"{}\"\norigin = \"{}\"\nsha256 = \"{}\"\nsize = \"{}\"\nusage = \"{}\"\n",
                blob.license, blob.origin, blob.sha256, blob.size, blob.usage
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 黄金向量（Python 现役件 mkapp.py M.canonical() 实产 2026-11-19，
    /// 13 形全谱：v0 旧形/词表/可选节组合/尾增节全谱/kitchen sink）。
    /// 再钉命令：python3 -c "import sys; sys.path.insert(0,'tools');
    /// import mkapp, json; ..."（LOG.md 本轮条目存生成脚本形态）。
    const GOLDEN: [(&str, &str); 13] = [
        (
            "legacy_minimal",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n",
        ),
        (
            "caps_fs_net",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = true\nfs = [\"rootfs\"]\nnet = [\"client\"]\n",
        ),
        (
            "runtime",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = true\n\n[runtime]\nstack_pages = 16\n",
        ),
        (
            "compat_full",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.2.0\"\ncompile = \"0.2.0\"\nmin_level = 3\n\n[capabilities]\nconsole = false\n",
        ),
        (
            "compat_no_ml",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[compat]\nmin = \"1.0.0\"\ntarget = \"1.2.3\"\ncompile = \"1.2.3\"\n\n[capabilities]\nconsole = false\n",
        ),
        (
            "abi_only",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[compat]\nabi_revision = 1\n\n[capabilities]\nconsole = false\n",
        ),
        (
            "compat_full_ar",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.1.0\"\ncompile = \"0.1.0\"\nmin_level = 0\nabi_revision = 65535\n\n[capabilities]\nconsole = false\n",
        ),
        (
            "sec",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[security]\nprofile = \"standard\"\nkey_lineage = \"lineage-a\"\nintegrator = \"oem-1\"\n",
        ),
        (
            "ui_full",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[ui]\nicon = \"icons/app.byi\"\nlabel = \"主界面\"\nviews = \"views/main\"\n",
        ),
        (
            "ui_partial",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[ui]\nicon = \"icons/a.byi\"\n",
        ),
        (
            "aa",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[agent-actions]\nread-inbox = \"routine\"\nsend-msg = \"sensitive\"\nwipe-all = \"destructive\"\n",
        ),
        (
            "blob",
            "[app]\nid = \"demo.app\"\nversion = \"0.1.0\"\nentry = \"bin/demo\"\ntype = \"app\"\nintent = \"utility\"\n\n[capabilities]\nconsole = false\n\n[blob]\nlicense = \"Apache-2.0\"\norigin = \"https://example.com/src\"\nsha256 = \"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"\nsize = \"123456\"\nusage = \"demo usage text\"\n",
        ),
        (
            "kitchen_sink",
            "[app]\nid = \"x.y-z_1\"\nversion = \"9.9.9\"\nentry = \"bin/xyz\"\ntype = \"app\"\nintent = \"interactive\"\n\n[compat]\nmin = \"0.1.0\"\ntarget = \"0.99.0\"\ncompile = \"0.99.0\"\nmin_level = 7\nabi_revision = 1\n\n[capabilities]\nconsole = true\nfs = [\"rootfs\"]\nnet = [\"client\"]\n\n[runtime]\nstack_pages = 64\n\n[security]\nprofile = \"locked\"\nkey_lineage = \"k2\"\nintegrator = \"i2\"\n\n[ui]\nicon = \"icons/i.byi\"\nlabel = \"label\"\nviews = \"views/v\"\n\n[agent-actions]\na = \"routine\"\n\n[blob]\nlicense = \"MIT\"\norigin = \"o\"\nsha256 = \"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\"\nsize = \"42\"\nusage = \"u\"\n",
        ),
    ];

    fn view_for(name: &str) -> ManifestView<'static> {
        match name {
            "legacy_minimal" => ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility"),
            "caps_fs_net" => ManifestView {
                console: true,
                fs: Some("rootfs"),
                net: Some("client"),
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "runtime" => ManifestView {
                console: true,
                has_runtime: true,
                stack_pages: 16,
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "compat_full" => ManifestView {
                compat: Some(((0, 1, 0), (0, 2, 0), (0, 2, 0))),
                min_level: Some(3),
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "compat_no_ml" => ManifestView {
                compat: Some(((1, 0, 0), (1, 2, 3), (1, 2, 3))),
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "abi_only" => ManifestView {
                abi_revision: Some(1),
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "compat_full_ar" => ManifestView {
                compat: Some(((0, 1, 0), (0, 1, 0), (0, 1, 0))),
                min_level: Some(0),
                abi_revision: Some(65535),
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "sec" => ManifestView {
                sec: Some(Sec { profile: "standard", key_lineage: "lineage-a", integrator: "oem-1" }),
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "ui_full" => ManifestView {
                ui: Some(Ui { icon: Some("icons/app.byi"), label: Some("主界面"), views: Some("views/main") }),
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "ui_partial" => ManifestView {
                ui: Some(Ui { icon: Some("icons/a.byi"), label: None, views: None }),
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "aa" => ManifestView {
                agent_actions: vec![
                    ("read-inbox", "routine"),
                    ("send-msg", "sensitive"),
                    ("wipe-all", "destructive"),
                ],
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "blob" => ManifestView {
                blob: Some(Blob {
                    license: "Apache-2.0",
                    origin: "https://example.com/src",
                    sha256: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    size: "123456",
                    usage: "demo usage text",
                }),
                ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
            },
            "kitchen_sink" => ManifestView {
                id: "x.y-z_1",
                version: "9.9.9",
                entry: "bin/xyz",
                intent: "interactive",
                console: true,
                fs: Some("rootfs"),
                net: Some("client"),
                has_runtime: true,
                stack_pages: 64,
                compat: Some(((0, 1, 0), (0, 99, 0), (0, 99, 0))),
                min_level: Some(7),
                abi_revision: Some(1),
                sec: Some(Sec { profile: "locked", key_lineage: "k2", integrator: "i2" }),
                ui: Some(Ui { icon: Some("icons/i.byi"), label: Some("label"), views: Some("views/v") }),
                agent_actions: vec![("a", "routine")],
                blob: Some(Blob {
                    license: "MIT",
                    origin: "o",
                    sha256: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                    size: "42",
                    usage: "u",
                }),
            },
            _ => panic!("未知黄金向量名 {name}"),
        }
    }

    #[test]
    fn golden_canonical_byte_exact() {
        for (name, want) in GOLDEN {
            let got = view_for(name).canonical();
            assert_eq!(got, want, "向量 {name} canonical 字节漂移");
        }
    }

    #[test]
    fn canonical_idempotent_roundtrip() {
        // canonical 是纯函数：同视图重发射恒等（确定性纪律——同输入同字节）。
        for (name, _) in GOLDEN {
            let v = view_for(name);
            assert_eq!(v.canonical(), v.canonical(), "{name} 重发射不确定");
        }
    }

    #[test]
    fn legacy_minimal_matches_v0_shape() {
        // 尾增节全缺时输出与 v0 逐字节同（字节零漂移承诺的最小验证）。
        let v = ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility");
        let c = v.canonical();
        assert!(c.ends_with("console = false\n"));
        assert!(!c.contains("[compat]"));
        assert!(!c.contains("[runtime]"));
        assert!(!c.contains("[security]"));
        assert!(!c.contains("[ui]"));
        assert!(!c.contains("[agent-actions]"));
        assert!(!c.contains("[blob]"));
    }

    #[test]
    fn abi_revision_alone_emits_compat_section() {
        // 打戳产物形：无三值仅 abi_revision ⇒ [compat] 节内唯一键。
        let v = ManifestView {
            abi_revision: Some(1),
            ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
        };
        let c = v.canonical();
        assert!(c.contains("\n[compat]\nabi_revision = 1\n\n[capabilities]"));
    }

    #[test]
    fn min_level_only_emitted_with_compat_triple() {
        // min_level 孤立（无三值）不出现在输出——Python 侧孤立即被门禁拒，
        // 视图层 None 化；canonical 对 None 不发射（防伪形漏网钉板）。
        let v = ManifestView {
            min_level: Some(3),
            ..ManifestView::legacy("demo.app", "0.1.0", "bin/demo", "utility")
        };
        let c = v.canonical();
        assert!(!c.contains("min_level"), "min_level 孤立不得发射: {c}");
        assert!(!c.contains("[compat]"));
    }

    #[test]
    fn ver_str_no_leading_zero_normalization() {
        // 文法禁前导零 ⇒ 收下的三元组重发射即归一形（§3.1 要点①）。
        assert_eq!(ver_str((0, 1, 0)), "0.1.0");
        assert_eq!(ver_str((12, 345, 6)), "12.345.6");
    }

    #[test]
    fn constants_mirror_python_source() {
        // 常量镜像钉板（对账锚点单列——改 Python 侧须同步本表并重钉）。
        assert_eq!(ID_MAX, 24);
        assert_eq!(VERSION_MAX, 16);
        assert_eq!(ENTRY_MAX, 32);
        assert_eq!(STACK_PAGES_MIN, 1);
        assert_eq!(STACK_PAGES_MAX, 64);
        assert_eq!(ABI_REVISION_MAX, 65535);
        assert_eq!(ABI_REVISION_V1, 1);
        assert_eq!(SDK_CURRENT, (0, 1, 0));
        assert_eq!(INTENTS.len(), 6);
        assert_eq!(RISK_TIERS, ["routine", "sensitive", "destructive"]);
        assert_eq!(BLOB_KEYS, ["license", "origin", "sha256", "size", "usage"]);
        assert_eq!(SEC_KEYS, ["profile", "key_lineage", "integrator"]);
    }
}
