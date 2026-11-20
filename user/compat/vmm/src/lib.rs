//! berry-vmm——G2 刀 10：MicroVM 虚拟机管理器。
//!
//! 架构：
//! - **Guest 配置**：IPA（Intermediate Physical Address）空间布局——
//!   RAM 基址/大小、Kernel 加载地址、DTB 地址。
//! - **vCPU 生命周期**：create → run → exit_reason 分派 → 再 run/销毁。
//! - **冷启动**：极简 Linux 内核（无设备树→内存探测→SMP 单核→串口
//!   输出——firecracker 模型）。
//!
//! 纯面纪律：Guest 布局规划/vCPU 状态机/退出原因分型 = 零 I/O 纯函数。
#![cfg_attr(not(test), no_std)]

/// Guest 物理地址空间布局（IPA——Intermediate Physical Address）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuestLayout {
    /// IPA 基址（guest 视角的物理地址起点）。
    pub ipa_base: u64,
    /// IPA 大小（字节）。
    pub ipa_size: u64,
    /// 内核加载偏移（ipa_base 起）。
    pub kernel_offset: u64,
    /// 内核大小上限。
    pub kernel_max: u64,
    /// DTB（设备树）偏移。
    pub dtb_offset: u64,
    /// initrd 偏移。
    pub initrd_offset: u64,
}

impl GuestLayout {
    /// firecracker 极简默认布局。
    pub fn minimal() -> GuestLayout {
        GuestLayout {
            ipa_base: 0x4000_0000,     // 1 GiB——QEMU virt 同基
            ipa_size: 128 * 1024 * 1024, // 128 MiB
            kernel_offset: 0x0_0000,     // kernel at ipa_base
            kernel_max: 16 * 1024 * 1024, // 16 MiB
            dtb_offset: 0x0700_0000,    // DTB at 112MiB（< 128MiB 帽内）
            initrd_offset: 0x0720_0000, // initrd after DTB（< 128MiB 帽内）
        }
    }

    /// kernel IPA 终址。
    pub fn kernel_end(&self) -> u64 {
        self.ipa_base + self.kernel_offset + self.kernel_max
    }

    /// DTB IPA。
    pub fn dtb_addr(&self) -> u64 {
        self.ipa_base + self.dtb_offset
    }

    /// 验证布局——ipa_size 容得下 kernel+dtb+initrd。
    pub fn validate(&self) -> bool {
        self.ipa_size > 0
            && self.dtb_offset + 0x1000 <= self.ipa_size
            && self.initrd_offset + 0x1000 <= self.ipa_size
            && self.kernel_offset < self.ipa_size
    }
}

/// vCPU 退出原因（exit_reason——vcpu_run 返回值语义）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcpuExit {
    /// guest 执行 HLT/WFI——正常暂停。
    Hlt,
    /// guest 访问未映射 IPA——(ipa, write, size)。
    MMIO { ipa: u64, write: bool, size: u32 },
    /// guest 执行 SMCCC/HVC 调用。
    Hypercall { fn_id: u32 },
    /// guest 触发异常（未处理）。
    Exception { esr: u32 },
    /// guest I/O（virtio MMIO 等）。
    Io { port: u64, write: bool },
    /// vCPU 被外部中断打断。
    Irq,
}

impl VcpuExit {
    /// 从原始退出码解析（真机面由 ESR_EL2/VTTBR_FAULT_EL2 填充）。
    pub fn from_raw(raw: u64, ipa: u64, esr: u32) -> VcpuExit {
        match raw {
            0 => VcpuExit::Hlt,
            1 => VcpuExit::MMIO { ipa, write: (raw >> 32) & 1 == 1, size: 4 },
            2 => VcpuExit::Hypercall { fn_id: esr },
            3 => VcpuExit::Exception { esr },
            4 => VcpuExit::Io { port: ipa, write: false },
            5 => VcpuExit::Irq,
            _ => VcpuExit::Exception { esr },
        }
    }

    /// 是否需要 VMM 干预后才能继续运行。
    pub fn needs_handling(&self) -> bool {
        !matches!(self, VcpuExit::Hlt | VcpuExit::Irq)
    }
}

/// vCPU 生命周期状态机。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcpuState {
    /// 已创建未运行。
    Created,
    /// 正在运行（EL2 ERET 到 guest）。
    Running,
    /// 已退出待 VMM 处理。
    Exited(VcpuExit),
    /// 已销毁。
    Destroyed,
}

impl VcpuState {
    /// 状态转移——非法转移返 None。
    pub fn transition(&self, event: VcpuEvent) -> Option<VcpuState> {
        use VcpuState::*;
        match (self, event) {
            (Created, VcpuEvent::Run) => Some(Running),
            (Running, VcpuEvent::Exit(e)) => Some(Exited(e)),
            (Exited(_), VcpuEvent::Run) => Some(Running),
            (Exited(_), VcpuEvent::Destroy) | (Created, VcpuEvent::Destroy) => Some(Destroyed),
            _ => None,
        }
    }
}

/// vCPU 状态转移事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VcpuEvent {
    Run,
    Exit(VcpuExit),
    Destroy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minimal_layout_valid() {
        let l = GuestLayout::minimal();
        assert!(l.validate());
        assert_eq!(l.ipa_base, 0x4000_0000);
        assert!(l.kernel_max <= 16 * 1024 * 1024);
        assert!(l.dtb_addr() > l.ipa_base);
    }

    #[test]
    fn layout_zero_size_invalid() {
        let l = GuestLayout { ipa_size: 0, ..GuestLayout::minimal() };
        assert!(!l.validate());
    }

    #[test]
    fn vcpu_lifecycle() {
        let s = VcpuState::Created;
        let s = s.transition(VcpuEvent::Run).unwrap();
        assert_eq!(s, VcpuState::Running);
        let s = s.transition(VcpuEvent::Exit(VcpuExit::Hlt)).unwrap();
        assert_eq!(s, VcpuState::Exited(VcpuExit::Hlt));
        let s = s.transition(VcpuEvent::Destroy).unwrap();
        assert_eq!(s, VcpuState::Destroyed);
    }

    #[test]
    fn vcpu_invalid_transition() {
        // Created 不能直接 Exit。
        let s = VcpuState::Created;
        assert_eq!(s.transition(VcpuEvent::Exit(VcpuExit::Hlt)), None);
        // Destroyed 不能 Run。
        let d = VcpuState::Destroyed;
        assert_eq!(d.transition(VcpuEvent::Run), None);
    }

    #[test]
    fn exit_reason_dispatch() {
        let e = VcpuExit::MMIO { ipa: 0x4000_1000, write: true, size: 4 };
        assert!(e.needs_handling());
        let h = VcpuExit::Hlt;
        assert!(!h.needs_handling());
    }

    #[test]
    fn exit_from_raw() {
        let e = VcpuExit::from_raw(0, 0, 0);
        assert_eq!(e, VcpuExit::Hlt);
        let e = VcpuExit::from_raw(1, 0x4000_1000, 0);
        assert!(matches!(e, VcpuExit::MMIO { .. }));
    }
}
