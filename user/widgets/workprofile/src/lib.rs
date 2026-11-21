//! berry-workprofile——G5 刀 21：企业工作域与 DLP 剪贴板隔离。
//!
//! UID 2000 独立企业空间+CE 凭据密钥隔离+剪贴板 DLP 策略引擎。
//!
//! 纯面纪律：域归属判定/剪贴板策略/数据分类 = 零 I/O 纯函数。
#![cfg_attr(not(test), no_std)]

/// 用户域。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    Personal, // UID 1000+
    Work,     // UID 2000+
}

/// 数据分级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataClass {
    Public,       // 公开——自由流转
    Internal,     // 内部——域内可流转
    Confidential, // 机密——不可出工作域
}

/// 剪贴板数据条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClipboardEntry {
    pub source_domain: Domain,
    pub source_app: u32,
    pub classification: DataClass,
}

/// DLP 策略决策。
pub struct DlpPolicy {
    /// 工作域应用是否可读个人剪贴板。
    pub work_reads_personal: bool,
    /// 个人域应用是否可读工作剪贴板。
    pub personal_reads_work: bool,
}

impl DlpPolicy {
    pub fn strict() -> DlpPolicy {
        DlpPolicy { work_reads_personal: false, personal_reads_work: false }
    }

    pub fn moderate() -> DlpPolicy {
        DlpPolicy { work_reads_personal: true, personal_reads_work: false }
    }

    /// 剪贴板跨域读取是否放行。
    pub fn allow_clipboard(&self, entry: &ClipboardEntry, target_domain: Domain) -> bool {
        if entry.source_domain == target_domain {
            return true; // 同域恒通
        }
        match (entry.source_domain, target_domain) {
            (Domain::Personal, Domain::Work) => self.work_reads_personal,
            (Domain::Work, Domain::Personal) => {
                entry.classification == DataClass::Public && self.personal_reads_work
            }
            _ => true,
        }
    }
}

/// 域管理器——应用归属+UID 映射。
pub struct DomainManager {
    /// 工作域应用列表（定容 16）。
    work_apps: [u32; 16],
    work_count: usize,
    /// 企业域 UID 基址。
    pub work_uid_base: u32,
}

impl DomainManager {
    pub fn new() -> DomainManager {
        DomainManager { work_apps: [0; 16], work_count: 0, work_uid_base: 2000 }
    }

    /// 注册应用到工作域。
    pub fn register_work_app(&mut self, app_id: u32) -> bool {
        if self.work_count >= self.work_apps.len() {
            return false;
        }
        if !self.work_apps[..self.work_count].contains(&app_id) {
            self.work_apps[self.work_count] = app_id;
            self.work_count += 1;
        }
        true
    }

    /// 判定应用所属域。
    pub fn domain_of(&self, app_id: u32) -> Domain {
        if self.work_apps[..self.work_count].contains(&app_id) {
            Domain::Work
        } else {
            Domain::Personal
        }
    }

    /// 判定 UID 所属域（UID ≥ 2000 = 工作域）。
    pub fn domain_of_uid(&self, uid: u32) -> Domain {
        if uid >= self.work_uid_base {
            Domain::Work
        } else {
            Domain::Personal
        }
    }

    /// CE 凭据密钥域隔离——不同域用不同密钥槽。
    pub fn ce_key_slot(&self, uid: u32) -> u8 {
        match self.domain_of_uid(uid) {
            Domain::Personal => 0,
            Domain::Work => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn domain_registration() {
        let mut dm = DomainManager::new();
        dm.register_work_app(100);
        dm.register_work_app(200);
        assert_eq!(dm.domain_of(100), Domain::Work);
        assert_eq!(dm.domain_of(999), Domain::Personal);
    }

    #[test]
    fn uid_domain() {
        let dm = DomainManager::new();
        assert_eq!(dm.domain_of_uid(1000), Domain::Personal);
        assert_eq!(dm.domain_of_uid(2000), Domain::Work);
        assert_eq!(dm.domain_of_uid(2500), Domain::Work);
    }

    #[test]
    fn ce_key_isolation() {
        let dm = DomainManager::new();
        assert_eq!(dm.ce_key_slot(1000), 0);
        assert_eq!(dm.ce_key_slot(2000), 1);
    }

    #[test]
    fn dlp_strict_blocks_cross() {
        let p = DlpPolicy::strict();
        let work_entry = ClipboardEntry { source_domain: Domain::Work, source_app: 100, classification: DataClass::Confidential };
        assert!(!p.allow_clipboard(&work_entry, Domain::Personal));
        let personal_entry = ClipboardEntry { source_domain: Domain::Personal, source_app: 999, classification: DataClass::Public };
        assert!(!p.allow_clipboard(&personal_entry, Domain::Work));
    }

    #[test]
    fn dlp_same_domain_ok() {
        let p = DlpPolicy::strict();
        let entry = ClipboardEntry { source_domain: Domain::Work, source_app: 100, classification: DataClass::Confidential };
        assert!(p.allow_clipboard(&entry, Domain::Work));
    }

    #[test]
    fn dlp_moderate_work_reads_personal() {
        let p = DlpPolicy::moderate();
        let personal = ClipboardEntry { source_domain: Domain::Personal, source_app: 999, classification: DataClass::Public };
        assert!(p.allow_clipboard(&personal, Domain::Work));
        let work = ClipboardEntry { source_domain: Domain::Work, source_app: 100, classification: DataClass::Internal };
        assert!(!p.allow_clipboard(&work, Domain::Personal));
    }
}
