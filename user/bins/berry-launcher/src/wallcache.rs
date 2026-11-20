//! wallcache —— B1 壁纸预缩放缓存（guest 渲染性能刀·壁纸绘制路径）。
//!
//! 动机（侦查案卷 guest-render-perf 路 fix_options 首条与壁纸成本账）：
//! 桌面壁纸叶是 135×240 rodata 静态源（appshell wallpaper.rs）以
//! SmoothArgb 双线性放大到视口（1088×1920）——实测 6.61ns/px ≈ 13.7ms/
//! 全帧（60Hz 预算 82%），且桌面页无底色、遮挡剔除只认实心 Rect 叶
//!（Icon/Text 恒不计遮挡源），翻页 damage 条内壁纸全量可见全量重算
//! ——正是显示体系设计「全帧 SmoothArgb 禁入每帧路径」所禁形态。本
//! 模块在 boot 期一次性把源双线性烘进全尺寸缓存，此后壁纸发射退化为
//! Argb 平拷贝（实测 0.8ns/px 量级）——主导成本 ÷8，像素逐位零漂移。
//!
//! 零漂移论证：烘焙直接复用 berry-gfx 生产光栅（Queue::submit +
//! ImageSrc::SmoothArgb + 全幅 pass）——即逐帧路径本来要跑的同一段
//! smooth_argb_blit（raster.rs），同源同几何同 clip；缓存经 Argb 回放
//! 时 r/g/b 通道原样进出 desc.format().pack，与 smooth 路径落笔像素
//! 逐位相同。纹理格式（Rgbx/Bgrx）pack 不携带 alpha 位，缓存像素经
//! 一遍 `|= 0xFF00_0000` 补笔满足 Argb 面 1-bit 透明键（A!=0 才落
//! 笔）；源全不透明（32400 px alpha 全 0xFF——boot 期运行时复验）⇒
//! smooth 臂每目标格四采样加权和 ≥252 ≥128 必落笔，盲 OR 不触未写格。
//!
//! 内存面取舍（全尺寸缓冲 1088×1920×4 ≈ 8.36MB 的预算核账）：三路
//! 分配里只有 VMO 路放得下——①heap-4m 堆 4MiB 装不下；②大 static
//! .bss 犯双禁：spawn 映幅帽 16MiB（berry-spawngeo ELF_MAP_MAX，现
//! launcher 映像 span 已 ~9.9MiB，+8.36MB 即穿帽拒启）+「elf2flat 物
//! 化 .bss」禁大 static 判例（icons 头注）；③VMO 三连（create/
//! commit/map）= pump 帧 VMO 同形先例——本进程已映射同尺寸 8.36MB
//! 帧 VMO，phone 档客机 256MiB 内存足容。映射位 = mmap VA 窗
//! [0x5000_0000,0x6000_0000) 内固定 +32MiB 防御位（pump FRAME_VA=
//! +16MiB 同款纪律，first-fit 底段 16MiB 留 libc mmap 零交叠）。
//!
//! 生命周期：缓存与壁纸源绑定——源不变即恒有效（桌面壁纸 v1 不热
//! 换）；监督重启 = 新进程重烘（一次性 ~13.7ms 可接受）。任何一步
//! 失败（self vmar gift 缺席/VMO 拒/几何拒/源含透明像素/submit 拒）
//! → 弃缓存整体降级回逐帧 SmoothArgb 原路径——三面全透传内层，行为
//! 与今日逐位一致 + 一行失败诊断（失败面 eprintln 惯例）。

use berry_gfx::cmd::{Cmd, Color, DisplayList, ImageSrc};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_libc::io::self_vmar_h;
use berry_libc::mmap::VMO_MAX;
use berry_sys::{
    berry_handle_close, berry_vmar_map, berry_vmar_unmap, berry_vmo_commit, berry_vmo_create,
    Handle, RIGHT_READ, RIGHT_WRITE,
};
use berry_ui::IconSource;

/// 缓存映射 VA（mmap VA 窗 [0x5000_0000,0x6000_0000) 内固定 +32MiB
/// ——pump.rs FRAME_VA=0x5100_0000（+16MiB）同款「与假想 first-fit
/// 分配零交叠」防御位；帧 VMO 区自 +16MiB 起 ≤+24MiB 的前提 = 页取整
/// 显示字节 ≤16MiB（w·h ≤ 4,194,304 px——对抗审查 P3 钉板：1088×1920
/// =2.09M 在域，未来大屏超此域时帧区可漫过 +32MiB 撞本位，bake 以
/// 16MiB 卫返 None 降级挡之）；底段 [0x5000_0000,0x5100_0000) 不相交；
/// 页对齐）。
const WALLCACHE_VA: usize = 0x5200_0000;

/// 烘好的全尺寸壁纸面（boot 单线程写毕后进程终身只读——经
/// IconSource 共享引用消费，无再写面）。
struct WallCache {
    w: u32,
    h: u32,
    px: &'static [u32],
}

/// 装饰供件：壁纸 id 换供缓存面，其余 id 原样透传内层（appshell
/// ShellIcons）；`smooth_scale`/`is_empty` 同透传。降级态（cache=
/// None）下三面全等价内层 = 今日行为；缓存态下 desktop 壁纸叶请求
/// 恰 (w,h)=视口 ⇒ paint.rs emit_image `same` 命中 → Argb 直发，
/// smooth_scale 旗不再被咨询（缓存几何 ≠ 请求几何时自动回退
/// SmoothArgb 自缓存缩放——结构性不可达的防御面）。
struct WallpaperCacheSource {
    inner: &'static dyn IconSource,
    cache: Option<WallCache>,
}

impl IconSource for WallpaperCacheSource {
    fn dims(&self, id: u16) -> Option<(u32, u32)> {
        if let (Some(c), true) = (&self.cache, id == crate::icons::WALLPAPER) {
            return Some((c.w, c.h));
        }
        self.inner.dims(id)
    }

    fn pixels(&self, id: u16) -> Option<&[u32]> {
        if let (Some(c), true) = (&self.cache, id == crate::icons::WALLPAPER) {
            return Some(c.px);
        }
        self.inner.pixels(id)
    }

    fn smooth_scale(&self, id: u16) -> bool {
        self.inner.smooth_scale(id)
    }

    fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

/// boot 期装配（main 单次，`ui.set_icon_source` 消费位）：内层供件包
/// 壁纸缓存装饰层并当场烘焙。返 `&'static dyn`（Box::leak 堆置——
/// 供给槽生命周期面，UiMain 同例）。烘焙失败打一行失败诊断后仍返
/// 装饰层——cache=None 即全透传，零行为变化。
pub fn wrap(inner: &'static dyn IconSource, vp_w: u32, vp_h: u32) -> &'static dyn IconSource {
    let cache = bake(inner, vp_w, vp_h);
    if cache.is_none() {
        eprintln!("launcher: wallpaper cache degraded -- per-frame smooth scale");
    }
    Box::leak(Box::new(WallpaperCacheSource { inner, cache }))
}

/// 一次性烘焙：壁纸源（135×240 rodata，经内层供件取）双线性放大到
/// 视口几何的 VMO 映射面。任何一步失败 → None（调用方降级）。
fn bake(inner: &dyn IconSource, vp_w: u32, vp_h: u32) -> Option<WallCache> {
    // 几何先验：TextureDesc 拒面下界（w≥8/h≥16）+ 单 VMO 帽镜像
    //（berry-mmap VMO_MAX=32MiB——libc 侧保守镜像；内核 VMO_MAX_SIZE
    // 自 A1-2 起为 128MiB，镜像漂移本体另开小刀统一）。
    if vp_w < 8 || vp_h < 16 {
        return None;
    }
    let px_count = (vp_w as u64) * (vp_h as u64);
    let byte_len = px_count.checked_mul(4)?;
    if byte_len > VMO_MAX as u64 {
        return None;
    }
    let page_len = ((byte_len + 0xFFF) & !0xFFF) as usize;
    // 源面经内层供件取（dims×pixels 恰长由 submit 期硬门复验）。
    let (sw, sh) = inner.dims(crate::icons::WALLPAPER)?;
    let src = inner.pixels(crate::icons::WALLPAPER)?;
    // 全覆盖先验：smooth 臂 alpha 键 = 四采样加权和 ≥128 才落笔——源含
    // 透明像素则目标可有未写格（页清零底），盲补 alpha 会把它变成不
    // 透明黑，与逐帧路径「跳过不写」语义分叉。此时弃缓存保原路径。
    if src.iter().any(|p| p >> 24 == 0) {
        return None;
    }
    // 帧区让位卫（VA 注的像素域前提执法——超域即降级，防未来大屏帧
    // VMO 漫过 +32MiB 撞本位）。
    if page_len > 16 * 1024 * 1024 {
        return None;
    }
    // 光栅件先构（纯构造零副作用——对抗审查 P2：desc/dl/pass 的 .ok()?
    // 失败口必须位于任何内核对象创建之前，使映射窗内唯一失败口=submit，
    // 该口已有 unmap+close 回滚）。fg/bg 对 ARGB 面不适用（cmd.rs 变体注）。
    let desc = TextureDesc::new(vp_w as usize, vp_h as usize, vp_w as usize * 4, TextureFormat::Rgbx)
        .ok()?;
    let mut dl = DisplayList::new();
    dl.push(Cmd::DrawImage {
        src: ImageSrc::SmoothArgb { sw, sh, dw: vp_w, dh: vp_h, px: src },
        x: 0,
        y: 0,
        fg: Color { r: 0, g: 0, b: 0 },
        bg: None,
    })
    .ok()?;
    let pass = PassDescriptor::new(LoadOp::Load, StoreOp::Store, None); // 无 dirty = 全幅 clip：整格必写
    // VMO 三连（pump 帧 VMO 同形）：create→commit→map @固定防御位。
    let hv = self_vmar_h();
    if hv == 0 {
        return None; // 无自 vmar gift（老 spawn 面）——mmap 系同拒面
    }
    let mut vmo: Handle = 0;
    if berry_vmo_create(page_len as u64, 0, &mut vmo) != 0 || vmo == 0 {
        return None;
    }
    if berry_vmo_commit(vmo, 0, page_len as u64) != 0 {
        let _ = berry_handle_close(vmo);
        return None;
    }
    let mut va = WALLCACHE_VA;
    if berry_vmar_map(hv, 0, vmo, 0, &mut va, page_len, RIGHT_READ | RIGHT_WRITE) != 0
        || va != WALLCACHE_VA
    {
        // 失败形 A（map 拒）= 零映射直接 close；形 B（成功但落位 ≠ 请求
        // 位）= 按已映口径回滚（对抗审查 P3：映射中 VMO 直接 close 撞
        // -EBUSY 拒面——mmap.rs:190 同款先 unmap 后 close）。
        if va != WALLCACHE_VA && va != 0 {
            let _ = berry_vmar_unmap(hv, va, page_len);
        }
        let _ = berry_handle_close(vmo);
        return None;
    }
    // SAFETY: [WALLCACHE_VA, +page_len) 刚由 vmar_map 落位且整段已
    // commit（内核逐页出页清零——mmap.rs 匿名档同语义）；boot 单线程
    // 独占写窗。submit 失败 = 先验后写零笔迹，回滚映射即可。
    let submitted = unsafe {
        let buf: &mut [u8] = core::slice::from_raw_parts_mut(va as *mut u8, page_len);
        Queue::new().submit(&dl, buf, &desc, &pass).is_ok()
    };
    if !submitted {
        let _ = berry_vmar_unmap(hv, va, page_len);
        let _ = berry_handle_close(vmo);
        return None;
    }
    // alpha 补笔：Rgbx pack = r<<16|g<<8|b 无 alpha 位 → Argb 1-bit
    // 键要求 A!=0；全不透明源下 smooth 每格必写（模块头论证），盲 OR
    // 恰等价。px_count ≤ 映射段长（页取整尾零不触）。
    // SAFETY: 同上映射窗，boot 单线程独占。
    unsafe {
        let px: &mut [u32] = core::slice::from_raw_parts_mut(va as *mut u32, px_count as usize);
        for p in px.iter_mut() {
            *p |= 0xFF00_0000;
        }
    }
    // 冻结为进程终身只读面（pump 帧 VMO 同款纪律：永不 unmap/close，
    // 映射与句柄随进程终局回收——close 映射中 VMO 亦会被 -EBUSY 拒）。
    // SAFETY: 指针来自存续整个进程生命的映射；boot 单线程写窗已闭，
    // 此后只读共享。
    let px: &'static [u32] =
        unsafe { core::slice::from_raw_parts(va as *const u32, px_count as usize) };
    Some(WallCache { w: vp_w, h: vp_h, px })
}
