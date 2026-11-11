//! # berry-crypto —— SDK-2 分发签名链 crate（P0-2：ed25519/SHA-512 原语过向量门）
//!
//! 设计 = `docs/SDK-2-分发与签名链设计-v1.2.md` §二（信任账本）/§三（签名）
//! + `research/厂商定制化深研/24 号` §3（P0 最小件设计）+ 26 号 §3 选型
//! 终裁（自研 ed25519 优先——保 user/ workspace 零外部依赖纪律，31 crate
//! 先例）。「解析器即产品」家族形态：**零分配、零 unsafe、零依赖、输入
//! 只借用、`no_std`**（本工作区编 none target 时带 std/alloc 依赖即构建
//! 链编译失败——berry-manifest 头注同款）。
//!
//! ## P0-1 / P0-2 边界声明（26 号 §3 终裁，本 crate 的刀界合同）
//!
//! - **P0-1（已合入）= 对选型零敏感件**：`.sig` 信封 v1 TLV 解析/序列化
//!   （[`envelope`]）、信任账本 TOML 读写+校验+水位比较器（[`ledger`]）、
//!   RFC 8032 测试向量表 + 向量门框架（[`vectors`]）。
//! - **P0-2（本刀）= 原语本体，已过 RFC 8032 向量门**：SHA-512
//!   （[`sha512`]，RFC 6234 官方向量门）+ ed25519 verify（strict）/
//!   sign（[`ed25519`]，RFC 8032 §5.1.7/§5.1.6）+ 支撑件 fe（Fp 域算术
//!   5×51 limb）/ ge（edwards25519 点运算，解压缩含非常规点拒收）/ sc
//!   （标量算术 mod L）。**[`vectors::PRIMITIVES_IN`] 翻真，
//!   RFC 8032 §7.1 官方向量全 5 条（verify+sign 双向）全绿 = 本刀合入
//!   门槛**（26 号 §3-2；测试 `vectors::tests::gate_armed_in_p02` /
//!   `sign_side_full_gate`）。
//! - **P0-2 姊妹刀已合流（本版）**：Wycheproof ed25519 210 条 strict 门
//!   入 [`wycheproof_vectors`] + [`vectors`] 批跑门——小阶/混阶公钥
//!   拒收由 verify 步 1.5 torsion 检查执法（[8]A==O + [L]A≠O 双臂，
//!   29 号 §F1 修正）：valid 117 全过 / acceptable 29 全拒 / invalid 64
//!   全拒。26 号 §3-3 fallback 时钟（mixed-order 门）随之清账——
//!   不再触发外购重议。
//! - **未完成面（如实）**：①~~SHA-512/256 截断变体~~ 已随 P1 mkapp
//!   集成刀落（[`sha512`] `digest256`/`new_256`——信封 alg_id=1 摘要
//!   面，两源向量门在册）；②设备侧常时性审计 = P3（当前声明：常数
//!   时间风格、未审计、不用于在线敌手环境——[`ed25519`] 模块头；
//!   sc_muladd 尾进位例外见 sc.rs F2 注记，29 号 §F2）。P1 增量：host
//!   CLI `berry-sign`（bin target，keygen/sign/verify/selftest——mkapp
//!   `--sign-key`/`--verify-key` 消费面；验签组合逻辑暂居 bin，P3 设备
//!   侧常驻消费出现时上移 lib）。
//! - 若 fallback 翻案外购：envelope/ledger/vectors 骨架不变、原语模块
//!   换装（24 号 §3.1 预案）。
//!
//! ## 「P0 合入 = 闸 B」警示（开工前必读）
//!
//! **P0 合入 = 闸 B（SDK-2 开刀/账本首实现合入）触发 = Q6 账本 schema
//! 重议窗关闭**（24 号 §4.2 + `docs/架构决策.md` :301）。此后 anchor_type
//! 枚举与账本条目字段进入 add-only 冻结轨道（改 = 全量账本迁移）。
//! Q3 信封格式**未冻结**（最晚重议 = 首个编队 OTA 实现合入前 ≈ P3 前；
//! P0–P2 内野无消费者，改信封零成本）。
//!
//! ## 帽常量单源 + 镜像同步义务（家族惯例）
//!
//! lineage/id 帽 `1..=16B [a-z0-9-]` 与 berry-manifest `SEC_VAL_MAX`
//! （`user/berry-manifest/src/lib.rs`）+ `tools/mkapp.py` `SEC_RE` :144
//! **同值同语义**——三处单源，改帽须同刀双侧同步（P1 mkapp 集成刀兑现
//! 镜像注释）。
//!
//! ## 两步发布纪律注释位（Q6，v1 单实现期 = 空操作）
//!
//! 新 anchor_type 枚举值进账本前，全网最低验证器版本先升（v1.2 §二
//! :44–45 + #82 附录二 Q6）。v1 无在野验证器 → 空操作；P2/P3 有在野
//! 版本后在此落实「先升验证器、后进新枚举」的发布顺序。
//!
//! ## 索引行待收口同步
//!
//! 本 crate 为 P0-1 新增，`docs/索引.md` 索引行**待主会话收口统一同步**
//! （本波多 agent 并行，共享索引文件不在此刀开写——任务书纪律）。
//!
//! 定名注：`berry-crypto` 暂名，定名随 #77（ADR #59「TLV 校验器定名随
//! SDK-2 开刀」同窗；24 号 §3.2/§3.9-4）。

#![cfg_attr(not(test), no_std)]

pub mod ed25519;
pub mod envelope;
pub mod fe;
pub mod ge;
pub mod ledger;
pub mod sc;
pub mod sha512;
pub mod vectors;

/// Wycheproof ed25519 全向量常量表（**生成文件勿手改**——
/// `tools/genwycheproof.py` 产物，fixture `tools/wycheproof_ed25519_test.json`
/// 单源；strict 门的表基，消费 = [`vectors`] 批跑门）。
pub mod wycheproof_vectors;

/// 原语公开 API 顶层再导出（P1 mkapp/平台签发消费面——任务书口径
/// 「verify(pk,sig,msg) 与 sign(暂名，平台签发用)」）。
pub use ed25519::{public_key, sign, verify};
