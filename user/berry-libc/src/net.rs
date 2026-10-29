//! # net —— socket ABI 纯函数面（A3-1）
//!
//! `docs/事件循环A3设计草案.md` §3.1/§3.3/§3.4 的 libc 侧落点：
//! - 字节序四件（htons/ntohs/htonl/ntohl——sockaddr_in 构造消费面）；
//! - sockaddr/addrinfo ABI `#[repr(C)]` + offset_of 全字段编译期钉板
//!   （C-2a Stat 布局案同族执法：「同尺寸不同字段序 = 最阴的静默错」；
//!   布局对源 = libc crate linux_like 非 android 分支——addrinfo 的
//!   ai_addr 紧随 ai_addrlen（glibc 序，musl 同；hermit 是 canonname
//!   先行的异序反例，不可跨 OS 平移））；
//! - socket fd 表宿主（`SOCKETS` static = berry-fsclient `SocketTable`，
//!   fd 15..=22 段）+ fcntl F_GETFL/F_SETFL 旗投影（stubs.rs fcntl 的
//!   socket 腿消费）；
//! - getaddrinfo/freeaddrinfo 真实现（**IP 字面量档**：node 必须点分
//!   四段 IPv4，非字面量 → EAI_NONAME——DNS 档留 A4+，草案 §5「不做什么」）。
//!
//! socket/connect/bind/listen/accept/send/recv 桩转真 = **A3-2**（RPC 面
//! 随该刀接）：本刀 socket() 仍 ENOSYS，表只经 fcntl 旗投影路径可达——
//! 先立「表 + 旗投影 + ABI 钉板 + getaddrinfo」的地基。sockaddr_storage
//! 零消费方，随 A3-2 的 accept 出参面真消费时钉板（不长第二套纪律）。
//!
//! 本模块无 host 测试（berry-tls 先例：libc 同名符号在 host 测试二进制
//! 对 libSystem 符号插入劫自旋——test-host recipe 注释）；钉板全走编译
//! 期 const assert，none/musl 双 target `just build-user` 执法。

use crate::{c_char, c_int, c_void};
use berry_fsclient::{AF_INET, IPPROTO_TCP, SockEntry, SOCK_STREAM, SocketTable, parse_ipv4};

// —— 字节序四件（`arpa/inet.h`；小端 AArch64 上 to_be/from_be = 字节
//    交换——核心一行族，`rotate` 非必需）——

#[no_mangle]
pub extern "C" fn htons(v: u16) -> u16 {
    v.to_be()
}

#[no_mangle]
pub extern "C" fn ntohs(v: u16) -> u16 {
    u16::from_be(v)
}

#[no_mangle]
pub extern "C" fn htonl(v: u32) -> u32 {
    v.to_be()
}

#[no_mangle]
pub extern "C" fn ntohl(v: u32) -> u32 {
    u32::from_be(v)
}

// —— sockaddr ABI（musl aarch64 asm-generic 口径，草案 §3.4）——

pub type sa_family_t = u16;
pub type in_port_t = u16;
pub type in_addr_t = u32;

/// IPv4 套接字地址（16B）：sin_port/sin_addr 网络序（BE——构造经
/// htons/htonl），sin_zero 恒零。
#[repr(C)]
pub struct SockAddrIn {
    pub sin_family: sa_family_t,
    pub sin_port: in_port_t,
    pub sin_addr: in_addr_t,
    pub sin_zero: [u8; 8],
}

/// 通用套接字地址（16B：family + 14B 数据区）。
#[repr(C)]
pub struct SockAddr {
    pub sa_family: sa_family_t,
    pub sa_data: [u8; 14],
}

/// getaddrinfo 结果链节点（48B；libc crate linux_like 字段序：ai_addrlen
/// u32 + 4B 自然填充 → ai_addr@24。std（musl 预编译）经 libc crate 绑定
/// 读 ai_addr/ai_next——布局错位 = 调用方读野指针，编译期钉死）。
#[repr(C)]
pub struct AddrInfo {
    pub ai_flags: c_int,
    pub ai_family: c_int,
    pub ai_socktype: c_int,
    pub ai_protocol: c_int,
    pub ai_addrlen: u32,
    /// 显式填充（repr(C) 自然填充的具名化——加字段时错位炸得更响）。
    pub __pad: u32,
    pub ai_addr: *mut SockAddr,
    pub ai_canonname: *mut c_char,
    pub ai_next: *mut AddrInfo,
}

// 布局钉板（fs.rs Stat 全字段钉板同族；相邻同型对 ai_family↔ai_socktype
// ↔ai_protocol、ai_addr↔ai_canonname↔ai_next 全在覆盖内）。
const _: () = {
    use core::mem::{offset_of, size_of};
    assert!(size_of::<SockAddrIn>() == 16);
    assert!(offset_of!(SockAddrIn, sin_family) == 0);
    assert!(offset_of!(SockAddrIn, sin_port) == 2);
    assert!(offset_of!(SockAddrIn, sin_addr) == 4);
    assert!(offset_of!(SockAddrIn, sin_zero) == 8);
    assert!(size_of::<SockAddr>() == 16);
    assert!(offset_of!(SockAddr, sa_family) == 0);
    assert!(size_of::<AddrInfo>() == 48);
    assert!(offset_of!(AddrInfo, ai_flags) == 0);
    assert!(offset_of!(AddrInfo, ai_family) == 4);
    assert!(offset_of!(AddrInfo, ai_socktype) == 8);
    assert!(offset_of!(AddrInfo, ai_protocol) == 12);
    assert!(offset_of!(AddrInfo, ai_addrlen) == 16);
    assert!(offset_of!(AddrInfo, ai_addr) == 24);
    assert!(offset_of!(AddrInfo, ai_canonname) == 32);
    assert!(offset_of!(AddrInfo, ai_next) == 40);
};

// —— socket fd 表（宿主；单线程 static mut 直写——fs.rs FDT/io.rs
//    ALIASES 同口径）——

static mut SOCKETS: SocketTable = SocketTable::new();

/// fd 是在册 socket（15..=22 且槽非空）——stubs.rs fcntl socket 腿的
/// 分派谓词（fs miss 后查；io.rs read/write/close 分派链同序接入 =
/// A3-2）。
pub(crate) fn sock_valid(fd: c_int) -> bool {
    // SAFETY: 只读；唯一写点 = 本模块（单线程 lib.rs ABI 口径）。
    unsafe { SOCKETS.get(fd).is_some() }
}

/// F_GETFL 旗投影：socket fd 恒读写两用（POSIX 访问模式位 O_RDWR），
/// O_NONBLOCK 按表项旗。非在册 socket = None（EBADF 面，调用方收口）。
pub(crate) fn sock_getfl(fd: c_int) -> Option<c_int> {
    // SAFETY: 只读借用（生命周期不跨调用）。
    unsafe {
        SOCKETS
            .get(fd)
            .map(|e| crate::fs::O_RDWR | if e.nonblock { crate::fs::O_NONBLOCK } else { 0 })
    }
}

/// F_SETFL 旗写入：v1 只认 O_NONBLOCK 一位（其余位忽略不拒——std 的
/// 读改写形态 F_GETFL→改位→F_SETFL 天然兼容：读回什么写回什么，只有
/// O_NONBLOCK 真生效）。非在册 = None（EBADF 面）。
pub(crate) fn sock_setfl(fd: c_int, arg: c_int) -> Option<()> {
    // SAFETY: get_mut 短借写旗（单线程无并发 take）。
    unsafe { SOCKETS.get_mut(fd).map(|e| e.nonblock = (arg & crate::fs::O_NONBLOCK) != 0) }
}

/// 表安装口（A3-2 的 socket()/connect()/accept() 转真刀消费；本刀立
/// API 面——语义冻结挂账，vnet `close_sid` 同款先例）：fd 15..=22 最低
/// 空位，满表 None → EMFILE。
#[allow(dead_code)]
pub(crate) fn sock_alloc(e: SockEntry) -> Option<c_int> {
    // SAFETY: 单线程 static mut 直写（lib.rs ABI 口径）。
    unsafe { SOCKETS.alloc(e) }
}

// —— getaddrinfo/freeaddrinfo 真实现（IP 字面量档，草案 §3.3）——

/// C 字符串 → 字节切片（止于首 NUL；strlen 同 crate 单源）。
unsafe fn cstr<'a>(p: *const c_char) -> &'a [u8] {
    // SAFETY: 调用方保证 NUL 终结（C 契约）。
    unsafe { core::slice::from_raw_parts(p as *const u8, crate::strmem::strlen(p)) }
}

/// service → 端口（host 序）：NULL = 0（不解析端口，sin_port=0）；十
/// 进制（前导零按十进制——strtoul 基 10 口径）、值 ≤65535；空串/非数字/
/// 超域 = None（EAI_SERVICE——v1 无 getservbyname，服务名档拒）。
fn parse_port(service: *const c_char) -> Option<u16> {
    if service.is_null() {
        return Some(0);
    }
    // SAFETY: C 字符串契约。
    let s = unsafe { cstr(service) };
    if s.is_empty() {
        return None;
    }
    let mut v: u32 = 0;
    for &c in s {
        if !c.is_ascii_digit() {
            return None;
        }
        v = v * 10 + u32::from(c - b'0');
        if v > 65535 {
            return None;
        }
    }
    Some(v as u16)
}

/// getaddrinfo（真实现·IP 字面量档）：node 必须点分四段 IPv4 字面量
/// （musl 语义：字面量自消化不走 DNS；node NULL → EAI_NONAME——AI_PASSIVE
/// 0.0.0.0 档 v1 不做，std::net 消费面不走此形，文档化）。hints 域检：
/// family ∈ {AF_INET, 0}（外 → EAI_FAMILY）、socktype ∈ {SOCK_STREAM,
/// 0}（外 → EAI_SERVICE——musl 对不支持 socktype 用此码）；AI_* flags
/// v1 忽略（ai_canonname 恒 NULL）。结果 = 单节点链（malloc ai 48B +
/// sa 16B；任一失败释放已分配块 → EAI_MEMORY，无泄漏路径）。成功 0 且
/// `*res` 指链头。
#[no_mangle]
pub unsafe extern "C" fn getaddrinfo(
    node: *const c_char,
    service: *const c_char,
    hints: *const AddrInfo,
    res: *mut *mut AddrInfo,
) -> c_int {
    // SAFETY: 出参写点（libc 契约：错误路径也置 NULL）。
    unsafe { *res = core::ptr::null_mut() };
    if node.is_null() {
        return crate::EAI_NONAME;
    }
    if !hints.is_null() {
        // SAFETY: hints 指调用方 AddrInfo（libc 契约）。
        let h = unsafe { &*hints };
        if h.ai_family != 0 && h.ai_family != AF_INET {
            return crate::EAI_FAMILY;
        }
        if h.ai_socktype != 0 && h.ai_socktype != SOCK_STREAM {
            return crate::EAI_SERVICE;
        }
    }
    // SAFETY: C 字符串契约。
    let addr = match parse_ipv4(unsafe { cstr(node) }) {
        Some(a) => a,
        None => return crate::EAI_NONAME,
    };
    let port = match parse_port(service) {
        Some(p) => p,
        None => return crate::EAI_SERVICE,
    };
    let ai = crate::alloc::malloc(core::mem::size_of::<AddrInfo>()) as *mut AddrInfo;
    if ai.is_null() {
        return crate::EAI_MEMORY;
    }
    let sa = crate::alloc::malloc(core::mem::size_of::<SockAddrIn>()) as *mut SockAddrIn;
    if sa.is_null() {
        crate::alloc::free(ai as *mut c_void);
        return crate::EAI_MEMORY;
    }
    // SAFETY: malloc 返回块 ≥16B 对齐（alloc.rs 堆口径）——指针字段对齐
    // 达标；两块均为本调用私有（未逃逸），全字段初始化无读未定义。
    unsafe {
        *sa = SockAddrIn {
            sin_family: AF_INET as sa_family_t,
            sin_port: htons(port),
            sin_addr: htonl(addr),
            sin_zero: [0; 8],
        };
        *ai = AddrInfo {
            ai_flags: 0,
            ai_family: AF_INET,
            ai_socktype: SOCK_STREAM,
            ai_protocol: IPPROTO_TCP,
            ai_addrlen: core::mem::size_of::<SockAddrIn>() as u32,
            __pad: 0,
            ai_addr: sa as *mut SockAddr,
            ai_canonname: core::ptr::null_mut(),
            ai_next: core::ptr::null_mut(),
        };
        *res = ai;
    }
    0
}

/// freeaddrinfo（真实现·链释放）：沿 ai_next 逐节点释放——每节点先取
/// next 再 free（UAF 防线）、先 free ai_addr 再 free 本体；free(NULL)
/// no-op（alloc.rs 在册）。NULL 入参 = 空循环。与 getaddrinfo 的 malloc
/// 面严格对偶。
#[no_mangle]
pub unsafe extern "C" fn freeaddrinfo(ai: *mut AddrInfo) {
    let mut p = ai;
    while !p.is_null() {
        // SAFETY: p 指本链节点（getaddrinfo 产物或 NULL——libc 契约）；
        // 先取 next/addr 再释放本节点。
        unsafe {
            let next = (*p).ai_next;
            let addr = (*p).ai_addr;
            crate::alloc::free(addr as *mut c_void);
            crate::alloc::free(p as *mut c_void);
            p = next;
        }
    }
}
