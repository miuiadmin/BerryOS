//! klog! 宏（阶段五第 14 步，`2026-11-15-启动产物与状态体系全面建设计划.md`
//! §可观测契约）：新内核日志行统一 `klog!(lvl, ...)` 形态——
//! `[ns][I|W|E] msg` **整行单 PRINT_LOCK 窗发射**。
//!
//! - **ns 时间戳**：`cntvct()` + `ticks_to_ns()`（boot 早期可用——慢路直读
//!   CNTFRQ 不依赖 timer init 顺序，`timer.rs::ticks_to_ns` 注）。
//! - **no_std 无 alloc**：`core::fmt::Write` 到栈缓冲（`main.rs` panic 的
//!   UartWriter 先例），一次 `writeln!` 拼整行后单窗发射（G44 拼行 API：
//!   多段 uart_puts 原子的宏面等价物）。
//! - **锁纪律**：发射经 HAL `with_print_lock`（preempt 计数包络）。**禁在
//!   PRINT_LOCK 窗内调 klog!**（自递归取锁 = 同核自旋死锁）；panic /
//!   lockdep 绕锁通道不走本宏（panic handler 直写 UART bypass，纪律不变）。
//!   IRQ 语境调用须自带 preempt_count 门（`pl061_notify` 先例）。
//! - **行帽 192B**（A60 控制台原子消息帽同值）：超长截断保尾换行（诚实
//!   截断，fmt Err 不静默吞）。
//! - **串口镜像**：每字符经 `uart_putc` 进 HAL klog 环（阶段五第 16 步，
//!   `arch/aarch64/src/klog.rs`）——宏行自动进 0x84 快照面，零第二写点。
//! - 存量 `uart_puts` 不强制迁移（渐进债，计划§设计原则 6）。

use core::fmt::Write;

/// klog 行帽（含 `[ns][I] ` 前缀与尾换行；A60 控制台原子消息帽同值）。
pub const KLOG_LINE_MAX: usize = 192;

/// 日志级别（`klog!` 首参；编码 `[I|W|E]` 单字符）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    /// v1 预留（过滤面 v2 起消费；首个 Error 消费点落地前豁免 dead_code）。
    #[allow(dead_code)]
    Error,
}

impl Level {
    /// 行内级别标记字符。
    fn tag(self) -> char {
        match self {
            Level::Info => 'I',
            Level::Warn => 'W',
            Level::Error => 'E',
        }
    }
}

/// 定长行缓冲（栈上；超帽截断保尾换行）。
struct LineBuf {
    buf: [u8; KLOG_LINE_MAX],
    len: usize,
}

impl Write for LineBuf {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let room = KLOG_LINE_MAX - self.len;
        if s.len() > room {
            // 截断：装得下的部分照收，Err 上浮让调用方停笔（ writeln! 对
            // 返回值只透传，emit 尾部统一补换行）。
            let n = room.min(s.len());
            self.buf[self.len..self.len + n].copy_from_slice(&s.as_bytes()[..n]);
            self.len += n;
            return Err(core::fmt::Error);
        }
        self.buf[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
        self.len += s.len();
        Ok(())
    }
}

/// 发射一条 klog 行（宏的实体）：拼 `[ns][tag] msg\n` → 单 PRINT_LOCK 窗
/// 整行 `uart_puts`。ns 取发射时刻（换算源 = `timer::ticks_to_ns` 慢路，
/// boot 早期直读 CNTFRQ 可用）。
pub fn emit(level: Level, args: core::fmt::Arguments) {
    let ns = berry_hal_aarch64::timer::ticks_to_ns(berry_hal_aarch64::timer::cntvct());
    let mut w = LineBuf { buf: [0; KLOG_LINE_MAX], len: 0 };
    // 溢出面（>192B）只丢尾——头部前缀与已拼段保留，尾换行统一补。
    let _ = writeln!(w, "[{}][{}] {}", ns, level.tag(), args);
    if w.len == 0 || w.buf[w.len - 1] != b'\n' {
        if w.len < KLOG_LINE_MAX {
            w.buf[w.len] = b'\n';
            w.len += 1;
        } else {
            // 帽满无余地：末字节让位换行（消息丢 1 字节保行完整性）。
            w.buf[KLOG_LINE_MAX - 1] = b'\n';
        }
    }
    let line = &w.buf[..w.len];
    // 整行单窗发射（G44）。调用方不得已持 PRINT_LOCK（模块头纪律）。
    berry_hal_aarch64::kern_lock::with_print_lock(|| {
        berry_hal_aarch64::uart_puts(line);
    });
}

/// `[ns][I|W|E] msg` 整行单窗发射的日志宏（形态契约见模块头）。
///
/// 用法：`klog!(Info, "dpg: watchdog fired")` /
/// `klog!(Warn, "x={}", x)`——首参 [`Level`] 变体名，余参 `format_args!`
/// 语法。**禁在 PRINT_LOCK 窗内使用**（自递归取锁死锁）；panic / lockdep
/// 绕锁通道不使用（直写 UART bypass 纪律不变）。
// macro_export：bin crate 内 `crate::klog!` 路径调用的前提（dpg 迁移点用
// 全路径，防与局部名混淆）。
#[macro_export]
macro_rules! klog {
    ($lvl:ident, $($arg:tt)+) => {
        $crate::klog::emit($crate::klog::Level::$lvl, format_args!($($arg)+))
    };
}
