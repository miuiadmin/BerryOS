//! SEM-A1：runtime-only semantic overlay 与 P0 记录投影。
//!
//! 语义树是保留树/a11y/C1/文本侧表的导出面，不是第二焦点、命中或几何
//! 权威。为避免普通 app 栈预算膨胀，本表只存 a11y 薄面没有的 overlay
//! 字段；role/label 仍以 legacy a11y 为单源。

use crate::desc::{NodeId, TextBuf};
use crate::text_input::TextSelection;
use berry_guiproto::surface::{
    A11Y_ACTION_ACTIVATE, A11Y_ACTION_ADJUST, A11Y_ACTION_FOCUS, A11Y_ACTION_KNOWN,
    A11Y_ACTION_TEXT_EDIT, A11Y_LABEL_MAX, A11Y_STATE_CHECKED, A11Y_STATE_DISABLED, A11yRegion,
};

/// 语义 overlay 容量。每页交互控件 v1 帽 8；UITEST page0 实际 5 项
/// （Button/Switch/TextField/ListDetail 双 ListItem）。满表 NodeFull loud。
pub const SEM_MAX: usize = 8;

/// P0 导出 label 帽（32B）。BerryUI AST a11y 薄面仍允许 64B，投影时
/// 超过本帽 loud 拒，不截断。
pub const SEM_LABEL_MAX: usize = A11Y_LABEL_MAX;

pub const SEM_ACTION_FOCUS: u8 = A11Y_ACTION_FOCUS;
pub const SEM_ACTION_ACTIVATE: u8 = A11Y_ACTION_ACTIVATE;
pub const SEM_ACTION_ADJUST: u8 = A11Y_ACTION_ADJUST;
pub const SEM_ACTION_TEXT_EDIT: u8 = A11Y_ACTION_TEXT_EDIT;
pub const SEM_STATE_DISABLED: u8 = A11Y_STATE_DISABLED;
pub const SEM_STATE_CHECKED: u8 = A11Y_STATE_CHECKED;

/// runtime-only 语义 overlay。必须挂在已登记 legacy a11y 的节点上；
/// role/label 不重复存储。FOCUS action / VISIBLE / FOCUSED 全部派生。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SemEntry {
    pub node: NodeId,
    /// 声明动作。FOCUS 位由 C1 focus 表派生，写入时拒绝。
    pub actions: u8,
    /// 静态 Disabled/Checked；Visible/Focused 由快照派生。
    pub state: u8,
    /// 语义 value / text range 暂不进 P0 52B wire，仅供 host/app 消费。
    pub value: TextBuf,
    pub text_range: TextSelection,
    /// 官方 accessibilityGroup 形态：快照跳过后代语义。
    pub merge_descendants: bool,
}

impl SemEntry {
    pub fn new(
        node: NodeId,
        actions: u8,
        state: u8,
        value: &[u8],
        anchor_cp: u16,
        caret_cp: u16,
        merge_descendants: bool,
    ) -> Result<Self, crate::UiError> {
        if actions & !A11Y_ACTION_KNOWN != 0
            || actions & SEM_ACTION_FOCUS != 0
            || state & !(SEM_STATE_DISABLED | SEM_STATE_CHECKED) != 0
            || anchor_cp > caret_cp
            || !crate::text_input::is_valid_utf8(value)
        {
            return Err(crate::UiError::NodeBad);
        }
        let value = TextBuf::new(value)?;
        if caret_cp > crate::utf8::count(value.as_bytes()) as u16 {
            return Err(crate::UiError::NodeBad);
        }
        let entry = Self {
            node,
            actions,
            state,
            value,
            text_range: TextSelection { anchor_cp, caret_cp },
            merge_descendants,
        };
        entry.validate()?;
        Ok(entry)
    }

    pub fn text_range(&self) -> TextSelection {
        self.text_range
    }

    pub(crate) fn validate(&self) -> Result<(), crate::UiError> {
        let value_len = self.value.len as usize;
        if self.actions & !A11Y_ACTION_KNOWN != 0
            || self.actions & SEM_ACTION_FOCUS != 0
            || self.state & !(SEM_STATE_DISABLED | SEM_STATE_CHECKED) != 0
            || value_len > crate::TEXT_INLINE
            || self.value.bytes[value_len..].iter().any(|&byte| byte != 0)
            || !crate::text_input::is_valid_utf8(self.value.as_bytes())
            || self.text_range.anchor_cp > self.text_range.caret_cp
            || self.text_range.caret_cp > crate::utf8::count(self.value.as_bytes()) as u16
        {
            return Err(crate::UiError::NodeBad);
        }
        Ok(())
    }
}

/// P0 52B 语义记录 round-trip 谓词：编码再解析必须逐字段相等。
pub fn a11y_region_roundtrip(region: &A11yRegion) -> bool {
    let mut wire = [0u8; berry_guiproto::surface::A11Y_REGION_LEN];
    match berry_guiproto::surface::encode_a11y_region(&mut wire, region) {
        Some(len) if len == berry_guiproto::surface::A11Y_REGION_LEN => {
            berry_guiproto::surface::parse_a11y_region(&wire).as_ref() == Some(region)
        }
        _ => false,
    }
}

const _: () = assert!(SEM_LABEL_MAX == 32);
