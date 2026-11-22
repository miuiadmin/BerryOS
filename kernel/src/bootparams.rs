//! G5 内核半：`etc/boot-params.toml` 消费端（产品配置层 §1.6-补——设计
//! 预置的内核落体；工具侧投放 = `tools/berry_assemble.py` 计划面条目，
//! 生成件归全产线步⑥ mkcpio）。
//!
//! **通道裁决**（任务书 G5 步 1——侦察后 5 行记录）：
//! 1. 「内核读不了 rootfs」前提不成立：rootfs boot 期缓冲（main.rs 三源
//!    装载，:449-614）本就是内核可达字节流——cpio 定址查找是新原语但
//!    机制可行，设计 §1.6-补 原案直接落体。
//! 2. 采原案通道：rootfs cpio（= INITRD 角色）内定址文件
//!    `etc/boot-params.toml`（文本 TOML 子集）——locale 同构 + 内核解析
//!    面零新二进制格式（§1.6-补「与 §1.6-4 的关系」明文）。
//! 3. 候选 a（INITRD.BIN 段承载 params）不采：需给产链新增序列化段面，
//!    与「单一序列化器纪律」冲突且工具侧投放已定文本文件形。
//! 4. 候选 b（svcmgr 早期读 etc 回传内核）不采：svcmgr spawn 在
//!    timer::init/sched init 之后（main.rs :737/:782 vs :1318），回传无
//!    通道且时序倒挂——slice_hz 须在 timer::init 前定。
//! 5. 两键 v1 均内核侧可生效（无需降记「仅 intent_default 生效」）：
//!    sched_slice_hz → timer 换算注入；intent_default → Task 装配注入。
//!    CP-11（§3.8 机制面·2026-11-15）词表 v1 扩第三键 `timer_slack`：
//!    u32 纳秒、缺省 0 = 无松弛（现役零漂移）→ sched 全局
//!    `set_timer_slack_ns`（`block_current` 入队点 LATE-only 消费——
//!    intent >= UTILITY 档顺延、交互档恒等；判定律单源 slackface）。
//!
//! **消费契约 v1**：文件缺席（含空 params 不投放的工具侧合同）= 正常
//! 缺省态**零输出**（与 §1.6-补投放面「缺省零扰动」配套——件缺打诊断
//! 行会让每个默认镜像 boot 都多一行，破 smoke 逐行零漂移，此处按机制
//! 修正设计表「件缺 → loud 诊断行」为「件缺 = 静默缺省」；诊断行只对
//! **坏值/坏格式/schema 不符**打，坏键回缺省、好键照常应用——boot 期
//! 无 panic 纪律）。解析有界：内容 ≤ 4KiB、行 ≤ 64（承 §1.6-4 count 帽
//! 32 的解析面定界）；schema 头 ≠ 1 = 未知格式全段弃（防 v2 语义被 v1
//! 内核误用——§1.6-4「超界即弃段用缺省」）。未知键跳过不弃段（v1
//! 内核跑 v2 镜像的向前兼容面——工具侧未知键硬拒是产线执法，两侧
//! 宽严分工非漂移）。
//!
//! eco 打点：`bootparams slice_hz=N slice=CYC intent=WORD timer_slack=NS`
//! （QEMU 可观测——只打在位键；slice = 单源算式 [`berry_hal_aarch64::timer::
//! slice_cycles_for_hz`] 的换算直证；timer_slack = 纳秒原值）。诊断行
//! `bootparams: <cause>`（首错一条，ASCII）。ktest 纯函数面 =
//! `ktest.rs test_bootparams`（静默执法——G3-1a f4 先例）。
//!
//! **W3-Q140 单源化**：newc 定址查找状态机与 TOML 子集解析的**判定律
//! 单源外迁 `berry-bpface`**（host 钉板 20 测——ktest 同源覆盖 host 复
//! 现 + host-only 拒面：hex8 全域/几何恰界/魔数与 namesize 拒面/双魔数
//! 变体）。本模块留物理臂：`Src` 归一视图（Vmo read 走 vmo.rs）+
//! `alloc` 内容缓冲 + `apply_and_report` HAL 注入与 eco 打点。常量族/
//! `Outcome`/`parse`/`find`/`hex_field`/`parse_u32`/`intent_code`/
//! `INTENT_WORDS` 消费 crate 单源（改语义 = 改 boot 行为）。

use berry_hal_aarch64 as hal;

// 单源 re-export（消费面：main.rs/ktest.rs）；其余常量族（HDR/LINE_*/
// NAME_MAX/SLICE_HZ_*/parse_u32/trim/EntryGeometry）单源在 bpface，未消
// 费不 re-export（私有 mod 的 pub use 无消费 = unused 警告）。
pub use berry_bpface::{find_in as find, parse, ByteSource, FILE_MAX, INTENT_WORDS, NAME, Outcome};

/// rootfs 源视图（main.rs `RootfsBuf` 三形态归一：借片/堆整档字节直读，
/// fw_cfg 多卷拼装件按**逻辑长**（非页对齐容量）走 VMO 只读 read）。
pub enum Src<'a> {
    Bytes(&'a [u8]),
    Vmo(&'a crate::vmo::Vmo, usize),
}

impl berry_bpface::ByteSource for Src<'_> {
    fn total(&self) -> usize {
        match self {
            Src::Bytes(b) => b.len(),
            Src::Vmo(_, n) => *n,
        }
    }

    /// 有界读：越界（几何违约）返 false——调用方按「未找到」降级。
    ///（trait 方法天然 pub——ktest 合成归档数据区对账用。逻辑长预检
    /// 对两臂生效：Vmo 多卷拼装件逻辑长 < 页容量，容量内逻辑长外须拒
    ///——与 base 形逐字对齐。）
    fn read_at(&self, off: usize, out: &mut [u8]) -> bool {
        if off > self.total() || out.len() > self.total() - off {
            return false;
        }
        match self {
            Src::Bytes(b) => {
                out.copy_from_slice(&b[off..off + out.len()]);
                true
            }
            Src::Vmo(v, _) => v.read(off, out).is_ok(),
        }
    }
}

/// 全链扫描：cpio 定址查找 → 一次读取 → 有界解析。
/// `None` = 文件缺席（正常缺省态，调用方静默）；`Some` = 已解析件（含
/// 坏件——diag 在内，应用归 [`apply_and_report`]）。
pub fn scan(src: &Src<'_>) -> Option<Outcome> {
    let (off, len) = berry_bpface::find_in(src, NAME)?;
    if len == 0 || len > FILE_MAX {
        return Some(Outcome::bad("size"));
    }
    let mut content = alloc::vec![0u8; len];
    if !src.read_at(off, &mut content) {
        return Some(Outcome::bad("read"));
    }
    Some(parse(&content))
}

/// 应用 + eco 打点（调用点 = main.rs rootfs 装载段，ROOTFS ok 行之后）。
///
/// 注入点时序：`timer::set_slice_hz` 须在 `timer::init`（main.rs :737）前
/// ——SLICE 换算基准即改；`sched::set_intent_default` 须在首个 spawn
/// （:789 起）前——新任务装配即带；`set_timer_slack_ns` 只须在首个
/// `block_current` 前。三处本函数调用点均满足。
pub fn apply_and_report(o: &Outcome) {
    if let Some(hz) = o.slice_hz {
        // SAFETY: boot 单线程、timer::init 之前（唯一调用点）。
        unsafe { hal::timer::set_slice_hz(hz as u64) };
    }
    if let Some(code) = o.intent {
        berry_sched::set_intent_default(code);
    }
    // CP-11：timer_slack → sched 全局（消费点 = block_current 入队——
    // 首个睡眠者之前生效即足，本点远早于首 spawn，时序恒满足）。
    if let Some(ns) = o.timer_slack {
        berry_sched::set_timer_slack_ns(ns as u64);
    }
    // 全段弃（schema 拒面：三键皆空）只打诊断行——eco 行只在有在位键时打。
    if o.slice_hz.is_some() || o.intent.is_some() || o.timer_slack.is_some() {
        hal::uart_puts(b"bootparams");
        if let Some(hz) = o.slice_hz {
            hal::uart_puts(b" slice_hz=");
            hal::uart_put_dec(hz as u64);
            // 换算直证：单源算式同 timer::init 的 SLICE（62.5MHz/250 = 250000）。
            hal::uart_puts(b" slice=");
            hal::uart_put_dec(hal::timer::slice_cycles_for_hz(hz as u64));
        }
        if let Some(code) = o.intent {
            hal::uart_puts(b" intent=");
            // 码 → 词（解析面镜像反查；域外码不可能——parse 只出词表码）。
            if let Some((w, _)) = INTENT_WORDS.iter().find(|&(_, c)| *c == code) {
                hal::uart_puts(w);
            } else {
                hal::uart_put_dec(code as u64);
            }
        }
        if let Some(ns) = o.timer_slack {
            // 纳秒原值直打（换算截尾律单源 berry_sched::timer_slack_defer
            //——eco 不做第二换算面）。
            hal::uart_puts(b" timer_slack=");
            hal::uart_put_dec(ns as u64);
        }
        hal::uart_puts(b"\n");
    }
    if let Some(d) = o.diag {
        hal::uart_puts(b"bootparams: ");
        hal::uart_puts(d.as_bytes());
        hal::uart_puts(b"\n");
    }
}
