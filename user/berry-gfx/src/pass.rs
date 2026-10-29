//! pass 描述（六件套 ① pass = 唯一同步/合成边界 day-one）：
//! loadOp / storeOp / clear 色 / dirty rect 四事实。
//!
//! soft 后端把 pass 当**屏障照跑**（语义不特化）：submit 先执行
//! load_op，再回放命令，全程裁剪到 dirty 区——**dirty 区外 = 别人的
//! 画面，结构性不可触碰**（fbcon sliver/padding 铁律的推广形态）。

/// pass 起始载荷。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadOp {
    /// 清成纯色（回放前铺 dirty ∩ 纹理界；无 dirty = 整帧）。
    Clear { r: u8, g: u8, b: u8 },
    /// 保留现内容（不动——增量绘制的常态）。
    Load,
}

/// pass 结束存储。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreOp {
    /// 保留 pass 结果（soft：内容本就在目标缓冲里，恒等语义）。
    Store,
    /// 丢弃 pass 结果。**真后端语义 = pass 后纹理内容未定义**（tile
    /// memory 类后端可不写回）；**soft 后端 = no-op**——内容已在调用
    /// 方缓冲里，「丢弃」无从执法。调用方自行按未定义理解，勿依赖
    /// soft 后端上的残存内容。
    Discard,
}

/// pass 描述：一个 submit 的载荷/存储/dirty 三事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PassDescriptor {
    pub load_op: LoadOp,
    pub store_op: StoreOp,
    /// 脏区约束：`Some(r)` = 本 pass 的**写**只落在该区内（区外像素
    /// 一律不写——「区外 = 别人的画面」）；`None` = 整帧。**读侧**
    /// （如 blit 源取样）只受纹理界约束、不受 dirty 限——dirty 是写侧
    /// 纪律，读写两面分立（审查 L2：原注「区内读写」对读侧过度宣称）。
    /// r 负宽/高非法（submit 期 loud 拒）；r 与纹理界交叠为空 = 本
    /// pass 全 no-op（合法——空脏区不是错误）。
    pub dirty: Option<crate::cmd::Rect>,
}

impl PassDescriptor {
    /// 全事实构造。
    pub fn new(load_op: LoadOp, store_op: StoreOp, dirty: Option<crate::cmd::Rect>) -> PassDescriptor {
        PassDescriptor { load_op, store_op, dirty }
    }

    /// 便捷整帧清屏 pass（Clear + Store，无 dirty）。
    pub fn clear(r: u8, g: u8, b: u8) -> PassDescriptor {
        PassDescriptor::new(LoadOp::Clear { r, g, b }, StoreOp::Store, None)
    }

    /// 便捷增量 pass（Load + Store，无 dirty——dirty 按需用 `new` 组装）。
    pub fn load() -> PassDescriptor {
        PassDescriptor::new(LoadOp::Load, StoreOp::Store, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::Rect;

    // —— 构造便捷面与全事实构造等价 ——
    #[test]
    fn descriptor_constructors() {
        let p = PassDescriptor::clear(1, 2, 3);
        assert_eq!(p.load_op, LoadOp::Clear { r: 1, g: 2, b: 3 });
        assert_eq!(p.store_op, StoreOp::Store);
        assert_eq!(p.dirty, None);
        let p = PassDescriptor::load();
        assert_eq!(p.load_op, LoadOp::Load);
        let p = PassDescriptor::new(LoadOp::Load, StoreOp::Discard, Some(Rect::new(0, 0, 8, 8)));
        assert_eq!(p.dirty, Some(Rect::new(0, 0, 8, 8)));
        assert_eq!(p.store_op, StoreOp::Discard);
        // Copy 语义钉板（submit 按借用取用后原描述不搬走）。
        let q = p;
        let _ = p;
        let _ = q;
    }
}
