//! rtc —— 壁钟注入面（波一刀 B：状态栏可信化；研究底稿
//! `research/桌面UX审计/02-系统导航与状态栏.md` §6.1 M1①）。
//!
//! 背景：本仓无 RTC 设备驱动（全仓 grep 无 rtc 驱动；QEMU virt 亦未挂
//! goldfish-rtc），但 fw_cfg 具名文件通道在位——`opt/berry/display`
//! （宿主 `tools/berry/src/qemu.rs` 注入 / 内核 `main.rs` 消费）是现役
//! 先例。同形最小做：宿主 berry 启动镜像时把**当下 epoch 秒**（十进制
//! ASCII，`-fw_cfg name=opt/berry/rtc,string=<secs>`）写进 fw_cfg；本
//! 模块 boot 期单次消费：epoch 秒 + 同刻 MONOTONIC 纳秒锚 → 双
//! AtomicU64 全局（写者 = `rust_main` 单线程段，先于一切用户态；此后
//! 只读无竞，Relaxed 足够）。
//!
//! 消费面：`sys_clock_read(CLOCK_REALTIME)` = `epoch_s×1e9 +
//! (now_mono − 锚)`（syscall.rs）。注入缺席/畸形（非十进制/超 20B/0）
//! → epoch 保持 0 → REALTIME 面 -ENOTSUP——用户态降级静态占位（launcher
//! 12:00），既有 -kernel 冒烟拓扑（零 fw_cfg 具名件）逐位零漂移。
//!
//! 精度口径（如实）：锚 = 读 fw_cfg 时刻的 CNTVCT 换算，宿主 string 值
//! = spawn 前取值——TCG boot 秒级时延即本钟的固有漂移界（分钟级摆字
//! 语境可忽略）。epoch 为 UTC 口径；宿主本地时区换算挂账波二。

use core::sync::atomic::{AtomicU64, Ordering};

use berry_hal_aarch64::fwcfg;

/// 注入的 epoch 秒（0 = 缺席/未注入——REALTIME 不可用的哨兵值）。
static EPOCH_S: AtomicU64 = AtomicU64::new(0);
/// 与 epoch 同刻的 MONOTONIC 纳秒锚（读 fw_cfg 后立即取样）。
static BOOT_MONO_NS: AtomicU64 = AtomicU64::new(0);

/// fw_cfg 具名项名（display 先例同族；宿主 qemu.rs 注入面单源约定）。
const RTC_FWCFG_NAME: &str = "opt/berry/rtc";
/// 注入串长度帽（十进制 epoch ≤ 11 位 + NUL 填充余量；display 32B 帽同款防畸形大项）。
const RTC_MAX_LEN: usize = 20;

/// CNTVCT/CNTFRQ → MONOTONIC 纳秒（sys_clock_read 同式：先商后余防
/// 中间溢出——ticks×1e9 在 ~5 分钟处溢 u64）。
fn vct_to_ns() -> Option<u64> {
    let ticks = berry_hal_aarch64::timer::cntvct();
    let freq = berry_hal_aarch64::timer::cntfrq();
    if freq == 0 {
        return None; // 频率不可读（真机异常，同 sys_clock_read 语境）
    }
    Some((ticks / freq) * 1_000_000_000 + ((ticks % freq) * 1_000_000_000) / freq)
}

/// boot 期单次消费（`rust_main` 早期调用——ktest/svcmgr 之前，EFI 与
/// -kernel 两路同点）。缺席/畸形零行零扰动；成功打一行串口证据
/// （`RTC ok epoch=<n>`，DISP ok 行同款信息形态，非 smoke 断言面）。
pub fn boot_init() {
    let Some(e) = fwcfg::find_file(RTC_FWCFG_NAME) else { return };
    if e.size == 0 || e.size as usize > RTC_MAX_LEN {
        return;
    }
    let mut buf = [0u8; RTC_MAX_LEN];
    if !fwcfg::read_to(&e, &mut buf[..e.size as usize]) {
        return;
    }
    let Ok(s) = core::str::from_utf8(&buf[..e.size as usize]) else { return };
    // QEMU string 项按 4B 对齐 NUL 填充——display 同款 trim 口径。
    let s = s.trim_matches(|c: char| c == '\0' || c.is_ascii_whitespace());
    let Ok(epoch) = s.parse::<u64>() else { return };
    if epoch == 0 {
        return; // 0 = 哨兵值，视为未注入
    }
    let Some(mono) = vct_to_ns() else { return };
    EPOCH_S.store(epoch, Ordering::Relaxed);
    BOOT_MONO_NS.store(mono, Ordering::Relaxed);
    berry_hal_aarch64::uart_puts(b"RTC ok epoch=");
    berry_hal_aarch64::uart_put_dec(epoch);
    berry_hal_aarch64::uart_puts(b"\n");
}

/// REALTIME 纳秒（`None` = 无注入 → 调用方折 -ENOTSUP）。
pub fn realtime_ns() -> Option<u64> {
    let epoch = EPOCH_S.load(Ordering::Relaxed);
    if epoch == 0 {
        return None;
    }
    let boot = BOOT_MONO_NS.load(Ordering::Relaxed);
    let now = vct_to_ns()?;
    // 饱和防御：boot 后回绕/巨值不 panic（ABI「永不 panic」面）。
    Some(epoch.saturating_mul(1_000_000_000).saturating_add(now.saturating_sub(boot)))
}
