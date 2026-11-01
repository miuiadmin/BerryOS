//! berry-gfx：pass 形态图形 API + soft 光栅核（GUI-2 段①——
//! `docs/GUI里程碑规划-2026-10.md` GUI-2 行 + ADR #51；概念/命名 1:1
//! 对齐 WebGPU，语义按本仓几何铁律收敛）。
//!
//! # 六件套 day-one 契约（GUI-10 附录 C §5——逐条在场）
//!
//! 1. **pass = 唯一同步/合成边界**：loadOp / storeOp / clear 色 /
//!    dirty rect（[`pass`]）——**dirty 区外 = 别人的画面，结构性不可
//!    触碰**（fbcon sliver/padding 铁律的推广）；
//! 2. **Caps**：后端能力查询（[`caps`]）——soft v1 定值；
//! 3. **着色器 = 构建产物**：API 无运行时编译入口（[`cmd::PaintDesc`]
//!    头注钉死纪律——不给 JIT 留门），v1 = 纯色；
//! 4. **记录/回放分离**：display list（[`cmd::DisplayList`]) 记录、
//!    [`queue::Queue::submit`] 回放——同一列表可对多纹理重放；
//! 5. **transient 显式声明**：[`caps::Usage::Transient`] 标记位——
//!    soft 语义 = 复用调用方 CPU buffer（零新分配）；
//! 6. **文字三段解耦**：shape（v1 = codepoint 直映射点阵索引）/
//!    图集（[`font::GlyphAtlas`]）/ pass 内绘制（[`cmd::Cmd::DrawGlyphRun`]）。
//!
//! # v1 操作集边界（规划钦定最小面，防过度设计）
//!
//! fill_rect / line / blit（纹理内区域拷贝）/ glyph-run / draw_image
//! （段②：单色位图图像/图标——与字形同一位法）/ present——fbcon 六
//! 原语超集 + 文本/图像位；AA/宽色域/compute/3D 可后补。像素语义
//! 与 berry-fbcon `render.rs` 共识契约逐位同（u32 小端 `to_le_bytes`
//! 落盘 / glyph MSB=最左列 / stride 行尾 padding 永不写），glyph 路径
//! host 测试对 fbcon 蜂格路径逐字节对拍（两套独立代码、像素必同）。
//!
//! # 内存纪律
//!
//! 全库零 alloc no_std：纹理 = 调用方供 `&mut [u8]` 借用（VMO-backed
//! 纹理留位后续刀——缓冲的 VMO 借用面随真后端/首个 guest 消费者定
//! 形），命令表 = 定容数组（满 → loud `Err`，绝不静默丢）。
//! 拒面全 loud：非法参数/容量满/缓冲不足/坏矩形/图像位畸形一律 `Err`
//! （自定义错误类型），绝不 panic、绝不静默改语义；坐标乘加全 checked
//! 或先验界内再算（敌意巨参不炸）。LRU（[`lru`]）= `Cell` 内部可变性
//! 单线程语境（`&dyn` 图集查询碰热——见模块头）。
//!
//! # 段② 已落 / 留位清单
//!
//! 段② 本刀已落：genfbfont 三档多字号（[`font`]——1x/2x/3x 静态图集 +
//! NN 放大共识测试）、图标格式 + ARGB 解码 + LRU 图集（[`icon`] BYI1
//! `decode` 数据面 / [`icon`]/[`lru`] 图集面）、`draw_image` 命令面
//! （[`cmd::ImageSrc`]——mono 位图/图标槽位 + **ARGB 彩色面**：解码
//! 产物绘制消费口 [`cmd::ImageSrc::Argb`]，A 位 1-bit 透明键，无
//! blend）。仍留位（本刀不做）：S2' icount 定标、
//! berry-guiproto（submit/damage/frame_callback/vote 归 proto 层——
//! 定形锚点留给真正的首个消费者）、guest gfxdemo + smoke 标记、双后端
//! （soft ↔ virtio-gpu 共识）、**VMO-backed 纹理**（纹理缓冲的 VMO 借
//! 用面——guest 侧 fb/VMO 映射的接驳口，随真后端/首个消费者定形）、
//! 半透明真 blend（ARGB 面 A 位目前只做 1-bit 透明键——`A==0` 跳过 /
//! `A!=0` 不透明覆盖，混合语义随真后端定形）。
#![cfg_attr(not(test), no_std)]

pub mod caps;
pub mod color;
pub mod cmd;
pub mod font;
pub mod icon;
pub mod lru;
pub mod pass;
pub mod queue;
mod raster;
