//! gui-wmd lib —— 波3 wmd 策略核纯逻辑面（host 单测宿主）。
//!
//! 双面 crate（gui-inputd 先例）：本 lib = 权威窗口表状态机（两层窗 +
//! Z 序 + AVOID_SYS 避让 + 单焦点 + version 纪元计数 + 波3 新面：焦点
//! 维护 / OP_WM_BACK 路由 + 500ms 单槽兜底 / 属主事件通道 / 表变更广播
//! / 无窗订阅者）——**全部 I/O 走 [`Port`] 缝**（时间/收发/建联/关闭），
//! 零 berry-libc/berry-sys 调用点（仅 `Handle = u32` 类型引用），host
//! `cargo test` 直跑；bin（main.rs）= guest 入口（ChanPort 接
//! berry_sys + 两路 poll-first serve + 事件通道并监 + 兜底软定时）。
//!
//! 纯函数纪律（berry-guiproto/gui-inputd 同款）：零分配、零 unsafe、
//! 零 panic 数据面（`Port::fail` 是防御面出口——表项值域由构造保证，
//! 不可达分支 loud 不静默）。host 单测全盖（inline `tests`——MockPort
//! 喂真 wire 帧（mock 客户端）+ 应答/事件/关闭三记录面断言）。

#![cfg_attr(not(test), no_std)]

pub mod table;

use berry_sys::Handle;

/// 事件帧非阻塞单发结果（Port 缝的收窄回传面——berry_sys 错码不进 lib）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendEvtStatus {
    /// 已入对端队列。
    Queued,
    /// 对端队列满（满即丢——真值恒经 TABLE_GET，丢弃至多迟一拍）。
    Full,
    /// 余错码（通道病态——调用方关闭拆除降级）。
    Dead,
}

/// wmd 策略核 I/O 缝（bin 侧真实现 = ChanPort；host 测试 = mock）。
pub trait Port {
    /// 单调钟现值（ns）。
    fn now_ns(&mut self) -> u64;
    /// 阻塞应答 send（真实现：EBUSY 重臂 + 总帽到点 trap——失败面归
    /// Port，策略核零重试逻辑）。
    fn send(&mut self, ch: Handle, rep: &[u8]);
    /// 阻塞应答 send + 随行 handle（gift 面——CREATE ack / SUBSCRIBE ack）。
    fn send_handles(&mut self, ch: Handle, rep: &[u8], hs: &[Handle]);
    /// 事件帧非阻塞单发（永不重试等待——结果收窄 [`SendEvtStatus`]）。
    fn send_nowait(&mut self, ch: Handle, frame: &[u8]) -> SendEvtStatus;
    /// 事件通道非阻塞收：`Some(n)` = 收到一帧（buf 前 n 字节有效）/
    /// `None` = 空（真实现：非 -EAGAIN 错码 = 链断，Port 内 loud）。
    fn recv_nowait(&mut self, ch: Handle, buf: &mut [u8]) -> Option<usize>;
    /// close 一个 handle（属主通道拆除/拒收随行）。
    fn close(&mut self, h: Handle);
    /// 建通道对：`(wmd 侧端, 客户端端)`；None = 内核资源面拒绝（CREATE
    /// 降级无通道窗 / SUBSCRIBE ST_FULL）。
    fn channel_create(&mut self) -> Option<(Handle, Handle)>;
    /// 防御面 loud（不可达分支——真实现 = 自陷只死自己；测试 = panic）。
    fn fail(&mut self) -> !;
    /// close 一组 handle（拒收公共出口——默认逐个 [`Port::close`]）。
    fn close_all(&mut self, hs: &[Handle]) {
        for &h in hs {
            self.close(h);
        }
    }
}

#[cfg(test)]
mod tests {
    //! mock 客户端协议面单测（波3 件④）：MockPort 喂**真 wire 帧**过
    //! `handle_wm`/`wait`/`sweep_evt`/`on_back_tick`，应答（replies/gifts）、
    //! 事件帧（events）、关闭（closed）三记录面逐条断言——状态机行为与
    //! wire 面共识一体钉板（C-4 FakeSrc 同法：确定性序列 + 状态谓词）。

    use crate::{Port, SendEvtStatus};
    use crate::table::{WmTable, WMD_BACK_FALLBACK_NS, WM_EVT_SUBS_MAX};
    use berry_guiproto as gp;
    use std::vec::Vec;

    /// mock wm_srv 客户端应答通道句柄。
    const CH_WM: u32 = 1;
    /// mock push_srv 客户端应答通道句柄。
    const CH_PUSH: u32 = 2;
    /// mock 第二 wm 客户端（多客户端语义面）。
    const CH_WM2: u32 = 3;
    const MS: u64 = 1_000_000;

    // —— MockPort：三记录面 + 四脚本位（建联/满/死/收件箱） ——

    struct MockPort {
        now: u64,
        replies: Vec<(u32, Vec<u8>)>,
        gifts: Vec<(u32, Vec<u8>, Vec<u32>)>,
        events: Vec<(u32, Vec<u8>)>,
        closed: Vec<u32>,
        created: Vec<(u32, u32)>,
        create_ok: bool,
        next_srv: u32,
        next_cli: u32,
        inbox: Vec<(u32, Vec<u8>)>,
        full: Vec<u32>,
        dead: Vec<u32>,
    }

    impl MockPort {
        fn new() -> Self {
            MockPort {
                now: 1_000_000,
                replies: Vec::new(),
                gifts: Vec::new(),
                events: Vec::new(),
                closed: Vec::new(),
                created: Vec::new(),
                create_ok: true,
                next_srv: 100,
                next_cli: 200,
                inbox: Vec::new(),
                full: Vec::new(),
                dead: Vec::new(),
            }
        }

        /// 时间步进（ms）。
        fn step(&mut self, ms: u64) {
            self.now += ms * MS;
        }

        /// 属主/订阅者通道注入来帧（BACK_ACK 等——recv_nowait 弹出序 = 注入序）。
        fn inject(&mut self, ch: u32, frame: &[u8]) {
            self.inbox.push((ch, frame.to_vec()));
        }

        /// 事件帧记录面查询（ch 上全部帧）。
        fn events_on(&self, ch: u32) -> Vec<&[u8]> {
            self.events.iter().filter(|(h, _)| *h == ch).map(|(_, b)| b.as_slice()).collect()
        }
    }

    impl Port for MockPort {
        fn now_ns(&mut self) -> u64 {
            self.now
        }
        fn send(&mut self, ch: u32, rep: &[u8]) {
            self.replies.push((ch, rep.to_vec()));
        }
        fn send_handles(&mut self, ch: u32, rep: &[u8], hs: &[u32]) {
            self.gifts.push((ch, rep.to_vec(), hs.to_vec()));
        }
        fn send_nowait(&mut self, ch: u32, frame: &[u8]) -> SendEvtStatus {
            self.events.push((ch, frame.to_vec()));
            if self.dead.contains(&ch) {
                SendEvtStatus::Dead
            } else if self.full.contains(&ch) {
                SendEvtStatus::Full
            } else {
                SendEvtStatus::Queued
            }
        }
        fn recv_nowait(&mut self, ch: u32, buf: &mut [u8]) -> Option<usize> {
            let pos = self.inbox.iter().position(|(h, _)| *h == ch)?;
            let (_, f) = self.inbox.remove(pos);
            buf[..f.len()].copy_from_slice(&f);
            Some(f.len())
        }
        fn close(&mut self, h: u32) {
            self.closed.push(h);
        }
        fn channel_create(&mut self) -> Option<(u32, u32)> {
            if !self.create_ok {
                return None;
            }
            let pair = (self.next_srv, self.next_cli);
            self.next_srv += 1;
            self.next_cli += 1;
            self.created.push(pair);
            Some(pair)
        }
        fn fail(&mut self) -> ! {
            panic!("mock: 防御面 fail 触达（表项值域不变量破缺）");
        }
    }

    // —— 驱动助手：真 wire 帧进出 ——

    struct Re {
        plain: Option<Vec<u8>>,
        gifted: Option<(Vec<u8>, Vec<u32>)>,
    }

    /// 单帧过 wm 域分发，收本帧新增应答（恰 0/1 条；parked WAIT = 0）。
    fn wm(t: &mut WmTable, p: &mut MockPort, ch: u32, frame: &[u8]) -> Re {
        let (r0, g0) = (p.replies.len(), p.gifts.len());
        t.handle_wm(p, ch, frame, &[]);
        let plain = if p.replies.len() > r0 {
            Some(p.replies[r0].1.clone())
        } else {
            None
        };
        let gifted = if p.gifts.len() > g0 {
            let (_, b, hs) = &p.gifts[g0];
            Some((b.clone(), hs.clone()))
        } else {
            None
        };
        Re { plain, gifted }
    }

    fn re_st(re: &Re) -> u8 {
        re.plain.as_ref().expect("应有 1B 应答")[0]
    }

    /// 建窗助手：返回 (wid, Option<客户端事件端>, wmd 侧事件端 Option)。
    fn create_win(
        t: &mut WmTable,
        p: &mut MockPort,
        layer: u8,
        y: u16,
        h: u16,
        avoid: bool,
    ) -> (u16, Option<u32>, Option<u32>) {
        let req = gp::WmCreateReq {
            layer,
            flags: if avoid { gp::WM_CREATE_F_AVOID_SYS } else { 0 },
            parent: 0,
            group: 0,
            x: 0,
            y,
            w: 800,
            h,
        };
        let mut f = [0u8; 16];
        let n = gp::encode_wm_create_req(&mut f, &req).unwrap();
        let re = wm(t, p, CH_WM, &f[..n]);
        match &re.gifted {
            Some((ack, hs)) => {
                assert_eq!(hs.len(), 1, "gift ack 恰 1 随行");
                let wid = gp::parse_wm_create_ack(ack).expect("ack 应 OK 形");
                let (srv, _cli) = p.created[p.created.len() - 1];
                (wid, Some(hs[0]), Some(srv))
            }
            None => {
                let ack = re.plain.as_ref().expect("降级 ack 应零随行");
                let wid = gp::parse_wm_create_ack(ack).expect("ack 应 OK 形");
                (wid, None, None)
            }
        }
    }

    fn table_get(t: &mut WmTable, p: &mut MockPort) -> gp::WmSnapshot {
        let mut f = [0u8; 8];
        let n = gp::encode_wm_table_get_req(&mut f);
        let re = wm(t, p, CH_WM, &f[..n]);
        match gp::parse_wm_snapshot_rep(&re.plain.expect("快照应答")) {
            Some(Ok(s)) => s,
            other => panic!("快照解析失败: {:?}", other.map(|r| r.err())),
        }
    }

    fn focused_id(s: &gp::WmSnapshot) -> Option<u16> {
        (0..s.count as usize)
            .find(|&i| s.entries[i].flags & gp::WM_WIN_F_FOCUSED != 0)
            .map(|i| s.entries[i].id)
    }

    fn entry<'a>(s: &'a gp::WmSnapshot, id: u16) -> &'a gp::WmEntry {
        (0..s.count as usize)
            .map(|i| &s.entries[i])
            .find(|e| e.id == id)
            .expect("表项在册")
    }

    fn focus_req(t: &mut WmTable, p: &mut MockPort, win: u16) -> u8 {
        let mut f = [0u8; 8];
        let n = gp::encode_wm_focus_req(&mut f, win);
        re_st(&wm(t, p, CH_WM, &f[..n]))
    }

    fn back_req(t: &mut WmTable, p: &mut MockPort) -> u8 {
        let mut f = [0u8; 8];
        let n = gp::encode_wm_back_req(&mut f);
        re_st(&wm(t, p, CH_WM, &f[..n]))
    }

    fn destroy_req(t: &mut WmTable, p: &mut MockPort, id: u16) -> u8 {
        let mut f = [0u8; 8];
        let n = gp::encode_wm_destroy_req(&mut f, id);
        re_st(&wm(t, p, CH_WM, &f[..n]))
    }

    fn mk_table() -> (WmTable, MockPort) {
        (WmTable::new(800, 600, 1_000_000), MockPort::new())
    }

    // —— 焦点维护（② OP_WM_FOCUS） ——

    #[test]
    fn focus_take_release_and_rejects() {
        let (mut t, mut p) = mk_table();
        let (w1, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        let (w2, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        // CREATE 默认接焦：末建窗持焦。
        assert_eq!(focused_id(&table_get(&mut t, &mut p)), Some(w2));
        // 授予 w1。
        assert_eq!(focus_req(&mut t, &mut p, w1), gp::ST_OK);
        assert_eq!(focused_id(&table_get(&mut t, &mut p)), Some(w1));
        // 让出（win=0）：焦点清空不动他窗。
        assert_eq!(focus_req(&mut t, &mut p, 0), gp::ST_OK);
        assert_eq!(focused_id(&table_get(&mut t, &mut p)), None);
        assert_eq!(table_get(&mut t, &mut p).count, 2);
        // 不在册目标 ST_NOENT。
        assert_eq!(focus_req(&mut t, &mut p, 0xBEEF), gp::ST_NOENT);
        // 隐藏窗不可聚焦（持焦窗恒可见不变量——策略拒走 ST_BAD，
        // OP_INPUT_INJECT 拒面先例）；SHOW 后恢复。
        let mut f = [0u8; 20];
        let sr = gp::WmSetReq {
            mask: gp::WM_SET_F_VIS,
            window_id: w1,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            vis: gp::WM_VIS_HIDE,
            z: 0,
        };
        let n = gp::encode_wm_set_req(&mut f, &sr).unwrap();
        assert_eq!(re_st(&wm(&mut t, &mut p, CH_WM, &f[..n])), gp::ST_OK);
        assert_eq!(focus_req(&mut t, &mut p, w1), gp::ST_BAD);
        let sr = gp::WmSetReq { vis: gp::WM_VIS_SHOW, ..sr };
        let n = gp::encode_wm_set_req(&mut f, &sr).unwrap();
        let _ = wm(&mut t, &mut p, CH_WM, &f[..n]);
        assert_eq!(focus_req(&mut t, &mut p, w1), gp::ST_OK);
        assert_eq!(focused_id(&table_get(&mut t, &mut p)), Some(w1));
        // 畸形帧（截断）→ ST_BAD。
        let mut bad = [0u8; 7];
        bad[0] = gp::OP_WM_FOCUS;
        assert_eq!(re_st(&wm(&mut t, &mut p, CH_WM, &bad)), gp::ST_BAD);
    }

    // —— SERVICE 层窗表支持（② WM_LAYER_SERVICE=2） ——

    #[test]
    fn layer_service_table_and_no_hole_carve() {
        let (mut t, mut p) = mk_table();
        // SERVICE 建窗合法入表（layer=2 回显原值）。
        let (svc, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_SERVICE, 560, 40, false);
        let (sys, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_SYSTEM, 0, 30, false);
        let (app, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, true);
        let s = table_get(&mut t, &mut p);
        assert_eq!(entry(&s, svc).layer, gp::WM_LAYER_SERVICE);
        assert_eq!(entry(&s, sys).layer, gp::WM_LAYER_SYSTEM);
        // AVOID_SYS 只对 SYSTEM 挖坑：app 有效几何被顶带收缩、SERVICE 底带
        // 不挖（覆盖语义——波3设计 §5 裁决）。
        let e = entry(&s, app);
        assert_eq!((e.x, e.y, e.w, e.h), (0, 30, 800, 570), "SERVICE 不挖坑");
        // 隐藏 SYSTEM 窗不挖坑（既有不变量在 SERVICE 收宽下保持）。
        let mut f = [0u8; 20];
        let sr = gp::WmSetReq {
            mask: gp::WM_SET_F_VIS,
            window_id: sys,
            x: 0,
            y: 0,
            w: 0,
            h: 0,
            vis: gp::WM_VIS_HIDE,
            z: 0,
        };
        let n = gp::encode_wm_set_req(&mut f, &sr).unwrap();
        let _ = wm(&mut t, &mut p, CH_WM, &f[..n]);
        let s = table_get(&mut t, &mut p);
        let e = entry(&s, app);
        assert_eq!((e.y, e.h), (0, 600), "隐藏系统窗不挖坑");
    }

    // —— 返回路由（③ OP_WM_BACK → WM_EVT_BACK + BACK_ACK/兜底状态机） ——

    #[test]
    fn back_routes_to_focus_owner_and_ack_releases() {
        let (mut t, mut p) = mk_table();
        let (w1, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        let (w2, _, Some(srv2)) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false)
        else {
            panic!("应建联事件通道")
        };
        // 焦点 = w2（末建接焦）——BACK 路由到 w2 属主。
        assert_eq!(back_req(&mut t, &mut p), gp::ST_OK);
        let evts = p.events_on(srv2);
        assert_eq!(evts.len(), 2, "建窗广播 + BACK 投递"); // 第一条 = CREATE 广播
        assert_eq!(gp::parse_wm_evt_back(evts[1]), Some(w2), "BACK 投递携 wid");
        assert!(t.back_deadline().is_some(), "兜底槽武装");
        // 窗存活 + 焦点不变（受理 ≠ 处理）。
        let s = table_get(&mut t, &mut p);
        assert_eq!(s.count, 2);
        assert_eq!(focused_id(&s), Some(w2));
        // 属主 ACK → 兜底解除；600ms 后 tick 不摘窗。
        let mut ack = [0u8; 1];
        let _ = gp::encode_wm_back_ack(&mut ack);
        p.inject(srv2, &ack);
        t.sweep_evt(&mut p);
        assert!(t.back_deadline().is_none(), "BACK_ACK 解除兜底");
        p.step(600);
        let now = p.now_ns();
        t.on_back_tick(&mut p, now);
        let s = table_get(&mut t, &mut p);
        assert_eq!(s.count, 2, "ACK 后窗存活");
        assert_eq!(focused_id(&s), Some(w2));
        assert!(entry(&s, w1).id == w1);
    }

    #[test]
    fn back_timeout_fallback_destroy_and_refocus() {
        let (mut t, mut p) = mk_table();
        let (w1, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        let (_w2, _, Some(srv2)) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false)
        else {
            panic!("应建联事件通道")
        };
        assert_eq!(back_req(&mut t, &mut p), gp::ST_OK);
        let ver = table_get(&mut t, &mut p).version;
        // 499ms 未到点：不摘。
        p.step(499);
        let now = p.now_ns();
        t.on_back_tick(&mut p, now);
        assert_eq!(table_get(&mut t, &mut p).count, 2);
        // 500ms 到点：兜底 DESTROY(w2) + 表序内建 refocus（w1 = 最末可见）。
        p.step(1);
        let now = p.now_ns();
        t.on_back_tick(&mut p, now);
        let s = table_get(&mut t, &mut p);
        assert_eq!(s.count, 1, "兜底摘 w2");
        assert_eq!(entry(&s, w1).id, w1);
        assert_eq!(focused_id(&s), Some(w1), "refocus 回最末可见窗");
        assert!(s.version > ver, "表变更广播面抬版本");
        assert!(t.back_deadline().is_none(), "bump 清槽");
        // 兜底摘窗后属主通道关联关闭（DESTROY 关联关闭立法）。
        assert!(p.closed.contains(&srv2), "属主通道关闭");
        // 再 tick 无重复动作（幂等）。
        p.step(100);
        let now = p.now_ns();
        t.on_back_tick(&mut p, now);
        assert_eq!(table_get(&mut t, &mut p).count, 1);
    }

    #[test]
    fn back_legacy_window_direct_destroy() {
        let (mut t, mut p) = mk_table();
        p.create_ok = false; // 内核资源拒绝 → 无通道窗（旧式面同形）
        let (_w1, cli, srv) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        assert!(cli.is_none() && srv.is_none(), "降级 ack 零随行");
        // 无通道目标窗 = 直接 DESTROY（返回语义仍正确——设计 §3 立法）。
        assert_eq!(back_req(&mut t, &mut p), gp::ST_OK);
        let s = table_get(&mut t, &mut p);
        assert_eq!(s.count, 0, "直接摘窗");
        assert_eq!(focused_id(&s), None);
        assert!(t.back_deadline().is_none(), "无兜底槽（无投递面）");
    }

    #[test]
    fn back_full_channel_still_arms_and_falls_back() {
        let (mut t, mut p) = mk_table();
        let (_, _, Some(srv)) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false)
        else {
            panic!("应建联事件通道")
        };
        p.full.push(srv); // 属主不排空 → 投递满即丢
        assert_eq!(back_req(&mut t, &mut p), gp::ST_OK);
        assert_eq!(p.events_on(srv).len(), 2, "投递尝试已记录（CREATE 广播 + BACK）");
        assert!(t.back_deadline().is_some(), "满即丢同武装——兜底网兜");
        p.step((WMD_BACK_FALLBACK_NS / MS) + 1);
        let now = p.now_ns();
        t.on_back_tick(&mut p, now);
        assert_eq!(table_get(&mut t, &mut p).count, 0, "500ms 无 ACK 兜底摘窗");
    }

    #[test]
    fn back_no_focus_noent() {
        let (mut t, mut p) = mk_table();
        assert_eq!(back_req(&mut t, &mut p), gp::ST_NOENT, "无焦点窗 = 无路由目标");
        // 焦点让出后同形。
        let (_w1, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        assert_eq!(focus_req(&mut t, &mut p, 0), gp::ST_OK);
        assert_eq!(back_req(&mut t, &mut p), gp::ST_NOENT);
        assert_eq!(table_get(&mut t, &mut p).count, 1, "窗不受影响");
    }

    #[test]
    fn back_single_slot_displacement_and_stale_ack() {
        let (mut t, mut p) = mk_table();
        let (w1, _, Some(_)) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false)
        else {
            panic!("应建联事件通道")
        };
        let (w2, _, Some(srv2)) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false)
        else {
            panic!("应建联事件通道")
        };
        // 槽 1：BACK → 焦点 w2（末建）。换焦 w1 再 BACK = 单槽覆盖重臂。
        assert_eq!(back_req(&mut t, &mut p), gp::ST_OK);
        assert_eq!(focus_req(&mut t, &mut p, w1), gp::ST_OK);
        assert_eq!(back_req(&mut t, &mut p), gp::ST_OK);
        // 位移者 w2 的迟到 ACK：wid 失配忽略。
        let mut ack = [0u8; 1];
        let _ = gp::encode_wm_back_ack(&mut ack);
        p.inject(srv2, &ack);
        t.sweep_evt(&mut p);
        assert!(t.back_deadline().is_some(), "失配 ACK 不解槽");
        // 在席槽（w1）到点 → 摘 w1；w2 存活。
        p.step(600);
        let now = p.now_ns();
        t.on_back_tick(&mut p, now);
        let s = table_get(&mut t, &mut p);
        assert_eq!((s.count, focused_id(&s)), (1, Some(w2)));
    }

    #[test]
    fn back_pending_cleared_by_table_change() {
        let (mut t, mut p) = mk_table();
        let (w1, _, Some(srv1)) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false)
        else {
            panic!("应建联事件通道")
        };
        assert_eq!(back_req(&mut t, &mut p), gp::ST_OK);
        // 优雅退出序（设计 §3 R3）：属主自 DESTROY（表变更）→ 兜底解除。
        assert_eq!(destroy_req(&mut t, &mut p, w1), gp::ST_OK);
        assert!(t.back_deadline().is_none(), "表变更解除兜底（无表变更条件）");
        p.step(600);
        let now = p.now_ns();
        t.on_back_tick(&mut p, now);
        assert_eq!(table_get(&mut t, &mut p).count, 0);
        assert!(p.closed.contains(&srv1));
        // 已摘窗的迟到 ACK 静默忽略（无窗可匹配）。
        let mut ack = [0u8; 1];
        let _ = gp::encode_wm_back_ack(&mut ack);
        p.inject(srv1, &ack);
        t.sweep_evt(&mut p);
    }

    // —— 属主事件通道建联/关闭 + 表变更广播 ——

    #[test]
    fn create_gift_and_destroy_closes_owner_channel() {
        let (mut t, mut p) = mk_table();
        let (w1, cli, srv) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        let (cli, srv) = (cli.unwrap(), srv.unwrap());
        // gift 面共识：ack 随行端 = channel_create 客户端半。
        assert_eq!(cli, 200, "首建窗客户端端");
        assert_eq!(srv, 100, "首建窗 wmd 侧端");
        // 建窗即广播 WM_EVENT_TABLE(version)。
        let evts = p.events_on(srv);
        assert_eq!(evts.len(), 1, "CREATE 广播恰一帧");
        assert_eq!(
            gp::parse_wm_evt_table(evts[0]),
            Some(table_get(&mut t, &mut p).version)
        );
        // DESTROY 关联关闭。
        assert_eq!(destroy_req(&mut t, &mut p, w1), gp::ST_OK);
        assert!(p.closed.contains(&srv));
    }

    #[test]
    fn broadcast_dead_owner_degrades_to_legacy() {
        let (mut t, mut p) = mk_table();
        let (w1, _, Some(srv1)) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false)
        else {
            panic!("应建联事件通道")
        };
        let (_w2, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        // w1 通道病态（余错码）：下一次广播关闭拆除 + 属主降级。
        p.dead.push(srv1);
        let (w3, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        assert!(p.closed.contains(&srv1), "病态通道关闭拆除");
        // 降级属主 = 无通道窗：BACK 直接 DESTROY。
        assert_eq!(focus_req(&mut t, &mut p, w1), gp::ST_OK);
        assert_eq!(back_req(&mut t, &mut p), gp::ST_OK);
        let s = table_get(&mut t, &mut p);
        assert_eq!(s.count, 2, "w2/w3 存活");
        assert_eq!(focused_id(&s), Some(w3), "摘 w1 后 refocus 回表序最末可见窗");
    }

    // —— 无窗订阅者（OP_WM_SUBSCRIBE——guifbd 表快照通知面） ——

    #[test]
    fn subscribe_broadcast_and_cap() {
        let (mut t, mut p) = mk_table();
        // 订阅：ack 1B OK + 随行恰 1 订阅端。
        let mut f = [0u8; 8];
        let n = gp::encode_wm_subscribe_req(&mut f);
        let re = wm(&mut t, &mut p, CH_WM, &f[..n]);
        let (ack, hs) = re.gifted.expect("SUBSCRIBE gift 应答");
        assert_eq!(ack, vec![gp::ST_OK]);
        assert_eq!(hs.len(), 1);
        let sub_cli = hs[0];
        let sub_srv = p.created[0].0;
        // 表变更广播到达订阅端（与属主端同帧同版本）。
        let (_w1, _, Some(srv1)) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false)
        else {
            panic!("应建联事件通道")
        };
        let ver = table_get(&mut t, &mut p).version;
        assert_eq!(gp::parse_wm_evt_table(p.events_on(sub_srv)[0]), Some(ver));
        assert_eq!(gp::parse_wm_evt_table(p.events_on(srv1)[0]), Some(ver));
        // 满即丢：订阅端不排空 → 广播丢弃不炸（真值恒经 TABLE_GET）。
        p.full.push(sub_srv);
        let (_w2, _, _) = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        let ver2 = table_get(&mut t, &mut p).version;
        assert_eq!(
            gp::parse_wm_evt_table(p.events_on(sub_srv)[1]),
            Some(ver2),
            "满帧也先记录再丢"
        );
        // 帽满 ST_FULL（4 路）。
        for i in 0..WM_EVT_SUBS_MAX - 1 {
            let re = wm(&mut t, &mut p, CH_WM2, &f[..n]);
            assert_eq!(re.gifted.expect("帽内订阅应答").1.len(), 1, "第 {} 路", i);
        }
        let re = wm(&mut t, &mut p, CH_WM, &f[..n]);
        assert_eq!(re_st(&re), gp::ST_FULL, "订阅帽满");
        // 随行 handle 拒面（wm 域零随行执法）。
        let mut bad = vec![0u8; 4];
        bad[0] = gp::OP_WM_SUBSCRIBE;
        let (r0, c0) = (p.replies.len(), p.closed.len());
        t.handle_wm(&mut p, CH_WM, &bad, &[sub_cli]);
        assert_eq!(p.replies[r0].1, vec![gp::ST_BAD]);
        assert_eq!(p.closed.len(), c0 + 1, "随行 close");
    }

    // —— TABLE_WAIT 冻结语义钉板（① 波3设计 §9 立法） ——

    #[test]
    fn table_wait_frozen_single_slot_semantics() {
        let (mut t, mut p) = mk_table();
        let ver0 = table_get(&mut t, &mut p).version;
        // 同版本 park：零应答（inputd 驻留面）。
        let mut f = [0u8; 8];
        let n = gp::encode_wm_table_wait_req(&mut f, ver0);
        let r0 = p.replies.len();
        let _ = wm(&mut t, &mut p, CH_PUSH, &f[..n]);
        assert_eq!(p.replies.len(), r0, "park 无即答");
        // 后到 WAIT 现答现快照覆盖 parked 不排队（位移者立即答）。
        let _ = wm(&mut t, &mut p, CH_WM2, &f[..n]);
        assert_eq!(p.replies.len(), r0 + 1, "位移者立即答");
        assert_eq!(p.replies[r0].0, CH_PUSH, "被位移的是旧 parked");
        // 表变更 → 当前 parked（CH_WM2）先于建窗 ack 收全表快照并清空。
        let r1 = p.replies.len();
        let _ = create_win(&mut t, &mut p, gp::WM_LAYER_APP, 0, 600, false);
        let parked_rep = p.replies[r1..].iter().find(|(h, _)| *h == CH_WM2).expect("parked 应答");
        match gp::parse_wm_snapshot_rep(&parked_rep.1) {
            Some(Ok(s)) => assert_eq!(s.count, 1),
            other => panic!("快照应答: {:?}", other.map(|r| r.err())),
        }
    }

    #[test]
    fn unknown_wm_op_still_bad() {
        let (mut t, mut p) = mk_table();
        let mut f = [0u8; 8];
        f[0] = 0x69; // 未定形 wm op
        assert_eq!(re_st(&wm(&mut t, &mut p, CH_WM, &f)), gp::ST_BAD);
        // TABLE_WAIT 保留位不拒（前向兼容锚）——version ≠ last_seen 即答快照。
        f[0] = gp::OP_WM_TABLE_WAIT;
        f[1] = 0xEE;
        let re = wm(&mut t, &mut p, CH_WM, &f);
        assert!(gp::parse_wm_snapshot_rep(&re.plain.expect("即答快照")).is_some());
    }
}
