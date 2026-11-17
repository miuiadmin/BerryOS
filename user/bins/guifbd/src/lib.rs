//! guifbd lib —— B1-b+c vgpu 后端纯面（显示与加速体系 02 号 §3.2 落法、
//! `research/显示与加速体系设计/02-...-v1.0.md` §6 刀序 B1-b/c）。
//!
//! lib/bin 双面 crate（gui-inputd 先例）：本文件 = **无 syscall 的可测面**
//! （host `cargo test`——依赖 berry-gpuproto/berry-guiproto/berry-virtio
//! 三纯 crate）；guest 入口（真 virtio-gpu 设备链 + berry-libc/berry-sys
//! syscall 面）在 `src/main.rs` 壳 + `src/guest.rs`。纯面五件：
//!
//! 1. **后端分派**（[`backend_from_handoff`]）：boot 探测的交接协议——
//!    svcmgr 预拉 guifbd 后把 virtio-mmio 窗映进子空间 `VIRTIO_MMIO_VA`
//!    （`ChildHandles::hv`——mmio_vmo 单窗槽已归 fb 窗，第二窗走父侧
//!    后映射），经 stdin 通道发 [`HANDOFF_VGPU`] 单字节交接；guifbd 首个
//!    syscall = 读该字节（窗未映先读窗 = D-abort——读字节是序保证，非
//!    数据面）。无窗/旧 svcmgr（feeder EOF）→ Ramfb 零漂移。
//! 2. **INDIRECT 链构造**（[`chain_put`]）：控制队列命令/响应同步对的
//!    间接描述符表（`DESC_F_INDIRECT`——02 号 §3.2 坑①，ATTACH_BACKING
//!    的 mem_entry 随行面；v1 backing = fb 窗连续物理段单 entry，8KB 级
//!    per-page 表挂 v2，间接表是统一传输面）。
//! 3. **提交序**（[`INIT_SEQ`]/[`frame_flush_cmds`]）：初始化链四命令序
//!    （GET_DISPLAY_INFO → CREATE_2D → ATTACH_BACKING → SET_SCANOUT——
//!    QEMU `virtio_gpu_set_scanout` 对 scanout 资源 require_backing，序
//!    反了即 RESP_ERR_INVALID_RESOURCE_ID）与帧提交序（全帧
//!    TRANSFER_TO_HOST_2D → RESOURCE_FLUSH——Linux `virtgpu_plane.c` dumb
//!    bo 同构流的 v1 全帧退化形，per-rect 挂 v2）。
//! 4. **v2 帧批面**（per-rect flush + 异步发车——12 号 §4.3 模型）：
//!    [`rects_clip_into`]（damage 裁剪+包络）/[`frame_flush_batch`]（K×
//!    TRANSFER 逐矩形 + 1×FLUSH 包络，fence 置 FLUSH 头）/`batch_resps_ok`
//!    （回执桶延迟消费校验）。协议定谳：FLUSH/TRANSFER 每命令恰一矩形
//!    （uapi 无 N-rect 数组形）——「FLUSH 带矩形数组」不在协议面，包络
//!    是合法同构形（Linux `virtgpu_plane.c` 同款）。
//! 5. **带合成 v1 纯面**（波3 批B）：[`classify_surface`]（surface 六档
//!    分类——快照本地查表零 RPC）/[`plan_bands`]（快照带集规划：3 窗口
//!    带槽 + 底常量 fallback 槽）/[`band_replay_plan`]（SERVICE→SYSTEM
//!    定序的 damage∩带 重放计划——FULL 帧恒触发）/[`replay_rows_u32`]
//!    （A==0 透明键跳写的行重放）。guest 侧缓存写入与重放已随批B 接线
//!    消费（本面 host 可测纯逻辑）。
//!
//! 纯函数纪律（berry-virtio/berry-gpuproto 同款）：零分配、零 unsafe、
//! 编不出就拒绝（None）不截断。

#![cfg_attr(not(test), no_std)]

use berry_guiproto as gp;
use berry_gpuproto as vgp;
use berry_virtio as vio;

/// 交接字节：virtio 窗已映进本进程空间（svcmgr 后映射成功后发）。
pub const HANDOFF_VGPU: u8 = b'w';
/// 交接字节：无窗面（H_MMIO 缺席/映射失败——fb 相内理论不可达的防御档，
/// svcmgr 恒发本值兜底，guifbd 收到即走 Ramfb）。
pub const HANDOFF_NONE: u8 = b'n';

/// 后端分派产物。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// 现役 ramfb 路径（FB_VA 直写 = QEMU ramfb 设备扫描 GOP DRAM）。
    Ramfb,
    /// virtio-gpu 2D 路径（资源 = fb 窗同一物理段 backing，帧提交走
    /// TRANSFER + FLUSH 控制队列对）。
    Vgpu,
}

/// 交接字节 → 后端（Some(`HANDOFF_VGPU`) → Vgpu；None（旧 svcmgr feeder
/// EOF）/其它任意值 → Ramfb——**缺省安全**：交接面任何破绽都落回现役
/// 行为而非死路）。
pub fn backend_from_handoff(b: Option<u8>) -> Backend {
    match b {
        Some(HANDOFF_VGPU) => Backend::Vgpu,
        _ => Backend::Ramfb,
    }
}

// —— DMA VMO 布局（两页：page0 = virtqueue 整环，page1 = 命令/响应/间接
// 表三缓冲——vblk「整 virtqueue 落一页」几何承诺延承，02 号 §3.2 坑①
// 的解法=间接表把链挪出 ring 页）——

/// virtqueue 页偏移（desc/avail/used 整环 ≤ 646B）。
pub const DMA_RING_OFF: u64 = 0;
/// 命令/响应缓冲页偏移。
pub const DMA_BUF_OFF: u64 = 4096;
/// page1 内：命令帧偏移（最大 56B = TRANSFER_TO_HOST_2D）。
pub const CMD_OFF: u64 = 0;
/// page1 内：响应帧偏移（容量 [`RESP_CAP`]——DISPLAY_INFO 408B 为最大）。
pub const RESP_OFF: u64 = 64;
/// page1 内：间接描述符表偏移（容量 [`CHAIN_MAX`] × 16B = 256B）。
pub const IND_OFF: u64 = 512;
/// 响应帧容量 = `RESP_DISPLAY_INFO_SZ`（408B；EDID 1056B 不在 v1 面）。
pub const RESP_CAP: usize = vgp::RESP_DISPLAY_INFO_SZ;
/// 间接表容量（= qsize 16——QEMU `virtqueue_pop` 对间接链的链长帽同
/// vring.num，越帽 = 设备侧丢链）。
pub const CHAIN_MAX: usize = 16;

/// 间接描述符表项（纯面视图：addr/len/write——guest 侧按序展成 DESC 条目）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainEnt {
    pub addr: u64,
    pub len: u32,
    /// true = device-write（响应缓冲）。
    pub write: bool,
}

/// 构造间接描述符表：entries 按序占表（非末项置 NEXT 且 next = i+1 表内
/// 下家——Linux virtio_ring 间接表同款保守形，邻接序与 next 域双满足；
/// 末项收链 NEXT/next 归 0）；越 [`CHAIN_MAX`] 或表切片不足 → None。返回
/// 表字节长（entries × 16）。LE 布局与 berry-virtio `put_desc` 同构
/// （addr u64 + len u32 + flags u16 + next u16），但表在 ring 页外——
/// berry-virtio 的 put 绑定 RingLayout，此处独立成面供 host 钉板。
pub fn chain_put(table: &mut [u8], entries: &[ChainEnt]) -> Option<usize> {
    if entries.is_empty() || entries.len() > CHAIN_MAX {
        return None;
    }
    let total = entries.len() * vio::DESC_SZ;
    if table.len() < total {
        return None;
    }
    for (i, e) in entries.iter().enumerate() {
        if e.len == 0 || e.addr == 0 {
            return None; // 零长/零址描述符 = 传输面畸形,编不出就拒绝
        }
        let last = i + 1 == entries.len();
        let off = i * vio::DESC_SZ;
        table[off..off + 8].copy_from_slice(&e.addr.to_le_bytes());
        table[off + 8..off + 12].copy_from_slice(&e.len.to_le_bytes());
        let flags = if e.write { vio::DESC_F_WRITE } else { 0 }
            | if last { 0 } else { vio::DESC_F_NEXT };
        let next = if last { 0 } else { (i + 1) as u16 };
        table[off + 12..off + 14].copy_from_slice(&flags.to_le_bytes());
        table[off + 14..off + 16].copy_from_slice(&next.to_le_bytes());
    }
    Some(total)
}

/// v1 backing 形态：fb 窗是 boot 授能的**连续物理段**（RamDxe
/// AllocatePages 产物，`create_mmio(fb_pa, ceil_page(stride×h))` 包窗）——
/// ATTACH_BACKING 的 mem_entry 表 = 单 entry 恰覆盖帧字节。None 拒面：
/// 零长 / fb_bytes 超 u32 / pa=0。1080P = 8,294,400B 单段；per-page 表
/// （非连续 anon VMO backing，02 号 §3.2 坑① 的 8KB 级形态）挂 v2 动态
/// 分辨率刀。
pub fn attach_entry(fb_pa: u64, fb_bytes: u64) -> Option<(u64, u32)> {
    if fb_pa == 0 || fb_bytes == 0 || fb_bytes > u32::MAX as u64 {
        return None;
    }
    Some((fb_pa, fb_bytes as u32))
}

/// 初始化链命令序（wire kind 面——guest 侧按此序发车，QEMU 完成语义全
/// 即答）：GET_DISPLAY_INFO 先行验传输往返（带载荷响应），CREATE_2D 起
/// 资源，ATTACH_BACKING 挂 backing，SET_SCANOUT 绑 scanout（对资源
/// require_backing——QEMU `virtio_gpu_do_set_scanout` 源码序执法）。
pub const INIT_SEQ: [u32; 4] = [
    vgp::CMD_GET_DISPLAY_INFO,
    vgp::CMD_RESOURCE_CREATE_2D,
    vgp::CMD_RESOURCE_ATTACH_BACKING,
    vgp::CMD_SET_SCANOUT,
];

/// 帧提交序两命令帧（全帧 v1：r = {0,0,w,h}、offset=0——QEMU
/// `virtio_gpu_transfer_host_2d` 整宽快路单发 `iov_to_buf(stride×h)`，
/// guest 侧 stride 须 = w×4〔packed 几何，guest 侧执法〕）。out 切片须
/// ≥ 56+48；返回 (transfer 长, flush 长)。**序即正确性**：TRANSFER 先
/// FLUSH 后（flush 只把已上传像素交 `qemu_console_update`——先 flush
/// 后 transfer = 设备侧呈现旧帧 + 本帧延迟一拍）。
///
/// v2 起 guest 帧提交走 [`frame_flush_batch`]（批编帧+异步发车）；本函数
/// 保留为 v1 全帧对的编码钉板（全帧回退形态的 host 回归锚——批形式 K=1
/// 全帧矩形与本对逐字节相容，见 `frame_flush_batch_full_matches_v1` 测）。
pub fn frame_flush_cmds(out: &mut [u8], w: u32, h: u32, resource_id: u32) -> Option<(usize, usize)> {
    let full = vgp::Rect::new(0, 0, w, h);
    let n_t = vgp::encode_transfer_to_host_2d(
        out,
        &vgp::CtrlHdr::new(vgp::CMD_TRANSFER_TO_HOST_2D),
        &vgp::TransferToHost2d { r: full, offset: 0, resource_id },
    )?;
    let n_f = vgp::encode_resource_flush(
        &mut out[n_t..],
        &vgp::CtrlHdr::new(vgp::CMD_RESOURCE_FLUSH),
        &vgp::ResourceFlush { r: full, resource_id },
    )?;
    Some((n_t, n_f))
}

// —— v2 帧批面（per-rect flush + 异步发车——12 号 §4.3 模型落法）——
//
// 协议定谳（如实）：TRANSFER_TO_HOST_2D 与 RESOURCE_FLUSH **每命令恰一
// 矩形**——uapi `virtio_gpu_resource_flush` 无 N-rect 数组形（berry-gpuproto
// 冻结面同构）。per-rect 帧提交 = K×TRANSFER（逐矩形，offset = y×stride +
// x×4）+ 1×FLUSH（包络）——Linux `virtgpu_plane.c` 同构（dumb bo 路：
// damage 逐 clip transfer_to_host_2d，resource_flush 单发包络）。矩形合并
// （包络内空洞剔除/条带并）挂 v3。

/// 单帧 damage 矩形帽 = berry-guiproto `DAMAGE_MAX`（两 crate 均冻结，值在
/// 此单源给批形状；guest 侧编译期断言互钉）。
pub const FLUSH_RECT_MAX: usize = 8;
/// 单帧批命令帽 = K×TRANSFER + 1×FLUSH 包络（≤ virtqueue qsize 16——单帧
/// 在途的结构背压保证，见 guest `submit_frame` 注）。
pub const FRAME_CMD_MAX: usize = FLUSH_RECT_MAX + 1;
/// 批命令槽步长（64B——最大命令帧 56B 圆整；page1 内连排）。
pub const ACMD_SZ: usize = 64;
/// page1 内批命令区偏移（v1 同步三缓冲区之后：CMD 0..64 / RESP 64..472 /
/// IND 512..768）。
pub const ACMD_OFF: u64 = 768;
/// 批响应桶偏移（逐命令 [`vgp::RESP_OK_NODATA_SZ`] = 24B 连排——设备逐链
/// 回写各自槽，回收时单次读回）。
pub const ARESP_OFF: u64 = 1344;
/// 批间接表区偏移（逐命令 2 entry × [`vio::DESC_SZ`] = 32B 连排）。
pub const AIND_OFF: u64 = 1560;

/// damage 矩形裁剪进 `dst`（与 fb 几何 (w,h) 求交，空集丢弃）+ 包络并集。
/// 返回 None = src 超 dst 容（调用方 bug 拒面）；Some((kept, env))——
/// kept == 0 时 env 为全零哨兵（全界外 = blit 零写入，「无 damage 不提交」
/// 的诚实 no-op 判据）。裁剪语义与 guest blit 路同构：交域
/// [x, min(x+w, w)) × [y, min(y+h, h)) 非空才留（u64 和防 u32 溢出后回夹）。
pub fn rects_clip_into(
    dst: &mut [vgp::Rect],
    src: &[vgp::Rect],
    w: u32,
    h: u32,
) -> Option<(usize, vgp::Rect)> {
    if src.len() > dst.len() {
        return None;
    }
    let mut kept = 0usize;
    let (mut ex0, mut ey0, mut ex1, mut ey1) = (u32::MAX, u32::MAX, 0u32, 0u32);
    for &r in src {
        let x1 = ((r.x as u64 + r.width as u64).min(w as u64)) as u32;
        let y1 = ((r.y as u64 + r.height as u64).min(h as u64)) as u32;
        if r.x >= w || r.y >= h || x1 <= r.x || y1 <= r.y {
            continue; // 界外/零面积：裁剪空集丢弃（guest blit 路 continue 同款）
        }
        ex0 = ex0.min(r.x);
        ey0 = ey0.min(r.y);
        ex1 = ex1.max(x1);
        ey1 = ey1.max(y1);
        dst[kept] = vgp::Rect::new(r.x, r.y, x1 - r.x, y1 - r.y);
        kept += 1;
    }
    let env = if kept > 0 {
        vgp::Rect::new(ex0, ey0, ex1 - ex0, ey1 - ey0)
    } else {
        vgp::Rect::new(0, 0, 0, 0)
    };
    Some((kept, env))
}

/// v2 帧批编帧（异步发车面）：`rects`（已裁剪界内）逐个
/// TRANSFER_TO_HOST_2D 落槽 i（槽 i @ i×[`ACMD_SZ`]，offset = y×stride +
/// x×4——QEMU `virtio_gpu_transfer_host_2d` 行循环口径 src = offset +
/// stride×h），末槽 RESOURCE_FLUSH（包络，FLAG_FENCE + fence_id——QEMU 2D
/// 即答路径回执回显 fence_id〔`virtio_gpu_ctrl_response`〕，批身份正向校验
/// 锚）。返回命令数（K+1）。拒面：rects 空/超 [`FLUSH_RECT_MAX`]/任一矩形
/// 或包络零面积/越界（QEMU 对越界 TRANSFER/FLUSH 回
/// RESP_ERR_INVALID_PARAMETER——编帧侧预拒）/out 切片不足。**序即正确性**：
/// TRANSFER 全部先于 FLUSH（同批内设备 FIFO 处理序）。
pub fn frame_flush_batch(
    out: &mut [u8],
    resource_id: u32,
    w: u32,
    h: u32,
    rects: &[vgp::Rect],
    envelope: &vgp::Rect,
    fence_id: u64,
) -> Option<usize> {
    if rects.is_empty() || rects.len() > FLUSH_RECT_MAX {
        return None;
    }
    if out.len() < FRAME_CMD_MAX * ACMD_SZ {
        return None;
    }
    let inb = |r: &vgp::Rect| {
        r.width > 0
            && r.height > 0
            && (r.x as u64 + r.width as u64) <= w as u64
            && (r.y as u64 + r.height as u64) <= h as u64
    };
    if !rects.iter().all(|r| inb(r)) || !inb(envelope) {
        return None;
    }
    let stride = w as u64 * 4;
    for (i, &r) in rects.iter().enumerate() {
        let off = i * ACMD_SZ;
        let offset = stride * r.y as u64 + 4 * r.x as u64;
        vgp::encode_transfer_to_host_2d(
            &mut out[off..off + ACMD_SZ],
            &vgp::CtrlHdr::new(vgp::CMD_TRANSFER_TO_HOST_2D),
            &vgp::TransferToHost2d { r, offset, resource_id },
        )?;
    }
    let off = rects.len() * ACMD_SZ;
    vgp::encode_resource_flush(
        &mut out[off..off + ACMD_SZ],
        &vgp::CtrlHdr::new(vgp::CMD_RESOURCE_FLUSH).with_fence(fence_id),
        &vgp::ResourceFlush { r: *envelope, resource_id },
    )?;
    Some(rects.len() + 1)
}

/// 回执桶校验（延迟消费面）：批 n 命令的响应连排桶（n×
/// [`vgp::RESP_OK_NODATA_SZ`]）逐个须 RESP_OK_NODATA，且**末命令**（FLUSH）
/// 回执回显 FLAG_FENCE + fence_id（批身份正向锚——槽与链一一对应，末槽
/// 恒为 FLUSH）。任一错误族/缺回显/回显错值/桶截断 → false（调用方定死法
/// ——停刷不杀服务，v1 `dead` 语义延承）。
pub fn batch_resps_ok(resps: &[u8], n: usize, fence_id: u64) -> bool {
    let sz = vgp::RESP_OK_NODATA_SZ;
    if n == 0 || n > FRAME_CMD_MAX || resps.len() < n * sz {
        return false;
    }
    for i in 0..n {
        let hdr = match vgp::parse_resp_nodata(&resps[i * sz..(i + 1) * sz]) {
            Some(hd) => hd,
            None => return false, // 错误族/未知 kind：parse_resp_nodata 恰长+kind 双门
        };
        if i + 1 == n
            && (hdr.flags & vgp::FLAG_FENCE == 0 || hdr.fence_id != fence_id)
        {
            return false;
        }
    }
    true
}

// ==========================================================================
// 波3 批B 带合成 v1 纯面（SPEC-batchB §1/§3/§4——surface 分类 / 带集规划 /
// 重放计划 / 行重放；快照为本地状态零 RPC，guest 侧后续实施段接线消费）
// ==========================================================================

/// 带缓存槽数：3 窗口带槽（快照序）+ 1 底常量 fallback 带（[`plan_bands`]
/// 布局——槽 3 恒归 fallback，窗口带永不越槽 2）。
pub const BAND_SLOTS: usize = 4;
/// 单带高帽（px）：盖 guest 侧 big 档 band_top 117 / band_bot 135 留余量；
/// 超帽窗（如 softimed 全高 780px）不进带集——[`plan_bands`] warn 置位、
/// guest 侧 loud 一次。
pub const BAND_H_CAP: u32 = 160;
/// 带缓存总量启动门（4MiB）：boot 一次性堆分配前经 [`band_cache_bytes`]
/// 执法，超门或分配失败 → 带面禁用 loud（裁剪与现网行为不变，仅无缓存/
/// 重放）。
pub const BAND_CACHE_BYTES_MAX: usize = 4 << 20;
/// 系统面 surface_id marker（wire SUBMIT 既有 u32 域的本地语义分档，非
/// 协议号——launcher 全提交带系统带视觉）。批B 过渡语义：SysFace 档不裁
/// + seed 带缓存；0xFFFF 值域退役归批C（launcher 切 wid 提交后）。
pub const SYS_SURFACE_ID: u32 = 0xFFFF;

/// surface 分类产物（SPEC-batchB §1 语义表——blit 面「裁剪 / 带缓存写入 /
/// 带重放」三档语义的单源裁决；快照为本地状态，分类零 RPC）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SurfaceClass {
    /// surface_id==[`SYS_SURFACE_ID`]（launcher 全提交）——不裁、写带缓存
    /// （原始 targets∩带.rect，从 bounce 抄录）、不重放。
    SysFace,
    /// guest 内部 hdi COMMIT refresh 面（槽内容属任意应用）——不裁、不写
    /// 带缓存（防 P5 槽污染）、不重放；仅由内部调用点显式传入，
    /// [`classify_surface`] 永不返回本值。
    InternalRefresh,
    /// surface_id==0 或 wid 不在快照——保守保护带：Y 裁带（现网兼容档原样
    /// 保留）、不缓存、重放（原始 targets∩带.rect，FULL 帧恒触发）。
    AppCompat,
    /// wid∈快照 ∧ vis ∧ layer∈{SYSTEM,SERVICE}——不裁、写该窗带槽、不重放
    /// （带帧自缓存自呈现）。
    WinBand,
    /// wid∈快照 ∧ vis ∧ layer==APP——不裁、不缓存、重放（窗化应用
    /// edge-to-edge 可行）。
    WinApp,
    /// wid∈快照 ∧ !vis——空 targets 呈现（blit 面=空集恒 pass，token/
    /// credits/held 记账照常、正常 8B ack）。
    Hidden,
}

/// wire surface_id → 分类。0xFFFF 值域档先于 wid 查表（id 恰 0xFFFF 的窗
/// 不冒领系统面）；wid 不在快照按 AppCompat（保守保护带——快照滞后一拍
/// 错分 = 新鲜度窗口 v1 语义，下帧自愈）。count 为快照有效前缀长。
pub fn classify_surface(surface_id: u32, entries: &[gp::WmEntry], count: usize) -> SurfaceClass {
    if surface_id == SYS_SURFACE_ID {
        return SurfaceClass::SysFace;
    }
    if surface_id == 0 {
        return SurfaceClass::AppCompat;
    }
    for e in entries.iter().take(count.min(entries.len())) {
        if e.id as u32 != surface_id {
            continue;
        }
        if e.flags & gp::WM_WIN_F_VISIBLE == 0 {
            return SurfaceClass::Hidden;
        }
        return match e.layer {
            gp::WM_LAYER_SYSTEM | gp::WM_LAYER_SERVICE => SurfaceClass::WinBand,
            _ => SurfaceClass::WinApp, // APP 层；未知层值 wire 已双侧执法，防御档按窗呈现
        };
    }
    SurfaceClass::AppCompat
}

/// 单带槽计划（[`plan_bands`] 产物——guest 侧缓存写入与重放的几何真值）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BandPlan {
    /// 带源层（WM_LAYER_SYSTEM/WM_LAYER_SERVICE；fallback 底带记 SYSTEM——
    /// 重放序随 SYSTEM 组）。
    pub layer: u8,
    /// 带矩形（fb 界内半开 [x,x+w)×[y,y+h)：窗口带 = 窗有效几何∩fb；fallback
    /// 带 = (0, fb_h−band_bot, fb_w, band_bot)）。
    pub rect: gp::Rect,
    /// true = 第 4 槽底常量 fallback 带（批C 底窗落成后自然被覆盖而退役）。
    pub fallback: bool,
}

/// 带集规划（纯函数）：快照序扫候选窗（vis ∧ layer∈{SYSTEM,SERVICE} ∧
/// rect∩fb≠∅），形态帽内（窗自身 h≤[`BAND_H_CAP`] ∧ w≤fb_w）依序占前
/// 3 槽；底常量带 (0, fb_h−band_bot, fb_w, band_bot) 与已选带零重叠时占
/// 第 4 槽 fallback（退化 band_bot——0 或 ≥fb_h——不建）。
///
/// warn（按候选序槽位，任一 true = guest 侧 loud 一次）：warn[i<3] = 快照
/// 序第 i 个候选因高/宽帽被拒；warn[3] = 候选序越 warn 数组（第 5+ 候选
/// 超帽）与窗口槽耗尽（超 3 带截断）的合并旗。返回（槽位数组、warn、
/// 选中带总数）。
pub fn plan_bands(
    entries: &[gp::WmEntry],
    count: usize,
    fb_w: u32,
    fb_h: u32,
    band_bot: u32,
) -> ([Option<BandPlan>; BAND_SLOTS], [bool; BAND_SLOTS], u8) {
    let mut bands: [Option<BandPlan>; BAND_SLOTS] = [None; BAND_SLOTS];
    let mut warn = [false; BAND_SLOTS];
    let mut n_win = 0usize; // 已占窗口带槽数（≤3——槽 3 恒归 fallback）
    let mut cand = 0usize; // 候选序（vis∧层∧∩fb 已成立者，快照序）
    for e in entries.iter().take(count.min(entries.len())) {
        if e.flags & gp::WM_WIN_F_VISIBLE == 0
            || (e.layer != gp::WM_LAYER_SYSTEM && e.layer != gp::WM_LAYER_SERVICE)
        {
            continue; // vis / 层过滤：隐藏窗与 APP 层不带化（非候选、零 warn）
        }
        // 窗有效几何∩fb（u64 和防溢出；WmEntry x/y/w/h 为 u16 无负值）。
        let x0 = e.x as u32;
        let y0 = e.y as u32;
        let x1 = ((e.x as u64 + e.w as u64).min(fb_w as u64)) as u32;
        let y1 = ((e.y as u64 + e.h as u64).min(fb_h as u64)) as u32;
        if x0 >= fb_w || y0 >= fb_h || x1 <= x0 || y1 <= y0 {
            continue; // rect∩fb 空集：全界外窗不带化（非候选、零 warn）
        }
        // 候选成立——形态帽按窗自身 w/h 执法（非裁后值：帽的是窗不是交域）。
        if e.h as u32 > BAND_H_CAP || e.w as u32 > fb_w {
            warn[cand.min(BAND_SLOTS - 1)] = true;
            cand += 1;
            continue;
        }
        if n_win < BAND_SLOTS - 1 {
            bands[n_win] = Some(BandPlan {
                layer: e.layer,
                rect: gp::Rect { x: x0, y: y0, w: x1 - x0, h: y1 - y0 },
                fallback: false,
            });
            n_win += 1;
        } else {
            warn[BAND_SLOTS - 1] = true; // 超 3 带截断：快照序前 3 优先
        }
        cand += 1;
    }
    // 底常量 fallback 带：与已选带零重叠才占第 4 槽。
    if band_bot > 0 && band_bot < fb_h {
        let fr = gp::Rect { x: 0, y: fb_h - band_bot, w: fb_w, h: band_bot };
        let overlap = bands[..n_win].iter().flatten().any(|b| rect_isect(&b.rect, &fr).is_some());
        if !overlap {
            bands[BAND_SLOTS - 1] = Some(BandPlan {
                layer: gp::WM_LAYER_SYSTEM,
                rect: fr,
                fallback: true,
            });
        }
    }
    let total = n_win + usize::from(bands[BAND_SLOTS - 1].is_some());
    (bands, warn, total as u8)
}

/// 带重放计划（纯函数）：定序 SERVICE 窗口带 → SYSTEM 窗口带与 fallback
/// （各自槽序，「序即正确性」——后画压先画，服务面视觉恒压系统面）；每带
/// ×每条 damage 求交（交域再裁回 fb 界——带矩形本 ⊆ fb，双保险零漂移），
/// 非空即出 (slot, rect) 对——slot = bands 槽位（guest 侧读该槽缓存行）。
/// full=true 时 damage 视为单全屏 rect（FULL 帧恒触发重放——保 marker
/// 确定性；重放区⊆原始 damage 界 → vgpu 上传集自然覆盖）。返回写出条数。
pub fn band_replay_plan(
    damage: &[gp::Rect],
    damage_n: usize,
    full: bool,
    fb_w: u32,
    fb_h: u32,
    bands: &[Option<BandPlan>; BAND_SLOTS],
    out: &mut [(usize, gp::Rect); BAND_SLOTS * gp::DAMAGE_MAX + 2],
) -> usize {
    // damage 帽：wire 帽 DAMAGE_MAX 双保险（out 容量公式的前提——越帽截
    // 断不越界；guest 侧恒 ≤帽）。
    let n = if full { 1 } else { damage_n.min(damage.len()).min(gp::DAMAGE_MAX) };
    let fb_rect = gp::Rect { x: 0, y: 0, w: fb_w, h: fb_h };
    let mut written = 0usize;
    for pass in 0..2 {
        for (si, b) in bands.iter().enumerate() {
            let Some(b) = b else { continue };
            let svc = b.layer == gp::WM_LAYER_SERVICE;
            if (pass == 0) != svc {
                continue; // 两遍定序：pass 0 只 SERVICE，pass 1 收 SYSTEM 与 fallback
            }
            for di in 0..n {
                let r = if full { fb_rect } else { damage[di] };
                if let Some(hit) = rect_isect(&r, &b.rect).and_then(|c| rect_isect(&c, &fb_rect)) {
                    out[written] = (si, hit);
                    written += 1;
                }
            }
        }
    }
    written
}

/// 带缓存行重放（u32 ARGB 逐像素）：A==0（`p>>24==0`——1-bit 透明键，
/// berry-gfx raster.rs argb_blit 同律）跳写保底穿透，A!=0 覆写原值（无
/// blend——半透也是不透明覆盖）；返回实际写入数（guest 侧「重放 ≥1 像素」
/// marker 判据）。长度不等按短者（调用方保证同长行切片——带行几何由
/// [`BandPlan::rect`] 单源，防御档不 panic）。
pub fn replay_rows_u32(dst: &mut [u32], src: &[u32]) -> usize {
    let mut n = 0usize;
    for (d, &s) in dst.iter_mut().zip(src.iter()) {
        if s >> 24 == 0 {
            continue; // A==0：透明键跳写（底穿透）
        }
        *d = s;
        n += 1;
    }
    n
}

/// 带缓存总量（boot 一次性堆分配字节数）：fb_w × [`BAND_H_CAP`] × 4B ×
/// [`BAND_SLOTS`]。fb_w==0（几何未就绪）或超 [`BAND_CACHE_BYTES_MAX`] 门
/// → None（guest 侧带面禁用 loud——裁剪与现网行为不变，仅无缓存/重放）。
pub fn band_cache_bytes(fb_w: u32) -> Option<usize> {
    let bytes = fb_w as u64 * BAND_H_CAP as u64 * 4 * BAND_SLOTS as u64;
    if bytes == 0 || bytes > BAND_CACHE_BYTES_MAX as u64 {
        return None;
    }
    Some(bytes as usize)
}

/// 两半开矩形求交（u64 和防 u32 溢出——wire damage 的 w/h 为任意 u32）；
/// 空集 → None。交域宽/高 < min(两矩形宽/高) ≤ u32 域，落回 u32 安全。
fn rect_isect(a: &gp::Rect, b: &gp::Rect) -> Option<gp::Rect> {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x as u64 + a.w as u64).min(b.x as u64 + b.w as u64);
    let y1 = (a.y as u64 + a.h as u64).min(b.y as u64 + b.h as u64);
    if x1 <= x0 as u64 || y1 <= y0 as u64 {
        return None;
    }
    Some(gp::Rect {
        x: x0,
        y: y0,
        w: (x1 - x0 as u64) as u32,
        h: (y1 - y0 as u64) as u32,
    })
}

// ==========================================================================
// host 测试（B1-b/c 刀面：后端状态机 / INDIRECT 链构造 / ATTACH 单段 /
// 提交序——协议编解码本体已在 berry-gpuproto B1-a 冻结，此处测后端语义；
// 波3 批B 刀面：带合成 v1 纯面——分类/带集/重放序/行重放）
// ==========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_dispatch() {
        // 状态机三态：交接字节 w → Vgpu；n/None（旧 svcmgr feeder EOF——
        // 版本混布防御）/任意野值 → Ramfb（缺省安全：交接面破绽落现役）。
        assert_eq!(backend_from_handoff(Some(HANDOFF_VGPU)), Backend::Vgpu);
        assert_eq!(backend_from_handoff(Some(HANDOFF_NONE)), Backend::Ramfb);
        assert_eq!(backend_from_handoff(None), Backend::Ramfb);
        assert_eq!(backend_from_handoff(Some(0)), Backend::Ramfb);
        assert_eq!(backend_from_handoff(Some(b'x')), Backend::Ramfb);
        assert_eq!(backend_from_handoff(Some(u8::MAX)), Backend::Ramfb);
    }

    #[test]
    fn chain_layout_and_links() {
        // 两段链（命令 → 响应）：desc 布局逐字节钉板（LE addr/len/flags/
        // next——与 berry-virtio put_desc 同构但表在 ring 页外）。
        let mut t = [0u8; 256];
        let ents = [
            ChainEnt { addr: 0x1234, len: 56, write: false },
            ChainEnt { addr: 0xabcd, len: RESP_CAP as u32, write: true },
        ];
        let n = chain_put(&mut t, &ents).unwrap();
        assert_eq!(n, 32);
        assert_eq!(vio::get_u64(&t, 0).unwrap(), 0x1234);
        assert_eq!(vio::get_u32(&t, 8).unwrap(), 56);
        assert_eq!(vio::get_u16(&t, 12).unwrap(), vio::DESC_F_NEXT);
        assert_eq!(vio::get_u16(&t, 14).unwrap(), 1); // next = 表内下家 1
        assert_eq!(vio::get_u64(&t, 16).unwrap(), 0xabcd);
        assert_eq!(vio::get_u32(&t, 24).unwrap(), RESP_CAP as u32);
        assert_eq!(vio::get_u16(&t, 28).unwrap(), vio::DESC_F_WRITE); // 末项收链无 NEXT
        assert_eq!(vio::get_u16(&t, 30).unwrap(), 0);
        // 单段链（间接头恰指表首）：唯一项 = 末项,flags 恰 WRITE 无 NEXT。
        let n1 = chain_put(&mut t, &ents[..1]).unwrap();
        assert_eq!(n1, 16);
        assert_eq!(vio::get_u16(&t, 12).unwrap(), 0);
    }

    #[test]
    fn chain_rejects() {
        let mut t = [0u8; 256];
        // 空链 / 超 CHAIN_MAX / 表切片不足 / 零长 / 零址 → None。
        assert!(chain_put(&mut t, &[]).is_none());
        let full: Vec<ChainEnt> =
            (0..CHAIN_MAX + 1).map(|i| ChainEnt { addr: i as u64 + 1, len: 4, write: false }).collect();
        assert!(chain_put(&mut t, &full).is_none());
        let ok: Vec<ChainEnt> =
            (0..CHAIN_MAX).map(|i| ChainEnt { addr: i as u64 + 1, len: 4, write: false }).collect();
        assert!(chain_put(&mut t, &ok).is_some()); // 恰满过（sanity）
        let mut tiny = [0u8; 15];
        assert!(chain_put(&mut tiny, &ok[..1]).is_none()); // 放不下一条
        assert!(chain_put(&mut t, &[ChainEnt { addr: 1, len: 0, write: false }]).is_none());
        assert!(chain_put(&mut t, &[ChainEnt { addr: 0, len: 4, write: false }]).is_none());
    }

    #[test]
    fn attach_single_contiguous_entry() {
        // fb 窗连续物理段单 entry：1080P/800×600 两档 + 拒面三连。
        let (a, l) = attach_entry(0x7b2e_0000, 8_294_400).unwrap();
        assert_eq!((a, l), (0x7b2e_0000, 8_294_400));
        let (a, l) = attach_entry(0x1000, 1_920_000).unwrap();
        assert_eq!((a, l), (0x1000, 1_920_000));
        assert!(attach_entry(0, 4096).is_none());
        assert!(attach_entry(0x1000, 0).is_none());
        assert!(attach_entry(0x1000, u32::MAX as u64 + 1).is_none());
    }

    #[test]
    fn init_sequence_order() {
        // 提交序钉板①：初始化链四命令——DISPLAY_INFO 先验往返、SET_SCANOUT
        // 殿后（require_backing 序执法,反序 = 设备侧 RESP_ERR 族）。
        assert_eq!(INIT_SEQ[0], vgp::CMD_GET_DISPLAY_INFO);
        assert_eq!(INIT_SEQ[1], vgp::CMD_RESOURCE_CREATE_2D);
        assert_eq!(INIT_SEQ[2], vgp::CMD_RESOURCE_ATTACH_BACKING);
        assert_eq!(INIT_SEQ[3], vgp::CMD_SET_SCANOUT);
        // 全员在册 2D 九命令集（wire 合法性——gpuproto 单源判定）。
        for &k in &INIT_SEQ {
            assert!(vgp::is_cmd(k));
        }
    }

    #[test]
    fn frame_flush_sequence_and_shapes() {
        // 提交序钉板②：全帧提交对——TRANSFER 先 FLUSH 后,两帧可各自
        // 恰长解析回同参（round-trip),矩形/offset/resource_id 三域对拍。
        let mut out = [0u8; 128];
        let (n_t, n_f) = frame_flush_cmds(&mut out, 1920, 1080, 7).unwrap();
        assert_eq!(n_t, vgp::CMD_TRANSFER_TO_HOST_2D_SZ);
        assert_eq!(n_f, vgp::CMD_RESOURCE_FLUSH_SZ);
        let (_, t) = vgp::parse_transfer_to_host_2d(&out[..n_t]).unwrap();
        assert_eq!(t.r, vgp::Rect::new(0, 0, 1920, 1080));
        assert_eq!(t.offset, 0);
        assert_eq!(t.resource_id, 7);
        let (_, f) = vgp::parse_resource_flush(&out[n_t..n_t + n_f]).unwrap();
        assert_eq!(f.r, vgp::Rect::new(0, 0, 1920, 1080));
        assert_eq!(f.resource_id, 7);
        // 尾随字节拒面（帧边界执法——顺带钉住 n_t+n_f 不越 128B out）。
        let mut trailing = out[..n_t + n_f].to_vec();
        trailing.push(0);
        assert!(vgp::parse_transfer_to_host_2d(&trailing[..n_t + 1]).is_none());
        // out 切片不足 → None 不截断。
        let mut small = [0u8; 40];
        assert!(frame_flush_cmds(&mut small, 800, 600, 1).is_none());
    }

    #[test]
    fn dma_layout_disjoint() {
        // DMA 布局三缓冲互不重叠（page1 内 64/512 边界 + 容量帽恰容）：
        // cmd ≤ 56B < RESP_OFF；resp ≤ 408B；ind ≤ 256B;page 幅 4096 内。
        assert!(vgp::CMD_TRANSFER_TO_HOST_2D_SZ as u64 <= RESP_OFF);
        assert!(RESP_CAP as u64 + RESP_OFF <= IND_OFF);
        assert!((IND_OFF as usize + CHAIN_MAX * vio::DESC_SZ) <= 4096);
        assert_eq!(RESP_CAP, 408);
        assert_eq!(CHAIN_MAX, 16);
        assert_eq!(vio::DESC_SZ, 16);
    }

    // —— v2 帧批面测试（per-rect + 异步发车——矩形编码链/回执桶/布局）——

    #[test]
    fn v2_async_dma_layout_disjoint() {
        // v2 批区与 v1 同步三缓冲区互不重叠 + 页内封闭 + 槽几何钉板：
        // 同步区 [0,768) = CMD 0..64 / RESP 64..472 / IND 512..768；
        // 批区 [768,1848) = cmds 768..1344 / resp 桶 1344..1560 / ind 表 1560..1848。
        assert_eq!(ACMD_OFF, IND_OFF + (CHAIN_MAX * vio::DESC_SZ) as u64);
        assert_eq!(ACMD_OFF + (FRAME_CMD_MAX * ACMD_SZ) as u64, ARESP_OFF);
        assert_eq!(
            ARESP_OFF + (FRAME_CMD_MAX * vgp::RESP_OK_NODATA_SZ) as u64,
            AIND_OFF
        );
        assert_eq!(
            AIND_OFF + (FRAME_CMD_MAX * 2 * vio::DESC_SZ) as u64,
            1848
        );
        assert!(1848 <= 4096);
        // 槽容量：cmd 槽 ≥ 最大命令帧 56B；resp 槽 = 恰长 24B；批命令帽 ≤
        // virtqueue qsize 16（单帧在途结构背压——guest 面再钉）。
        assert!(ACMD_SZ >= vgp::CMD_TRANSFER_TO_HOST_2D_SZ);
        assert!(ACMD_SZ >= vgp::CMD_RESOURCE_FLUSH_SZ);
        assert_eq!(FLUSH_RECT_MAX, 8);
        assert_eq!(FRAME_CMD_MAX, 9);
        assert!(FRAME_CMD_MAX <= 16);
    }

    #[test]
    fn v2_rects_clip_into_shapes() {
        // 裁剪语义与 guest blit 路同构：交域非空才留、包络 = 并集外接。
        let mut dst = [vgp::Rect::new(0, 0, 0, 0); FLUSH_RECT_MAX];
        // ① 全界内三矩形（gfxdemo 半帧对 + 角块）→ 全留,包络 = 外接。
        let src = [
            vgp::Rect::new(0, 0, 800, 300),
            vgp::Rect::new(0, 300, 800, 300),
            vgp::Rect::new(16, 32, 8, 16),
        ];
        let (k, env) = rects_clip_into(&mut dst, &src, 800, 600).unwrap();
        assert_eq!(k, 3);
        assert_eq!(env, vgp::Rect::new(0, 0, 800, 600));
        assert_eq!(dst[0], src[0]);
        assert_eq!(dst[1], src[1]);
        assert_eq!(dst[2], src[2]);
        // ② 部分越界 → 夹到界内（u64 和防溢出口径：x+w 超 u32 也不炸）。
        let src = [vgp::Rect::new(700, 500, u32::MAX, u32::MAX)];
        let (k, env) = rects_clip_into(&mut dst, &src, 800, 600).unwrap();
        assert_eq!(k, 1);
        assert_eq!(dst[0], vgp::Rect::new(700, 500, 100, 100));
        assert_eq!(env, vgp::Rect::new(700, 500, 100, 100));
        // ③ 全界外/零面积 → kept=0 + 全零哨兵（「无 damage 不提交」判据）。
        let (k, env) =
            rects_clip_into(&mut dst, &[vgp::Rect::new(900, 0, 10, 10)], 800, 600).unwrap();
        assert_eq!((k, env), (0, vgp::Rect::new(0, 0, 0, 0)));
        let (k, _) =
            rects_clip_into(&mut dst, &[vgp::Rect::new(10, 10, 0, 5)], 800, 600).unwrap();
        assert_eq!(k, 0);
        // ④ 混合：一留一弃,包络只圈留者。
        let src = [
            vgp::Rect::new(100, 100, 50, 50),
            vgp::Rect::new(0, 700, 50, 50),
        ];
        let (k, env) = rects_clip_into(&mut dst, &src, 800, 600).unwrap();
        assert_eq!(k, 1);
        assert_eq!(env, vgp::Rect::new(100, 100, 50, 50));
        // ⑤ src 超 dst 容 → None（调用方 bug 拒面）。
        let big: Vec<vgp::Rect> =
            (0..FLUSH_RECT_MAX + 1).map(|i| vgp::Rect::new(i as u32, 0, 4, 4)).collect();
        assert!(rects_clip_into(&mut dst, &big, 800, 600).is_none());
    }

    #[test]
    fn v2_frame_flush_batch_per_rect_shapes() {
        // K=3 逐矩形批：4 命令（3×TRANSFER 槽 0..2 + FLUSH 槽 3）——逐槽
        // round-trip 对拍（rect/offset/resource_id）、offset = y×stride+x×4
        // （QEMU transfer 行循环口径）、FLUSH = 包络 + fence 回显位。
        let mut b = [0u8; FRAME_CMD_MAX * ACMD_SZ];
        let rects = [
            vgp::Rect::new(0, 0, 800, 300),
            vgp::Rect::new(0, 300, 400, 100),
            vgp::Rect::new(700, 500, 100, 100),
        ];
        let env = vgp::Rect::new(0, 0, 800, 600);
        let n = frame_flush_batch(&mut b, 1, 800, 600, &rects, &env, 0xAB).unwrap();
        assert_eq!(n, 4);
        let stride = 800u64 * 4;
        for (i, &r) in rects.iter().enumerate() {
            let slot = &b[i * ACMD_SZ..][..vgp::CMD_TRANSFER_TO_HOST_2D_SZ];
            let (h, t) = vgp::parse_transfer_to_host_2d(slot).unwrap();
            assert_eq!(h.kind, vgp::CMD_TRANSFER_TO_HOST_2D);
            assert_eq!(h.flags, 0); // TRANSFER 恒不带 fence（fence 只锚 FLUSH）
            assert_eq!(t.r, r);
            assert_eq!(t.offset, stride * r.y as u64 + 4 * r.x as u64);
            assert_eq!(t.resource_id, 1);
        }
        let fslot = &b[3 * ACMD_SZ..][..vgp::CMD_RESOURCE_FLUSH_SZ];
        let (h, f) = vgp::parse_resource_flush(fslot).unwrap();
        assert_eq!(h.kind, vgp::CMD_RESOURCE_FLUSH);
        assert_eq!(h.flags, vgp::FLAG_FENCE);
        assert_eq!(h.fence_id, 0xAB);
        assert_eq!(f.r, env);
        assert_eq!(f.resource_id, 1);
        // 槽内尾随（槽 64B > 命令 56B）不碍——解析恰长门按切片执法。
    }

    #[test]
    fn v2_frame_flush_batch_full_matches_v1() {
        // 全帧回退形态零漂移锚：批 K=1 全帧矩形 vs v1 `frame_flush_cmds`
        // 全帧对——TRANSFER 槽逐字节相同;FLUSH 槽除 flags/fence_id（v2 置
        // fence 回显位）外逐字节相同。
        let mut v1 = [0u8; 128];
        let (n_t, n_f) = frame_flush_cmds(&mut v1, 800, 600, 3).unwrap();
        let mut b = [0u8; FRAME_CMD_MAX * ACMD_SZ];
        let full = vgp::Rect::new(0, 0, 800, 600);
        let n = frame_flush_batch(&mut b, 3, 800, 600, &[full], &full, 7).unwrap();
        assert_eq!(n, 2);
        assert_eq!(&b[..n_t], &v1[..n_t]); // TRANSFER 全帧：offset=0 同形
        let fs = &b[ACMD_SZ..][..n_f];
        let (h, f) = vgp::parse_resource_flush(fs).unwrap();
        let (_, f1) = vgp::parse_resource_flush(&v1[n_t..n_t + n_f]).unwrap();
        assert_eq!((h.flags, h.fence_id), (vgp::FLAG_FENCE, 7));
        assert_eq!(f, f1); // FLUSH 体（rect/resource_id）与 v1 全等
        assert_eq!(fs[..4], v1[n_t..n_t + 4]); // 头 kind 域相同（flags = fence 位差,上文已断）
        assert_eq!(fs[16..], v1[n_t + 16..n_t + n_f]); // ctx/ring/pad 起同
    }

    #[test]
    fn v2_frame_flush_batch_rejects() {
        // 拒面六连：空集 / 超帽 / 越界矩形 / 越界包络 / 零面积 / out 不足。
        let mut b = [0u8; FRAME_CMD_MAX * ACMD_SZ];
        let env = vgp::Rect::new(0, 0, 800, 600);
        assert!(frame_flush_batch(&mut b, 1, 800, 600, &[], &env, 1).is_none());
        let nine: Vec<vgp::Rect> =
            (0..FLUSH_RECT_MAX + 1).map(|i| vgp::Rect::new(i as u32 * 8, 0, 8, 8)).collect();
        assert!(frame_flush_batch(&mut b, 1, 800, 600, &nine, &env, 1).is_none());
        let over = [vgp::Rect::new(790, 0, 20, 10)];
        assert!(frame_flush_batch(&mut b, 1, 800, 600, &over, &env, 1).is_none());
        let okr = [vgp::Rect::new(0, 0, 10, 10)];
        let bad_env = vgp::Rect::new(0, 0, 900, 600);
        assert!(frame_flush_batch(&mut b, 1, 800, 600, &okr, &bad_env, 1).is_none());
        let zero = [vgp::Rect::new(0, 0, 0, 10)];
        assert!(frame_flush_batch(&mut b, 1, 800, 600, &zero, &env, 1).is_none());
        let mut tiny = [0u8; 64];
        assert!(frame_flush_batch(&mut tiny, 1, 800, 600, &okr, &env, 1).is_none());
        // 恰满帽 K=8 过（sanity）。
        let eight: Vec<vgp::Rect> =
            (0..FLUSH_RECT_MAX).map(|i| vgp::Rect::new(i as u32 * 8, 0, 8, 8)).collect();
        assert_eq!(frame_flush_batch(&mut b, 1, 800, 600, &eight, &env, 1).unwrap(), 9);
    }

    #[test]
    fn v2_batch_resps_ok_bucket() {
        // 回执桶：全 OK + 末命令 fence 回显 → true;错误族/缺回显/错值/
        // 截断/空批 → false（错误族与 NODATA 同形 24B——parse 恰长+kind 门）。
        let mk = |flags: u32, fence: u64| {
            let mut out = [0u8; 24];
            let hdr = vgp::CtrlHdr {
                kind: vgp::RESP_OK_NODATA,
                flags,
                fence_id: fence,
                ctx_id: 0,
                ring_idx: 0,
            };
            vgp::encode_resp_nodata(&mut out, &hdr).unwrap();
            out
        };
        let mut bucket = [0u8; FRAME_CMD_MAX * vgp::RESP_OK_NODATA_SZ];
        for i in 0..3 {
            let s = mk(0, 0);
            bucket[i * 24..(i + 1) * 24].copy_from_slice(&s);
        }
        let last = mk(vgp::FLAG_FENCE, 42);
        bucket[3 * 24..4 * 24].copy_from_slice(&last);
        assert!(batch_resps_ok(&bucket, 4, 42));
        // 末命令缺 fence 回显。
        let nof = mk(0, 42);
        bucket[3 * 24..4 * 24].copy_from_slice(&nof);
        assert!(!batch_resps_ok(&bucket, 4, 42));
        // 回显 fence 值错。
        let wrong = mk(vgp::FLAG_FENCE, 41);
        bucket[3 * 24..4 * 24].copy_from_slice(&wrong);
        assert!(!batch_resps_ok(&bucket, 4, 42));
        // 中位命令错误族（kind 门拒）。
        let mut err = [0u8; 24];
        vgp::encode_resp_err(
            &mut err,
            &vgp::CtrlHdr {
                kind: vgp::RESP_ERR_INVALID_PARAMETER,
                flags: 0,
                fence_id: 0,
                ctx_id: 0,
                ring_idx: 0,
            },
        )
        .unwrap();
        bucket[24..48].copy_from_slice(&err);
        bucket[3 * 24..4 * 24].copy_from_slice(&last);
        assert!(!batch_resps_ok(&bucket, 4, 42));
        // 桶截断 / n=0 / n 超帽。
        assert!(!batch_resps_ok(&bucket[..3 * 24], 4, 42));
        assert!(!batch_resps_ok(&bucket, 0, 42));
        assert!(!batch_resps_ok(&bucket, FRAME_CMD_MAX + 1, 42));
    }

    // —— 波3 批B 带合成 v1 测试（分类/带集/重放序/行重放——纯逻辑零 QEMU）——

    /// 快照表项构造助手（x/y/w/h 即有效几何域——WmEntry 语义）。
    fn wm_ent(id: u16, layer: u8, flags: u8, x: u16, y: u16, w: u16, h: u16) -> gp::WmEntry {
        gp::WmEntry { id, parent: 0, group: 0, x, y, w, h, layer, z: 0, flags }
    }

    #[test]
    fn classify_surface_six_classes() {
        // 语义表六类（SPEC-batchB §5 矩阵 1）：0xFFFF 值域档优先于 wid 查表
        // （id 恰 0xFFFF 的窗不冒领）；InternalRefresh 为内部调用点专值，
        // classify 永不产出——枚举六变体互异自证。
        let entries = [
            wm_ent(7, gp::WM_LAYER_SYSTEM, gp::WM_WIN_F_VISIBLE, 0, 0, 800, 100),
            wm_ent(9, gp::WM_LAYER_SERVICE, gp::WM_WIN_F_VISIBLE, 0, 0, 800, 44),
            wm_ent(11, gp::WM_LAYER_APP, gp::WM_WIN_F_VISIBLE, 0, 0, 800, 400),
            wm_ent(13, gp::WM_LAYER_APP, 0, 0, 0, 800, 400), // APP 隐藏窗
            wm_ent(0xFFFF, gp::WM_LAYER_SYSTEM, gp::WM_WIN_F_VISIBLE, 0, 0, 800, 40),
        ];
        assert_eq!(classify_surface(SYS_SURFACE_ID, &entries, 5), SurfaceClass::SysFace);
        assert_eq!(classify_surface(0, &entries, 5), SurfaceClass::AppCompat);
        assert_eq!(classify_surface(999, &entries, 5), SurfaceClass::AppCompat); // 未知 wid
        assert_eq!(classify_surface(0x1_0000, &entries, 5), SurfaceClass::AppCompat); // u16 域外
        assert_eq!(classify_surface(7, &entries, 5), SurfaceClass::WinBand); // SYSTEM 层
        assert_eq!(classify_surface(9, &entries, 5), SurfaceClass::WinBand); // SERVICE 层
        assert_eq!(classify_surface(11, &entries, 5), SurfaceClass::WinApp);
        assert_eq!(classify_surface(13, &entries, 5), SurfaceClass::Hidden); // vis=false
        // 快照有效前缀（count=0 = 空快照 → 一切 wid 走 AppCompat）。
        assert_eq!(classify_surface(9, &entries, 0), SurfaceClass::AppCompat);
        let six = [
            SurfaceClass::SysFace,
            SurfaceClass::InternalRefresh,
            SurfaceClass::AppCompat,
            SurfaceClass::WinBand,
            SurfaceClass::WinApp,
            SurfaceClass::Hidden,
        ];
        for i in 0..six.len() {
            for j in (i + 1)..six.len() {
                assert_ne!(six[i], six[j]);
            }
        }
    }

    #[test]
    fn plan_bands_filters_and_caps() {
        // 选取门四连（SPEC-batchB §5 矩阵 2）：vis 过滤、层过滤（APP 不带化）、
        // 高帽（780px softimed 形态 → 拒 + warn[候选序 0]）、宽帽（w>fb_w →
        // 拒 + warn[候选序 1]）、全界外（rect∩fb=∅ → 静默跳零 warn）。band_bot=0
        // 隔离 fallback 面。
        let entries = [
            wm_ent(1, gp::WM_LAYER_SERVICE, 0, 0, 0, 800, 44),               // !vis：跳
            wm_ent(2, gp::WM_LAYER_APP, gp::WM_WIN_F_VISIBLE, 0, 0, 800, 100), // APP 层：跳
            wm_ent(3, gp::WM_LAYER_SERVICE, gp::WM_WIN_F_VISIBLE, 0, 0, 800, 780), // 高超帽
            wm_ent(4, gp::WM_LAYER_SYSTEM, gp::WM_WIN_F_VISIBLE, 0, 0, 900, 100), // 宽超 fb_w
            wm_ent(5, gp::WM_LAYER_SYSTEM, gp::WM_WIN_F_VISIBLE, 900, 0, 100, 100), // 全界外
        ];
        let (bands, warn, n) = plan_bands(&entries, 5, 800, 600, 0);
        assert_eq!(bands, [None; BAND_SLOTS]);
        assert_eq!(n, 0);
        assert_eq!(warn, [true, true, false, false]); // 界外/过滤不算候选
        // 正选：两窗依快照序占槽 0/1；触底带窗与 fallback 重叠 → 第 4 槽退役。
        let entries = [
            wm_ent(6, gp::WM_LAYER_SERVICE, gp::WM_WIN_F_VISIBLE, 0, 0, 800, 44),
            wm_ent(7, gp::WM_LAYER_SYSTEM, gp::WM_WIN_F_VISIBLE, 0, 556, 800, 44),
        ];
        let (bands, warn, n) = plan_bands(&entries, 2, 800, 600, 44);
        assert_eq!(warn, [false; BAND_SLOTS]);
        assert_eq!(n, 2);
        assert_eq!(bands[0].unwrap().rect, gp::Rect { x: 0, y: 0, w: 800, h: 44 });
        assert_eq!(bands[0].unwrap().layer, gp::WM_LAYER_SERVICE);
        assert!(!bands[0].unwrap().fallback);
        assert_eq!(bands[1].unwrap().rect, gp::Rect { x: 0, y: 556, w: 800, h: 44 });
        assert!(bands[3].is_none()); // 窗带 y 556..600 ∩ fallback 556..600 非空
    }

    #[test]
    fn plan_bands_overflow_truncates() {
        // 超 3 带截断：4 形态合格候选只取快照序前 3，warn[3] 置位（SPEC-batchB
        // §5 矩阵 2；band_bot=0 隔离 fallback）。
        let entries = [
            wm_ent(1, gp::WM_LAYER_SYSTEM, gp::WM_WIN_F_VISIBLE, 0, 0, 400, 40),
            wm_ent(2, gp::WM_LAYER_SERVICE, gp::WM_WIN_F_VISIBLE, 400, 0, 400, 40),
            wm_ent(3, gp::WM_LAYER_SYSTEM, gp::WM_WIN_F_VISIBLE, 0, 40, 400, 40),
            wm_ent(4, gp::WM_LAYER_SERVICE, gp::WM_WIN_F_VISIBLE, 400, 40, 400, 40),
        ];
        let (bands, warn, n) = plan_bands(&entries, 4, 800, 600, 0);
        assert_eq!(n, 3);
        assert_eq!(bands[0].unwrap().layer, gp::WM_LAYER_SYSTEM);
        assert_eq!(bands[1].unwrap().layer, gp::WM_LAYER_SERVICE);
        assert_eq!(bands[2].unwrap().layer, gp::WM_LAYER_SYSTEM);
        assert!(bands[3].is_none());
        assert_eq!(warn, [false, false, false, true]);
    }

    #[test]
    fn plan_bands_fallback_activation_and_retirement() {
        // 底常量 fallback：空快照 = fallback-only（第 4 槽恰为
        // (0, fb_h−band_bot, fb_w, band_bot)）；零重叠（含半开邻接）激活、
        // 一像素重叠退役；退化 band_bot（0 / ≥fb_h）不建。
        let empty: [gp::WmEntry; 0] = [];
        let (bands, warn, n) = plan_bands(&empty, 0, 1080, 1920, 135);
        assert_eq!(n, 1);
        assert_eq!(warn, [false; BAND_SLOTS]);
        let fb = bands[3].unwrap();
        assert!(fb.fallback);
        assert_eq!(fb.layer, gp::WM_LAYER_SYSTEM);
        assert_eq!(fb.rect, gp::Rect { x: 0, y: 1920 - 135, w: 1080, h: 135 });
        // 顶带窗与底带零重叠 → fallback 仍占第 4 槽。
        let entries = [wm_ent(1, gp::WM_LAYER_SYSTEM, gp::WM_WIN_F_VISIBLE, 0, 0, 1080, 117)];
        let (bands, _, n) = plan_bands(&entries, 1, 1080, 1920, 135);
        assert_eq!(n, 2);
        assert!(bands[3].is_some());
        // 半开邻接（窗 y 1670..1785 vs 底带 1785..1920）零重叠 → 激活。
        let entries = [wm_ent(2, gp::WM_LAYER_SYSTEM, gp::WM_WIN_F_VISIBLE, 0, 1670, 1080, 115)];
        let (bands, _, _) = plan_bands(&entries, 1, 1080, 1920, 135);
        assert!(bands[3].is_some());
        // 一像素重叠（窗 y 1670..1786 vs 底带 1785..1920）→ 退役。
        let entries = [wm_ent(3, gp::WM_LAYER_SYSTEM, gp::WM_WIN_F_VISIBLE, 0, 1670, 1080, 116)];
        let (bands, _, _) = plan_bands(&entries, 1, 1080, 1920, 135);
        assert!(bands[3].is_none());
        // 退化 band_bot：0 与 ≥fb_h 都不建。
        let (b0, _, n0) = plan_bands(&empty, 0, 800, 600, 0);
        assert_eq!(n0, 0);
        assert!(b0[3].is_none());
        let (bh, _, _) = plan_bands(&empty, 0, 800, 600, 600);
        assert!(bh[3].is_none());
    }

    #[test]
    fn plan_bands_clips_window_rect_into_fb() {
        // 部分越界窗：rect∩fb 非空仍入槽，槽内 rect = 裁回 fb 界的交域；
        // 形态帽按窗自身 w/h 执法（h=300 帽外——裁后 100 也不收）。
        let entries = [wm_ent(1, gp::WM_LAYER_SERVICE, gp::WM_WIN_F_VISIBLE, 700, 550, 50, 100)];
        let (bands, warn, n) = plan_bands(&entries, 1, 800, 600, 44);
        assert_eq!(warn, [false; BAND_SLOTS]);
        assert_eq!(n, 1);
        assert_eq!(bands[0].unwrap().rect, gp::Rect { x: 700, y: 550, w: 50, h: 50 });
        assert!(bands[3].is_none()); // 裁后仍触底带区 → fallback 退役
        // 窗自身高超帽（裁后 h=100 在帽内）：拒 + warn[0]，仅 fallback 存活。
        let entries = [wm_ent(2, gp::WM_LAYER_SERVICE, gp::WM_WIN_F_VISIBLE, 0, 500, 800, 300)];
        let (bands, warn, n) = plan_bands(&entries, 1, 800, 600, 44);
        assert_eq!(n, 1); // fallback-only
        assert!(bands[0].is_none());
        assert!(bands[3].unwrap().fallback);
        assert_eq!(warn, [true, false, false, false]);
    }

    #[test]
    fn band_replay_plan_orders_service_before_system() {
        // 定序「序即正确性」（frame_flush_batch 先例）：SERVICE 槽先于
        // SYSTEM 槽，fallback 槽随 SYSTEM 组殿后——后画压先画。
        let bands = [
            Some(BandPlan { layer: gp::WM_LAYER_SYSTEM, rect: gp::Rect { x: 0, y: 0, w: 800, h: 40 }, fallback: false }),
            Some(BandPlan { layer: gp::WM_LAYER_SERVICE, rect: gp::Rect { x: 0, y: 40, w: 800, h: 40 }, fallback: false }),
            None,
            Some(BandPlan { layer: gp::WM_LAYER_SYSTEM, rect: gp::Rect { x: 0, y: 556, w: 800, h: 44 }, fallback: true }),
        ];
        let damage = [gp::Rect { x: 0, y: 0, w: 800, h: 600 }];
        let mut out = [(0usize, gp::Rect { x: 0, y: 0, w: 0, h: 0 }); BAND_SLOTS * gp::DAMAGE_MAX + 2];
        let n = band_replay_plan(&damage, 1, false, 800, 600, &bands, &mut out);
        assert_eq!(n, 3);
        assert_eq!(out[0].0, 1); // SERVICE 槽 1 先
        assert_eq!(out[1].0, 0); // SYSTEM 窗口带槽 0 次
        assert_eq!(out[2].0, 3); // fallback 槽 3 随 SYSTEM 序殿后
        assert_eq!(out[0].1, gp::Rect { x: 0, y: 40, w: 800, h: 40 });
        assert_eq!(out[2].1, gp::Rect { x: 0, y: 556, w: 800, h: 44 });
    }

    #[test]
    fn band_replay_plan_full_frame_always_triggers() {
        // FULL 帧恒触发（damage 视为单全屏 rect——无视 damage_n=0）：每 Some
        // 带恰出 1 条整带 rect；零带 plan（带面禁用形）= 0 条；非 FULL 与带
        // 零交 = 0 条。
        let bands = [
            Some(BandPlan { layer: gp::WM_LAYER_SYSTEM, rect: gp::Rect { x: 0, y: 0, w: 800, h: 40 }, fallback: false }),
            None,
            None,
            Some(BandPlan { layer: gp::WM_LAYER_SYSTEM, rect: gp::Rect { x: 0, y: 556, w: 800, h: 44 }, fallback: true }),
        ];
        let mut out = [(0usize, gp::Rect { x: 0, y: 0, w: 0, h: 0 }); BAND_SLOTS * gp::DAMAGE_MAX + 2];
        let no: [gp::Rect; 0] = [];
        let n = band_replay_plan(&no, 0, true, 800, 600, &bands, &mut out);
        assert_eq!(n, 2);
        assert_eq!(out[0], (0, gp::Rect { x: 0, y: 0, w: 800, h: 40 }));
        assert_eq!(out[1], (3, gp::Rect { x: 0, y: 556, w: 800, h: 44 }));
        let empty_bands = [None; BAND_SLOTS];
        assert_eq!(band_replay_plan(&no, 0, true, 800, 600, &empty_bands, &mut out), 0);
        let mid = [gp::Rect { x: 0, y: 300, w: 800, h: 100 }];
        assert_eq!(band_replay_plan(&mid, 1, false, 800, 600, &bands, &mut out), 0);
    }

    #[test]
    fn band_replay_plan_intersection_shapes() {
        // damage∩带求交形态（v2_rects_clip_into_shapes 先例）：部分重叠取交
        // 域、带序内 damage 保序、u32 溢出域（w=u32::MAX）不炸且裁回界内、
        // damage_n 之外的条目不参与。
        let bands = [
            Some(BandPlan { layer: gp::WM_LAYER_SERVICE, rect: gp::Rect { x: 100, y: 0, w: 600, h: 40 }, fallback: false }),
            None,
            None,
            None,
        ];
        let damage = [
            gp::Rect { x: 0, y: 0, w: 300, h: 20 },          // 左交：x 100..300
            gp::Rect { x: 600, y: 10, w: u32::MAX, h: 40 },  // 溢出域：交 x 600..700
            gp::Rect { x: 0, y: 500, w: 800, h: 50 },        // 零交
        ];
        let mut out = [(0usize, gp::Rect { x: 0, y: 0, w: 0, h: 0 }); BAND_SLOTS * gp::DAMAGE_MAX + 2];
        let n = band_replay_plan(&damage, 2, false, 800, 600, &bands, &mut out);
        assert_eq!(n, 2); // damage_n=2：第 3 条不参与
        assert_eq!(out[0], (0, gp::Rect { x: 100, y: 0, w: 200, h: 20 }));
        assert_eq!(out[1], (0, gp::Rect { x: 600, y: 10, w: 100, h: 30 }));
        // damage_n=3：第 3 条与带零交 → 仍 2 条。
        assert_eq!(band_replay_plan(&damage, 3, false, 800, 600, &bands, &mut out), 2);
    }

    #[test]
    fn replay_rows_u32_sentinel() {
        // 哨兵缓冲（berry-gfx icon_format.rs 形态蓝本）：A=0 透明键底穿透
        // （保底值不触——RGB 非零也跳，键在 A 位）、A!=0 一律覆写原值（半透
        // 也是不透明覆盖——1-bit 键非 blend）、返回写数 = 非 A=0 像素数。
        let mut dst = [0xDEAD_BEEFu32; 8];
        let src = [
            0x0000_0000, // A=0：跳
            0x00FF_0000, // A=0（RGB 非零也跳）
            0x8000_FF00, // 半透：原值覆写
            0xFFFF_FFFF, // 不透明
            0x0100_0000, // A 最小非零值：覆写
            0x0000_00FF, // A=0：跳
            0xFF00_00FF,
            0xAB12_3456,
        ];
        let n = replay_rows_u32(&mut dst, &src);
        assert_eq!(n, 5);
        assert_eq!(dst[0], 0xDEAD_BEEF); // 底穿透
        assert_eq!(dst[1], 0xDEAD_BEEF);
        assert_eq!(dst[2], 0x8000_FF00);
        assert_eq!(dst[3], 0xFFFF_FFFF);
        assert_eq!(dst[4], 0x0100_0000);
        assert_eq!(dst[5], 0xDEAD_BEEF);
        assert_eq!(dst[6], 0xFF00_00FF);
        assert_eq!(dst[7], 0xAB12_3456);
        // 全透行：0 写、底全穿透。
        let all_t = [0x00FF_FFFFu32; 4];
        let mut d4 = [7u32; 4];
        assert_eq!(replay_rows_u32(&mut d4, &all_t), 0);
        assert_eq!(d4, [7, 7, 7, 7]);
        // 长度不等按短者（防御档不 panic）。
        let mut d2 = [0u32; 2];
        assert_eq!(replay_rows_u32(&mut d2, &[0xFF00_0000; 3]), 2);
        assert_eq!(d2, [0xFF00_0000; 2]);
    }

    #[test]
    fn band_cache_bytes_gate() {
        // 容量边界（SPEC-batchB §5 矩阵 5）：ramfb 800 与 big 1080 两档在
        // 4MiB 门内；临界 fb_w=1638 恰入（4,193,280）、1639 越门 → None（带
        // 面禁用降级）；fb_w=0（几何未就绪）拒。
        assert_eq!(band_cache_bytes(800), Some(800 * 160 * 4 * 4));
        assert_eq!(band_cache_bytes(1080), Some(1080 * 160 * 4 * 4));
        assert_eq!(band_cache_bytes(1638), Some(4_193_280));
        assert_eq!(band_cache_bytes(1639), None);
        assert_eq!(band_cache_bytes(0), None);
        assert_eq!(BAND_SLOTS, 4);
        assert_eq!(BAND_H_CAP, 160);
        assert_eq!(BAND_CACHE_BYTES_MAX, 4 << 20);
    }
}
