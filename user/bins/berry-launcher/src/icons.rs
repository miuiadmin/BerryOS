//! icons —— app 注册表 + 图标供给桥（GUI-8 段③ W2，docs/
//! GUI-8实施设计草案.md §4 段③「IconSource 供件桥接」）。
//!
//! 责任：boot 期 APP_LIST 拉取（S3 蓝图：**一请求 → 分片流**——svcmgr
//! `app_serve_list` 对单个 APP_LIST_REQ 发整条 ENTRY/CHUNK/END 流，应答
//! 逐帧到账为独立 channel 消息 → 客户端 send-once + recv-loop）→
//! `ListDecoder` 三执法（idx 单调 +1 / blob 收支 / END 对账——任何 None
//! 弃整流，半张清单不进渲染面）→ `AppEntry` 装配 + BYI1 两遍解码 →
//! `LauncherIcons`（berry-ui `IconSource` 真供件）。
//!
//! 冻结相（R1 修约 1/2）：launcher extras 恰 [wm] → `app_cli_h()==0`
//! → `init` 即返（零 RPC 零打印——空注册表常态，desktop 全占位格）。
//!
//! **图标 id 稠密域契约**：仅解码成功的条目按序编 0..n 稠密 id
//! （`NO_ICON` = 域外哨兵）——desktop 只对域内 id 建 Icon 节点；
//! `LauncherIcons` 是**真供件**（`is_empty()==false`）：berry-ui 契约下
//! 真供件对未注册 id 返 None = debug_assert loud（layout.rs icon_dims /
//! paint.rs emit_image 两面同律）。稠密赋值即一致性构造：id ∈ 域 ⟺
//! FACES[id] 在册——两面查表恒 Some，loud 面结构性不可达。
//!
//! **像素常驻帽**（草案 kill criteria ③ 回落）：Σ w*h ≤ 32MiB/4 像素
//! ——超帽条目降 `NO_ICON`（label-only 格）+ eprintln 诊断，不崩不静默
//! 吞。boot 全量常驻（无 LRU 换入换出——D28 供给槽是借用面，缓存编排
//! 归消费侧；berry-gfx `IconLruCache` 服务 GlyphBits mono 面非本面）。
//!
//! 静态面：`OnceLock` 装载（免 static mut 直写 + 单写幂等——boot 单线程
//! 无竞态面）；像素/条目 `Box::leak` 堆置（elf2flat 会把 .bss 物化进平像
//! ——svcmgr 单页墙判例：禁大 static 解码 scratch，解码缓冲走堆 Vec）。

use std::sync::OnceLock;

use berry_appproto as ap;
use berry_gfx::icon::{decode, IconError};
use berry_layout::ID_MAX;
use berry_sys::Handle;
use berry_ui::IconSource;

use crate::{rpc_recv, rpc_send};

/// 无图标哨兵（icon 域外值——desktop 不建 Icon 节点）。
pub const NO_ICON: u16 = u16::MAX;

/// 注册表条目帽（防御面：wire 无条目数先验，超帽 = 协议滥用 → 弃整流；
/// CELL_MAX=48 恰满 + 裕量）。
const REG_MAX: usize = 64;

/// 单条目 BYI1 blob 帽（wire blob_len u32 先验拒面：w*h ≤ 65536 px 的
/// 最坏 RLE 流 ≈ 384KiB，512KiB 帽 = 慷慨上界；超帽弃整流）。
const BLOB_MAX: usize = 512 * 1024;

/// 像素常驻帽（32MiB / 4B = 8'388'608 px 累计；kill criteria ③）。
const PX_CAP: u64 = 32 * 1024 * 1024 / 4;

/// 注册表条目（APP_LIST 应答逐条镜像；id 域 = berry-layout/
/// berry-manifest ID_MAX=24 同源词法；label 域 = wire APP_ENTRY label[64]
/// ——草案 §5，两帽值以彼处为对账锚点）。
#[derive(Clone, Copy)]
pub struct AppEntry {
    pub id: [u8; ID_MAX],
    pub id_len: u8,
    pub label: [u8; 64],
    pub label_len: u8,
    /// 稠密图标 id（`NO_ICON` = 无图标/解码失败/超帽）。
    pub icon: u16,
}

impl AppEntry {
    /// id 有效字节切片。
    pub fn id_bytes(&self) -> &[u8] {
        &self.id[..self.id_len as usize]
    }

    /// 显示名有效字节切片（桌面 label / a11y label 同源）。
    pub fn label_bytes(&self) -> &[u8] {
        &self.label[..self.label_len as usize]
    }
}

/// boot 注册表（未装载 = 空切片——冻结相常态）。
static REGISTRY: OnceLock<&'static [AppEntry]> = OnceLock::new();

/// 注册表只读面（未装载 = 空切片）。
pub fn registry() -> &'static [AppEntry] {
    REGISTRY.get().copied().unwrap_or(&[])
}

/// 注册表装载（幂等——首装定局，二次 set 恒保首装）。
fn set_registry(entries: &'static [AppEntry]) {
    let _ = REGISTRY.set(entries);
}

/// 解码产物（堆常驻像素面；`Box::leak` 外层切片即整体终身——内层 Vec
/// 随之永不回收 = 分配即常驻，boot 单次无增长面）。
struct IconFace {
    w: u32,
    h: u32,
    px: Vec<u32>,
}

/// 稠密 id → 解码面（`REGISTRY` 同批装配——两表索引一致性由构造序保证）。
static FACES: OnceLock<&'static [IconFace]> = OnceLock::new();

/// boot 期初始化（main 早期单次调用）。`app_h == 0`（冻结相 extras 恰
/// [wm]）→ 即返空注册表零 RPC；在场 → APP_LIST 拉取装载（失败 = eprintln
/// 诊断 + 空注册表——半张清单不进渲染面，launcher 不死）。
pub fn init(app_h: Handle) {
    if app_h == 0 {
        return;
    }
    match pull(app_h) {
        Some(entries) => set_registry(entries),
        None => eprintln!("launcher: app list unavailable -- empty registry"),
    }
}

/// 流中待竣工条目（ENTRY 开工 → CHUNK 供料 → 满员转竣工解码）。
struct Pending {
    id: [u8; ID_MAX],
    id_len: u8,
    label: [u8; 64],
    label_len: u8,
    blob: Vec<u8>,
}

/// APP_LIST 拉取 + 装配（FACES 同批装载）。返 None = 弃整流（诊断已打）。
fn pull(app_h: Handle) -> Option<&'static [AppEntry]> {
    let mut req = [0u8; 256];
    let m = ap::encode_list_request(&mut req);
    rpc_send(app_h, &req[..m], &[]);

    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let mut dec = ap::ListDecoder::new();
    let mut entries: Vec<AppEntry> = Vec::new();
    let mut faces: Vec<IconFace> = Vec::new();
    let mut px_used: u64 = 0;
    let mut pending: Option<Pending> = None;

    loop {
        let (n, nh) = rpc_recv(app_h, &mut rep, &mut rhs);
        if nh > 0 {
            // 应答流无随行 handle 面——协议破缺，即关防泄漏后弃流。
            for i in 0..nh {
                crate::close_h(rhs[i]);
            }
            eprintln!("launcher: app list reply carries handles");
            return None;
        }
        if let Some(()) = ap::parse_err_reply(&rep, n) {
            eprintln!("launcher: app list st=bad");
            return None;
        }
        let frame = match dec.feed(&rep, n) {
            Some(f) => f,
            None => {
                eprintln!("launcher: app list stream corrupt at {}", entries.len());
                return None;
            }
        };
        match frame {
            ap::ListFrame::Entry { id, label, icon_w, icon_h, blob_len } => {
                if entries.len() >= REG_MAX {
                    eprintln!("launcher: app list > {REG_MAX} entries");
                    return None;
                }
                if blob_len as usize > BLOB_MAX {
                    eprintln!("launcher: app icon blob {blob_len} > cap");
                    return None;
                }
                let _ = (icon_w, icon_h); // 尺寸真值源 = blob 头（decode 产物）
                // 前条目已满员（decoder 执法 remain==0 才放行 ENTRY）→ 竣工。
                if let Some(pe) = pending.take() {
                    finish_entry(&mut entries, &mut faces, &mut px_used, pe);
                }
                let mut e = Pending {
                    id: [0; ID_MAX],
                    id_len: id.len() as u8,
                    label: [0; 64],
                    label_len: label.len() as u8,
                    blob: Vec::with_capacity(blob_len as usize),
                };
                e.id[..id.len()].copy_from_slice(id);
                e.label[..label.len()].copy_from_slice(label);
                pending = Some(e);
            }
            ap::ListFrame::Chunk { data, .. } => {
                match pending.as_mut() {
                    Some(pe) => pe.blob.extend_from_slice(data),
                    // decoder 已执法「无待收拒 CHUNK」——此臂结构性不可达。
                    None => {
                        eprintln!("launcher: app chunk without entry");
                        return None;
                    }
                }
            }
            ap::ListFrame::End { .. } => {
                if let Some(pe) = pending.take() {
                    finish_entry(&mut entries, &mut faces, &mut px_used, pe);
                }
                break;
            }
        }
    }

    let leaked: &'static [AppEntry] = Box::leak(entries.into_boxed_slice());
    let _ = FACES.set(Box::leak(faces.into_boxed_slice()));
    Some(leaked)
}

/// 条目竣工：BYI1 两遍解码 + 稠密 id 赋值 + 常驻帽执法。
/// 拒面（magic/版本/几何/截断/超帽）→ `NO_ICON` + eprintln（label-only
/// 格降级，不弃整流——单条目坏件不连坐整桌）。
fn finish_entry(entries: &mut Vec<AppEntry>, faces: &mut Vec<IconFace>, px_used: &mut u64, pe: Pending) {
    let mut icon = NO_ICON;
    // 第一遍空缓冲探尺：头合法 → BufferTooSmall{need = blob 真值 w*h}
    //（magic/版本/flags/几何拒面也在此遍全数暴露）；need ≤ 65536 由
    // decode 几何帽执法——帽/账以 blob 头为准（ENTRY 帧尺寸域非真值源）。
    match decode(&pe.blob, &mut []) {
        Ok(_) => {} // w*h ≥ 1 恒不可达（空缓冲必 BufferTooSmall）
        Err(IconError::BufferTooSmall { need, .. }) => {
            if *px_used + need as u64 > PX_CAP {
                eprintln!("launcher: icon residency cap hit (need {need} px) -- label only");
            } else {
                let mut px = vec![0u32; need];
                match decode(&pe.blob, &mut px) {
                    Ok(desc) => {
                        *px_used += need as u64;
                        icon = faces.len() as u16;
                        faces.push(IconFace { w: desc.w, h: desc.h, px });
                    }
                    Err(e) => eprintln!("launcher: icon decode {e:?}"),
                }
            }
        }
        Err(e) => eprintln!("launcher: icon decode {e:?}"),
    }
    entries.push(AppEntry {
        id: pe.id,
        id_len: pe.id_len,
        label: pe.label,
        label_len: pe.label_len,
        icon,
    });
}

/// launcher 图标供件（berry-ui D28 供给槽真身——`Ui::set_icon_source`
/// 消费）。
pub struct LauncherIcons;

static ICONS: LauncherIcons = LauncherIcons;

/// 供件引用（`&'static dyn`——供给槽生命周期面；unit static 即终身）。
pub fn icon_source() -> &'static dyn IconSource {
    &ICONS
}

impl IconSource for LauncherIcons {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        let faces = FACES.get().copied().unwrap_or(&[]);
        let f = faces.get(id as usize)?;
        Some((f.w, f.h))
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        let faces = FACES.get().copied().unwrap_or(&[]);
        let f = faces.get(id as usize)?;
        Some(&f.px)
    }
}
