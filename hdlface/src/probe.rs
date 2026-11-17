//! berry-hdlface probe 模块——handle 类型探测链折叠纯函数面（W3-Q91，
//! 02 号 IPC 快路径审计机会四）。
//!
//! **动机**：send 随行 handle 预检与 `wait_get` 取对象此前对**每枚**
//! handle 逐一试 7 个 `ObjType` 的 `get_raw`（`kernel/src/syscall.rs`
//! send 预检环 / `wait_get` 五类型链）——命中前的每个类型都是一次完整
//! lookup（界检+代比）+ ObjRef 解引用 + 类型比对；`CHAN_HND_MAX=8` 最坏
//! **7×8=56 次查表探测**，且旧注释「7 次定点查表 × nhandles ≤ 4」的
//! 「≤4」是 HND_MAX=6 时代的陈旧口径。`wait_get` 同形五类型链
//! （wait_one/wait_many 共用）：Channel 等待者（server 循环主力形）付
//! 2 查、SuspendToken 付 5 查。而 handle.rs **已有类型无关查询面**
//! `rights_of`（P3-1 起在册：查表+代号校验后直接回权利全集，不验类型
//! ）——两路径语义**逐位等价**（死值同 -EBADF、活值同 rights）。
//!
//! 本模块把折叠判据抽纯函数钉板：
//! - [`WAITABLE_TYPES`]：wait 面收的类型全表（Event→Channel→Process→
//!   Thread→SuspendToken——`wait_get` 链序冻结面，4-2/Plan B/T-1/AM-1.5
//!   分批扩到五类）。
//! - [`SEND_PROBE_TYPES`]：send 随行预检探测表（KernelObj 全变体镜像：
//!   新增对象类型须同步——旧 syscall.rs 注释同款纪律的承接位）。
//! - [`waitable`]：类型是否可等（五类词表判——wait 面外类型即拒）。
//! - [`probe_transfer_ok`]：随行迁移资格判（活值 + RIGHT_TRANSFER 位
//!   ——send 预检环体单查后的判定律，死值/缺位错误码折叠在此钉板）。
//! - [`ERR_DEAD`]/[`ERR_DENIED`]：错误码镜像（-EBADF/-EACCES——kernel
//!   `abi_gen::ERR_BAD_HANDLE/ERR_ACCESS_DENIED` 的独立声明保持本 crate
//!   零依赖，值域由 host 测试对 ABI 常量钉死）。
//!
//! 与冻结面的关系：syscall ABI 零触碰；探测链只是取值机械，替换为类型
//! 无关单查后错误码/权利面逐位不变（host 钉板对拍执法）。旧的
//! 「HandleTable 无类型无关查询面」注释与 `rights_of` 在册事实相抵，
//! 随本刀翻正。

/// wait 面错误码镜像：坏 handle（kernel `ERR_BAD_HANDLE` = -9）。
pub const ERR_DEAD: i32 = -9;
/// wait 面错误码镜像：缺权利位（kernel `ERR_ACCESS_DENIED` = -13）。
pub const ERR_DENIED: i32 = -13;

/// 随行迁移资格位（kernel `RIGHT_TRANSFER` = 1<<5——RIGHT_TRANSFER 全
/// 生态执法面〔安全轮 2 Medium 缺口关闭〕的判定位；值域 host 钉板）。
/// 注：kernel 侧 `Rights = u64`，本 crate 零依赖声明 u32——判定只看
/// 低位一位，kernel 消费点经 `as u32` 收窄（高位权利位域 v1 未启用）。
pub const RIGHT_TRANSFER: u32 = 1 << 5;

/// wait 面可等类型词表（`wait_get` 链序冻结面）。
///
/// 顺序 = 旧探测链的试序（Event → Channel → Process → Thread →
/// SuspendToken）：折叠为单查后顺序不再参与行为，但保留链序作为词表
/// 的冻结口径（与 `signal_state` 的对象类型全集一致）。
pub const WAITABLE_TYPES: [u8; 5] = [6, 5, 1, 2, 12];

/// send 随行预检探测表（KernelObj 全变体镜像——`ObjType` 判 1..=7 与
/// 12 共八值；新增对象类型须同步，见模块注释）。
pub const SEND_PROBE_TYPES: [u8; 7] = [1, 2, 3, 4, 5, 6, 12];

/// 类型是否可等（wait 面：五类词表判）。
///
/// 等价律：`waitable(ty)` 为真 ⟺ 旧五类型链中某 `get_raw(h, ty, 0)`
/// 会命中（活值时）——词表外的类型（Vmo/Vmar 等）旧链全拒 -EBADF，
/// 新路径单查后按同码拒，行为逐位一致。
#[inline]
pub fn waitable(ty: u8) -> bool {
    WAITABLE_TYPES.contains(&ty)
}

/// 随行迁移资格判（send 预检环体判定律）。
///
/// 输入 = 类型无关单查的结果（`Ok(rights)` / `Err(code)`——
/// `rights_of` 面）；输出 = 旧探测链环体的判定位：
/// - `Err(_)`（死值：全类型未命中）→ `Some(ERR_DEAD)`（-EBADF；
///   探活语义内联于探测）；
/// - `Ok(r)` 且 `r & RIGHT_TRANSFER == 0`（活值缺迁移位）→
///   `Some(ERR_DENIED)`（-EACCES；能力委托流转受控面）；
/// - `Ok(_)` 带位 → `None`（过门）。
///
/// 等价律：对任意 handle，旧链（7 类型逐一 `get_raw(need=0)` 探测后
/// 按同门折叠）与本函数输出**逐位一致**——kernel 侧换用后错误码面
/// 不变，host 对拍钉板执法。
#[inline]
pub fn probe_transfer_ok(probe: Result<u32, i32>) -> Option<i32> {
    match probe {
        Err(_) => Some(ERR_DEAD),
        Ok(r) if r & RIGHT_TRANSFER == 0 => Some(ERR_DENIED),
        Ok(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 值域钉板：错误码/权利位镜像与 kernel abi_gen 冻结值逐位一致。
    /// （ERR_BAD_HANDLE=-9、ERR_ACCESS_DENIED=-13、RIGHT_TRANSFER=1<<5
    /// ——镜像面防漂移的最小执法。）
    #[test]
    fn mirrors_frozen_abi_values() {
        assert_eq!(ERR_DEAD, -9);
        assert_eq!(ERR_DENIED, -13);
        assert_eq!(RIGHT_TRANSFER, 1 << 5);
    }

    /// 词表钉板：wait 面五类全表 + 链序冻结（Event→Channel→Process→
    /// Thread→SuspendToken = 6,5,1,2,12）。
    #[test]
    fn waitable_table_frozen() {
        assert_eq!(WAITABLE_TYPES, [6, 5, 1, 2, 12]);
        // 全 ObjType 词域（1..=12）恰五真：1/2/5/6/12——其余（Vmo3/
        // Vmar4/EventPair7/Interrupt8/Resource9/Timer10/Job11）全拒。
        let trues: Vec<u8> = (1u8..=12).filter(|&t| waitable(t)).collect();
        assert_eq!(trues, vec![1, 2, 5, 6, 12]);
    }

    /// 词表钉板：send 探测表 = KernelObj 全变体镜像七值（1,2,3,4,5,6,12
    /// ——含 Vmo/Vmar：随行迁移不限制对象类型，与 wait 面词域不同源）。
    #[test]
    fn send_probe_table_frozen() {
        assert_eq!(SEND_PROBE_TYPES, [1, 2, 3, 4, 5, 6, 12]);
        // 去重不变式（镜像表防重防漏的自检面）：排序后相邻比对。
        let mut sorted = SEND_PROBE_TYPES;
        sorted.sort_unstable();
        assert!(sorted.windows(2).all(|w| w[0] != w[1]), "send 探测表有重复值");
    }

    /// 折叠律三态：死值 -EBADF / 活值缺位 -EACCES / 带位过门 None。
    #[test]
    fn probe_transfer_ok_three_faces() {
        // 死值（lookup 拒——槽空/越界/代号不符/预留位任一）→ -EBADF。
        assert_eq!(probe_transfer_ok(Err(-9)), Some(ERR_DEAD));
        assert_eq!(probe_transfer_ok(Err(-999)), Some(ERR_DEAD)); // 任意 err 同码
        // 活值缺 TRANSFER 位 → -EACCES。
        assert_eq!(probe_transfer_ok(Ok(0)), Some(ERR_DENIED));
        assert_eq!(probe_transfer_ok(Ok(1 << 0)), Some(ERR_DENIED));
        // 活值带位（含任意其它位组合）→ 过门。
        assert_eq!(probe_transfer_ok(Ok(RIGHT_TRANSFER)), None);
        assert_eq!(probe_transfer_ok(Ok(0xff)), None);
    }

    /// 等价对拍：旧 7 类型探测链 vs `probe_transfer_ok` 单查折叠——
    /// 全 rights 域 × 死活两态逐位一致。
    ///
    /// 旧链模型：每类型探测命中即取槽 rights（need=0 恒不触发 -EACCES
    /// 臂），全未命中 = 死值 -EBADF；随后 TRANSFER 门折叠。模型里
    /// 「命中类型」不改变输出（rights 只与槽绑定）——即两路径等价的
    /// 结构性证据。
    #[test]
    fn probe_chain_equivalence_all_rights_domain() {
        // 活值全 rights 域（含 0）：门判 = 位有无。
        for r in 0u32..256 {
            let legacy = if r & RIGHT_TRANSFER == 0 {
                Some(ERR_DENIED)
            } else {
                None
            };
            assert_eq!(probe_transfer_ok(Ok(r)), legacy, "rights={r}");
        }
        // 死值：旧链任一错误码（含跨类型混合——模型不区分来源）折叠同
        // -EBADF。
        for e in [-9, -13, -22, -3] {
            assert_eq!(probe_transfer_ok(Err(e)), Some(ERR_DEAD));
        }
    }

    /// 边界：nhandles 域内每枚独立判（环体无跨枚状态——旧链环体与新
    /// 单查环体共享此性质，折叠不引入次序敏感）。
    #[test]
    fn per_handle_independence() {
        let hs = [Ok(RIGHT_TRANSFER), Ok(0), Err(-9), Ok(RIGHT_TRANSFER | 4)];
        let out: [Option<i32>; 4] = hs.map(probe_transfer_ok);
        assert_eq!(out, [None, Some(ERR_DENIED), Some(ERR_DEAD), None]);
    }
}
