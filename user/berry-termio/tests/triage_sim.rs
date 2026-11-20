//! triage 事件流仿真（P4 ⑦——pipe_sim 先例同款纪律）：berry-term 帧循环
//! 键路／排空步／忙段提示的 host 侧钉板。以 ChanSim（深 8 有界通道——
//! CHAN_DEPTH=8 镜像）＋不读 stdin 的忙子（bsh 内建 sleep 形态——病灶一
//! 现场：单线程 usleep 步进环零读）构造事件流，验证：满队零死判（EBUSY
//! 从死判面除名——G5）、键字节跨段（通道＋本地缓冲）全程 FIFO（G6）、
//! 忙段提示恰一次、排空后续键直达、损失对账（全达或 dropped 计数平衡）。
//! **流是假定解剖**（运行期真值由 berry-term e2e 场景甲/乙在 QEMU 执法
//! ——刀谱 D3 存证）；接线语义改动时本测试与 main.rs 排空步同批维护。

use berry_termio::triage::{Backlog, KeyAct, SendFeed, CHILD_BUSY_HINT_NS};

/// 有界通道仿真（CHAN_DEPTH=8 镜像——满即 EBUSY 形）。
struct ChanSim {
    queued: Vec<u8>,
    depth: usize,
}

impl ChanSim {
    fn new() -> ChanSim {
        ChanSim { queued: Vec::new(), depth: 8 }
    }

    /// 单发非阻塞（child_probe 镜像——三态）。
    fn send(&mut self, b: u8) -> SendFeed {
        if self.queued.len() < self.depth {
            self.queued.push(b);
            SendFeed::Sent
        } else {
            SendFeed::Full
        }
    }

    /// 子读全排空（bsh 醒来回读——recv 至 EAGAIN）。
    fn child_read_all(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.queued)
    }
}

/// 键路单步（main M2 接线镜像）：pending 非空**不探测**直接入缓冲（G6
/// ——feed 语义上等价 Full）；pending 空才探测，LinkErr → 死分诊（Err）。
fn admit(chan: &mut ChanSim, bl: &mut Backlog, b: u8, now: u64) -> Result<KeyAct, ()> {
    if bl.pending_len() != 0 {
        Ok(bl.on_key(SendFeed::Full, b, now))
    } else {
        match chan.send(b) {
            // 仿真通道不可达 LinkErr（v1 近不可达面——kernel channel 对端
            // 关闭检测裁剪）；死分诊入口由 DeathEntry 表测执法（lib ⑥）。
            SendFeed::LinkErr => Err(()),
            feed => Ok(bl.on_key(feed, b, now)),
        }
    }
}

/// 排空步（帧循环顶 M2 接线镜像——满队期唯一发送者）：连发至 Full／空；
/// episode 回报（排空毕 dropped 总数）入 reports，死判计数返回。
fn drain_step(chan: &mut ChanSim, bl: &mut Backlog, reports: &mut Vec<usize>) -> usize {
    let mut deaths = 0usize;
    while let Some(b) = bl.next_pending() {
        match chan.send(b) {
            SendFeed::Sent => {
                if let Some(d) = bl.on_sent() {
                    reports.push(d);
                }
            }
            SendFeed::Full => break,        // 满队留队头下帧再试
            SendFeed::LinkErr => {
                deaths += 1; // 死分诊后 break（键留队待陪葬报账）
                break;
            }
        }
    }
    deaths
}

/// ⑦ 忙段事件流主场景（e2e 场景甲的 host 钉板）：sleep 30 忙子期间
/// 8 键满通道＋12 键转缓（零死判）、忙段提示恰一次（附 12）、子醒后排
/// 空全达、续键直达——键序全链一致、损失对账零丢。
#[test]
fn busy_backlog_episode_flow_pass() {
    let mut chan = ChanSim::new();
    let mut bl = Backlog::new();
    let mut typed: Vec<u8> = Vec::new();
    let mut received: Vec<u8> = Vec::new();
    let mut reports: Vec<usize> = Vec::new();
    let mut hints = 0usize;
    let mut hint_n = 0usize;
    let mut deaths = 0usize;
    let t0 = 1_000u64;
    let mut t = t0;

    // —— 忙子期（sleep 形态：零读）：前 8 键直达满通道（CHAN_DEPTH=8）。
    for i in 0..8u8 {
        typed.push(b'a' + i);
        assert_eq!(admit(&mut chan, &mut bl, b'a' + i, t).unwrap(), KeyAct::Delivered);
    }
    assert_eq!(chan.queued.len(), 8);
    // 第 9 键起满队转缓——**零死判**（病灶一的误杀形态在本接线下属名）。
    for i in 0..12u8 {
        typed.push(b'A' + i);
        assert_eq!(admit(&mut chan, &mut bl, b'A' + i, t).unwrap(), KeyAct::Buffered);
    }
    assert_eq!(bl.pending_len(), 12);

    // —— 忙段提示（帧循环 hint_due 面）：阈值前两次查无、到点恰一次。
    if let Some(n) = bl.hint_due(t + CHILD_BUSY_HINT_NS - 1) {
        hints += 1;
        hint_n = n;
    }
    if let Some(n) = bl.hint_due(t + CHILD_BUSY_HINT_NS) {
        hints += 1;
        hint_n = n;
    }
    if let Some(n) = bl.hint_due(t + CHILD_BUSY_HINT_NS + 1) {
        hints += 1;
        hint_n = n;
    }
    assert_eq!(hints, 1);
    assert_eq!(hint_n, 12); // 「child busy, 12 keys buffered」

    // —— sleep 30 醒（Writable 醒源）：子排空在队 8 键；此后子处交互
    //    读态——帧循环「排空步＋子读」交替至清空（裁决 13：排空步每轮
    //    无条件试，正确性不依赖唤醒粒度；通道深 8 < pending 12——首步
    //    只发得进 8 键，满队留队头下轮续发）。
    t += 30 * 1_000_000_000;
    received.extend(chan.child_read_all());
    loop {
        deaths += drain_step(&mut chan, &mut bl, &mut reports);
        let got = chan.child_read_all();
        let n = got.len();
        received.extend(got);
        if bl.pending_len() == 0 {
            break;
        }
        assert!(n > 0, "排空停摆：子醒期 pending 非空却零腾位");
    }

    // —— 续键（排空后）：pending 空、通道空 → 直达。
    t += 1;
    for i in 0..5u8 {
        typed.push(b'0' + i);
        assert_eq!(admit(&mut chan, &mut bl, b'0' + i, t).unwrap(), KeyAct::Delivered);
    }
    received.extend(chan.child_read_all());

    // —— 断言：零死判、键全达且跨段 FIFO（typed 序＝received 序）、
    //    损失对账（零 dropped、episode 回报恰一笔零丢）。
    assert_eq!(deaths, 0);
    assert_eq!(reports, vec![0]);
    assert_eq!(received, typed);
}

/// 反证（pipe_sim bite 同款纪律）：同一事件序下「旁路直发」接线
///（pending 非空仍探测直发——排空与键收割间隙子腾位竞窗的改造前形态）
/// 产乱序、Backlog 接线保序——证明本仿真的 FIFO 断言真在咬。
#[test]
fn fifo_bite_under_bypass_wiring() {
    // 事件序：8 键满通道 → 4 键入缓 → 子排空通道（腾位）→ 新键 k13 到
    //（排空步未跑——竞窗）→ 排空 4 键 → 子全收。
    let typed: Vec<u8> = (0..8u8)
        .chain(8..12u8)
        .chain([13u8].into_iter())
        .collect();

    // —— 形甲（旁路接线）：k13 在竞窗直发，跳到 k9..k12 之前——乱序。
    let mut chan = ChanSim::new();
    let mut received: Vec<u8> = Vec::new();
    let mut pending: Vec<u8> = Vec::new();
    for k in 0..8u8 {
        assert_eq!(chan.send(k), SendFeed::Sent);
    }
    for k in 8..12u8 {
        assert_eq!(chan.send(k), SendFeed::Full);
        pending.push(k); // 旁路形自管缓存（无 Backlog 仲裁）
    }
    received.extend(chan.child_read_all()); // 子腾位
    assert_eq!(chan.send(13), SendFeed::Sent); // 竞窗直发——跳队
    for k in pending.drain(..) {
        assert_eq!(chan.send(k), SendFeed::Sent);
    }
    received.extend(chan.child_read_all());
    assert_ne!(received, typed, "旁路形应收乱序（断言在咬的负证）");

    // —— 形乙（Backlog 接线——同事件序）：pending 非空不探测，k13 入缓
    //    尾接，排空步保 FIFO——全序一致。
    let mut chan = ChanSim::new();
    let mut bl = Backlog::new();
    let mut received = Vec::new();
    let mut reports = Vec::new();
    let mut deaths = 0usize;
    for k in 0..8u8 {
        assert_eq!(admit(&mut chan, &mut bl, k, 0).unwrap(), KeyAct::Delivered);
    }
    for k in 8..12u8 {
        assert_eq!(admit(&mut chan, &mut bl, k, 0).unwrap(), KeyAct::Buffered);
    }
    received.extend(chan.child_read_all()); // 子腾位（竞窗开）
    // k13 到达：pending 非空 → 不探测直接入缓冲（G6 结构性规则）。
    assert_eq!(admit(&mut chan, &mut bl, 13, 0).unwrap(), KeyAct::Buffered);
    deaths += drain_step(&mut chan, &mut bl, &mut reports);
    received.extend(chan.child_read_all());
    assert_eq!(deaths, 0);
    assert_eq!(received, typed); // 保序：k13 尾接 k9..k12 之后
}
