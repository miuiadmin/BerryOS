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
//! fill_rect / line / blit（纹理内区域拷贝）/ glyph-run / present——
//! fbcon 六原语超集 + 文本位；AA/宽色域/compute/3D 可后补。像素语义
//! 与 berry-fbcon `render.rs` 共识契约逐位同（u32 小端 `to_le_bytes`
//! 落盘 / glyph MSB=最左列 / stride 行尾 padding 永不写），glyph 路径
//! host 测试对 fbcon 蜂格路径逐字节对拍（两套独立代码、像素必同）。
//!
//! # 内存纪律
//!
//! 全库零 alloc no_std：纹理 = 调用方供 `&mut [u8]` 借用（VMO-backed
//! 留位段②+），命令表 = 定容数组（满 → loud `Err`，绝不静默丢）。
//! 拒面全 loud：非法参数/容量满/缓冲不足/坏矩形一律 `Err`（自定义
//! 错误类型），绝不 panic、绝不静默改语义；坐标乘加全 checked 或
//! 先验界内再算（敌意巨参不炸）。
//!
//! # 段②留位清单（本刀不做）
//!
//! genfbfont 多字号（[`font::FontSize`] 占位）、图标格式 + LRU 图集、
//! S2' icount 定标、berry-guiproto（submit/damage/frame_callback/vote
//! 归 proto 层——定形锚点留给真正的首个消费者）、guest gfxdemo +
//! smoke 标记、双后端（soft ↔ virtio-gpu 共识）。
#![cfg_attr(not(test), no_std)]

pub mod caps;
pub mod color;
pub mod cmd;
pub mod font;
pub mod pass;
pub mod queue;
mod raster;
