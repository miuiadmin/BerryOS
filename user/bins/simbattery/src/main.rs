//! simbattery —— sim-S1 电池快照注入服务（M2-b，`docs/sim设备通道与契约
//! 设计-v1.1.md` §6）。
//!
//! 由 svcmgr rootfs 相预拉（门 = 表 0x0001_0005 在席 = boot 侧 fw_cfg
//! `opt/berry/simdev` 门开）：fd0 = 帧通道 RECV 端（task_simcon 转发的
//! UART1 字节流）、GIFT_H[0] = ACK 通道 SEND 端（`sim_ack_h()`，9B 回程）、
//! stdout = console 别名。契约单源 = berry-pwproto（M1）；服务只持 seq
//! 状态机与标记，零设备知识（案 C「纯 stdin/stdout bin」）。
//!
//! 主循环走 **echo 形直收**（`berry_channel_recv` → 空 -EAGAIN → `wait_one`
//! READABLE 长帽重臂 → 重收），不经 libc `read(0)`——v1.1 修正④：libc
//! `read(0)` 的 10s 空闲帽 EOF 化返 0（feeder 断供判定的协议面），与 §6
//! 「无注入 = 永睡零唤醒」冲突；直收 + 1h 长帽重臂 = 观察窗内零空闲唤醒
//! （帽到点重臂不自杀，P6 A-3 改板同款先例）。
//!
//! 标记纪律（A60）：`SIMBAT ok seq=N\n` 整行一条 channel 消息（console 单
//! 写者拼行）；**仅 ST_OK 打**——BAD/STALE 无 SIMBAT 行（§8 验收：坏值帧 =
//! ACK BAD + 无标记；重放帧 = ACK STALE + 无标记）。panic = abort = park
//! → 标记缺失兜底 + 宿主 client 超时对拍双兜底（G-eco 家族惯例）。

use berry_libc::io::{console_write, sim_ack_h, stdin_h};
use berry_pwproto::{encode_ack, StreamDecoder, Step, DEV_BATTERY, ST_BAD, ST_OK, ST_STALE};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_object_wait_one, Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, SIGNAL_READABLE,
    SIGNAL_WRITABLE,
};

/// 空闲等待帽（1h）：「永睡」的工程近似——帽到点重臂（零标记零 ACK，一次/h
/// 的 wait 返程在 10s 粒度 wake 记账面下即零唤醒）。
const IDLE_TIMEOUT_NS: u64 = 3_600_000_000_000;

/// ACK 回写等待帽（echo 同款 10s：真依赖面——task_simcon 每醒排空 ack 端，
/// 持续满/超时 = 转发任务断 = loud）。
const IO_TIMEOUT_NS: u64 = 10_000_000_000;

/// 帧字节接收缓冲（≥ CHAN_MSG_MAX：一条消息整收不残段；帧帽 98B）。
const RECV_BUF: usize = 256;

fn main() {
    let stdin = stdin_h();
    let ack = sim_ack_h();
    if stdin == 0 || ack == 0 {
        // 未交付形态（svcmgr 门控 spawn 下不可达；bsh 手跑/裸相才见）：
        // 单行诊断 + park（零 SIMBAT 标记扰动，GUI-8 launcher 降级同款）。
        eprintln!("simbattery: no sim gifts (stdin={} ack={})", stdin, ack);
        berry_libc::park();
    }
    let mut dec = StreamDecoder::new();
    let mut last_seq: Option<u16> = None; // seq 状态机（§5 防重放，服务自持）
    let mut buf = [0u8; RECV_BUF];
    loop {
        let n = recv_blocking(stdin, &mut buf);
        for &b in &buf[..n] {
            match dec.push(b) {
                Step::NeedMore => {}
                Step::Frame(f) => {
                    let st = if f.dev != DEV_BATTERY {
                        // 帧合法但非本服务设备（thermal/modem = sim-S2/S3 面，
                        // 走各自服务）：拒 BAD；last_seq 只记 battery 序。
                        ST_BAD
                    } else if last_seq.map_or(true, |l| f.seq > l) {
                        last_seq = Some(f.seq);
                        ST_OK
                    } else {
                        ST_STALE // seq 回退/重放：不应用、无标记（§5）
                    };
                    send_ack(ack, f.dev, f.seq, st);
                    if st == ST_OK {
                        print_ok(f.seq);
                    }
                }
                // 头合法帧收齐但语义拒（ver/值域）：显式 BAD——「没收到」
                // （无 ACK）与「收到但拒」线上可分（§5）。
                Step::Bad { dev, seq } => send_ack(ack, dev, seq, ST_BAD),
            }
        }
    }
}

/// 收一件通道消息（echo `recv_blocking` 同形）：空 -EAGAIN → wait
/// READABLE（1h 长帽）→ 重收；醒来重验是 wait 面纪律，超时 = 重臂续睡
/// （安静是合法态）。其余错码 panic（真链断 loud）。零长消息（EOF 形态）
/// = 本链协议外（task_simcon 只发帧字节），跳过续等。
fn recv_blocking(h: Handle, buf: &mut [u8]) -> usize {
    loop {
        let mut nb = buf.len();
        let mut nh = 0usize;
        let s = berry_channel_recv(h, buf.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            if nb > 0 {
                return nb;
            }
            continue; // 零长 = EOF 形态：续等（见函数注）
        }
        if s != BERRY_ERR_AGAIN {
            panic!("simbattery: recv err {}", s);
        }
        let mut obs = 0u64;
        let w = berry_object_wait_one(h, SIGNAL_READABLE, now() + IDLE_TIMEOUT_NS, &mut obs);
        if w != 0 && w != BERRY_ERR_TIMEDOUT {
            panic!("simbattery: wait err {}", w);
        }
    }
}

/// ACK 9B 回写（echo `send_retry` 同形）：满 -EBUSY → wait WRITABLE
/// （10s 帽）腾位重试；其余错码/超时 panic——ACK 丢失由宿主 client 超时
/// 对拍暴露，但通道面断链要 loud（标记缺失 + client 双兜底）。
fn send_ack(h: Handle, dev: u8, seq: u16, st: u8) {
    let mut out = [0u8; 9]; // ACK_LEN（pwproto 冻结值）
    let n = encode_ack(&mut out, dev, seq, st).expect("ack out 9B");
    loop {
        let s = berry_channel_send(h, out.as_ptr(), n, core::ptr::null(), 0);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            panic!("simbattery: ack send err {}", s);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, now() + IO_TIMEOUT_NS, &mut obs) != 0 {
            panic!("simbattery: ack wait timeout");
        }
    }
}

/// `SIMBAT ok seq=N\n` 单消息标记（A60：整行一条 channel 消息——手工拼行
/// 一次 `console_write`，不走 fmt/分片；与现役标记互非子串，§6）。
fn print_ok(seq: u16) {
    const PREFIX: &[u8] = b"SIMBAT ok seq="; // 14B
    let mut line = [0u8; PREFIX.len() + 5 + 1]; // +u16 十进制 ≤5 位 + '\n'
    line[..PREFIX.len()].copy_from_slice(PREFIX);
    let mut i = PREFIX.len();
    let mut digits = [0u8; 5];
    let mut nd = 0usize;
    let mut v = seq as u32;
    loop {
        digits[nd] = b'0' + (v % 10) as u8;
        nd += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    while nd > 0 {
        nd -= 1;
        line[i] = digits[nd];
        i += 1;
    }
    line[i] = b'\n';
    i += 1;
    console_write(&line[..i]);
}

/// now = clock_read（失败 panic——时钟链不在容错面，echo 同款）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        panic!("simbattery: clock");
    }
    t
}
