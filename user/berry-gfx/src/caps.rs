//! 设备能力面（六件套 ② Caps day-one）：「这个后端能给什么」的查询口。
//! v1 soft 后端定值：blit 支持 / 瞬时附件支持（soft 语义 = 复用调用方
//! CPU buffer，本就零新分配）/ 无 MSAA / Rgbx+Bgrx 双格式 / 纹理单边
//! 上界 = **8192 承诺地板**（WebGPU spec 默认 limits——见
//! [`Caps::max_texture_extent`] 字注）/ 命令表容量 = [`crate::cmd::MAX_CMDS`]
//! （单一事实源引用，防两处漂移）。
//!
//! [`Usage::Transient`]（六件套 ⑤）在此占位：transient 显式声明——
//! soft 路径它只是意图标记，真后端（tile memory 类）才是性能提示。

/// 多重采样档位（WebGPU sampleCount 口径的档位子集）。v1 soft 无 MSAA
/// （`None` = 1x）；真后端按硬件面扩变体（4x/8x），扩档不改 `None` 语义。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Msaa {
    /// 无多重采样（1x）。
    None,
}

/// 纹理格式能力集（Caps 的格式查询位——「这个后端吃哪些格式」）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatCaps {
    /// Rgbx（GOP 0）支持。
    pub rgbx: bool,
    /// Bgrx（GOP 1）支持。
    pub bgrx: bool,
}

impl FormatCaps {
    /// 格式支持谓词。
    pub fn supports(&self, f: crate::color::TextureFormat) -> bool {
        match f {
            crate::color::TextureFormat::Rgbx => self.rgbx,
            crate::color::TextureFormat::Bgrx => self.bgrx,
        }
    }
}

/// 资源使用标记（占位枚举——六件套 ⑤ transient 显式声明）。v1 仅
/// [`Usage::Transient`]：瞬时附件 = 单 pass 内即弃的中间面。
/// **soft 语义 = 复用调用方 CPU buffer**（零新分配，标记仅作意图声明，
/// 回放行为不变）；真后端语义 = tile memory 类后端可抛弃/复用的附件
/// 提示。更多档位（GPU 常驻/拷贝源等）随真后端入册。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Usage {
    /// 瞬时附件：生命周期 = 单 pass。
    Transient,
}

/// 后端能力快照（WebGPU `Device::limits`/特征查询的 v1 子集形态）。
/// 字段全 pub（纯数据查询面，无不变量需要钉）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Caps {
    /// 纹理内区域拷贝（blit）支持。
    pub blit: bool,
    /// 瞬时附件支持（见 [`Usage`]）。soft 恒 true 且**平凡成立**——
    /// 纹理本是调用方供 CPU buffer，无独占 tile memory 可弃，不存在
    /// 「不支持 transient」的 soft 形态。此位在 soft 的执法面 = 定值
    /// 钉板防静默漂移（caps_misc 组），**真后端（virtio-gpu / tile
    /// memory 类）接入时才变为实测位**（false = 后端拒绝 transient
    /// 附件——届时才有可证伪面；审查 L4）。
    pub transient_attachments: bool,
    /// MSAA 档位（v1 = `Msaa::None`）。
    pub msaa: Msaa,
    /// 支持的纹理格式集。
    pub texture_formats: FormatCaps,
    /// 纹理单边像素上界（w/h 同界）。**报承诺不报余量**（GUI-10 增件⑤）：
    /// 值 = 8192 —— WebGPU spec 默认 limits `maxTextureDimension2D` 的
    /// 共同地板（Dawn 最低特性集同款），任何真后端（virtio-gpu 类）都
    /// 必须兑现的下界承诺；soft 的**实际**几何界是光栅核 i32 坐标域
    /// （`color::DescError::TooLarge` 构造期执法），比承诺宽得多——那
    /// 是余量不是承诺，消费侧按 ≤ 8192 编程才跨后端可移植。真后端此处
    /// 填 `min(硬件/驱动实际限制, 共同地板)`。
    pub max_texture_extent: u32,
    /// 单显示列表命令容量上界（= [`crate::cmd::MAX_CMDS`]）。
    pub max_cmds: usize,
}

/// 逻辑设备（六件套的入口句柄）。v1 soft 无真设备语义——`Device` 是
/// API 形态锚点：能力查询 + 队列获取；真后端（virtio-gpu）时此处持
/// 设备会话，接口面不变。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Device;

impl Device {
    /// 构造（v1 无失败面；真后端此处可返 Err——接口留位）。
    pub fn new() -> Device {
        Device
    }

    /// 能力查询：soft v1 定值快照。
    pub fn caps(&self) -> Caps {
        Caps {
            blit: true,
            transient_attachments: true,
            msaa: Msaa::None,
            texture_formats: FormatCaps { rgbx: true, bgrx: true },
            max_texture_extent: 8192,
            max_cmds: crate::cmd::MAX_CMDS,
        }
    }

    /// 默认队列（v1 soft 单队列零内态；真后端双队列/优先级档扩位）。
    pub fn default_queue(&mut self) -> crate::queue::Queue {
        crate::queue::Queue::new()
    }
}

impl Default for Device {
    fn default() -> Self {
        Device::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::TextureFormat;

    // —— soft v1 能力定值钉板（防静默漂移：每字段字面断言）——
    #[test]
    fn soft_caps_pinned() {
        let c = Device::new().caps();
        assert!(c.blit);
        assert!(c.transient_attachments);
        assert_eq!(c.msaa, Msaa::None);
        assert_eq!(c.texture_formats, FormatCaps { rgbx: true, bgrx: true });
        // max_cmds 与命令表容量单一事实源同值（漂移 = 容量谎报）。
        assert_eq!(c.max_cmds, crate::cmd::MAX_CMDS);
        assert!(c.max_cmds > 0);
        // max_texture_extent = 8192 承诺地板（WebGPU 默认 limits——报
        // 承诺不报余量，字注详；漂移 = 承诺谎报）。
        assert_eq!(c.max_texture_extent, 8192);
        // 格式查询谓词双面。
        assert!(c.texture_formats.supports(TextureFormat::Rgbx));
        assert!(c.texture_formats.supports(TextureFormat::Bgrx));
    }

    // —— Usage/Msaa 占位枚举可判别可比较（形态面编译期锚）——
    #[test]
    fn marker_enums_inhabited() {
        assert_eq!(Usage::Transient, Usage::Transient);
        assert_eq!(Usage::Transient as u32, 0, "唯一变体判别值 0");
        assert_eq!(Msaa::None, Msaa::None);
    }
}
