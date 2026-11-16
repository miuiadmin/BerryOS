//! berry-dpgface —— pager 表批窗 ABI 装配纯面（W3-Q73，tblwin 模块）。
//!
//! **动机**：`sys_pager_supply_mapped_table`（0x24，X-6 刀① 表批）的
//! 入参装配段——n ∈ [1,32] 门、`PagerWindow` 16B 小端拼装（dv u32 /
//! sv u32 / delta i64）、delta 页对齐预检（fail-fast 先于门控）——此前
//! 只活在 `kernel/src/syscall.rs` 的 handler 内联里（kernel crate host
//! 不可达）。n 门的「全宽读」（x2 高 32 位脏值同拒，不做 u32 截断
//! 受纳）、`% PAGE_SIZE` 对 i64 负 delta 的 Rust 语义（-4096 % 4096 == 0
//! 放行、-1 拒）、拷入-预检-门控的**判序**（EFAULT 透传先于
//! delta 拒、delta 拒先于 -EBADF 门控）无 host 钉板。本模块把「字节 →
//! 窗数组」的装配判定律按值域抽成纯函数 + 表驱动测试，kernel handler
//! 壳消费同一函数（零第二实现——W3-Q33 polface tblabi / W3-Q34 wqmatch
//! waitm 同款解码纪律，先例族第 22 件；dpgface 主面〔W3-Q24 槽池判定
//! 律〕同源延续，零新 crate）。
//!
//! 抽离面（与消费侧逐字对齐，改这里的语义 = 改 syscall 行为）：
//! - [`PAGER_TABLE_MAX`]：单次收表上限（X-6 冻结 N=32）——与 kernel
//!   旧 `PAGER_TABLE_MAX` / dpg `EREQ_SLOTS`（= 主面 [`crate::SLOTS`]）/
//!   svcmgr `PAGER_SLOTS` 四源镜像；抬帽走 len 兼容不破冻。
//! - [`PAGE_SIZE`]：delta 页对齐判的除数（镜像 `berry_mm::va::PAGE_SIZE`
//!   ——face crate 零依赖纪律，字面值 4096 钉板防漂移）。
//! - [`n_gate`]：n ∈ [1, MAX] 门——**0 拒**（与 Q33 policy 空表合法相
//!   反：0x24 语义「至少一窗」，旧内核 `n == 0 ||` 逐字序钉板）；超限
//!   同拒（全宽 usize 读——kernel 侧 `tf.regs[2] as usize` 先行，本面
//!   不做截断受纳）。
//! - [`decode_window`]：单窗 16B 小端拼装（dv/sv/delta 三域）+ delta
//!   页对齐预检——非对齐 `BadDelta`（fail-fast 先于门控的参数面整批
//!   拒；i64 可负——恒等选源常数，同 0x23）。
//! - [`decode_table`]:整表装配单源（n 门 + 逐窗循环 + 预检）——kernel
//!   handler 的 staging-预检段逐字等价替换（`copy_from_user` 电压与
//!   栈 staging 留壳；错误码映射 -EINVAL 留 kernel）。
//! - [`window_to_bytes`]：窗 → 16B 字节镜像（host 往返测试用——往返
//!   即「装配律冻结」的指纹；svcmgr 用户态侧同表对拍锚）。
//!
//! 不钉的面：`copy_from_user` 电压（-EFAULT 透传）、门控（dv
//! Vmo+RIGHT_SET_POLICY / sv Vmo+RIGHT_READ 的 get_raw——物理臂）、
//! retain/release 跨睡眠钉活、收货环（continue-take / block_current /
//! count-wins）——留 kernel syscall.rs 机械壳（本面零 unsafe、零 HAL）。
//!
//! 判序冻结（X-6 §2.1/§2.2 语义裁定的装配段投影）：EFAULT（拷入败）
//! → delta 拒（预检环）→ -EBADF（门控）——预检在门控**之前**（懒门控
//! 会造半供给 + 晚 -EBADF 与 count-wins 打架；boot 探测 0x24(dv:0,n:1)
//! 依赖门败早返区分新核/旧核，装配段任何重排都会改变可观测错误码序）。

use crate::SLOTS;

/// 单次收表上限（X-6 冻结 N=32）。四源镜像执法：本帽 = kernel
/// `PAGER_TABLE_MAX` = dpg `EREQ_SLOTS`（= 主面 [`crate::SLOTS`] 的
/// const assert 单源）= svcmgr `PAGER_SLOTS`——见 kernel abi_gen 注。
pub const PAGER_TABLE_MAX: usize = SLOTS;

/// 页尺寸（delta 对齐判除数）——镜像 `berry_mm::va::PAGE_SIZE`（face
/// crate 零依赖纪律；字面值钉板，测试另有冻结锚）。
pub const PAGE_SIZE: i64 = 4096;

/// 每窗字节数（repr(C) u32+u32+i64 = 16B，无填充）。
pub const WINDOW_BYTES: usize = 16;

/// 表批窗纯投影——kernel `PagerWindow`（#[repr(C)]）的值域镜像。
///
/// kernel 侧保留 repr(C) 结构体本体（ABI 面 + ktest 直造）；本结构
/// 承载装配判定所需的值（decode 产物），字段语义逐字同源。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Window {
    /// 目标 demand VMO 句柄（门控 Vmo+RIGHT_SET_POLICY——sv==0 零页臂
    /// 判在门控段，非装配面）。
    pub dv: u32,
    /// 源 VMO 句柄（0 = 零页臂——装配面原样放行，0 语义留门控段）。
    pub sv: u32,
    /// 恒等选源常数（页对齐 i64 可负——同 0x23）。
    pub delta: i64,
}

/// [`n_gate`] 拒绝态（消费侧映射 -EINVAL——错误码留 kernel）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NGate {
    /// n == 0：0x24 至少一窗（`n == 0 ||` 逐字序——与 Q33 空表合法
    /// 相反的语义钉板，防误抄 policy 口径）。
    Empty,
    /// n > [`PAGER_TABLE_MAX`]：越界整批拒（全宽读——x2 高 32 位脏值
    /// 同拒，不做 u32 截断受纳）。
    TooBig,
}

/// 收表条数门：0 与超限皆拒（-EINVAL 面）。判序即旧内核逐字序
/// （`n == 0 || n > MAX`——Empty 先于 TooBig，短路左先；两态同码
/// 无观测差，钉板保判序稳定防误加「0 = 空表合法」）。
pub fn n_gate(n: usize) -> Result<(), NGate> {
    if n == 0 {
        return Err(NGate::Empty);
    }
    if n > PAGER_TABLE_MAX {
        return Err(NGate::TooBig);
    }
    Ok(())
}

/// [`decode_window`] 拒绝态（消费侧映射 -EINVAL）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinErr {
    /// delta 非页对齐（fail-fast 参数面整批拒——先于门控；i64 负值
    /// 合法域：-4096 对齐放行、-1 拒）。
    BadDelta,
}

/// 单窗 16B 小端拼装 + delta 页对齐预检。
///
/// 拼装永不失败（定长字节）；唯一门 = `delta % PAGE_SIZE != 0`
/// （旧内核逐字：`w.delta % PAGE_SIZE as i64 != 0`——Rust `%` 对负
/// 被除数取同号余：-4096 % 4096 == 0 放行、-4095 == -4095 拒）。
pub fn decode_window(raw: &[u8]) -> Result<Window, WinErr> {
    let dv = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
    let sv = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
    let delta = i64::from_le_bytes(
        [raw[8], raw[9], raw[10], raw[11], raw[12], raw[13], raw[14], raw[15]],
    );
    if delta % PAGE_SIZE != 0 {
        return Err(WinErr::BadDelta);
    }
    Ok(Window { dv, sv, delta })
}

/// [`decode_table`] 拒绝态（装配段全谱——判序即 kernel handler 可观测
/// 错误码序：N 门 → 逐窗预检首错即停）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TableErr {
    /// n 门拒（Empty/TooBig 两态内嵌）。
    NGate(NGate),
    /// 第 `idx` 窗 delta 非对齐（首错即停——旧内核 return 逐字序；
    /// idx 供测试锚定首错位置，消费侧同码不分流）。
    BadDelta { idx: usize },
}

/// 整表装配单源：n 门 + 逐窗 16B 拼装 + delta 预检首错即停。
///
/// kernel handler 的 staging-预检段（n 门 → 拷入 → 预检环）逐字等价
/// 替换——`copy_from_user` 电压（-EFAULT）在调用侧先行，本函数从
/// 「raw 字节已就位」处接管。返回前 `MAX` 项窗数组 + 实收 n（尾部
/// 零窗——kernel 侧 `[PagerWindow{0,0,0}; MAX]` staging 初值同态）。
pub fn decode_table(raw: &[u8], n: usize) -> Result<([Window; SLOTS], usize), TableErr> {
    n_gate(n).map_err(TableErr::NGate)?;
    let mut wins = [Window::default(); SLOTS];
    for i in 0..n {
        let o = i * WINDOW_BYTES;
        match decode_window(&raw[o..o + WINDOW_BYTES]) {
            Ok(w) => wins[i] = w,
            Err(WinErr::BadDelta) => return Err(TableErr::BadDelta { idx: i }),
        }
    }
    Ok((wins, n))
}

/// 窗 → 16B 小端字节镜像（host 往返指纹；svcmgr `PagerWindow` 用户态
/// 双镜像同表——§8.3 原型层对拍锚）。
pub fn window_to_bytes(w: &Window) -> [u8; WINDOW_BYTES] {
    let mut b = [0u8; WINDOW_BYTES];
    b[0..4].copy_from_slice(&w.dv.to_le_bytes());
    b[4..8].copy_from_slice(&w.sv.to_le_bytes());
    b[8..16].copy_from_slice(&w.delta.to_le_bytes());
    b
}
