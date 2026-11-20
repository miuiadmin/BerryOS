//! C-3 段②：berryctl v0——系统控制/查询 CLI 雏形（shell设计 §2 开发
//! 梯队；AI 友好面 `--json` 自 trace 起逐件落地〔环读口刀 day-one 首件
//! ——schema add-only〕）。
//!
//! 四 RPC 子命令 → psproto 三 op（严格串行 RPC，ps bin 同链拓扑：svcmgr
//! 预建查询对，本进程持 ps_b——env 块 gift[2]，libc `gift2_h()`）：
//! - `ver`：`ver <串>`（svcmgr 编译期版本标识）；
//! - `ps`：拉起账本全表，行面与 ps bin 同（`{name} alive|exited(c)|spawnfail`）；
//! - `mem`：`mem vmos=<n> bytes=<t>`（svcmgr 经既有 vmo_get_size 计数
//!   自己可见的 VMO——内核遥测读 syscall 不新增）。
//! - `power`（桌面UX刀D）：**非 RPC 面**早分支——内核 `power_off`（0x88，
//!   PSCI SYSTEM_OFF 经 HVC）直调；成功 = 整机退出不归（无 gift 拓扑
//!   （bsh 手跑档）亦可调）；失败 stderr 一行 + exit 1（无 PSCI 机器
//!   诚实降级——宿主工具栏 OFF 仍是唯一关机路）。
//! - `trace`（环读口刀）：**非 RPC 面**早分支（power 先例位）——内核
//!   0x84 op=2 双族环快照直调（berry_sys 薄包装）：默认人读两行
//!   （switches/wakes——total/drop/per-cpu/kind/reason/src/延迟三分位）
//!   ＋`--json`（纯净 stdout 单对象、诊断 stderr——shell设计 §3 铁律
//!   首件）；降级段行内 degraded 标记（TCG 降级频度观测仪器——
//!   powertop/trace 梯队首块）。不进 smoke boot 序列（零标记面）。
//! - `powertop`（槽表读口刀）：**非 RPC 面**早分支（trace 先例位）——内核
//!   0x84 op=3 WakeSrc 槽表纯镜像直调（berry_sys 薄包装第四件）。「谁在
//!   挡住深睡」v1 判据＝频度×浪费率×源排行：默认单快照累计形（wakes
//!   降序源排行＋no_waiter/irq_count 浪费率锚＋安静时长＋ack 延迟单点
//!   代理诚实降档）；`-d <ms>` 差分形（`berry_thread_sleep` 真睡两帧逐
//!   槽饱和差分——区间 Δwakes/Δt 频度，头 now_vct 差自证区间锚；测量
//!   自身 sleep 超时醒＝TIMER +1 如实注记）；`--json`（day-one 第二件：
//!   mode 条件域两层、raw 单调计数＋因子出口、率/排名/合成指数归人读
//!   形——shell设计 §3 铁律）。不进 smoke boot 序列（零标记面）。
//!
//! 失败面（无 gift / RPC 帽尽 / 畸形应答 / 未知子命令）stderr 一行诊断
//! + exit 1——标记缺失 = smoke FAIL 兜底（链断 loud）。v0 边界：bsh
//! `run` 拉起档无 ps_b gift（gift 机制是 spawn 侧的）——「no svcmgr
//! gift」退 1 是诚实降级，服务发现式拉起留 v2。

use std::process::exit;

use berry_libc::io::gift2_h;
use berry_psproto::{
    encode_request_op, parse_mem_reply, parse_reply, parse_ver_reply, OP_MEM, OP_PS_LIST, OP_VER,
    STATE_ALIVE, STATE_EXITED,
};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_object_wait_one, Handle, BERRY_ERR_AGAIN, BERRY_ERR_BUSY, CLOCK_MONOTONIC,
    SIGNAL_READABLE, SIGNAL_WRITABLE,
};

// 链接种子（stdhello 同款）：无引用则 berry-libc crt0（含 `_start`）不进
// 链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 全帽（发 + 收各一）：30s——svcmgr 单线程顺序走到自验才服务本请求
/// （慢而活着不误杀，ps 同档；真挂死交帽尽 loud）。
const RPC_CAP_NS: u64 = 30 * 1_000_000_000;

/// now = clock_read（失败按 0 → deadline 0 = 立即超时，活证死路 loud）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

// ---- `trace` 子命令（环读口刀雏形——U1-U3）----
//
// 快照布局（内核 0x84 op=2 契约；头/段解析经 berry-sys re-export 的
// telface 编解码纯函数单源）：32B 头 + 每环 24B 段描述符 + 变长载荷
//（CPU 序段序 switch c0→c1→wake c0→c1、段内旧→新）。条目按 telface
// host 钉板偏移 LE 手解（SwitchRec 24B：vct@0/prev@8/next@12/kind@16/
// idle_next@17；WakeRec 32B：vct@0/since_vct@8/wait_obj@16/task_id@24/
// reason@28/source@29）。
//
// 观测职能（本刀最大风险「TCG 高切换流下降级频度」的实跑仪器）：
// 降级段（total>0 ∧ count==0）行内 degraded 标记；丢包 = 每环
// total − min(total, ring_len)（轮转淘汰派生，零热路径成本）。

/// 条目解码小件：LE 定宽读（钉板偏移手解——telface host 测试即单源）。
fn le_u32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
fn le_u64(b: &[u8], o: usize) -> u64 {
    u64::from_le_bytes(b[o..o + 8].try_into().unwrap())
}

/// 单段聚合（族内条目扫描一遍——人读形与 --json 同一具）。
struct SegAgg {
    seg: berry_sys::RingSegV1,
    /// switch 族：kind 四计数（Y/P/E/B——kind 域 0..=3）。
    kinds: [u64; 4],
    /// wake 族：reason 两计数（S/T）。
    reasons: [u64; 2],
    /// wake 族：延迟（vct−since_vct，cycles）全样本（分位用）。
    lat: Vec<u64>,
    /// wake 族：source 计数（值直，≤ count 项）。
    srcs: Vec<(u8, u64)>,
}

/// 分位（最近秩 ceil(p·m/100)，样本升序）——样本级精确秩（环内是明细
/// 条目非直方图；BENCHWAKE 桶上界形的样本版）。
fn percentile_ns(sorted: &[u64], p_num: u64, p_den: u64, cntfrq: u64) -> Option<u64> {
    if sorted.is_empty() || cntfrq == 0 {
        return None;
    }
    let m = sorted.len() as u64;
    let rank = (m * p_num + p_den - 1) / p_den; // ceil——≥1
    let idx = (rank - 1).min(m - 1) as usize;
    let ns = (sorted[idx] as u128) * 1_000_000_000u128 / cntfrq as u128;
    Some(ns.min(u64::MAX as u128) as u64)
}

fn trace_cmd(json: bool) {
    use berry_sys::{ring_seg_read, rings_hdr_read, RingSegV1, RING_FAMILY_SWITCH};

    // 满载 7296B（内核 RING_SNAP_STAGE 同容量；变长填充下实写由返回值
    // 自证——truncate 到 n 后按段自描述走读）。
    let mut buf = vec![0u8; berry_sys::TELEMETRY_RINGS_MAX]; // 跨刀 review：字面 7296 改单源（消双写链断节）
    let st = berry_sys::berry_telemetry_rings_snapshot(buf.as_mut_ptr(), buf.len());
    if st < 0 {
        eprintln!("berryctl: trace err {}", st);
        exit(1);
    }
    let n = st as usize;
    if n < 32 {
        eprintln!("berryctl: trace short snap n={n}");
        exit(1);
    }
    buf.truncate(n);
    let mut hb = [0u8; 32];
    hb.copy_from_slice(&buf[..32]);
    let hdr = rings_hdr_read(&hb);

    // 段自描述走读（消费方视角解包律——与内核 ktest 对拍臂同一序）。
    let mut aggs: Vec<SegAgg> = Vec::new();
    let mut cur = hdr.hdr_bytes as usize; // 跨刀 review：按头自述锚走读（powertop 同形）
        if cur < 32 { eprintln!("berryctl: trace hdr_bytes {} < 32", cur); std::process::exit(1); }
    while cur + 24 <= n {
        let mut d = [0u8; 24];
        d.copy_from_slice(&buf[cur..cur + 24]);
        let seg: RingSegV1 = ring_seg_read(&d);
        let bytes = seg.count as usize * seg.rec_size as usize;
        if cur + 24 + bytes > n {
            break; // 防御：帧尾不完整段弃（内核不会产——截尾在整条边界）
        }
        let mut a = SegAgg { seg, kinds: [0; 4], reasons: [0; 2], lat: Vec::new(), srcs: Vec::new() };
        let body = &buf[cur + 24..cur + 24 + bytes];
        if seg.family == RING_FAMILY_SWITCH {
            for i in 0..seg.count as usize {
                let o = i * 24;
                let kind = body[o + 16] as usize;
                if kind < 4 {
                    a.kinds[kind] += 1;
                }
            }
        } else {
            for i in 0..seg.count as usize {
                let o = i * 32;
                let reason = body[o + 28];
                let source = body[o + 29];
                if reason == 1 {
                    a.reasons[0] += 1;
                } else if reason == 2 {
                    a.reasons[1] += 1;
                }
                let lat = le_u64(body, o).saturating_sub(le_u64(body, o + 8));
                a.lat.push(lat);
                match a.srcs.iter_mut().find(|(s, _)| *s == source) {
                    Some((_, c)) => *c += 1,
                    None => a.srcs.push((source, 1)),
                }
            }
            a.lat.sort_unstable();
        }
        aggs.push(a);
        cur += 24 + bytes;
    }

    if json {
        // --json（shell设计 §3 铁律首件）：纯净 stdout 单对象；字段保守
        // 最小两层（schema add-only——首件即锚）；族内聚合域按 family
        // 可缺席（switch→kinds；wake→reasons/src_top/latency_ns）。
        let mut out = String::with_capacity(512);
        out.push_str("{\"version\":");
        out.push_str(&hdr.version.to_string());
        out.push_str(",\"cntfrq\":");
        out.push_str(&hdr.cntfrq.to_string());
        out.push_str(",\"ring_len\":");
        out.push_str(&hdr.ring_len.to_string());
        out.push_str(",\"ring_cpus\":");
        out.push_str(&hdr.ring_cpus.to_string());
        out.push_str(",\"segments\":[");
        for (i, a) in aggs.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            let s = &a.seg;
            out.push_str("{\"family\":");
            out.push_str(&s.family.to_string());
            out.push_str(",\"cpu\":");
            out.push_str(&s.cpu.to_string());
            out.push_str(",\"total\":");
            out.push_str(&s.total.to_string());
            out.push_str(",\"count\":");
            out.push_str(&s.count.to_string());
            out.push_str(",\"rec_size\":");
            out.push_str(&s.rec_size.to_string());
            if s.family == RING_FAMILY_SWITCH {
                out.push_str(",\"kinds\":[");
                for (j, k) in a.kinds.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    out.push_str(&k.to_string());
                }
                out.push(']');
            } else if s.count > 0 {
                out.push_str(",\"reasons\":[");
                for (j, r) in a.reasons.iter().enumerate() {
                    if j > 0 {
                        out.push(',');
                    }
                    out.push_str(&r.to_string());
                }
                out.push(']');
                if let Some((src, c)) = a.srcs.iter().max_by_key(|(_, c)| *c) {
                    out.push_str(",\"src_top\":{\"src\":");
                    out.push_str(&src.to_string());
                    out.push_str(",\"n\":");
                    out.push_str(&c.to_string());
                    out.push('}');
                }
                out.push_str(",\"latency_ns\":{");
                let mut first = true;
                for (name, num, den) in [("p50", 1u64, 2u64), ("p90", 9, 10), ("p99", 99, 100)] {
                    if let Some(v) = percentile_ns(&a.lat, num, den, hdr.cntfrq) {
                        if !first {
                            out.push(',');
                        }
                        first = false;
                        out.push_str("\"");
                        out.push_str(name);
                        out.push_str("\":");
                        out.push_str(&v.to_string());
                    }
                }
                out.push('}');
            }
            out.push('}');
        }
        out.push_str("]}");
        println!("{}", out);
        return;
    }

    // 默认人读形：单行紧凑键值两行（bsh uname/uptime 惯例族）。
    for family in [berry_sys::RING_FAMILY_SWITCH, berry_sys::RING_FAMILY_WAKE] {
        let fam_segs: Vec<&SegAgg> = aggs.iter().filter(|a| a.seg.family == family).collect();
        let total: u64 = fam_segs.iter().map(|a| a.seg.total).sum();
        // 丢包派生：每环 total − min(total, ring_len)（轮转淘汰数）。
        let drop: u64 = fam_segs
            .iter()
            .map(|a| a.seg.total.saturating_sub(a.seg.total.min(hdr.ring_len)))
            .sum();
        let mut line = String::with_capacity(96);
        if family == berry_sys::RING_FAMILY_SWITCH {
            line.push_str("switches");
        } else {
            line.push_str("wakes");
        }
        line.push_str(" total=");
        line.push_str(&total.to_string());
        line.push_str(" drop=");
        line.push_str(&drop.to_string());
        for a in &fam_segs {
            line.push_str(&format!(" c{}=", a.seg.cpu));
            if a.seg.count == 0 && a.seg.total > 0 {
                line.push_str("degraded"); // 读窗不稳诚实弃段（观测仪器职能）
            } else {
                line.push_str(&a.seg.count.to_string());
            }
        }
        if family == berry_sys::RING_FAMILY_SWITCH {
            line.push_str(&format!(
                " kinds Y={} P={} E={} B={}",
                fam_segs.iter().map(|a| a.kinds[0]).sum::<u64>(),
                fam_segs.iter().map(|a| a.kinds[1]).sum::<u64>(),
                fam_segs.iter().map(|a| a.kinds[2]).sum::<u64>(),
                fam_segs.iter().map(|a| a.kinds[3]).sum::<u64>()
            ));
        } else {
            line.push_str(&format!(
                " reasons S={} T={}",
                fam_segs.iter().map(|a| a.reasons[0]).sum::<u64>(),
                fam_segs.iter().map(|a| a.reasons[1]).sum::<u64>()
            ));
            // top source（并列取首见——确定性）。
            let mut all_srcs: Vec<(u8, u64)> = Vec::new();
            for a in &fam_segs {
                for (s, c) in &a.srcs {
                    match all_srcs.iter_mut().find(|(x, _)| x == s) {
                        Some((_, n)) => *n += c,
                        None => all_srcs.push((*s, *c)),
                    }
                }
            }
            if let Some((src, c)) = all_srcs.iter().max_by_key(|(_, c)| *c) {
                // wsrc=（非 src=）：与标记「ROOTFS ok src=」互非子串（canon
                // 机查裁决——新固定文案 vs 91 标记两两互非子串纪律）。
                line.push_str(&format!(" wsrc={}(n={})", src, c));
            }
            // 延迟三分位（跨段合并样本；ns = (vct−since_vct)×1e9/cntfrq
            //——BENCHWAKE 同数学用户态化）。
            let mut lat: Vec<u64> = fam_segs.iter().flat_map(|a| a.lat.iter().copied()).collect();
            lat.sort_unstable();
            for (name, num, den) in [("p50", 1u64, 2u64), ("p90", 9, 10), ("p99", 99, 100)] {
                if let Some(v) = percentile_ns(&lat, num, den, hdr.cntfrq) {
                    line.push_str(&format!(" {}={}ns", name, v));
                }
            }
        }
        println!("{}", line);
    }
}

// ---- `powertop` 子命令（槽表读口刀雏形——「谁在挡住深睡」观测面）----
//
// 取数 = 内核 0x84 op=3 WakeSrc 槽表全表纯镜像（40B 头 WsrcHdrV1＋
// 128×64B 行 WakeSrcRecV1，行号即源值；头/行解码经 berry-sys re-export
// 的 telface 纯函数单源，条目步进按头 rec_size 自述走读——条目偏移手解
// 不在本面）。活动槽谓词 = wakes>0||irq>0||no_waiter>0（user.rs 计数族
// 判例；first_ts≠MAX 禁用——漏 merge_hal 三槽与 PWRKEY 恰是要观测的无主
// 到达面）。
//
// v1 判据（谱裁决 9——频度×浪费率×源排行）：「深睡」v1 语义 = idle 深
// park 长驻留的打断（TCG 无 PSCI suspend/无 C-state/无 PMU——非 Linux
// 同名面）；频度主判据 = 区间 Δwakes/Δt（差分形；单快照形出 raw 累计
// 与源排行）；浪费率 = no_waiter/irq_count 到达锚 [0,1]（分母裁 irq_count
// 非 wakes——wakes 是广播乘数〔一次广播可醒任意多任务〕nw/wakes 无界
// 不可比；v1 真数面仅 UART_RX 良性竞态＋POWER_KEY 结构性无主两槽，TIMER
// 类浪费走环侧 WakeRec.reason==TIMEOUT 占比代理归 trace 面不复制）；排
// 行 = Δwakes 降序（单快照 wakes 降序）并列 irq 降序再并列 src 升序。
// 安静时长（now_vct−last_ts）与 ack 延迟（last_ts−last_ack_ts）= 展示
// 辅助单点代理（跨字段错位诚实降档「-」）。
//
// 差分 = berry_thread_sleep 真睡两帧（测量任务真睡让核 park、窗内醒源
// 直接归因＝powertop 本义；禁 clock 忙等——自抬 TIMER 到达计数污染被测
// 面）；区间分母 = 两帧头 now_vct 差自证（命令行标称值仅作睡眠 pacing
// 不作分母）；差分行集 = 两帧活动槽并集；测量自身贡献如实注记（本命令
// sleep 超时醒＝TIMER dw 含 +1 诚实可解释）。

/// `-d <ms>` 上帽（30s——RPC_CAP_NS 30s 判例；宿主等不起更长的诊断窗）。
const POWERTOP_D_CAP_MS: u64 = 30_000;

/// 源值→短名（本文件本地 8 槽表：0x01-0x06 on_wake 真源＋0x07 PWRKEY
/// 到达源＋0x7F UNKNOWN——名字映射归消费方公约，内核只出值；表外源
/// hex 兜底）。KBOOT=KERNEL_BOOT/UART=UART_RX/RESCHED·TLB=SGI×2。
fn wsrc_short_name(src: u8) -> Option<&'static str> {
    match src {
        0x01 => Some("KBOOT"),
        0x02 => Some("TIMER"),
        0x03 => Some("UART"),
        0x04 => Some("RESCHED"),
        0x05 => Some("TLB"),
        0x06 => Some("IPC"),
        0x07 => Some("PWRKEY"),
        0x7F => Some("UNK"),
        _ => None,
    }
}

/// vct→ms 换算（消费方派生；cntfrq==0 防御归 0——内核契约恒 >0）。
fn vct_ms(v: u64, cntfrq: u64) -> u64 {
    if cntfrq == 0 {
        return 0;
    }
    ((v as u128) * 1000 / cntfrq as u128).min(u64::MAX as u128) as u64
}

/// 一帧快照：头＋行区（下标即源值；行数 = min(头 slots, 实写余量整行数)）。
struct WsrcSnap {
    hdr: berry_sys::WsrcHdrV1,
    rows: Vec<berry_sys::WakeSrcRecV1>,
}

/// 单帧取数（trace 先例同形：st<0 / n<40 → stderr 一行 + exit 1）。行区
/// 自描述走读：起点 = 头 hdr_bytes、步进 = 头 rec_size（跨版本走读锚；v1
/// 不足 64B/几何越界 = 诚实拒——v1 行解码器只认 64B 镜像）。
fn wsrc_fetch() -> WsrcSnap {
    use berry_sys::{wsrc_hdr_read, wsrc_row_read};

    let mut buf = vec![0u8; berry_sys::TELEMETRY_WSRC_MAX];
    let st = berry_sys::berry_telemetry_wsrc_snapshot(buf.as_mut_ptr(), buf.len());
    if st < 0 {
        eprintln!("berryctl: powertop err {}", st);
        exit(1);
    }
    let n = st as usize;
    if n < 40 {
        eprintln!("berryctl: powertop short snap n={n}");
        exit(1);
    }
    let mut hb = [0u8; 40];
    hb.copy_from_slice(&buf[..40]);
    let hdr = wsrc_hdr_read(&hb);
    let start = hdr.hdr_bytes as usize;
    let stride = hdr.rec_size as usize;
    if start < 40 || stride < 64 || start > n {
        eprintln!("berryctl: powertop bad geom hb={} rs={}", hdr.hdr_bytes, hdr.rec_size);
        exit(1);
    }
    // v1 槽域 = 值域上界 0x7F+1 = 128（单源 wsrcface SLOTS 的 v1 钉值）。
    let rows_n = (hdr.slots as usize).min(128).min((n - start) / stride);
    let mut rows = Vec::with_capacity(rows_n);
    for i in 0..rows_n {
        let off = start + i * stride;
        let mut rb = [0u8; 64];
        rb.copy_from_slice(&buf[off..off + 64]);
        rows.push(wsrc_row_read(&rb));
    }
    WsrcSnap { hdr, rows }
}

/// 活动槽谓词（user.rs:2030 计数族判例——单调计数三元 OR）。
fn wsrc_active(r: &berry_sys::WakeSrcRecV1) -> bool {
    r.wakes > 0 || r.irq_count > 0 || r.no_waiter > 0
}

/// 行首「wsrc=0xXX(NAME)」/表外源「wsrc=0xXX」（wsrc= token 族——canon
/// 已过前缀；非「src=」——与标记「ROOTFS ok src=」互非子串纪律）。
fn wsrc_row_prefix(src: usize) -> String {
    let b = src as u8;
    match wsrc_short_name(b) {
        Some(name) => format!("wsrc=0x{:02X}({})", b, name),
        None => format!("wsrc=0x{:02X}", b),
    }
}

/// 人读单快照形：首行总账＋活动槽行（wakes 降序、并列 irq 降序、再并列
/// src 升序——确定性）。首行不出现「src=」「ok 」子串面（canon 机查门）。
fn powertop_human_cum(s: &WsrcSnap) {
    let cntfrq = s.hdr.cntfrq;
    let mut idx: Vec<usize> = (0..s.rows.len()).filter(|&i| wsrc_active(&s.rows[i])).collect();
    println!(
        "powertop acct={} cntfrq={} active={}/{} now={}ms",
        s.hdr.wake_acct_n,
        cntfrq,
        idx.len(),
        s.hdr.slots,
        vct_ms(s.hdr.now_vct, cntfrq)
    );
    idx.sort_by(|&a, &b| {
        let (ra, rb) = (&s.rows[a], &s.rows[b]);
        rb.wakes
            .cmp(&ra.wakes)
            .then(rb.irq_count.cmp(&ra.irq_count))
            .then(a.cmp(&b))
    });
    for i in idx {
        let r = &s.rows[i];
        let mut line = wsrc_row_prefix(i);
        line.push_str(&format!(" w={} i={} nw={}", r.wakes, r.irq_count, r.no_waiter));
        if r.irq_count > 0 {
            // 浪费率 = nw/irq 到达锚 [0,1]（跨字段错位瞬态可 >100%，夹 100
            // 诚实上限——结构上 nw≤irq，读窗错位除外）。
            let pct = ((r.no_waiter as u128) * 100 / r.irq_count as u128).min(100) as u64;
            line.push_str(&format!(" waste={pct}%"));
        }
        if r.first_ts == u64::MAX {
            line.push_str(" first=-"); // 哨兵＝未触（irq-only 槽常态）
        } else {
            line.push_str(&format!(" first={}ms", vct_ms(r.first_ts, cntfrq)));
        }
        if r.last_ts > 0 {
            line.push_str(&format!(" last={}ms", vct_ms(r.last_ts, cntfrq)));
        } else {
            line.push_str(" last=-");
        }
        // ack 延迟单点代理：仅 last_ack_ts>0 ∧ last_ts≥last_ack_ts 出
        //（跨字段错位诚实降档「-」）。
        if r.last_ack_ts > 0 && r.last_ts >= r.last_ack_ts {
            line.push_str(&format!(" ack={}ms", vct_ms(r.last_ts - r.last_ack_ts, cntfrq)));
        } else {
            line.push_str(" ack=-");
        }
        // 安静时长 = now_vct−last_ts（装配序保证 ≥0；错位防御饱和减）。
        if r.last_ts > 0 {
            line.push_str(&format!(" idle={}ms", vct_ms(s.hdr.now_vct.saturating_sub(r.last_ts), cntfrq)));
        } else {
            line.push_str(" idle=-");
        }
        println!("{}", line);
    }
    // TIMER 类浪费 v1 无槽级面——环侧 TIMEOUT 占比代理归 trace 面注记。
    println!("# waste=nw/irq 到达锚：v1 真数面仅 UART/PWRKEY；TIMER 类走 trace 环侧 TIMEOUT 代理");
}

/// 人读差分形：首行区间锚＋两帧活动槽并集行（Δwakes 降序、并列 Δirq
/// 降序、再并列 src 升序）；列同形换 dw=/dirq=/dnw=＋区间率 wake/s；帧 2
/// last_ts 单点随行。测量自身贡献如实注记一行。
fn powertop_human_diff(s1: &WsrcSnap, s2: &WsrcSnap, interval_vct: u64) {
    let cntfrq = s2.hdr.cntfrq;
    let n = s1.rows.len().min(s2.rows.len());
    let mut idx: Vec<usize> = (0..n)
        .filter(|&i| wsrc_active(&s1.rows[i]) || wsrc_active(&s2.rows[i]))
        .collect();
    let dacct = s2.hdr.wake_acct_n.saturating_sub(s1.hdr.wake_acct_n);
    println!(
        "powertop diff dacct={} cntfrq={} active={}/{} interval={}ms",
        dacct,
        cntfrq,
        idx.len(),
        s2.hdr.slots,
        vct_ms(interval_vct, cntfrq)
    );
    let dw = |i: usize| s2.rows[i].wakes.saturating_sub(s1.rows[i].wakes);
    let dirq = |i: usize| s2.rows[i].irq_count.saturating_sub(s1.rows[i].irq_count);
    idx.sort_by(|&a, &b| {
        dw(b).cmp(&dw(a)).then(dirq(b).cmp(&dirq(a))).then(a.cmp(&b))
    });
    for i in idx {
        let r2 = &s2.rows[i];
        let mut line = wsrc_row_prefix(i);
        line.push_str(&format!(" dw={} dirq={} dnw={}", dw(i), dirq(i), r2.no_waiter.saturating_sub(s1.rows[i].no_waiter)));
        if r2.last_ts > 0 {
            line.push_str(&format!(" last={}ms", vct_ms(r2.last_ts, cntfrq)));
        } else {
            line.push_str(" last=-");
        }
        // 区间率 = Δwakes/Δt（cntfrq 换算，消费方派生；整数百分率两位——
        // 分母 0 防御降档「-」）。
        if interval_vct > 0 {
            let centi = ((dw(i) as u128) * 100 * cntfrq as u128 / interval_vct as u128).min(u64::MAX as u128) as u64;
            line.push_str(&format!(" wake/s={}.{:02}", centi / 100, centi % 100));
        } else {
            line.push_str(" wake/s=-");
        }
        println!("{}", line);
    }
    println!("# 差分窗测量自身贡献：本命令 sleep 超时醒＝TIMER dw 含 +1（如实注记）");
}

/// --json：纯净 stdout 单对象（诊断 stderr——shell设计 §3 铁律）。
/// schema add-only（day-one 第二件）：顶层公共因子平铺（version/cntfrq/
/// slots/rec_size）＋mode 条件域两层——cum 形 {now_vct,wake_acct_n,
/// sources:[…raw 九域]}、diff 形 {t0:{now_vct,wake_acct_n},t1:{同},
/// interval_vct,sources:[…Δ 四域]}。字段单义：cum 行恒 raw 累计、diff
/// 行恒 Δ；raw 单调计数＋cntfrq＋now_vct 锚出口，率/比/排行/合成指数不
/// 进 JSON（公约 5——防测量窗口语义双义进 ABI）；first_ts 哨兵
/// u64::MAX 原样导出（＝未触）；ts_domain 不进（v1 冻结 0——add-only 后
/// 补通道在）；活动槽 src 升序、src 出数值不出名。
fn powertop_json_cum(s: &WsrcSnap) {
    let mut out = String::with_capacity(1024);
    out.push_str("{\"version\":");
    out.push_str(&s.hdr.version.to_string());
    out.push_str(",\"mode\":\"cum\",\"cntfrq\":");
    out.push_str(&s.hdr.cntfrq.to_string());
    out.push_str(",\"slots\":");
    out.push_str(&s.hdr.slots.to_string());
    out.push_str(",\"rec_size\":");
    out.push_str(&s.hdr.rec_size.to_string());
    out.push_str(",\"now_vct\":");
    out.push_str(&s.hdr.now_vct.to_string());
    out.push_str(",\"wake_acct_n\":");
    out.push_str(&s.hdr.wake_acct_n.to_string());
    out.push_str(",\"sources\":[");
    let mut first = true;
    for (src, r) in s.rows.iter().enumerate() {
        if !wsrc_active(r) {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str("{\"src\":");
        out.push_str(&src.to_string());
        out.push_str(",\"wakes\":");
        out.push_str(&r.wakes.to_string());
        out.push_str(",\"irq_count\":");
        out.push_str(&r.irq_count.to_string());
        out.push_str(",\"no_waiter\":");
        out.push_str(&r.no_waiter.to_string());
        out.push_str(",\"first_ts\":");
        out.push_str(&r.first_ts.to_string());
        out.push_str(",\"last_ts\":");
        out.push_str(&r.last_ts.to_string());
        out.push_str(",\"last_ack_ts\":");
        out.push_str(&r.last_ack_ts.to_string());
        out.push_str(",\"last_woken_id\":");
        out.push_str(&r.last_woken_id.to_string());
        out.push_str(",\"flags\":");
        out.push_str(&r.flags.to_string());
        out.push('}');
    }
    out.push_str("]}");
    println!("{}", out);
}

fn powertop_json_diff(s1: &WsrcSnap, s2: &WsrcSnap, interval_vct: u64) {
    let mut out = String::with_capacity(1024);
    out.push_str("{\"version\":");
    out.push_str(&s2.hdr.version.to_string());
    out.push_str(",\"mode\":\"diff\",\"cntfrq\":");
    out.push_str(&s2.hdr.cntfrq.to_string());
    out.push_str(",\"slots\":");
    out.push_str(&s2.hdr.slots.to_string());
    out.push_str(",\"rec_size\":");
    out.push_str(&s2.hdr.rec_size.to_string());
    out.push_str(",\"t0\":{\"now_vct\":");
    out.push_str(&s1.hdr.now_vct.to_string());
    out.push_str(",\"wake_acct_n\":");
    out.push_str(&s1.hdr.wake_acct_n.to_string());
    out.push_str("},\"t1\":{\"now_vct\":");
    out.push_str(&s2.hdr.now_vct.to_string());
    out.push_str(",\"wake_acct_n\":");
    out.push_str(&s2.hdr.wake_acct_n.to_string());
    out.push_str("},\"interval_vct\":");
    out.push_str(&interval_vct.to_string());
    out.push_str(",\"sources\":[");
    let n = s1.rows.len().min(s2.rows.len());
    let mut first = true;
    for src in 0..n {
        let (r1, r2) = (&s1.rows[src], &s2.rows[src]);
        if !wsrc_active(r1) && !wsrc_active(r2) {
            continue;
        }
        if !first {
            out.push(',');
        }
        first = false;
        out.push_str("{\"src\":");
        out.push_str(&src.to_string());
        out.push_str(",\"dwakes\":");
        out.push_str(&r2.wakes.saturating_sub(r1.wakes).to_string());
        out.push_str(",\"dirq_count\":");
        out.push_str(&r2.irq_count.saturating_sub(r1.irq_count).to_string());
        out.push_str(",\"dno_waiter\":");
        out.push_str(&r2.no_waiter.saturating_sub(r1.no_waiter).to_string());
        out.push_str(",\"last_ts\":");
        out.push_str(&r2.last_ts.to_string());
        out.push('}');
    }
    out.push_str("]}");
    println!("{}", out);
}

/// `powertop` 入口：args[2..] 扫描 `--json` 与 `-d <ms>`（u64 毫秒、帽
/// [`POWERTOP_D_CAP_MS`]；缺值/非数/越帽 stderr + exit 1）；无 `-d` ＝
/// 单快照累计形。差分 = snap1 → `berry_thread_sleep` 真睡（deadline 绝对
/// MONOTONIC ns；测量任务真睡让核 park——禁 clock 忙等，自抬 TIMER 到达
/// 计数污染被测面）→ snap2 → 逐槽饱和差分（单调族 Δ 恒非负）；区间 =
/// h2.now_vct−h1.now_vct（头时钟锚自证——命令行标称值仅作睡眠 pacing
/// 不作分母）。
fn powertop_cmd(args: &[String]) {
    let mut json = false;
    let mut dur_ms: Option<u64> = None;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "--json" {
            json = true;
        } else if a == "-d" {
            i += 1;
            let raw = match args.get(i) {
                Some(s) => s.as_str(),
                None => {
                    eprintln!("berryctl: powertop -d needs <ms>");
                    exit(1);
                }
            };
            let v = match raw.parse::<u64>() {
                Ok(v) => v,
                Err(_) => {
                    eprintln!("berryctl: powertop bad -d {}", raw);
                    exit(1);
                }
            };
            if v > POWERTOP_D_CAP_MS {
                eprintln!("berryctl: powertop -d cap {}ms", POWERTOP_D_CAP_MS);
                exit(1);
            }
            dur_ms = Some(v);
        } else {
            eprintln!("berryctl: powertop unknown arg {}", a);
            exit(1);
        }
        i += 1;
    }
    match dur_ms {
        None => {
            let s = wsrc_fetch();
            if json {
                powertop_json_cum(&s);
            } else {
                powertop_human_cum(&s);
            }
        }
        Some(ms) => {
            let s1 = wsrc_fetch();
            // 真睡：deadline = now + ms×1e6（绝对 ns；已过点立即返 0——
            // 幂等安全）。
            let st = berry_sys::berry_thread_sleep(now() + ms * 1_000_000);
            if st != 0 {
                eprintln!("berryctl: powertop sleep err {}", st);
                exit(1);
            }
            let s2 = wsrc_fetch();
            let interval_vct = s2.hdr.now_vct.saturating_sub(s1.hdr.now_vct);
            if json {
                powertop_json_diff(&s1, &s2, interval_vct);
            } else {
                powertop_human_diff(&s1, &s2, interval_vct);
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let sub = args.get(1).map(String::as_str).unwrap_or("");
    // `power` 早分支（桌面UX刀D）：走内核 PSCI SYSTEM_OFF（0x88），非
    // psproto RPC 面——无 svcmgr gift 亦可调（bsh 手跑档同形）。成功 =
    // 不归（整机退出）；失败 stderr 一行诊断 + exit 1（无 PSCI 拓扑
    // 诚实降级——「宿主工具栏 OFF」为唯一关机路的机器上如实报）。
    if sub == "power" {
        let st = berry_sys::berry_power_off();
        eprintln!("berryctl: power off unsupported (st={st}) -- use host toolbar OFF");
        exit(1);
    }
    // `trace` 早分支（环读口刀雏形——powertop/trace 梯队首块＋TCG 降级
    // 频度观测仪器）：直调内核 0x84 op=2（power 先例位——非 psproto RPC、
    // 无 gift 依赖，bsh 手跑档亦可调）。`--json`（args[2] 位）= 本 CLI
    // `--json` 首件（shell设计 §3 铁律：纯净 stdout 单对象、诊断走
    // stderr、schema add-only）。不进 smoke boot 序列（零标记面）。
    if sub == "trace" {
        trace_cmd(args.get(2).map(String::as_str) == Some("--json"));
        exit(0);
    }
    // `powertop` 早分支（槽表读口刀雏形——「谁在挡住深睡」观测面）：直调
    // 内核 0x84 op=3（trace 先例位——非 psproto RPC、无 gift 依赖，bsh 手
    // 跑档亦可调）。默认单快照累计形；`-d <ms>` 差分形；`--json` day-one
    // 第二件（schema add-only）。不进 smoke boot 序列（零标记面）。
    if sub == "powertop" {
        powertop_cmd(&args[2..]);
        exit(0);
    }
    let op: u8 = match sub {
        "ver" => OP_VER,
        "ps" => OP_PS_LIST,
        "mem" => OP_MEM,
        _ => {
            eprintln!("usage: berryctl ver|ps|mem|power|trace|powertop");
            exit(1);
        }
    };
    let h: Handle = gift2_h();
    if h == 0 {
        // 非本拓扑 spawn（bsh run 档/旧归档/手工拉起）——无查询端可用。
        eprintln!("berryctl: no svcmgr gift");
        exit(1);
    }
    // ① 发请求（1B 恰长；满 -EBUSY/空不足 -EAGAIN → 等 WRITABLE 重试
    // ——svcmgr send_retry 的 std 侧对称面，两码都续等）。
    let mut req = [0u8; 1];
    let rn = encode_request_op(&mut req, op);
    loop {
        let s = berry_channel_send(h, req.as_ptr(), rn, core::ptr::null(), 0);
        if s == 0 {
            break;
        }
        if s != BERRY_ERR_AGAIN && s != BERRY_ERR_BUSY {
            eprintln!("berryctl: send err {}", s);
            exit(1);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            eprintln!("berryctl: send timeout");
            exit(1);
        }
    }
    // ② 收应答（单帧；-EAGAIN → 等 READABLE；随行 handle 不在协议内——
    // 收下即弃）。
    let mut rep = [0u8; 256];
    let nb;
    loop {
        let mut n = rep.len();
        let mut hh = [0 as Handle; 6];
        let mut nh = hh.len();
        let s = berry_channel_recv(h, rep.as_mut_ptr(), &mut n, hh.as_mut_ptr(), &mut nh);
        if s == 0 {
            for i in 0..nh {
                // 多带 handle：丢弃勿用（P6 A-4 富余收口纪律的客户端面）。
                berry_handle_close(hh[i]);
            }
            nb = n;
            break;
        }
        if s != BERRY_ERR_AGAIN {
            eprintln!("berryctl: recv err {}", s);
            exit(1);
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            eprintln!("berryctl: reply timeout");
            exit(1);
        }
    }
    // ③ 按 op 解析 + 渲染（畸形帧整体拒——psproto 执法）。
    match op {
        OP_VER => match parse_ver_reply(&rep, nb) {
            Some(v) => println!("ver {}", String::from_utf8_lossy(v)),
            None => {
                eprintln!("berryctl: bad ver reply");
                exit(1);
            }
        },
        OP_MEM => match parse_mem_reply(&rep, nb) {
            Some((vmos, total)) => println!("mem vmos={} bytes={}", vmos, total),
            None => {
                eprintln!("berryctl: bad mem reply");
                exit(1);
            }
        },
        _ => {
            // OP_PS_LIST：行渲染（ps bin 同面）。
            let mut rows: Vec<(Vec<u8>, u8, i32)> = Vec::new();
            if parse_reply(&rep, nb, |name, st, code| {
                rows.push((name.to_vec(), st, code))
            })
            .is_none()
            {
                eprintln!("berryctl: bad ps reply");
                exit(1);
            }
            for (name, st, code) in &rows {
                let name = String::from_utf8_lossy(name);
                if *st == STATE_EXITED {
                    println!("{} exited({})", name, code);
                } else if *st == STATE_ALIVE {
                    println!("{} alive", name);
                } else {
                    println!("{} spawnfail", name);
                }
            }
        }
    }
    exit(0);
}
