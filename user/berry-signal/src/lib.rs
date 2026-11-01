//! # berry-signal —— POSIX 信号面纯函数底座（A7-4 协同式投递）
//!
//! ## Overview (for crates.io users)
//!
//! Pure-function POSIX-signal bookkeeping for the
//! [BerryOS](https://github.com/miuiadmin/BerryOS) microkernel: a
//! sigaction table, a signal-mask state machine
//! (`block`/`unblock`/`setmask`), a musl-compatible 128-byte
//! `sigset_t` bit-map layer, a default-disposition table, and a
//! per-signal **delivery classifier** ([`classify`], batched as
//! [`plan_dispatch`]) that maps `(sig, mask, actions)` to a
//! side-effect-free action — the consumer (berry-libc `signal.rs`)
//! re-classifies each pending signal **at execution time** so that
//! disposition changes made inside a handler take effect for later
//! steps of the same round (POSIX delivery-time semantics; mask
//! changes take effect for **nested** delivery — the libc Call
//! envelope restores the pre-handler mask on return, sigreturn
//! semantics). Delivery is
//! cooperative: the kernel merely sets a pending bit
//! (`signal_send`, ABI 0x86) and userland drains it
//! (`signal_take`, ABI 0x87) — see
//! `docs/事件循环A7-4信号面实施设计草案.md` §2 (plan B). This crate
//! holds no syscall face; no_std core; builds and unit-tests on any
//! host.
//!
//! Status: experimental (0.1.0) — the API moves with the BerryOS v1
//! userland ABI. Repo docs are mostly zh-CN.
//!
//! ## 卡位与形态（A7-4 裁定 B：协同式用户态投递）
//!
//! 信号语义（表 / 掩码 / 默认动作 / 投递执行）**全在用户态**
//! （berry-libc signal.rs）；内核只管「跨进程位置位」
//! （0x86 send / 0x87 take）。本 crate = 语义核的纯函数面
//! （berry-mmap / berry-complete 先例——「不长第二套」+ host 测试
//! 零 libc 符号面）。
//!
//! ### 归并式分诊（POSIX block 窗合并语义按构造成立）
//!
//! libc 投递循环形态：`total = take() | held_swap(0)` → 两遍走（③b
//! 修复轮二式：Hold 位**即刻落 held**保嵌套可见性——handler 内
//! UNBLOCK 补投从 held 取到「已 take 未处置」的阻塞位；工作集**逐位
//! [`classify`] 按步重查**——改表即时生效、掩码变更只对嵌套投递生效
//! （Call 包络返回时全量恢复，sigreturn 口径）；弃「轮首
//! `plan_dispatch` 冻结执行」形态——对抗审查 MAJOR）→ 执行（Call
//! 就地调 handler、Terminate `exit(128+sig)`）。同号信号跨「内核
//! pending」与「held 集」按位或归并——标准信号在 block 窗内多次
//! 到达只投一次（POSIX 2.4.1 合并语义），无需计数。`unblock` 后
//! libc 再跑一轮 dispatch 即自动补投 held 位。
//!
//! ### 掩码与内核同宽（u64）
//!
//! A7-4 草案 §3.2 原「libc 侧 mask: u128」为笔误级设计——语义位
//! 仅 1..=64，u64 与内核 `pending_signals: u64` 同宽同位序
//! （sig-1 为位号），拷贝零转换。musl `sigset_t`（128B = 1024
//! 位）的兼容读写由 [sigset 位图层]（[`set_add`] 族 +
//! [`mask_of_set`]/[`set_of_mask`] 桥）承担：**宽进严出**——入参
//! 低 8 字节 LE 取 u64、语义位外（位 64..=1023）忽略（musl
//! `sigfillset` = 全 128B 填 0xFF 的惯例用法必须活：严格拒会把
//! fillset→sigprocmask 组合打死）；出参只写语义位、其余 120B 恒零
//! （POSIX 未定义位读回任意，写零 = 干净诚实档）。
//!
//! ### 默认动作表（v1 无停止类，偏差文档化）
//!
//! POSIX 忽略类（SIGCHLD/SIGCONT/SIGURG/SIGWINCH）→ 丢弃；
//! SIGCONT 无停止态时 continue 是 no-op，同丢弃档。其余**全部**
//! 终止类 `exit(128+sig)`——含 SIGSTOP/SIGTSTP/SIGTTIN/SIGTTOU
//! （POSIX 停止类，v1 无停止态，终止 = 「进程不再跑」的最接近
//! 可观察语义，A7-4 草案 §3.4 偏差清单在册）。SIGKILL/SIGSTOP
//! 不可捕获不可忽略（[`capturable`] 拒 EINVAL——纯查询/装 DFL 档
//! 由 [`installable`] 放行）、**不可阻塞**——单一执法点 =
//! [`classify`] 投递期豁免（对其无视掩码直落表查，`kill -9`
//! 全掩码下仍必杀）；掩码**存储位**允许含 KILL/STOP（Linux
//! `sig_kernel_only` 同形——sigprocmask 回读可见、投递期绕过）。
//! POSIX 钦定三元组。
//!
//! ### errno 本地定值
//!
//! crate 零依赖，[`EINVAL`]/[`ESRCH`] 本地 pub const（与 musl
//! 值一致；berry-libc 消费侧以 const assert 对字面值钉板，漂移
//! 编译期红）。
//!
//! ### GETPID 镜像（单源注记）
//!
//! [`GETPID_MIRROR`] = 42 镜像 berry-libc `sys.rs` 的 getpid 定值
//! 锚（v1 无 pid 面，「无进程 ID 概念的诚实常数」档）——跨 crate
//! 无依赖不能共享常量，两值若漂移，berry-libc 侧 const assert
//! 即编译期红（berry-mmap `PTHREAD_STACK_VA_BASE_MIRROR` 同款
//! 手法）。kill 仅自杀档：`pid == getpid()` → raise 等价；else
//! ESRCH（诚实：v1 无 pid 面确无此进程）。

#![cfg_attr(not(test), no_std)]

// ---- 常量 ----

/// 语义信号域上界（内核 `pending_signals: u64` 位 63 = 信号 64）。
pub const SIG_MAX: i32 = 64;
/// 语义信号域下界（0 号非法——POSIX kill(pid,0) 探活档走 [`kill_probe`]）。
pub const SIG_MIN: i32 = 1;
/// SIGKILL（POSIX 钦定不可捕获不可忽略）。
pub const SIGKILL: i32 = 9;
/// SIGSTOP（同上）。
pub const SIGSTOP: i32 = 19;
/// SIGABRT（asm-generic 编号 6）——libc `abort` 改装消费（A7-4 ③b）。
pub const SIGABRT: i32 = 6;
/// SIGUSR1（10）——bsh sig_selftest 投递活证/回读步消费。
pub const SIGUSR1: i32 = 10;
/// SIGUSR2（12）——bsh sig_selftest 掩码档（held 再投证）消费。
pub const SIGUSR2: i32 = 12;
/// SIGTERM（15）——sigchild 默认动作终止档（collect 143 = 128+15）。
pub const SIGTERM: i32 = 15;
/// musl `sigset_t` 字节宽（1024 位；语义位仅 1..=64）。
pub const SIGSET_BYTES: usize = 128;

/// musl errno EINVAL（crate 零依赖本地定值——见头注）。
pub const EINVAL: i32 = -22;
/// musl errno ESRCH。
pub const ESRCH: i32 = -3;

/// sigaction 表值：默认动作（查 [`default_action`]）。
pub const SIG_DFL: u64 = 0;
/// sigaction 表值：忽略（丢弃）。
pub const SIG_IGN: u64 = 1;
/// sigaction 表槽位数（索引 = 信号号，0 槽占位不可用）。
pub const ACTIONS_LEN: usize = SIG_MAX as usize + 1;

/// getpid() 定值镜像（见头注「GETPID 镜像」）。
pub const GETPID_MIRROR: i32 = 42;

/// `sigprocmask` how = BLOCK（并入掩码）。
pub const MASK_BLOCK: i32 = 0;
/// `sigprocmask` how = UNBLOCK（从掩码清出）。
pub const MASK_UNBLOCK: i32 = 1;
/// `sigprocmask` how = SETMASK（整体替换）。
pub const MASK_SETMASK: i32 = 2;

/// 语义掩码类型——与内核 `pending_signals: u64` 同宽同位序（sig-1 为位号）。
pub type SigMask = u64;

// ---- 界检 ----

/// 信号号 ∈ 1..=64（内核 0x86 send 界检同源）。
pub fn valid_sig(sig: i32) -> bool {
    (SIG_MIN..=SIG_MAX).contains(&sig)
}

/// 可捕获界检：合法号 ∧ 非 SIGKILL/SIGSTOP（sigaction 装表面）。
pub fn capturable(sig: i32) -> Result<(), i32> {
    if !valid_sig(sig) {
        Err(EINVAL)
    } else if sig == SIGKILL || sig == SIGSTOP {
        Err(EINVAL)
    } else {
        Ok(())
    }
}

/// sigaction/signal 的装表面分诊（对抗审查 F3：POSIX EINVAL 只罩
/// 「attempt to catch **or ignore**」——**纯查询与装 SIG_DFL 对
/// KILL/STOP 合法**，musl/Linux 对拍同形：`sigaction(SIGKILL, NULL,
/// &old)` 返 0、oldact=SIG_DFL）。返回 `Ok(needs_store)`：false =
/// KILL/STOP 装 DFL（槽恒 DFL，no-op 免写）——调用方（libc 胶水）
/// 以此跳过表写。
pub fn installable(sig: i32, value: u64) -> Result<bool, i32> {
    if !valid_sig(sig) {
        return Err(EINVAL);
    }
    if sig == SIGKILL || sig == SIGSTOP {
        if value == SIG_DFL {
            return Ok(false); // 查询/复位档合法；槽恒 DFL 免写
        }
        return Err(EINVAL); // catch/ignore 拒（capturable 同口径）
    }
    Ok(true)
}

// ---- sigaction 表 ----

/// 每进程 handler 表：槽值 0 = SIG_DFL / 1 = SIG_IGN / ≥2 = handler
/// fn 地址（musl 口径三值编码；SA_SIGINFO 三参形态 v1 不分派——
/// A7-4 草案 §3.4 偏差清单）。
#[derive(Clone, Copy)]
pub struct ActionTable {
    slots: [u64; ACTIONS_LEN],
}

impl ActionTable {
    /// 全 DFL 初值（const——libc 侧静态表直落）。
    pub const fn new() -> Self {
        Self { slots: [SIG_DFL; ACTIONS_LEN] }
    }

    /// 读槽（界检同 [`valid_sig`]——KILL/STOP 恒 DFL，读合法）。
    pub fn get(&self, sig: i32) -> Result<u64, i32> {
        if !valid_sig(sig) {
            return Err(EINVAL);
        }
        Ok(self.slots[sig as usize])
    }

    /// 写槽（界检同 [`capturable`]——KILL/STOP 拒装，POSIX 钦定）。
    pub fn set(&mut self, sig: i32, value: u64) -> Result<(), i32> {
        capturable(sig)?;
        self.slots[sig as usize] = value;
        Ok(())
    }
}

// ---- 掩码状态机 ----

/// sig 的掩码位（sig-1 为位号；界检同 [`valid_sig`]）。
pub fn mask_bit(sig: i32) -> Result<SigMask, i32> {
    if !valid_sig(sig) {
        return Err(EINVAL);
    }
    Ok(1u64 << (sig - 1) as u32)
}

/// 掩码是否含 sig（界检同上）。
pub fn mask_has(mask: SigMask, sig: i32) -> Result<bool, i32> {
    Ok(mask & mask_bit(sig)? != 0)
}

/// `sigprocmask` 纯核心：`how ∈ {BLOCK=并, UNBLOCK=清, SETMASK=换}`，
/// 非法 how → EINVAL。arg 为 set 入参经 [`mask_of_set`] 转得的语义掩码。
pub fn apply_mask(how: i32, old: SigMask, arg: SigMask) -> Result<SigMask, i32> {
    match how {
        MASK_BLOCK => Ok(old | arg),
        MASK_UNBLOCK => Ok(old & !arg),
        MASK_SETMASK => Ok(arg),
        _ => Err(EINVAL),
    }
}

// ---- sigset 位图层（musl 128B 兼容，宽进严出——见头注）----

/// `sigemptyset`：128B 全零。
pub fn set_empty() -> [u8; SIGSET_BYTES] {
    [0u8; SIGSET_BYTES]
}

/// `sigfillset`：musl 语义全 128B 填 0xFF（1024 位全 1——惯例用法
/// 必须活，头注「宽进严出」档）。
pub fn set_full() -> [u8; SIGSET_BYTES] {
    [0xFFu8; SIGSET_BYTES]
}

/// `sigaddset`：位 (sig-1)，字节序 LE（musl `set->__bits[s/8] |= 1<<(s%8)`）。
pub fn set_add(set: &mut [u8; SIGSET_BYTES], sig: i32) -> Result<(), i32> {
    if !valid_sig(sig) {
        return Err(EINVAL); // 界检先于减法——sig=i32::MIN 的 `sig-1`
    } // debug 构建 arithmetic overflow panic（对抗审查 F6）
    let s = (sig - 1) as usize;
    set[s / 8] |= 1u8 << (s % 8);
    Ok(())
}

/// `sigdelset`：清位（同上位序）。
pub fn set_del(set: &mut [u8; SIGSET_BYTES], sig: i32) -> Result<(), i32> {
    if !valid_sig(sig) {
        return Err(EINVAL); // 同 set_add：先检后减
    }
    let s = (sig - 1) as usize;
    set[s / 8] &= !(1u8 << (s % 8));
    Ok(())
}

/// `sigismember`：测位（同上位序）。
pub fn set_ismember(set: &[u8; SIGSET_BYTES], sig: i32) -> Result<bool, i32> {
    if !valid_sig(sig) {
        return Err(EINVAL); // 同 set_add：先检后减
    }
    let s = (sig - 1) as usize;
    Ok(set[s / 8] & (1u8 << (s % 8)) != 0)
}

/// sigset → 语义掩码：**低 8 字节 LE 取 u64、语义位外忽略**（宽进
/// ——sigfillset 全 1 惯例必须活；头注「宽进严出」档）。
pub fn mask_of_set(set: &[u8; SIGSET_BYTES]) -> SigMask {
    let mut m: SigMask = 0;
    for i in 0..8 {
        m |= (set[i] as SigMask) << (8 * i);
    }
    m
}

/// 语义掩码 → sigset：**只写语义位、其余 120B 恒零**（严出——
/// oldset 真回读的出参形态）。
pub fn set_of_mask(mask: SigMask) -> [u8; SIGSET_BYTES] {
    let mut set = [0u8; SIGSET_BYTES];
    for i in 0..8 {
        set[i] = (mask >> (8 * i)) as u8;
    }
    set
}

// ---- 默认动作表 ----

/// 默认动作（v1 无停止类——头注「默认动作表」节偏差文档化）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultAction {
    /// 丢弃（POSIX 忽略类：SIGCHLD/SIGCONT/SIGURG/SIGWINCH）。
    Drop,
    /// 终止 `exit(128+sig)`（其余全部——含 POSIX 停止类的 v1 偏差档）。
    Terminate,
}

/// sig 的默认动作（界检同 [`valid_sig`]）。
pub fn default_action(sig: i32) -> Result<DefaultAction, i32> {
    if !valid_sig(sig) {
        return Err(EINVAL);
    }
    match sig {
        // 忽略类四件（asm-generic 编号）：SIGCHLD(17) / SIGCONT(18) /
        // SIGURG(23) / SIGWINCH(28)。
        17 | 18 | 23 | 28 => Ok(DefaultAction::Drop),
        _ => Ok(DefaultAction::Terminate),
    }
}

// ---- 投递分诊计划 ----

/// 单信号的投递处置（plan_dispatch 对 pending 每一位的裁定）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// 掩码内 → 暂存 held（不投；归并式分诊下同号跨 held/pending
    /// 只此一档，POSIX block 窗合并语义）。
    Hold,
    /// 丢弃：SIG_IGN 或默认忽略类。
    Drop,
    /// 调用户 handler（值 = 表槽值）。
    Call(u64),
    /// 默认动作终止档：`exit(128+sig)`。
    Terminate,
}

/// 分诊计划：升序 steps 覆盖 total 每一位恰一次 + 回写 held。
#[derive(Clone, Copy)]
pub struct Plan {
    /// `(sig, Act)` 升序（信号号序）——libc 执行序即此序。
    pub steps: [(i32, Act); SIG_MAX as usize],
    /// steps 有效长度（= total 位计数）。
    pub n: usize,
    /// 回写 held：`total & mask` 挖去 KILL/STOP 位（不可阻塞——
    /// 其位永不入 held；libc 侧执行完 steps 后写入 held 集）。
    pub new_held: SigMask,
}

/// 单信号投递裁定（[`plan_dispatch`] 的逐位单源）：掩码内（KILL/STOP
/// 不可阻塞豁免除外——POSIX：SIGKILL/SIGSTOP **不可阻塞**钦定三元组
/// 第三元，掩码位对其无效直落表查；KILL/STOP 槽恒 DFL → Terminate，
/// `kill -9` 全掩码下仍必杀——头注「恒走默认表」的执法点）→
/// [`Act::Hold`]；否则表槽三值分诊：SIG_DFL 查 [`default_action`]（忽略
/// 类 Drop / 其余 Terminate）、SIG_IGN → Drop、handler 值 → Call。
/// 前提：`sig ∈ 1..=64`（调用方按位号构造——libc 投递循环
/// `trailing_zeros()+1`、plan_dispatch 升序遍历，界外号结构性不可达，
/// 不再付费界检）。
///
/// libc 消费面（A7-4 ③b 修复轮）：投递循环**按步重查**——每执行一步前
/// 重读当前掩码与表再裁定：handler 中途改表 → 按新表裁定（POSIX
/// delivery-time disposition，表变更不回滚）；handler 中途 block 后续
/// 位 → 该步变 Hold（处置顺延）。**掩码变更只对嵌套投递即时生效**：
/// handler 返回后 libc 包络全量恢复投递前掩码（sigreturn 口径），后续
/// 位按恢复后掩码分诊（Linux 同形）。轮首冻结 plan 执行的旧形态不
/// 感知中途改动——已修（对抗审查 MAJOR）。
pub fn classify(sig: i32, mask: SigMask, actions: &ActionTable) -> Act {
    let bit = 1u64 << (sig - 1) as u32;
    if mask & bit != 0 && sig != SIGKILL && sig != SIGSTOP {
        return Act::Hold;
    }
    match actions.slots[sig as usize] {
        SIG_DFL => match default_action(sig) {
            Ok(DefaultAction::Drop) => Act::Drop,
            _ => Act::Terminate,
        },
        SIG_IGN => Act::Drop,
        handler => Act::Call(handler),
    }
}

/// 归并式分诊：`total`（= `take() | held_swap(0)`，libc 侧归并后的
/// 待处置全集）× 当前掩码 × handler 表 → 升序计划。纯函数零副作用
/// ——libc 侧只执行：Hold 位落 held、Call 就地调（errno
/// save/restore 由调用方包络）、Terminate `exit(128+sig)`。逐位裁定
/// 收敛 [`classify`]（单源——两者漂移由 host 对拍测试钉死）。
pub fn plan_dispatch(total: u64, mask: SigMask, actions: &ActionTable) -> Plan {
    let mut steps = [(0i32, Act::Drop); SIG_MAX as usize];
    let mut n = 0usize;
    let mut new_held: SigMask = 0;
    for sig in SIG_MIN..=SIG_MAX {
        let bit = 1u64 << (sig - 1) as u32;
        if total & bit == 0 {
            continue;
        }
        let act = classify(sig, mask, actions);
        if matches!(act, Act::Hold) {
            new_held |= bit;
        }
        steps[n] = (sig, act);
        n += 1;
    }
    Plan { steps, n, new_held }
}

// ---- kill 面 ----

/// `kill(pid, sig)` 的 pid 分诊：仅自杀档（pid == getpid 定值锚
/// 42）→ 调用方走 raise 等价；else 由调用方返 ESRCH。
pub fn kill_targets_self(pid: i32) -> bool {
    pid == GETPID_MIRROR
}

/// `kill(pid, 0)` 探活档（POSIX：不发信号只验存在）：self → Ok；
/// else ESRCH。libc 侧 sig==0 零内核调用直走此面。
pub fn kill_probe(pid: i32) -> Result<(), i32> {
    if kill_targets_self(pid) {
        Ok(())
    } else {
        Err(ESRCH)
    }
}

// ---- 测试（host；cfg(test) 放开 std——默认测试挂具）----

#[cfg(test)]
mod tests {
    use super::*;

    fn acts(plan: &Plan) -> Vec<(i32, Act)> {
        plan.steps[..plan.n].to_vec()
    }

    // 界检 ----

    #[test]
    fn valid_sig_bounds() {
        assert!(!valid_sig(0));
        assert!(valid_sig(1));
        assert!(valid_sig(64));
        assert!(!valid_sig(65));
        assert!(!valid_sig(-1));
    }

    #[test]
    fn capturable_rejects_kill_stop() {
        assert_eq!(capturable(SIGKILL), Err(EINVAL));
        assert_eq!(capturable(SIGSTOP), Err(EINVAL));
        assert_eq!(capturable(10), Ok(())); // SIGUSR1
        assert_eq!(capturable(0), Err(EINVAL));
        assert_eq!(capturable(65), Err(EINVAL));
    }

    #[test]
    fn installable_query_and_dfl_legal_on_kill_stop() {
        // POSIX EINVAL 只罩 catch/ignore——查询与装 DFL 对 KILL/STOP
        // 合法（musl/Linux 对拍，对抗审查 F3）。
        assert_eq!(installable(SIGKILL, SIG_DFL), Ok(false)); // no-op 免写
        assert_eq!(installable(SIGSTOP, SIG_DFL), Ok(false));
        assert_eq!(installable(SIGKILL, SIG_IGN), Err(EINVAL)); // ignore 拒
        assert_eq!(installable(SIGSTOP, 0x400090), Err(EINVAL)); // catch 拒
        assert_eq!(installable(10, SIG_DFL), Ok(true)); // 常规号全放行
        assert_eq!(installable(10, SIG_IGN), Ok(true));
        assert_eq!(installable(0, SIG_DFL), Err(EINVAL)); // 界外恒拒
        assert_eq!(installable(65, SIG_DFL), Err(EINVAL));
    }

    // sigaction 表 ----

    #[test]
    fn action_table_roundtrip() {
        let mut t = ActionTable::new();
        assert_eq!(t.get(10), Ok(SIG_DFL)); // 初值全 DFL
        assert_eq!(t.set(10, 0x400090), Ok(()));
        assert_eq!(t.get(10), Ok(0x400090));
        assert_eq!(t.set(10, SIG_IGN), Ok(()));
        assert_eq!(t.get(10), Ok(SIG_IGN));
        assert_eq!(t.get(9), Ok(SIG_DFL)); // KILL 读合法（恒 DFL）
    }

    #[test]
    fn action_table_bounds() {
        let mut t = ActionTable::new();
        assert_eq!(t.get(0), Err(EINVAL));
        assert_eq!(t.get(65), Err(EINVAL));
        assert_eq!(t.set(9, 0x1000), Err(EINVAL)); // KILL 拒装
        assert_eq!(t.set(19, 0x1000), Err(EINVAL)); // STOP 拒装
        assert_eq!(t.set(0, 0x1000), Err(EINVAL));
        assert_eq!(t.set(65, 0x1000), Err(EINVAL));
    }

    // 掩码状态机 ----

    #[test]
    fn mask_bit_and_has() {
        assert_eq!(mask_bit(1), Ok(1));
        assert_eq!(mask_bit(64), Ok(1 << 63));
        assert_eq!(mask_bit(0), Err(EINVAL));
        assert_eq!(mask_bit(65), Err(EINVAL));
        assert_eq!(mask_has(0b11, 1), Ok(true));
        assert_eq!(mask_has(0b11, 2), Ok(true));
        assert_eq!(mask_has(0b11, 3), Ok(false));
    }

    #[test]
    fn apply_mask_three_hows() {
        let old: SigMask = 0b0011;
        assert_eq!(apply_mask(MASK_BLOCK, old, 0b0100), Ok(0b0111));
        assert_eq!(apply_mask(MASK_UNBLOCK, old, 0b0010), Ok(0b0001));
        assert_eq!(apply_mask(MASK_SETMASK, old, 0b1000), Ok(0b1000));
        // 集合律钉板：BLOCK 幂等、UNBLOCK 挖空不越界
        assert_eq!(apply_mask(MASK_BLOCK, old, 0), Ok(old));
        assert_eq!(apply_mask(MASK_UNBLOCK, old, !0), Ok(0));
        assert_eq!(apply_mask(MASK_SETMASK, old, 0), Ok(0));
    }

    #[test]
    fn apply_mask_rejects_bad_how() {
        assert_eq!(apply_mask(3, 0, 0), Err(EINVAL));
        assert_eq!(apply_mask(-1, 0, 0), Err(EINVAL));
    }

    // sigset 位图层 ----

    #[test]
    fn sigset_empty_full_shapes() {
        let e = set_empty();
        assert!(e.iter().all(|&b| b == 0));
        let f = set_full();
        assert!(f.iter().all(|&b| b == 0xFF)); // musl fillset = 全 1024 位
    }

    #[test]
    fn sigset_add_del_member_across_bytes() {
        let mut s = set_empty();
        // 跨字节位序钉板：sig 9 → 字节 1 位 0；sig 17 → 字节 2 位 0；
        // sig 64 → 字节 7 位 7（musl `bits[s/8] |= 1<<(s%8)`，s = sig-1）。
        assert_eq!(set_add(&mut s, 9), Ok(()));
        assert_eq!(s[1], 0b1);
        assert_eq!(set_add(&mut s, 17), Ok(()));
        assert_eq!(s[2], 0b1);
        assert_eq!(set_add(&mut s, 64), Ok(()));
        assert_eq!(s[7], 0b1000_0000);
        assert_eq!(set_ismember(&mut s, 9), Ok(true));
        assert_eq!(set_ismember(&mut s, 10), Ok(false));
        assert_eq!(set_add(&mut s, 0), Err(EINVAL));
        assert_eq!(set_add(&mut s, 65), Err(EINVAL));
        // 极端号无溢出（对抗审查 F6）：sig-1 的 debug arithmetic
        // overflow panic 回归钉板——界检必须先于减法。
        assert_eq!(set_add(&mut s, i32::MIN), Err(EINVAL));
        assert_eq!(set_del(&mut s, i32::MIN), Err(EINVAL));
        assert_eq!(set_ismember(&s, i32::MIN), Err(EINVAL));
        assert_eq!(set_ismember(&s, 65), Err(EINVAL));
        assert_eq!(set_del(&mut s, 9), Ok(()));
        assert_eq!(s[1], 0);
        assert_eq!(set_ismember(&s, 9), Ok(false));
    }

    #[test]
    fn sigset_mask_bridge_wide_in_strict_out() {
        // 宽进：fillset（全 0xFF）→ 全掩码（sigfillset 惯例必须活）
        let f = set_full();
        assert_eq!(mask_of_set(&f), !0);
        // 严出：全掩码 → set 后语义位全 1、其余 120B 恒零
        let out = set_of_mask(!0);
        assert!(out[..8].iter().all(|&b| b == 0xFF));
        assert!(out[8..].iter().all(|&b| b == 0));
        // 往返：mask → set → mask 恒等
        assert_eq!(mask_of_set(&set_of_mask(0b101)), 0b101);
        // 语义位外忽略：字节 8（位 64..）任意值不进掩码
        let mut odd = set_empty();
        odd[8] = 0xFF;
        assert_eq!(mask_of_set(&odd), 0);
    }

    // 默认动作表 ----

    #[test]
    fn default_action_table() {
        // 忽略类四件
        for sig in [17, 18, 23, 28] {
            assert_eq!(default_action(sig), Ok(DefaultAction::Drop));
        }
        // 终止类抽样：常规 + KILL/STOP（v1 无停止类偏差档）+
        // TSTP/TTIN/TTOU + realtime 尾段
        for sig in [1, 6, 9, 15, 19, 20, 21, 22, 34, 64] {
            assert_eq!(default_action(sig), Ok(DefaultAction::Terminate));
        }
        assert_eq!(default_action(0), Err(EINVAL));
        assert_eq!(default_action(65), Err(EINVAL));
    }

    // plan_dispatch ----

    #[test]
    fn plan_empty_pending_is_empty() {
        let t = ActionTable::new();
        let p = plan_dispatch(0, 0, &t);
        assert_eq!(p.n, 0);
        assert_eq!(p.new_held, 0);
        assert!(acts(&p).is_empty());
    }

    #[test]
    fn plan_handler_ign_dfl() {
        let mut t = ActionTable::new();
        t.set(10, 0x400090).unwrap(); // USR1 → handler
        t.set(13, SIG_IGN).unwrap(); // PIPE → IGN
        // 15 TERM 未装 → DFL 终止；17 CHLD 未装 → DFL 忽略
        let p = plan_dispatch((1 << 9) | (1 << 12) | (1 << 14) | (1 << 16), 0, &t);
        assert_eq!(
            acts(&p),
            vec![
                (10, Act::Call(0x400090)),
                (13, Act::Drop),
                (15, Act::Terminate),
                (17, Act::Drop),
            ]
        );
        assert_eq!(p.new_held, 0);
    }

    #[test]
    fn plan_mask_holds_and_new_held() {
        let mut t = ActionTable::new();
        t.set(12, 0x400100).unwrap(); // USR2 → handler（被掩码暂存不投）
        let mask = mask_bit(12).unwrap();
        let p = plan_dispatch((1 << 11) | (1 << 9), mask, &t); // USR2 + USR1
        assert_eq!(
            acts(&p),
            vec![(10, Act::Terminate), (12, Act::Hold)] // USR1 DFL 终止 + USR2 暂存
        );
        assert_eq!(p.new_held, mask); // 回写 held = total & mask
    }

    #[test]
    fn plan_mask_beats_terminate() {
        // 掩码优先于投递：默认终止信号被 block → Hold 非 Terminate
        //（POSIX：blocked 信号即使默认终止也不投——KILL/STOP 除外
        //：不可阻塞（plan_kill_stop_not_blockable 钉板），其余全部
        //可挡）。
        let t = ActionTable::new();
        let mask = mask_bit(15).unwrap(); // TERM 被 block
        let p = plan_dispatch(1 << 14, mask, &t);
        assert_eq!(acts(&p), vec![(15, Act::Hold)]);
        assert_eq!(p.new_held, mask);
    }

    #[test]
    fn plan_ascending_and_full_count() {
        // 升序钉板 + n = 位计数：全 64 位 pending、掩码挡一半、表混三态
        let mut t = ActionTable::new();
        t.set(64, SIG_IGN).unwrap();
        t.set(1, 0x1000).unwrap();
        let mask: SigMask = 0xAAAA_AAAA_AAAA_AAAA; // 偶数信号号全 block
        let p = plan_dispatch(!0, mask, &t);
        assert_eq!(p.n, 64);
        let a = acts(&p);
        // 升序
        for w in a.windows(2) {
            assert!(w[0].0 < w[1].0);
        }
        // 抽样三态：sig1 (1<<0) 掩码位 0 → Call；sig2 掩码位 1 → Hold；
        // sig64 IGN 且掩码位 63（1 → mask 位 = (0xAAAA.. >> 63) & 1 = 1）→ Hold。
        assert_eq!(a[0], (1, Act::Call(0x1000)));
        assert_eq!(a[1], (2, Act::Hold));
        assert_eq!(a[63], (64, Act::Hold));
        // new_held = total & mask（偶数号全暂存）
        assert_eq!(p.new_held, !0 & mask);
        // 奇数号（掩码外）恰 32 位处置
        assert_eq!(a.iter().filter(|(_, x)| !matches!(x, Act::Hold)).count(), 32);
    }

    #[test]
    fn plan_merged_pending_no_double_delivery() {
        // 归并语义活证：同号在 total 只一位——libc 侧 `take()|held_swap(0)`
        // 归并后无论内核 pending 与 held 各到过几次，分诊只此一档
        //（POSIX block 窗合并，头注「归并式分诊」节）。
        let mut t = ActionTable::new();
        t.set(10, 0x400090).unwrap();
        let mask = mask_bit(10).unwrap();
        let p = plan_dispatch(1 << 9, mask, &t);
        assert_eq!(acts(&p), vec![(10, Act::Hold)]);
        assert_eq!(p.new_held, mask);
    }

    #[test]
    fn plan_kill_stop_not_blockable() {
        // POSIX：SIGKILL/SIGSTOP 不可阻塞——全掩码下仍直走表查
        //（KILL/STOP 槽恒 DFL → Terminate；new_held 恒不含其位）。
        let t = ActionTable::new();
        let mask = (1 << 8) | (1 << 18); // KILL(9) + STOP(19) 位
        let p = plan_dispatch(mask, !0, &t); // 全掩码
        assert_eq!(acts(&p), vec![(9, Act::Terminate), (19, Act::Terminate)]);
        assert_eq!(p.new_held, 0);
    }

    // classify（A7-4 修复轮单源抽面）----

    #[test]
    fn classify_matches_plan_dispatch_per_bit() {
        // 单源对拍：plan_dispatch 每一位的裁定 ≡ classify(sig, ..) 逐位
        // 重查——libc 投递循环「按步重查」与整轮计划同语义的执法面
        //（两函数漂移即红；classify 为 plan_dispatch 内联逻辑原样成
        // 函数，本测试是防未来分叉的钉板）。
        let mut t = ActionTable::new();
        t.set(1, 0x1000).unwrap();
        t.set(10, 0x400090).unwrap();
        t.set(13, SIG_IGN).unwrap();
        t.set(64, SIG_IGN).unwrap();
        for mask in [0u64, !0u64, 0xAAAA_AAAA_AAAA_AAAA, 0x0F0F_0F0F_0F0F_0F0F] {
            for total in [0u64, !0u64, (1 << 9) | (1 << 14) | (1 << 16), 0x8000_0000_0000_0001] {
                for (sig, act) in acts(&plan_dispatch(total, mask, &t)) {
                    assert_eq!(
                        classify(sig, mask, &t),
                        act,
                        "sig={} mask={:x} total={:x}",
                        sig,
                        mask,
                        total
                    );
                }
            }
        }
    }

    #[test]
    fn classify_kill_stop_bypass_and_hold_boundary() {
        let t = ActionTable::new();
        // KILL/STOP 掩码豁免直落表查（槽恒 DFL → Terminate——与
        // plan_kill_stop_not_blockable 同语义的 classify 面）。
        assert_eq!(classify(9, 1 << 8, &t), Act::Terminate);
        assert_eq!(classify(19, 1 << 18, &t), Act::Terminate);
        // 常规号：掩码内 → Hold；掩码外 DFL 终止 / 默认忽略类 Drop。
        assert_eq!(classify(15, 1 << 14, &t), Act::Hold);
        assert_eq!(classify(15, 0, &t), Act::Terminate);
        assert_eq!(classify(17, 0, &t), Act::Drop); // SIGCHLD 默认忽略
    }

    // kill 面 ----

    #[test]
    fn kill_self_and_probe() {
        assert!(kill_targets_self(GETPID_MIRROR));
        assert!(kill_targets_self(42));
        assert!(!kill_targets_self(999));
        assert!(!kill_targets_self(1)); // getppid=1 也不是自杀目标
        assert_eq!(kill_probe(42), Ok(())); // kill(pid,0) 探活档
        assert_eq!(kill_probe(999), Err(ESRCH));
    }

    // 常量钉板 ----

    #[test]
    fn constants_pinned() {
        assert_eq!(EINVAL, -22); // musl 值
        assert_eq!(ESRCH, -3);
        assert_eq!(GETPID_MIRROR, 42); // berry-libc sys.rs getpid 定值镜像
        assert_eq!(ACTIONS_LEN, 65);
        assert_eq!(SIGKILL, 9);
        assert_eq!(SIGSTOP, 19);
        assert_eq!(SIG_DFL, 0);
        assert_eq!(SIG_IGN, 1);
    }

    // A7-4 ③b 消费面常量 + 默认动作表钉板 ----

    #[test]
    fn sig_consts_asm_generic() {
        // asm-generic 编号（A7-4 ③b：abort 改装 / bsh selftest / sigchild）
        assert_eq!(SIGABRT, 6);
        assert_eq!(SIGUSR1, 10);
        assert_eq!(SIGUSR2, 12);
        assert_eq!(SIGTERM, 15);
        // 忽略类四件（SIGCHLD/CONT/URG/WINCH）+ 终止类抽样
        assert_eq!(default_action(17), Ok(DefaultAction::Drop));
        assert_eq!(default_action(18), Ok(DefaultAction::Drop));
        assert_eq!(default_action(23), Ok(DefaultAction::Drop));
        assert_eq!(default_action(28), Ok(DefaultAction::Drop));
        assert_eq!(default_action(SIGABRT), Ok(DefaultAction::Terminate));
        assert_eq!(default_action(SIGTERM), Ok(DefaultAction::Terminate));
    }
}
