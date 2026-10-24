//! berry-tcp 状态机 host 测试（`docs/TCP栈设计草案.md` §4 定值表 + §7 T-b 全状态分支）。
//!
//! 约定：我方 ISS=1000，对端 ISS=5000——握手后 Established 基线：
//! snd_nxt=1001 / snd_una=1001 / rcv_nxt=5001 / inflight=None。

use berry_tcp::{rto_ms, Action, Conn, MSS, RTO_START_MS, SegFlags, State, TcpSeg, TIME_WAIT_MS, WINDOW};

fn syn_ack(seq: u32, ack: u32, mss: Option<u16>) -> TcpSeg<'static> {
    TcpSeg {
        seq,
        ack,
        flags: SegFlags { syn: true, ack: true, fin: false, rst: false },
        window: WINDOW,
        mss,
        payload: &[],
    }
}

fn ack_seg(ack: u32) -> TcpSeg<'static> {
    TcpSeg {
        seq: 0,
        ack,
        flags: SegFlags { syn: false, ack: true, fin: false, rst: false },
        window: WINDOW,
        mss: None,
        payload: &[],
    }
}

fn data_seg<'a>(seq: u32, payload: &'a [u8]) -> TcpSeg<'a> {
    TcpSeg {
        seq,
        ack: 1001,
        flags: SegFlags { syn: false, ack: true, fin: false, rst: false },
        window: WINDOW,
        mss: None,
        payload,
    }
}

fn fin_seg(seq: u32) -> TcpSeg<'static> {
    TcpSeg {
        seq,
        ack: 1001,
        flags: SegFlags { syn: false, ack: true, fin: true, rst: false },
        window: WINDOW,
        mss: None,
        payload: &[],
    }
}

fn rst_seg() -> TcpSeg<'static> {
    TcpSeg {
        seq: 0,
        ack: 0,
        flags: SegFlags { syn: false, ack: false, fin: false, rst: true },
        window: WINDOW,
        mss: None,
        payload: &[],
    }
}

/// Established 基线：open(1000) + 收 SYN+ACK(seq=5000, ack=1001)。
fn established() -> Conn {
    let (mut c, _) = Conn::open(1000);
    let sa = syn_ack(5000, 1001, None);
    c.on_seg(&sa);
    c
}

#[test]
fn open_issues_syn() {
    let (c, out) = Conn::open(1000);
    assert_eq!(c.state, State::SynSent);
    assert_eq!(c.snd_nxt, 1001);
    assert_eq!(c.snd_una, 1000);
    assert_eq!(out.action, Action::Syn { seq: 1000 });
}

#[test]
fn handshake_completes() {
    let (mut c, _) = Conn::open(1000);
    let sa = syn_ack(5000, 1001, None);
    let out = c.on_seg(&sa);
    assert_eq!(c.state, State::Established);
    assert_eq!(c.snd_una, 1001);
    assert_eq!(c.rcv_nxt, 5001);
    assert_eq!(out.action, Action::Ack); // 握手第三步
    assert!(out.deliver.is_none());
}

#[test]
fn handshake_bad_ack_ignored() {
    let (mut c, _) = Conn::open(1000);
    // ACK 没指到 iss+1 → 忽略（防旧连接串扰）。
    let sa = syn_ack(5000, 999, None);
    let out = c.on_seg(&sa);
    assert_eq!(c.state, State::SynSent);
    assert_eq!(out.action, Action::None);
    // 纯 SYN（同时打开）v1 不支持 → 忽略。
    let mut pure_syn = syn_ack(5000, 1001, None);
    pure_syn.flags.ack = false;
    let out = c.on_seg(&pure_syn);
    assert_eq!(c.state, State::SynSent);
    assert_eq!(out.action, Action::None);
}

#[test]
fn syn_ack_mss_recorded() {
    let (mut c, _) = Conn::open(1000);
    c.on_seg(&syn_ack(5000, 1001, Some(1400)));
    assert_eq!(c.peer_mss, Some(1400));
    // 无 MSS 选项 = None 保持。
    let (mut c2, _) = Conn::open(2000);
    c2.on_seg(&syn_ack(5000, 2001, None));
    assert_eq!(c2.peer_mss, None);
}

#[test]
fn send_in_established() {
    let mut c = established();
    let out = c.send(64);
    assert_eq!(out.action, Action::Data { seq: 1001, len: 64 });
    assert_eq!(c.snd_nxt, 1065);
}

#[test]
fn send_gatekeeping() {
    // 非 Established 拒。
    let (mut c, _) = Conn::open(1000);
    assert_eq!(c.send(64).action, Action::None);
    // len=0 / 超 MSS 拒。
    let mut c = established();
    assert_eq!(c.send(0).action, Action::None);
    assert_eq!(c.send(MSS as u32 + 1).action, Action::None);
    // in-flight=1：未确认前第二段拒。
    c.send(64);
    assert_eq!(c.send(10).action, Action::None);
    assert_eq!(c.snd_nxt, 1065); // 状态未动
}

#[test]
fn data_advance() {
    let mut c = established();
    let seg = data_seg(5001, b"hello");
    let out = c.on_seg(&seg);
    assert_eq!(out.deliver, Some(&b"hello"[..]));
    assert_eq!(c.rcv_nxt, 5006);
    assert_eq!(out.action, Action::Ack);
}

#[test]
fn duplicate_seg_dropped() {
    let mut c = established();
    c.on_seg(&data_seg(5001, b"hello"));
    // 重复段（seq 落后 rcv_nxt）→ 丢弃 + 隐式重复 ACK。
    let dup = data_seg(5001, b"hello");
    let out = c.on_seg(&dup);
    assert!(out.deliver.is_none());
    assert_eq!(c.rcv_nxt, 5006); // 未推进
    assert_eq!(out.action, Action::Ack);
}

#[test]
fn out_of_order_dropped() {
    let mut c = established();
    let seg = data_seg(5033, b"later");
    let out = c.on_seg(&seg);
    assert!(out.deliver.is_none());
    assert_eq!(c.rcv_nxt, 5001); // 未推进（不缓存）
    assert_eq!(out.action, Action::Ack); // 回当前 ACK 靠对端重传
}

#[test]
fn ack_advances_una_and_clears_inflight() {
    let mut c = established();
    c.send(64);
    let ack = ack_seg(1065);
    let out = c.on_seg(&ack);
    assert_eq!(c.snd_una, 1065);
    assert_eq!(out.action, Action::None);
    // in-flight 已清 → RTO 到期无事可重传。
    assert_eq!(c.on_rto().action, Action::None);
    // 槽清了 → 可发下一段（in-flight=1 顺序单发）。
    assert_eq!(c.send(10).action, Action::Data { seq: 1065, len: 10 });
}

#[test]
fn ack_not_regression() {
    let mut c = established();
    c.send(64);
    // 旧 ACK（低于 una）不回退。
    c.on_seg(&ack_seg(500));
    assert_eq!(c.snd_una, 1001);
}

#[test]
fn rst_any_state_to_closed() {
    // SynSent / Established / FinWait1 三态收 RST → Closed；之后任意段被忽略。
    let states: Vec<Conn> = vec![
        {
            let (mut c, _) = Conn::open(1000);
            c.on_seg(&rst_seg());
            c
        },
        {
            let mut c = established();
            c.on_seg(&rst_seg());
            c
        },
        {
            let mut c = established();
            c.close();
            c.on_seg(&rst_seg());
            c
        },
    ];
    for mut c in states {
        assert_eq!(c.state, State::Closed);
        assert_eq!(c.on_seg(&ack_seg(9999)).action, Action::None);
        assert_eq!(c.on_rto().action, Action::None); // inflight 已清
    }
}

#[test]
fn fin_to_closewait() {
    let mut c = established();
    let f = fin_seg(5001);
    let out = c.on_seg(&f);
    assert_eq!(c.state, State::CloseWait);
    assert_eq!(c.rcv_nxt, 5002); // FIN 占一个序号
    assert_eq!(out.action, Action::Ack);
}

#[test]
fn closewait_close_and_lastack() {
    let mut c = established();
    c.on_seg(&fin_seg(5001)); // → CloseWatch
    let out = c.close();
    assert_eq!(c.state, State::LastAck);
    assert_eq!(out.action, Action::Fin { seq: 1001 });
    assert_eq!(c.snd_nxt, 1002);
    // FIN 的 ACK（覆盖 fin+1）→ Closed 正常终态。
    let ack = ack_seg(1002);
    let out = c.on_seg(&ack);
    assert_eq!(c.state, State::Closed);
    assert_eq!(out.action, Action::None);
}

#[test]
fn closewait_duplicate_fin_ignored() {
    let mut c = established();
    c.on_seg(&fin_seg(5001));
    let f = fin_seg(5001);
    let out = c.on_seg(&f); // 重复 FIN（我方 ACK 丢了）
    assert_eq!(c.state, State::CloseWait); // 不再转
    assert_eq!(c.rcv_nxt, 5002); // 不再推进
    assert_eq!(out.action, Action::None);
}

#[test]
fn active_close_full_path() {
    let mut c = established();
    let out = c.close();
    assert_eq!(c.state, State::FinWait1);
    assert_eq!(out.action, Action::Fin { seq: 1001 });
    assert_eq!(c.snd_nxt, 1002);
    // 我 FIN 被 ACK → FinWait2。
    let ack = ack_seg(1002);
    let out = c.on_seg(&ack);
    assert_eq!(c.state, State::FinWait2);
    assert_eq!(out.action, Action::None);
    // 对端 FIN → TimeWait + ACK。
    let f = fin_seg(5001);
    let out = c.on_seg(&f);
    assert_eq!(c.state, State::TimeWait);
    assert_eq!(c.rcv_nxt, 5002);
    assert_eq!(out.action, Action::Ack);
    // 重复 FIN → 重 ACK（对端没收到我方 ACK）。
    let f = fin_seg(5001);
    let out = c.on_seg(&f);
    assert_eq!(c.state, State::TimeWait);
    assert_eq!(out.action, Action::Ack);
    // 2s 到期 → Closed。
    c.expire_timewait();
    assert_eq!(c.state, State::Closed);
}

#[test]
fn simultaneous_close_direct_timewait() {
    // FinWait1 收「ACK + 对端 FIN」同段：③ 先转 TimeWait，④ 不再降级 FinWait2。
    let mut c = established();
    c.close(); // → FinWait1, inflight=Fin{1001}
    let mut both = fin_seg(5001);
    both.ack = 1002; // 同段确认我方 FIN
    let out = c.on_seg(&both);
    assert_eq!(c.state, State::TimeWait);
    assert_eq!(c.rcv_nxt, 5002);
    assert_eq!(out.action, Action::Ack);
}

#[test]
fn data_with_fin_single_ack() {
    // piggyback：数据 + FIN 同段 → 交付数据、收 FIN 转 CloseWatch、只回一个 ACK。
    let mut c = established();
    let mut seg = data_seg(5001, b"tail");
    seg.flags.fin = true;
    let out = c.on_seg(&seg);
    assert_eq!(out.deliver, Some(&b"tail"[..]));
    assert_eq!(c.rcv_nxt, 5006); // 5 字节 + FIN 序号
    assert_eq!(c.state, State::CloseWait);
    assert_eq!(out.action, Action::Ack);
}

#[test]
fn out_of_order_fin_dropped() {
    // 乱序 FIN（seq 未对齐 rcv_nxt）→ 丢弃（不转状态），靠对端重传。
    let mut c = established();
    let f = fin_seg(5050);
    let out = c.on_seg(&f);
    assert_eq!(c.state, State::Established);
    assert_eq!(c.rcv_nxt, 5001);
    assert_eq!(out.action, Action::None); // 无 payload 无 ack_out
}

#[test]
fn rto_retransmit_and_backoff() {
    let mut c = established();
    c.send(64);
    // 第一次 RTO：重传 Data + backoff 1。
    let out = c.on_rto();
    assert_eq!(out.action, Action::Data { seq: 1001, len: 64 });
    assert_eq!(c.rto_backoff, 1);
    // SYN in-flight 的重传面（握手丢包）。
    let (mut s, _) = Conn::open(7000);
    let out = s.on_rto();
    assert_eq!(out.action, Action::Syn { seq: 7000 });
    assert_eq!(s.rto_backoff, 1);
}

#[test]
fn rto_backoff_cap_gives_up() {
    let mut c = established();
    c.send(64);
    assert_eq!(c.on_rto().action, Action::Data { seq: 1001, len: 64 }); // backoff 1
    assert_eq!(c.on_rto().action, Action::Data { seq: 1001, len: 64 }); // backoff 2
    assert_eq!(c.on_rto().action, Action::Data { seq: 1001, len: 64 }); // backoff 3（帽）
    let out = c.on_rto(); // 第 4 次：放弃 → Closed
    assert_eq!(out.action, Action::None);
    assert_eq!(c.state, State::Closed);
    // 退避序列：3s → 6s → 12s → 24s。
    assert_eq!(
        (0..=3).map(rto_ms).collect::<Vec<_>>(),
        vec![3_000, 6_000, 12_000, 24_000]
    );
    assert_eq!(rto_ms(0), RTO_START_MS);
}

#[test]
fn ack_resets_backoff() {
    let mut c = established();
    c.send(64);
    c.on_rto();
    c.on_rto();
    assert_eq!(c.rto_backoff, 2);
    c.on_seg(&ack_seg(1065)); // in-flight 清 → 退避归零
    assert_eq!(c.rto_backoff, 0);
    assert_eq!(c.state, State::Established);
}

#[test]
fn close_only_from_open_states() {
    // SynSent / Closed / FinWait1 下 close() = None 不动。
    let (mut c, _) = Conn::open(1000);
    assert_eq!(c.close().action, Action::None);
    assert_eq!(c.state, State::SynSent);
    let mut c = established();
    c.close(); // FinWait1
    assert_eq!(c.close().action, Action::None);
    assert_eq!(c.state, State::FinWait1);
    assert_eq!(c.snd_nxt, 1002); // 只 +1 一次
}

#[test]
fn expire_timewait_only_in_timewait() {
    let mut c = established();
    c.expire_timewait(); // 非 TimeWait → 无操作
    assert_eq!(c.state, State::Established);
}

#[test]
fn rto_fin_retransmit() {
    // FIN in-flight 的 RTO 重传臂（FIN 丢失的唯一恢复路径，§8 指定 host 测试兜底）：
    // FinWait1（主动关闭）与 LastAck（被动关闭）两侧。
    let mut c = established();
    c.close(); // → FinWait1, inflight=Fin{1001}
    let out = c.on_rto();
    assert_eq!(out.action, Action::Fin { seq: 1001 });
    assert_eq!(c.rto_backoff, 1);
    let mut c = established();
    let f = fin_seg(5001);
    c.on_seg(&f); // → CloseWatch
    c.close(); // → LastAck, inflight=Fin{1001}
    let out = c.on_rto();
    assert_eq!(out.action, Action::Fin { seq: 1001 });
    assert_eq!(c.rto_backoff, 1);
}

#[test]
fn lastack_partial_ack_stays() {
    // ACK 未覆盖 FIN（off-by-one 探针：>=seq 误代 >=seq+1 时 ack_seg(1001)
    // 也误转 Closed——本断言必炸）：留在 LastAck、槽保留供 RTO 重发。
    let mut c = established();
    let f = fin_seg(5001);
    c.on_seg(&f); // → CloseWatch
    c.close(); // → LastAck, inflight=Fin{1001}
    let a = ack_seg(1001); // 只确认到 FIN 前一字节
    let out = c.on_seg(&a);
    assert_eq!(c.state, State::LastAck);
    assert_eq!(out.action, Action::None);
    assert_eq!(c.on_rto().action, Action::Fin { seq: 1001 }); // 槽未清
    // 重复 FIN 段（ack 未覆盖）进 LastAck → 不动。
    let f2 = fin_seg(5002);
    let out = c.on_seg(&f2);
    assert_eq!(c.state, State::LastAck);
    assert_eq!(out.action, Action::None);
    // 恰覆盖 fin+1 → Closed 正常终态。
    let a2 = ack_seg(1002);
    c.on_seg(&a2);
    assert_eq!(c.state, State::Closed);
}

#[test]
fn partial_ack_keeps_inflight() {
    // 部分 ACK（推进 snd_una 但未覆盖段尾）：槽保留 + RTO 从原 seq 全段重发
    // ——丢包恢复核心语义（对端按 rcv_nxt 丢重复前缀；「任何推进即清」型
    // 回归会让尾段永无重传载体 → 会话静默悬挂，此断言必炸）。
    let mut c = established();
    c.send(64); // inflight=Data{1001,64}, snd_nxt=1065
    let a = ack_seg(1030); // 只确认前 29B
    let out = c.on_seg(&a);
    assert_eq!(c.snd_una, 1030);
    assert_eq!(out.action, Action::None);
    assert_eq!(c.on_rto().action, Action::Data { seq: 1001, len: 64 });
    assert_eq!(c.rto_backoff, 1);
}

#[test]
fn close_overrides_data_inflight() {
    // 头注契约钉板：数据在途直接 close → FIN 覆盖单槽（数据重传面让位；
    // 累计 ACK 语义下 FIN 的 ACK 隐含确认数据）。
    let mut c = established();
    c.send(64); // snd_nxt=1065, inflight=Data{1001,64}
    let out = c.close();
    assert_eq!(c.state, State::FinWait1);
    assert_eq!(out.action, Action::Fin { seq: 1065 });
    // 槽已换主：RTO 重发 FIN 而非数据。
    assert_eq!(c.on_rto().action, Action::Fin { seq: 1065 });
    // ACK 只覆盖数据尾（未覆盖 fin+1=1066）：槽保留、状态停 FinWait1。
    let a = ack_seg(1065);
    c.on_seg(&a);
    assert_eq!(c.state, State::FinWait1);
    assert_eq!(c.on_rto().action, Action::Fin { seq: 1065 });
    // 恰覆盖 fin+1 → FinWait2。
    let a2 = ack_seg(1066);
    c.on_seg(&a2);
    assert_eq!(c.state, State::FinWait2);
}

#[test]
fn consts_pinned() {
    // §4 定值表钉板：MSS=128 / 窗口 512 / TIME_WAIT 2s。
    assert_eq!(MSS, 128);
    assert_eq!(WINDOW, 512);
    assert_eq!(TIME_WAIT_MS, 2_000);
    assert_eq!(RTO_START_MS, 3_000);
    // 窗口恒通告：任意操作序列后不变（滑窗 v2 的扩展点护栏）。
    let (mut c, _) = Conn::open(1000);
    assert_eq!(c.window, WINDOW);
    c.on_seg(&syn_ack(5000, 1001, Some(1400)));
    c.send(64);
    c.on_seg(&data_seg(5001, b"hello"));
    c.on_seg(&ack_seg(1065));
    c.close(); // FIN seq=1065（send 64B 已推进 snd_nxt）
    c.on_seg(&ack_seg(1066)); // 覆盖 fin+1 → FinWait2
    c.on_seg(&fin_seg(5006)); // 对齐交付 hello 后的 rcv_nxt → TimeWait
    assert_eq!(c.state, State::TimeWait);
    assert_eq!(c.window, WINDOW);
}

#[test]
fn closed_ignores_all() {
    let mut c = established();
    c.on_seg(&rst_seg()); // → Closed
    for seg in [syn_ack(5000, 1001, None), ack_seg(1002), fin_seg(5001)] {
        assert_eq!(c.on_seg(&seg).action, Action::None);
    }
    assert_eq!(c.state, State::Closed);
    assert_eq!(c.send(10).action, Action::None);
    assert_eq!(c.close().action, Action::None);
}
