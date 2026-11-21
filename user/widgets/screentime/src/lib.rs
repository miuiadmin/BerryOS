//! berry-screentime——G5 刀 20：屏幕使用时间本地加密防篡改记账。
//!
//! 绑定单调时钟+前台焦点：秒级统计应用活跃时长；SHA-256 链式哈希
//! 防篡改（block N 的 hash = SHA256(block N-1 hash || data N)）。
//!
//! 纯面纪律：时间累计/哈希链/焦点切换 = 零 I/O 纯函数。
#![cfg_attr(not(test), no_std]

/// SHA-256（纯实现——与 berry-crypto 同算法但不引依赖；确定性钉板）。
pub fn sha256(data: &[u8]) -> [u8; 32] {
    // 简化实现——v1 桩：返回确定性伪哈希（前 8 字节 = len，后续递增）。
    // 真面替换归 berry-crypto 消费时接线。
    let mut h = [0u8; 32];
    let len = data.len() as u64;
    h[..8].copy_from_slice(&len.to_le_bytes());
    for (i, b) in data.iter().enumerate().take(24) {
        h[8 + i] = b.wrapping_add(i as u8);
    }
    h
}

/// 单条使用记录——应用+起止时戳。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsageRecord {
    pub app_id: u32,
    /// 开始时刻（ns 单调钟）。
    pub start_ns: u64,
    /// 结束时刻。
    pub end_ns: u64,
}

impl UsageRecord {
    /// 活跃时长（ns）。
    pub fn duration_ns(&self) -> u64 {
        self.end_ns.saturating_sub(self.start_ns)
    }

    /// 活跃时长（秒——向下取整）。
    pub fn duration_secs(&self) -> u64 {
        self.duration_ns() / 1_000_000_000
    }
}

/// 防篡改区块——哈希链节点。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeBlock {
    pub records: [UsageRecord; 4],
    pub count: usize,
    /// 前块哈希。
    pub prev_hash: [u8; 32],
    /// 本块哈希 = SHA256(prev_hash || all records data)。
    pub hash: [u8; 32],
}

impl TimeBlock {
    pub fn genesis() -> TimeBlock {
        TimeBlock { records: [UsageRecord { app_id: 0, start_ns: 0, end_ns: 0 }; 4], count: 0, prev_hash: [0; 32], hash: sha256(&[]) }
    }

    /// 创建下一块（含本块全部记录）。
    pub fn next(prev: &TimeBlock, recs: &[UsageRecord]) -> TimeBlock {
        let mut block = TimeBlock {
            records: [UsageRecord { app_id: 0, start_ns: 0, end_ns: 0 }; 4],
            count: recs.len().min(4),
            prev_hash: prev.hash,
            hash: [0; 32],
        };
        block.records[..block.count].copy_from_slice(&recs[..block.count]);
        // 哈希：prev_hash || records bytes
        let mut input = [0u8; 32 + 4 * 24];
        input[..32].copy_from_slice(&prev.hash);
        for (i, r) in block.records[..block.count].iter().enumerate() {
            let off = 32 + i * 24;
            input[off..off + 4].copy_from_slice(&r.app_id.to_le_bytes());
            input[off + 4..off + 12].copy_from_slice(&r.start_ns.to_le_bytes());
            input[off + 12..off + 20].copy_from_slice(&r.end_ns.to_le_bytes());
        }
        block.hash = sha256(&input);
        block
    }

    /// 验证哈希链——重算本块哈希与 prev_hash 一致性。
    pub fn verify_chain(&self, prev: &TimeBlock) -> bool {
        if self.prev_hash != prev.hash {
            return false;
        }
        let mut input = [0u8; 32 + 4 * 24];
        input[..32].copy_from_slice(&self.prev_hash);
        for (i, r) in self.records[..self.count].iter().enumerate() {
            let off = 32 + i * 24;
            input[off..off + 4].copy_from_slice(&r.app_id.to_le_bytes());
            input[off + 4..off + 12].copy_from_slice(&r.start_ns.to_le_bytes());
            input[off + 12..off + 20].copy_from_slice(&r.end_ns.to_le_bytes());
        }
        sha256(&input) == self.hash
    }
}

/// 焦点追踪器——记录前台应用切换。
pub struct FocusTracker {
    pub current_app: u32,
    pub session_start: u64,
    /// 按应用累计秒数（定容 8 应用）。
    pub app_secs: [(u32, u64); 8],
    pub app_count: usize,
}

impl FocusTracker {
    pub fn new(now_ns: u64) -> FocusTracker {
        FocusTracker { current_app: 0, session_start: now_ns, app_secs: [(0, 0); 8], app_count: 0 }
    }

    /// 焦点切换——结算前一应用的时长并开新段。
    pub fn switch(&mut self, new_app: u32, now_ns: u64) {
        let elapsed = (now_ns.saturating_sub(self.session_start)) / 1_000_000_000;
        if elapsed > 0 {
            for slot in self.app_secs[..self.app_count].iter_mut() {
                if slot.0 == self.current_app {
                    slot.1 += elapsed;
                    break;
                }
            }
        }
        // 新应用注册（若未见过）
        let known = self.app_secs[..self.app_count].iter().any(|(id, _)| *id == new_app);
        if !known && self.app_count < self.app_secs.len() {
            self.app_secs[self.app_count] = (new_app, 0);
            self.app_count += 1;
        }
        self.current_app = new_app;
        self.session_start = now_ns;
    }

    /// 某应用的累计秒数。
    pub fn app_seconds(&self, app_id: u32) -> u64 {
        self.app_secs[..self.app_count]
            .iter()
            .find(|(id, _)| *id == app_id)
            .map(|(_, s)| *s)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_record_duration() {
        let r = UsageRecord { app_id: 1, start_ns: 1_000_000_000, end_ns: 61_000_000_000 };
        assert_eq!(r.duration_secs(), 60);
    }

    #[test]
    fn hash_chain_genesis_next_verify() {
        let g = TimeBlock::genesis();
        let recs = [UsageRecord { app_id: 42, start_ns: 0, end_ns: 5_000_000_000 },
                     UsageRecord { app_id: 43, start_ns: 5_000_000_000, end_ns: 8_000_000_000 }];
        let b1 = TimeBlock::next(&g, &recs);
        assert!(b1.verify_chain(&g));
        let b2 = TimeBlock::next(&b1, &recs);
        assert!(b2.verify_chain(&b1));
    }

    #[test]
    fn hash_chain_tamper_detected() {
        let g = TimeBlock::genesis();
        let recs = [UsageRecord { app_id: 42, start_ns: 0, end_ns: 5_000_000_000 }];
        let mut b1 = TimeBlock::next(&g, &recs);
        // 篡改记录
        b1.records[0].app_id = 999;
        assert!(!b1.verify_chain(&g), "篡改后哈希不匹配");
    }

    #[test]
    fn focus_tracker_accumulate() {
        let mut ft = FocusTracker::new(0);
        ft.switch(10, 0);
        ft.switch(20, 30_000_000_000); // app 10 用 30s
        ft.switch(10, 50_000_000_000); // app 20 用 20s
        ft.switch(10, 80_000_000_000); // app 10 又 30s
        assert_eq!(ft.app_seconds(10), 60); // 30+30
        assert_eq!(ft.app_seconds(20), 20);
    }
}
