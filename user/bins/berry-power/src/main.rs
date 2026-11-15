//! berry-power —— bin 壳（main 循环骨架）。
//!
//! 本件 = 调度彻底计划裁九落件（docs/调度彻底计划-v1.2.md §3 裁九/
//! §4.1 CP-08）：berry-power 服务本体的循环骨架。四件纯面（帽聚合点/
//! 充电限流票仲裁/util EWMA 消费位/pwproto 帧会话机）全在 lib（host 单测
//! 全盖，`cargo test -p berry-power`）；bin 只组装主循环形态。
//!
//! ## 留窗表（真实接线全留窗——如实注，不做假接线）
//!
//! - **窗 1（常驻挂接）**：装配半边已挂（CP-08 装配件，04 号复核 §6-5：
//!   `bin/berry-power` rootfs 条目 + build-user musl 段〔--features
//!   wired〕）；**svcmgr 预拉 arm 留窗**——svcmgr 平像满载（.bss 终点
//!   0x1dff0 距内核钉位 .pager_entry@0x1e000 仅 0x10，arm 最小面 ~0x1a0
//!   装不进，链接期 loud 炸响实证；扩容走 kernel 面 128KiB D3 帽/pager
//!   钉位——kernel/src/main.rs:1621 同值双改），扩容刀挂 arm 补消费。
//!   gift 交付位（通道端点/env 块 extras）随分端执法刀（窗 2 的 F2 终裁
//!   落地件）立法。当前 bin 形态 = 起机自检（下方 dry-run，引擎活面单轮
//!   走通）后 park 常驻（rootfs 在册未被拉起 = 零消费零行为变化）。
//! - **窗 2（真实通道）**：fd0 = sim 帧通道 RECV 端（task_simcon 转发
//!   UART1 字节流）、gift = ACK 9B 回程 SEND 端——recv 阻塞形态照
//!   simbattery `recv_blocking`（`berry_channel_recv` 空 -EAGAIN →
//!   `wait_one` READABLE 1h 长帽重臂——P6 A-3：服务端 wait 超时是心跳
//!   不是死线）；ACK 回写 `send_retry` 同形（-EBUSY → wait WRITABLE
//!   10s 帽）。**F2 终裁（04 号复核 §6-4）：sim 流按设备分端——本服务
//!   份额 = `DEV_MODEM` 遥测**（`DEV_BATTERY` 归 simbattery、
//!   `DEV_THERMAL` 归 berry-thermal，不主张全流消费权——双消费者分帧
//!   错乱的裁决答案；分端执法归通道拓扑接线刀）；真机相 battery 策略
//!   输入 = 电池驱动自报经同一会话机（lib `session` 面）。骨架以合成帧
//!   流代源（`synth_wire`——loopback 测试面，非 sim 流消费权主张）。
//! - **窗 3（util 通道）**：CP-08 共享环/快照 VMO ABI（形态待裁——
//!   PM-15 §4.1 六面零新 syscall 裁决在册）；schedutil 同构消费已就位
//!   （lib `util` 面）。CP-08/CP-10 验收线的内核侧两断言（调度路径零
//!   频率写点 / PM 崩溃内核保频继续跑）归 kernel/sched 面（owner 占用，
//!   本刀零碰）。
//! - **窗 4（内核聚合点写面）**：ADR #66 ②——频率帽汇入 berry-power 同
//!   一内核聚合点取 min = **唯一合法帽书写面**（帽不新开通道；本服务
//!   `FreqCapAgg::aggregate` 是其纯函数形态）。真实写通道（对内核
//!   DVFS/聚合点的写请求 op）随内核聚合点 ABI 立法挂 CP-08/CP-10 刀。
//! - **窗 5（chargerd 契约位）**：ADR #68 `CHG_OP_VOTE_LIMIT{limit_pct,
//!   reason:u8}` 的票持有/动态票主/断线自动撤票通道面 = chargerd 服务
//!   本体（立法件无代码）；lib `ChgArbiter` = 它将消费的仲裁数学单源。
//!   **F1 终裁（04 号复核 §6-3）：票面单位 = mA**（#66 电流版终态形态；
//!   #68 pct 面 = 冻结快照，经设计容量桥兼容读写——lib `charge`
//!   `pct_of_ma`/`ma_of_pct`/`vote_pct`，协议升版携 mA 时线面改口）。
//! - **窗 6（记账消费闭环）**：内核唤醒记账（KC 面——`docs/内核唤醒记账
//!   设计-v1.0.md`「实施随 berry-power 刀」）与挂起双钟 SLEPT（ADR #67
//!   ②——维护窗判定/idle 世代/能耗归因口径）的消费闭环，挂 PM 线后续刀。
//!
//! 骨架 dry-run（下方 `main`）：合成三设备帧流 + 一窗 util 账走通四件面，
//! 末尾一行诊断（**console 行只作诊断、非 smoke 标记**——判据走通道，
//! 雷册 §A145 口径；真实验收 = lib host 单测 + 挂接后的通道断言）。

use berry_power::{
    CoreEvent, PolicyParams, PowerCore, CAP_SRC_SCHEDHINT, CHG_REASON_USER, CHG_SLOT_USER,
};
use berry_pwproto::{
    encode_ack, encode_battery, encode_modem, encode_thermal, BatterySnapshot, ModemSnapshot,
    ThermalSensor, ThermalSnapshot, ThermalTrip, ACK_LEN, ST_OK, THERMAL_SENSORS_MAX,
    THERMAL_TRIPS_MAX,
};

/// 合成帧源：三设备各一帧（健康电池 → 深亏电电池 → 热档 → modem 尾能），
/// seq 递进——窗 2 真实通道的 loopback 代源。
fn synth_wire(out: &mut Vec<u8>) {
    let batt = |pct: u8| BatterySnapshot {
        present: 1,
        ac_online: 1,
        status: 2,
        health: 3,
        capacity_pct: pct,
        voltage_mv: 3990,
        temp_mc: 28_500,
        current_now_ua: 412_000,
        current_avg_ua: 395_000,
        charge_full_uah: 4_800_000,
        charge_counter_uah: 3_696_000,
        cycle_count: 143,
        timestamp_ns: 42,
        measured: 1,
    };
    let thermal = ThermalSnapshot {
        sensor_count: 1,
        sensors: [ThermalSensor {
            kind: berry_pwproto::THERMAL_KIND_REAL,
            temp_mc: 31_200,
            trip_count: 0,
            trips: [ThermalTrip { level: 0, threshold_mc: 0, clr_mc: 0 }; THERMAL_TRIPS_MAX],
        }; THERMAL_SENSORS_MAX],
        level_hint: 1,
    };
    let modem = ModemSnapshot { rrc_state: 5, tail_ms: 8_000, signal_tier: 2, in_maint_window: 0 };

    let mut buf = [0u8; 128];
    let mut seq = 0u16;
    seq += 1;
    let n = encode_battery(&mut buf, seq, &batt(77), 0);
    push_frame(out, &mut buf, n);
    seq += 1;
    let n = encode_battery(&mut buf, seq, &batt(8), 0); // 深亏电：低电帽+预充票双触发
    push_frame(out, &mut buf, n);
    seq += 1;
    let n = encode_thermal(&mut buf, seq, &thermal, 1);
    push_frame(out, &mut buf, n);
    seq += 1;
    let n = encode_modem(&mut buf, seq, &modem, 0);
    push_frame(out, &mut buf, n);
}

/// 一帧入流（编码 None = 构造性非法——合成源常量全合法，防御面丢弃）。
fn push_frame(out: &mut Vec<u8>, buf: &mut [u8], n: Option<usize>) {
    if let Some(n) = n {
        out.extend_from_slice(&buf[..n]);
    }
}

fn main() {
    let mut core = PowerCore::new(PolicyParams::default_sim());

    // —— 主循环骨架：recv(字节) → 会话/策略 → ACK 回写 → 裁决消费 ——
    // 真实形态（窗 1/2）：`recv_blocking(fd0)` 长帽重臂取字节；此处以
    // 合成帧流代源走同一逐字节入口（`PowerCore::push_byte`）。
    let mut wire = Vec::new();
    synth_wire(&mut wire);
    let (mut applied, mut rejected, mut acks) = (0u32, 0u32, 0u32);
    for &b in &wire {
        match core.push_byte(b) {
            CoreEvent::Idle => {}
            CoreEvent::Applied { dev, seq, verdict: _ } => {
                applied += 1;
                // ACK 回写半边（窗 2 真实形态 = gift SEND 端 send_retry）：
                // 骨架只编不计——9B 编码失败即计数面异常（合成帧构造性合法）。
                let mut ack = [0u8; ACK_LEN];
                if encode_ack(&mut ack, dev, seq, ST_OK).is_some() {
                    acks += 1;
                }
                // 裁决消费面（窗 4/5）：verdict.cap/chg/freq 的真实写通道
                // 留窗——内核聚合点写面 / chargerd 契约位。
            }
            CoreEvent::Rejected { .. } => rejected += 1,
        }
    }

    // —— util 输入口骨架（窗 3 真实通道 = CP-08 环/快照 VMO ABI）：一窗
    // 满忙账（accum == elapsed → util_raw = 1024 → EWMA 128）。
    core.ingest_util_window(0, 625_000, 625_000);

    // —— 外部输入面骨架：调度 hint 帽（#42 系统态叠加票词汇）与用户帽
    // （设置面——#68 冻结形 pct 票，经 F1 桥兼容投递换 mA 落票；换算
    // 单源字段 = 快照自带设计容量）各投一票——真实投递通道留窗（帽不
    // 新开通道，ADR #66 ②：外源帽只经聚合点源席投递）。
    let cap_uah = core.last_batt.map(|b| b.charge_full_uah).unwrap_or(4_800_000);
    let _ = core.caps.set(CAP_SRC_SCHEDHINT, Some(400_000));
    let _ = core.votes.vote_pct(CHG_SLOT_USER, 80, CHG_REASON_USER, cap_uah);

    let v = core.decide();
    let cap = match v.cap {
        Some(c) => format!("{}kHz@src{}", c.cap_khz, c.src),
        None => "none".to_string(),
    };
    // F1 兼容读演示：mA 真相面 → #68 pct 冻结形线值（同容量换算桥）。
    let compat_pct = v
        .chg
        .limit_pct(cap_uah)
        .map(|p| p.to_string())
        .unwrap_or_else(|| "n/a".to_string());
    // 诊断行（非 smoke 标记——见模块注窗表尾注）。
    println!(
        "berry-power: skeleton dry-run applied={} rejected={} acks={} cap={} chg={}mA(r{};compat {}pct@{}uah) freq req={}khz eff={}khz",
        applied, rejected, acks, cap, v.chg.limit_ma, v.chg.reason, compat_pct, cap_uah,
        v.freq.req_khz, v.freq.eff_khz
    );
    // CP-08 装配形态：自检（上方 dry-run = 引擎活面单轮走通）后 park 常驻
    // ——真实通道（窗 2 recv 长帽重臂）接线前，零产出动作零唤醒（被拉起
    // 也不写任何帽/票/发布位；berry-thermal park 壳同款骨架相）。
    berry_libc::park();
}
