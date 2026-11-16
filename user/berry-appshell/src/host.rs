//! host —— 消费者绑定表（libc 面注入点）。
//!
//! appshell 不直连 berry-libc（其 crt0 `global_asm!` 为 ELF 目标写法、
//! libc 同名符号对 host libSystem 插入——结构性不可 host `cargo test`，
//! berry-libc lib.rs「实况勘定」注记；user 工作区成员 crate 依赖它无先例
//! 且不可行）。凡是落在 libc 侧的面——诊断输出（eprintln）、常驻 park、
//! FAT 数据盘 fd 操作——都以 fn 指针进 [`Host`]，由 std musl 消费者在
//! boot 期 [`bind`] 一次、进程终身不变（boot 单线程无竞态面，Acquire/
//! Release 配对纯形式纪律）。
//!
//! **未绑定的防御语义**：`diag` 静默（no_std 无 console 可写——绑定缺失
//! 属消费者装配 bug，文档面如实）、`park` panic、`fs` 视为缺席。消费者
//! 模板见 crate 头注。

use core::sync::atomic::{AtomicPtr, Ordering};

/// 绑定表（`&'static`——消费者以 `static` 字面量装配；fn 指针面全
/// Copy，按值取用）。
#[derive(Clone, Copy)]
pub struct Host {
    /// 诊断行前缀（桶名——"launcher"/"settings"；与抽取前各诊断行字节
    /// 一致性的锚点）。
    pub tag: &'static str,
    /// 诊断一行（不含换行——launcher 绑 `|s| eprintln!("{s}")`）。
    pub diag: fn(&str),
    /// 常驻收口（失败不退出：监督/重启归 svcmgr 域——launcher 绑
    /// `berry_libc::park`）。
    pub park: fn() -> !,
    /// FAT 数据盘 fd 面（`None` = 冻结相无 fs gift——conf 面结构性缺席，
    /// 调用方以 readiness 门先导）。
    pub fs: Option<Fs>,
}

/// FAT 数据盘 fd 操作面（berry-libc io/fs 的函数指针镜像——签名统一为
/// 字节指针/i32/isize，c_char/c_void 差由消费者包装 fn 抹平）。
#[derive(Clone, Copy)]
pub struct Fs {
    /// `open(path, flags) -> fd`（NUL 结尾路径；`<0` = 失败 + errno）。
    pub open: unsafe fn(*const u8, i32) -> i32,
    /// `read(fd, buf, len) -> n`（`<0` 错 / `0` EOF）。
    pub read: fn(i32, *mut u8, usize) -> isize,
    /// `write(fd, buf, len) -> n`（`<0` 错 / `0` = 协议破缺面）。
    pub write: fn(i32, *const u8, usize) -> isize,
    /// `close(fd)`（读 fd 幂等；写 fd = commit 面）。
    pub close: fn(i32) -> i32,
    /// `errno()`（最近一次失败码）。
    pub errno: fn() -> i32,
}

/// 绑定（boot 期单次；重复绑定 = 后写覆盖，防御面非契约）。
pub fn bind(h: &'static Host) {
    HOST.store(h as *const Host as *mut Host, Ordering::Release);
}

/// 当前绑定（未绑 = 静默默认——见模块头注防御语义）。
pub(crate) fn host() -> &'static Host {
    let p = HOST.load(Ordering::Acquire);
    if p.is_null() {
        &DEFAULT
    } else {
        unsafe { &*p }
    }
}

/// 诊断一行（`"{tag}: {msg}"`——与抽取前 eprintln 字节一致）。
pub(crate) fn diag(msg: &str) {
    let h = host();
    (h.diag)(&alloc::format!("{}: {}", h.tag, msg));
}

static HOST: AtomicPtr<Host> = AtomicPtr::new(core::ptr::null_mut());

/// 未绑定默认（diag 静默/park panic/fs 缺席）。
static DEFAULT: Host = Host {
    tag: "appshell",
    diag: silent_diag,
    park: unbound_park,
    fs: None,
};

fn silent_diag(s: &str) {
    let _ = s;
}

fn unbound_park() -> ! {
    core::panic!("appshell: host not bound (bind() before use)")
}

// —— host 测试面：绑定/缺省/诊断格式三律 ——
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::Mutex;

    /// 诊断捕获（fn 指针无闭包捕获——经 static 中转）。
    static DIAG_N: AtomicUsize = AtomicUsize::new(0);
    static DIAG_BUF: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn capture_diag(s: &str) {
        DIAG_N.fetch_add(1, Ordering::Relaxed);
        DIAG_BUF.lock().unwrap().push(s.to_string());
    }

    fn never_park() -> ! {
        panic!("park must not run in tests")
    }

    #[test]
    fn unbound_default_is_silent_panic_absent_fs() {
        // 默认表（不绑定时 host() 的兜底）：fs 缺席 + diag 静默不炸 +
        // tag 兜底 "appshell"。直接验 DEFAULT 本尊——测试并行下 HOST
        // 绑定态不构成顺序依赖。
        let h: &Host = &DEFAULT;
        assert_eq!(h.tag, "appshell");
        assert!(h.fs.is_none());
        (h.diag)("silent"); // 静默默认：不 panic 不输出
    }

    #[test]
    fn diag_line_is_tag_prefixed() {
        static H: Host = Host {
            tag: "probe",
            diag: capture_diag,
            park: never_park,
            fs: None,
        };
        bind(&H);
        diag("rpc send err=-5");
        let buf = DIAG_BUF.lock().unwrap();
        let last = buf.last().unwrap();
        assert_eq!(last, "probe: rpc send err=-5");
        // 绑定后 tag/park/fs 三面就是绑定表本尊。
        assert_eq!(host().tag, "probe");
    }
}
