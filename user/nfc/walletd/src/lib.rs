//! berry-walletd——G4 刀 16：系统卡包服务。
//!
//! 交通卡/门禁卡/CCC 3.0 数字车钥匙——断电 5 小时保刷的电量域。
//!
//! 纯面纪律：卡片生命周期/优先级/低电量策略 = 零 I/O 纯函数。
#![cfg_attr(not(test), no_std)]

/// 卡类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardType {
    Transit,    // 交通卡
    Access,     // 门禁卡
    CarKey,     // CCC 3.0 数字车钥匙
    Payment,    // 银行卡（预留）
}

/// 卡状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardState {
    Active,
    Suspended,
    Expired,
}

/// 卡条目。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Card {
    pub id: u32,
    pub card_type: CardType,
    pub state: CardState,
    /// 优先级（高=先出——车钥匙 > 交通 > 门禁 > 支付）。
    pub priority: u8,
    /// 余额（分——交通卡面）。
    pub balance_cents: u32,
}

/// 卡包。
pub struct Wallet {
    cards: [Card; 8],
    count: usize,
    /// 电池电量百分比。
    pub battery_pct: u8,
    /// 断电模式下剩余可用小时。
    pub power_reserve_hours: f32,
}

impl Wallet {
    pub fn new() -> Wallet {
        Wallet { cards: [Card { id: 0, card_type: CardType::Transit, state: CardState::Expired, priority: 0, balance_cents: 0 }; 8], count: 0, battery_pct: 100, power_reserve_hours: 5.0 }
    }

    pub fn add(&mut self, card: Card) -> bool {
        if self.count >= self.cards.len() { return false; }
        self.cards[self.count] = card;
        self.count += 1;
        true
    }

    /// 默认出卡——当前应激活哪张卡。
    /// 车钥匙（priority 最高）> 交通 > 门禁 > 支付。
    pub fn default_card(&self) -> Option<Card> {
        self.cards[..self.count]
            .iter()
            .filter(|c| c.state == CardState::Active)
            .max_by_key(|c| c.priority)
            .copied()
    }

    /// 断电保刷策略——电量 ≤ 5% 时进入 Power Reserve。
    /// 交通卡在 Power Reserve 下仍可刷（余额 ≥ 最低票价）。
    pub fn power_reserve_active(&self) -> bool {
        self.battery_pct <= 5
    }

    /// 断电模式下某卡是否可用。
    pub fn card_usable_in_reserve(&self, card: &Card) -> bool {
        if !self.power_reserve_active() { return true; }
        match card.card_type {
            CardType::Transit => card.balance_cents >= 200, // ≥ ¥2.00
            CardType::CarKey => true, // 车钥匙恒通（安全需求）
            _ => false,
        }
    }

    /// 扣费——交通卡面。
    pub fn deduct(&mut self, card_id: u32, cents: u32) -> bool {
        for c in self.cards[..self.count].iter_mut() {
            if c.id == card_id && c.state == CardState::Active {
                if c.balance_cents >= cents {
                    c.balance_cents -= cents;
                    return true;
                }
                return false;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_priority() {
        let mut w = Wallet::new();
        w.add(Card { id: 1, card_type: CardType::Transit, state: CardState::Active, priority: 20, balance_cents: 5000 });
        w.add(Card { id: 2, card_type: CardType::CarKey, state: CardState::Active, priority: 40, balance_cents: 0 });
        let c = w.default_card().unwrap();
        assert_eq!(c.card_type, CardType::CarKey);
    }

    #[test]
    fn wallet_deduct() {
        let mut w = Wallet::new();
        w.add(Card { id: 1, card_type: CardType::Transit, state: CardState::Active, priority: 20, balance_cents: 1000 });
        assert!(w.deduct(1, 300));
        assert!(!w.deduct(1, 800)); // 余额不足
        assert_eq!(w.cards[0].balance_cents, 700);
    }

    #[test]
    fn power_reserve_transit() {
        let mut w = Wallet::new();
        w.battery_pct = 3; // 断电域
        let transit = Card { id: 1, card_type: CardType::Transit, state: CardState::Active, priority: 20, balance_cents: 500 };
        assert!(w.card_usable_in_reserve(&transit)); // ≥ ¥2
        let transit_low = Card { id: 2, card_type: CardType::Transit, state: CardState::Active, priority: 20, balance_cents: 100 };
        assert!(!w.card_usable_in_reserve(&transit_low)); // < ¥2
    }

    #[test]
    fn power_reserve_car_key_always() {
        let mut w = Wallet::new();
        w.battery_pct = 1;
        let key = Card { id: 1, card_type: CardType::CarKey, state: CardState::Active, priority: 40, balance_cents: 0 };
        assert!(w.card_usable_in_reserve(&key));
    }
}
