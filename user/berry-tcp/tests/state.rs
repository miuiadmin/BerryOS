//! berry-tcp 状态机 host 测试（`docs/TCP栈设计草案.md` §4 定值表 + §7 T-b 全状态分支）。
//!
//! 约定：我方 ISS=1000，对端 ISS=5000——握手后 Established 基线：
//! snd_nxt=1001 / snd_una=1001 / rcv_nxt=5001 / inflight=None。

use berry_tcp::{
    rto_ms, Action, Conn, MSS, RTO_START_MS, SegFlags, State, TcpSeg, TIME_WAIT_MS, TX_SLOTS,
    WINDOW, ACCEPT_Q_MAX, AcceptQueue,
};

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

/// 指定 window 的纯 ACK（二刀发送窗消费测试用）。
fn ack_seg_w(ack: u32, window: u16) -> TcpSeg<'static> {
    let mut s = ack_seg(ack);
    s.window = window;
    s
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
    // 二刀在途环：未确认段不再占位即拒——环满与越窗拒面各见专项测试
    //（send_ring_full_rejects / window_limits_inflight）。
    c.send(64);
    assert_eq!(c.send(10).action, Action::Data { seq: 1065, len: 10 });
    assert_eq!(c.snd_nxt, 1075);
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
    // SynSent / Established / FinWait1 / TimeWait 四态收 RST → Closed；之后任意段被忽略。
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
        {
            // TimeWait 是 2s 长驻态（杂散 RST 命中概率最高）：主动关闭全程
            // 至 TimeWait 再收 RST——on_seg 的 RST 分支先于状态分派，长驻
            // 态同样直转 Closed。
            let mut c = established();
            c.close(); // → FinWait1
            c.on_seg(&ack_seg(1002)); // FIN 被 ACK → FinWait2
            c.on_seg(&fin_seg(5001)); // 对端 FIN → TimeWait
            assert_eq!(c.state, State::TimeWait);
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
fn close_queues_fin_behind_data() {
    // 二刀改义钉板（旧 close_overrides_data_inflight：FIN 覆盖单槽——数据
    // 重传面让位；语义扩域为 FIN 排队尾，旧「槽已换主」断言废除）：数据在途
    // 直接 close → FIN 排在数据槽后，数据槽保留重发面。
    let mut c = established();
    c.send(64); // snd_nxt=1065, 环=[Data{1001,64}]
    let out = c.close();
    assert_eq!(c.state, State::FinWait1);
    assert_eq!(out.action, Action::Fin { seq: 1065 });
    // 环头仍是数据：GBN 队头重发数据而非 FIN。
    assert_eq!(c.on_rto().action, Action::Data { seq: 1001, len: 64 });
    // ACK 只覆盖数据尾（未覆盖 fin+1=1066）：数据槽弹出、FIN 槽保留、
    // 状态停 FinWait1（④ 环非空不转 FinWait2）。
    let a = ack_seg(1065);
    c.on_seg(&a);
    assert_eq!(c.state, State::FinWait1);
    assert_eq!(c.on_rto().action, Action::Fin { seq: 1065 });
    // 恰覆盖 fin+1 → FinWait2。
    let a2 = ack_seg(1066);
    c.on_seg(&a2);
    assert_eq!(c.state, State::FinWait2);
}

// ─── 二刀：在途环（Go-Back-N）+ 发送窗消费 ───

#[test]
fn tx_ring_two_segments_and_snd_nxt() {
    // 双段排队：一个 op 拆 128+122 两段连续入环（in-flight>1），snd_nxt 连续
    // 推进；RTO 重发环头（GBN 队头）。
    let mut c = established();
    let o1 = c.send(128);
    assert_eq!(o1.action, Action::Data { seq: 1001, len: 128 });
    let o2 = c.send(122);
    assert_eq!(o2.action, Action::Data { seq: 1129, len: 122 });
    assert_eq!(c.snd_nxt, 1251);
    assert_eq!(c.tx_inflight(), 2);
    assert_eq!(c.on_rto().action, Action::Data { seq: 1001, len: 128 });
}

#[test]
fn cross_seg_ack_pops_multi_straddle_keeps() {
    // 部分 ACK 落在第一段中间：straddling 保槽 + snd_una 推进 + backoff 归零
    // + RTO 仍从原 seq 全段重发（h14 语义在环上的延续）。
    let mut c = established();
    c.send(128); // [1001,1129)
    c.send(122); // [1129,1251)
    c.on_seg(&ack_seg(1050));
    assert_eq!(c.snd_una, 1050);
    assert_eq!(c.tx_inflight(), 2);
    assert_eq!(c.rto_backoff, 0);
    assert_eq!(c.on_rto().action, Action::Data { seq: 1001, len: 128 });
    // ACK 覆盖第一段尾（落在第二段中间）：第一段弹出、第二段 straddling 保留
    //——环头推进到第二段。
    let mut c2 = established();
    c2.send(128);
    c2.send(122);
    c2.on_seg(&ack_seg(1200));
    assert_eq!(c2.tx_inflight(), 1);
    assert_eq!(c2.on_rto().action, Action::Data { seq: 1129, len: 122 });
    // 跨段覆盖一次弹多头：ACK 覆盖两段尾 → 两槽一次弹尽、可续发。
    c.on_seg(&ack_seg(1251));
    assert_eq!(c.tx_inflight(), 0);
    assert_eq!(c.on_rto().action, Action::None);
    assert_eq!(c.send(10).action, Action::Data { seq: 1251, len: 10 });
}

#[test]
fn gbn_head_retransmit_and_cap_clears_ring() {
    // GBN 队头重发：RTO 恒重发环头最老段（后续段不随发），backoff 递增；
    // 到帽放弃 → Closed 清环。
    let mut c = established();
    c.send(128);
    c.send(122);
    assert_eq!(c.on_rto().action, Action::Data { seq: 1001, len: 128 });
    assert_eq!(c.rto_backoff, 1);
    assert_eq!(c.on_rto().action, Action::Data { seq: 1001, len: 128 });
    assert_eq!(c.on_rto().action, Action::Data { seq: 1001, len: 128 });
    let out = c.on_rto(); // 第 4 次：放弃
    assert_eq!(out.action, Action::None);
    assert_eq!(c.state, State::Closed);
    assert_eq!(c.tx_inflight(), 0);
}

#[test]
fn zero_window_blocks_until_update() {
    // 零窗停发（h06 判据翻新的树内镜像）：对端通告 0 → send 拒；窗更新 ACK
    //（纯 ACK 携新窗）放行。旧「零窗照发」语义域已扩为消费对端窗。
    let (mut c, _) = Conn::open(1000);
    let mut sa = syn_ack(5000, 1001, None);
    sa.window = 0;
    c.on_seg(&sa);
    assert_eq!(c.snd_wnd, 0);
    assert_eq!(c.send(64).action, Action::None, "零窗停发");
    c.on_seg(&ack_seg_w(1001, 512));
    assert_eq!(c.snd_wnd, 512);
    assert_eq!(c.send(64).action, Action::Data { seq: 1001, len: 64 });
}

#[test]
fn window_limits_inflight() {
    // 窗口限在途：128 窗下第一段（128B）恰放行，第二段拒（128+10 > 128）；
    // ACK 推进 snd_una 后放行（窗不变——放行来自在途清零）。
    let (mut c, _) = Conn::open(1000);
    let mut sa = syn_ack(5000, 1001, None);
    sa.window = 128;
    c.on_seg(&sa);
    assert_eq!(c.snd_wnd, 128);
    assert_eq!(c.send(128).action, Action::Data { seq: 1001, len: 128 });
    assert_eq!(c.send(10).action, Action::None, "越窗拒：在途 128+10 > 窗 128");
    c.on_seg(&ack_seg_w(1129, 128));
    assert_eq!(c.snd_una, 1129);
    assert_eq!(c.send(10).action, Action::Data { seq: 1129, len: 10 });
}

#[test]
fn data_seg_piggybacks_window_update() {
    // 数据段捎带窗更新：snd_wnd 记账对「一切携 ACK 段」生效——窗更新不
    // 必走纯 ACK，数据+ACK 同段携新窗同样解锁零窗停发（与
    // zero_window_blocks_until_update 的纯 ACK 路互补钉板）。
    let (mut c, _) = Conn::open(1000);
    let mut sa = syn_ack(5000, 1001, None);
    sa.window = 0;
    c.on_seg(&sa);
    assert_eq!(c.send(64).action, Action::None, "零窗停发");
    let mut seg = data_seg(5001, b"hi!");
    seg.window = 512;
    let out = c.on_seg(&seg);
    assert_eq!(c.rcv_nxt, 5004); // 数据正常交付
    assert_eq!(out.action, Action::Ack);
    assert_eq!(c.snd_wnd, 512, "数据+ACK 同段携新窗");
    assert_eq!(c.send(64).action, Action::Data { seq: 1001, len: 64 });
}

#[test]
fn window_shrink_limits_inflight() {
    // 缩窗即刻生效：宽窗放行 64B 在途后，携小窗 ACK 把 snd_wnd 压至 32
    // ——覆盖赋值即刻生效，在途 64 已超新窗，后续段拒（门控只会拦不会
    // 放，不腐化已发段——单向 0→512 放行的补面）。
    let (mut c, _) = Conn::open(1000);
    let mut sa = syn_ack(5000, 1001, None);
    sa.window = 512;
    c.on_seg(&sa);
    assert_eq!(c.send(64).action, Action::Data { seq: 1001, len: 64 });
    c.on_seg(&ack_seg_w(1001, 32)); // ACK 不推进 una，仅窗回缩
    assert_eq!(c.snd_una, 1001);
    assert_eq!(c.snd_wnd, 32);
    assert_eq!(c.send(10).action, Action::None, "缩窗后按新窗限在途：64+10 > 32");
}

#[test]
fn send_ring_full_rejects() {
    // 环满拒：宽窗（8192）下 4×128 恰满 TX_SLOTS，第 5 段拒；弹槽腾位后续发。
    let (mut c, _) = Conn::open(1000);
    let mut sa = syn_ack(5000, 1001, None);
    sa.window = 8192;
    c.on_seg(&sa);
    for i in 0..4 {
        let seq = 1001 + 128 * i;
        assert_eq!(c.send(128).action, Action::Data { seq, len: 128 });
    }
    assert_eq!(c.tx_inflight(), TX_SLOTS);
    assert_eq!(c.send(1).action, Action::None, "环满拒");
    c.on_seg(&ack_seg_w(1129, 8192)); // 弹第一段
    assert_eq!(c.tx_inflight(), TX_SLOTS - 1);
    assert_eq!(c.send(1).action, Action::Data { seq: 1513, len: 1 });
}

#[test]
fn handshake_consumes_peer_window() {
    // 握手消费对端窗：主动面 SYN+ACK(window=1400) → snd_wnd=1400；被动面
    // SYN(window=900) → passive_open 即记（第三步 ACK 再刷新）。
    let (mut c, _) = Conn::open(1000);
    let mut sa = syn_ack(5000, 1001, None);
    sa.window = 1400;
    c.on_seg(&sa);
    assert_eq!(c.snd_wnd, 1400);
    let mut ps = pure_syn(5000, None);
    ps.window = 900;
    let (sc, _) = Conn::passive_open(2000, &ps).unwrap();
    assert_eq!(sc.snd_wnd, 900);
}

#[test]
fn rst_clears_tx_ring() {
    // RST 清环：多段在途态收 RST → Closed + 环空 + on_rto 无重发面。
    let mut c = established();
    c.send(128);
    c.send(122);
    c.on_seg(&rst_seg());
    assert_eq!(c.state, State::Closed);
    assert_eq!(c.tx_inflight(), 0);
    assert_eq!(c.on_rto().action, Action::None);
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

// ─── 服务端半边（A2-2 段②，`docs/事件循环A2设计草案.md` §3.2）───
// 约定沿用：我方 ISS=1000、对端 SYN seq=5000；服务端握手 = 被动收 SYN →
// 回 SYN|ACK → 收第三步 ACK(=iss+1)。客户端 30 测零改动铁律的对面锚。

/// 对端纯 SYN（无 ack 旗）。
fn pure_syn(seq: u32, mss: Option<u16>) -> TcpSeg<'static> {
    let mut s = syn_ack(seq, 0, mss);
    s.flags.ack = false;
    s
}

/// 服务端被动打开 + 握手第三步 ACK → Established（对端 SYN seq 恒 5000）。
fn server_established(iss: u32) -> Conn {
    let syn = pure_syn(5000, None);
    let (mut c, _) = Conn::passive_open(iss, &syn).unwrap();
    c.on_seg(&ack_seg(iss.wrapping_add(1)));
    c
}

#[test]
fn listen_marker_ignores_all_including_rst() {
    // 监听哨兵：一切入段忽略——含 RST（「RST 任意状态」唯一豁免面：监听口无
    // 四元组上下文，误路由的杂散 RST 不得击毙整个监听服务）。
    let mut c = Conn::listen();
    assert_eq!(c.state, State::Listen);
    for seg in [syn_ack(5000, 1001, None), ack_seg(1001), fin_seg(5001), rst_seg()] {
        assert_eq!(c.on_seg(&seg).action, Action::None);
    }
    assert_eq!(c.state, State::Listen);
    assert_eq!(c.send(10).action, Action::None);
    assert_eq!(c.close().action, Action::None);
}

#[test]
fn passive_open_synack_shape() {
    // SYN(seq=5000, MSS=1400) → SynRcvd + SynAck{seq=iss, ack=5001}；记账四件
    // （snd_nxt/rcv_nxt/peer_mss/in-flight=Syn——RTO 重传臂的载体）。
    let syn = pure_syn(5000, Some(1400));
    let (c, out) = Conn::passive_open(1000, &syn).unwrap();
    assert_eq!(c.state, State::SynRcvd);
    assert_eq!(c.snd_nxt, 1001);
    assert_eq!(c.snd_una, 1000);
    assert_eq!(c.rcv_nxt, 5001);
    assert_eq!(c.peer_mss, Some(1400));
    assert_eq!(out.action, Action::SynAck { seq: 1000, ack: 5001 });
    assert!(out.deliver.is_none());
    // 非 SYN 段 → None（协议层执法，不赌调用方 demux 正确）。
    assert!(Conn::passive_open(1000, &ack_seg(1001)).is_none());
}

#[test]
fn handshake_ack_completes_server() {
    // 第三步纯 ACK：Established + in-flight 清（send 立即可用）+ 无出段（无可应答）。
    let syn = pure_syn(5000, None);
    let (mut c, _) = Conn::passive_open(1000, &syn).unwrap();
    let seg = ack_seg(1001);
    let out = c.on_seg(&seg);
    assert_eq!(c.state, State::Established);
    assert_eq!(c.snd_una, 1001);
    assert_eq!(out.action, Action::None);
    assert_eq!(c.send(10).action, Action::Data { seq: 1001, len: 10 });
}

#[test]
fn handshake_ack_piggyback_data() {
    // ACK+数据同段：握手完成 + 同段交付（echo 首包快路——一步到位不空转一轮）。
    let syn = pure_syn(5000, None);
    let (mut c, _) = Conn::passive_open(1000, &syn).unwrap();
    let seg = data_seg(5001, b"hi");
    let out = c.on_seg(&seg);
    assert_eq!(c.state, State::Established);
    assert_eq!(out.deliver, Some(&b"hi"[..]));
    assert_eq!(c.rcv_nxt, 5003);
    assert_eq!(out.action, Action::Ack);
}

#[test]
fn synrcvd_bad_ack_ignored() {
    let syn = pure_syn(5000, None);
    let (mut c, _) = Conn::passive_open(1000, &syn).unwrap();
    // ack 未指到 iss+1 → 忽略（防旧连接串扰——客户端 handshake_bad_ack_ignored 对偶）。
    assert_eq!(c.on_seg(&ack_seg(999)).action, Action::None);
    assert_eq!(c.state, State::SynRcvd);
    // 无 ack 旗的纯数据段 → 忽略（握手未完成）。
    let mut noack = data_seg(5001, b"x");
    noack.flags.ack = false;
    assert_eq!(c.on_seg(&noack).action, Action::None);
    assert_eq!(c.state, State::SynRcvd);
}

#[test]
fn synrcvd_resyn_reanswers() {
    // 对端重传 SYN（我方 SYN|ACK 丢失）：同 incarnation（seq+1 对齐 rcv_nxt）
    // 重答 SYN|ACK；不同 seq = 新 incarnation，v1 忽略且不改写 rcv_nxt。
    let syn = pure_syn(5000, None);
    let (mut c, _) = Conn::passive_open(1000, &syn).unwrap();
    let resyn = pure_syn(5000, None);
    let out = c.on_seg(&resyn);
    assert_eq!(out.action, Action::SynAck { seq: 1000, ack: 5001 });
    assert_eq!(c.state, State::SynRcvd);
    let other = pure_syn(6000, None);
    let out = c.on_seg(&other);
    assert_eq!(out.action, Action::None);
    assert_eq!(c.state, State::SynRcvd);
    assert_eq!(c.rcv_nxt, 5001);
}

#[test]
fn synrcvd_rto_retransmits_synack_then_cap() {
    // SYN|ACK 的 RTO 重传臂：SynAck 形（非客户端 Syn 形）+ 退避；帽满放弃 → Closed
    // （客户端 rto_backoff_cap_gives_up 的服务端对偶）。
    let syn = pure_syn(5000, None);
    let (mut c, _) = Conn::passive_open(1000, &syn).unwrap();
    for backoff in 1..=3 {
        let out = c.on_rto();
        assert_eq!(out.action, Action::SynAck { seq: 1000, ack: 5001 });
        assert_eq!(c.rto_backoff, backoff);
    }
    let out = c.on_rto(); // 第 4 次：放弃
    assert_eq!(out.action, Action::None);
    assert_eq!(c.state, State::Closed);
}

#[test]
fn synrcvd_rst_closes_conn() {
    // 半开 conn 自身吃 RST → Closed（统一 RST 规则对连接照走；豁免的只是监听哨兵）。
    let syn = pure_syn(5000, None);
    let (mut c, _) = Conn::passive_open(1000, &syn).unwrap();
    c.on_seg(&rst_seg());
    assert_eq!(c.state, State::Closed);
}

#[test]
fn server_side_full_lifecycle() {
    // 被动侧全生命周期：握手 → 收数据交付 → 发数据 → 被动收 FIN → 主动 close →
    // LastAck 收口（echo server 一轮连接的完整形状——A2-3 验收链的状态机地基）。
    let syn = pure_syn(5000, None);
    let (mut c, _) = Conn::passive_open(1000, &syn).unwrap();
    c.on_seg(&ack_seg(1001)); // → Established
    let seg = data_seg(5001, b"echo!");
    let out = c.on_seg(&seg);
    assert_eq!(out.deliver, Some(&b"echo!"[..]));
    assert_eq!(out.action, Action::Ack);
    assert_eq!(c.send(5).action, Action::Data { seq: 1001, len: 5 });
    c.on_seg(&ack_seg(1006)); // 我方数据被确认
    c.on_seg(&fin_seg(5006)); // 对端挥手 → CloseWait
    assert_eq!(c.state, State::CloseWait);
    let out = c.close(); // → LastAck
    assert_eq!(out.action, Action::Fin { seq: 1006 });
    c.on_seg(&ack_seg(1007)); // 覆盖 fin+1 → Closed 正常终态
    assert_eq!(c.state, State::Closed);
}

#[test]
fn accept_queue_fifo_cap_wraparound() {
    assert_eq!(ACCEPT_Q_MAX, 2); // 帽钉板（与 netdproto TCP_SESSIONS_MAX 独立——见头注）
    let mut q = AcceptQueue::new();
    assert!(q.is_empty());
    assert!(q.accept().is_none()); // 空队非阻塞 → None（ST_EAGAIN 上游来源）
    let c1 = server_established(1000);
    let c2 = server_established(2000);
    let c3 = server_established(3000);
    assert!(q.push(c1));
    assert!(q.push(c2));
    assert_eq!(q.len(), 2);
    assert!(q.is_full());
    assert!(!q.push(c3)); // 满队拒（不静默挤占——调用方以返回值执法容量纪律）
    // FIFO：弹最老。
    assert_eq!(q.accept().unwrap().snd_iss, 1000);
    assert_eq!(q.accept().unwrap().snd_iss, 2000);
    // 环回写：head 已前进（现为 0→1→0 一圈），新 conn 落 tail=(head+len)%MAX。
    assert!(q.push(c3));
    assert_eq!(q.accept().unwrap().snd_iss, 3000);
    assert!(q.is_empty());
}

#[test]
fn accept_queue_rejects_non_established() {
    // accept 语义协议层执法：半开 SynRcvd / 监听哨兵 / 已死 conn 一律拒；队列保持空。
    let mut q = AcceptQueue::new();
    let syn = pure_syn(5000, None);
    let (halfopen, _) = Conn::passive_open(1000, &syn).unwrap();
    assert!(!q.push(halfopen));
    assert!(!q.push(Conn::listen()));
    let (mut dead, _) = Conn::open(1000);
    dead.on_seg(&rst_seg());
    assert!(!q.push(dead));
    assert!(q.is_empty());
    assert!(q.accept().is_none());
}

// ─── 首刀防几何回归钉（NET-01 §3.3 入站 >MSS 单段黑洞——1460B 满段交付）───
// vnet 拒收线已锚 RX_SEG_MAX=1460（MTU 域），状态机对大段的交付语义在此
// 直喂钉板：vnet 几何回归（拒收线回落/pbuf 缩水）会让黑洞复现，而状态机
// 侧若引入长度上限/截断，本两测必炸。

#[test]
fn large_seg_in_order_delivered() {
    // 1460B 满段（对端 MTU 拼段形态——SLIRP 不读我方 MSS 通告）：按序
    // 全量交付不截断、rcv_nxt 推进 1460、单 ACK。
    let mut c = established();
    let big = [7u8; 1460];
    let seg = data_seg(5001, &big);
    let out = c.on_seg(&seg);
    assert_eq!(out.deliver, Some(&big[..]));
    assert_eq!(c.rcv_nxt, 5001 + 1460);
    assert_eq!(out.action, Action::Ack);
}

#[test]
fn large_seg_then_regular_continuity() {
    // 大段后接常规段：序号衔接与交付序正确——若大段被截断或 rcv_nxt
    // 推进错位，后续段即乱序丢弃，本断言必炸；对照臂：错位段仍丢弃回
    // 当前 ACK（乱序语义不因大段而漂移）。
    let mut c = established();
    let big = [7u8; 1460];
    c.on_seg(&data_seg(5001, &big));
    let tail = data_seg(5001 + 1460, b"tail");
    let out = c.on_seg(&tail);
    assert_eq!(out.deliver, Some(&b"tail"[..]));
    assert_eq!(c.rcv_nxt, 5001 + 1460 + 4);
    let stray = data_seg(5001 + 1460, b"xxxx");
    let out = c.on_seg(&stray);
    assert!(out.deliver.is_none());
    assert_eq!(out.action, Action::Ack);
    assert_eq!(c.rcv_nxt, 5001 + 1460 + 4);
}
