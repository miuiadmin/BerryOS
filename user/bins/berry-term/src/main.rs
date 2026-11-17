//! berry-term —— GUI 终端应用（TERMINAL；settings/files 后第三件真 GUI
//! app，GUI 终端首件）。骨架 = settings 镜像，四件终端面叠加。
//!
//! **使命**：黑底终端页 + 持久子 bsh 的**哑终端**宿主（2026-11-17 终端刀：
//! 显示单源 = 子 bsh 回显字节，本地第二编辑器退役）。用户键盘任意字节经
//! GUI 输入面抵达（inputd 键帧出向覆载：device_id = DEV_KEYBOARD、
//! x = 翻译后字节）后一视同仁透传子 stdin 通道（0x03 Ctrl-C/0x08 退格/
//! 0x0A Enter/0x09 Tab/0x1B Esc/可打印零本地语义——Ctrl-C 由子 feed 循环
//! Interrupt 臂〔交互态〕或 collect_fg 杀链〔前台态〕处理，两路径在子侧
//! 互斥立法；空行 Enter 照发 `\n`——子 bsh 空行仅重显 prompt）；子 stdout
//! 私有对端回流经 berry-termio `TermCore` 回显字节解释器（`\n` 切行、
//! `\b` 删尾、`\r` 丢弃、`ESC[2J` 清屏、`ESC[H` 等其它 finals 吞——覆盖集
//! = bsh 回显词汇表：纯文本＋`\n`＋BS-空格-BS 三连＋clear 7B＋`^C\n`＋
//! PROMPT 重显，逐臂核实封闭）进 24 行定容行环，滚动 = 行环推进 + 变化行
//! set_text 重排。编辑语义单源 = 子 bsh editor——Tab 补全后 Enter 提交
//! 本地旧值、clear 环显示错叠、prompt 不接续、历史显示缺失四族双编辑器
//! 失步降解面同根一次收口。每键一往返的通道延迟诚实文档化（本机异步
//! 消息，子输出即醒即收——显示变更的醒源全在子回显，键自身不 wake）。
//!
//! **链形态** = settings 镜像（musl std + berry-libc shim + CRT0 链接种子
//! + berry-ld 链）：wm 门控 → WM_GETINFO 几何 → UiMain Box::leak +
//! set_text_metrics → 建树 → 启动管道自检 → 持久子 bsh spawn → 帧循环
//! （渲染-SUBMIT-FRAME_CALLBACK-RELEASE 严格内联串行 + vote 变化才发 +
//! wait_many 稳态）。**wm 门控走 [`wm_present_h`]**（files 先例非
//! settings 的 wm_cli_h——本 app 与 files 同为 fs+ui 礼物布局
//! `[rootfs, fs, wm, input]`，wm 落 GIFT_H[4]；wm_cli_h 的 GIFT_H[0] 位
//! 对 wants_fs 程序承载 rootfs VMO，禁抄）。
//!
//! **键盘面**：appshell pump.rs 的 HostHooks::on_key 同签名消费面
//! `(&mut self, ui, code, ts_ns)`——本 crate 不依赖 berry-appshell（settings
//! 镜像自持帧循环，稳态 wait_many 需第三路子 stdout，pump 的 n≤2 等待面
//! 放不下），DEV_KEYBOARD 帧分流点内联（pump.rs :346 同形）；键帧消费 =
//! 单字节透传子 stdin（哑终端——无编辑分支）。
//!
//! **终端四件**：
//! 1. 渲染：app 掌整帧底色（黑底 CLEAR_BG——settings 同位先例）；定容
//!    行环 24 行 × 每行 64B（= berry-ui TEXT_INLINE 现行值，逐行 Text
//!    节点 Col 叠行；节点预算 24 行 + 输入行 + chrome ≈ 27 ≪
//!    MAX_NODES=512；25 行 × cell_h 在 800×600 调试档（S8x16）= 400px、
//!    1080×1920 大档（S16x32）= 800px，双档 viewport 内收敛）；滚动 =
//!    行环推进后对变化行 set_text 重排（本表 drawn 副本去重——同文跳过
//!    零冗余脏帧）。**软换行**：完成行超 64B 按 64B 切多物理行入环；
//!    尾段缓冲 TAIL_MAX=320（prompt 5B + bsh 行帽 256B 全容——输入行
//!    不再截 64B）；视图 = 环行居上 + 尾段前 K-1 块居下（K =
//!    ceil((尾段长+1)/64)，bsh 词汇表下 ≤5；可见历史随 K 收缩——环满时
//!    顶掉最老行保尾段可见）+ 末块 + 自绘光标作输入行（INPUT_FG；a11y
//!    ROLE_TEXT 登记位在输入行节点）。
//! 2. 尾段字节解释器（T2/T3 纯面 = berry-termio `TermCore`，T7）：行环/
//!    尾段/feed 解释器/软换行/视图计算全在纯 crate（host 单测护栏）；
//!    本文件只持 drawn 渲染副本与 UI 胶水（wake/child_send/wait_many
//!    不进纯 crate）。TEXT_INLINE/MAX_NODES 零触碰。
//! 3. 持久子 bsh + 自动 respawn：启动 spawn 一次（`--no-selftest` 跳自验
//!    链直进交互循环——term_selftest 铁律：不带旗 = TCG 分钟级自验 +
//!    term 链无界递归）；stdin = `stdin_handle` 通道端直传（feeder 模型
//!    反面——本 app 长命持续发键，非一次性泵尽；端被 env send take，
//!    本表留对端 in_s 发送）；stdout = 私有对端；stack_pages=16（svcmgr
//!    拉起 bsh 现值先例）；extra_gifts = rootfs/fs 双 dup（fs_gifts——
//!    term_selftest 同参）。子退出（EOF 零长消息/对端异常）→ 行环加一行
//!    `berry-term: bsh exited` + 记死亡时点；**Dead 态 2s 冷却后自动
//!    respawn**（成功打 `berry-term: respawn`；失败打诊断行续 2s 退避
//!    不杀页面；respawn 到期点并入稳态 deadline）；子存活 <2s 即死记一
//!    击（自然寿命死亡清零陈账），**三击熔断**——停自动重启、打
//!    `berry-term: respawn circuit broken (enter to restart)` 常驻行，
//!    Enter 手动重启并清零计数（熔断恢复路径）。归属：bsh 是 berry-term
//!    的内部实现资源（try_spawn_child 内部建制、Child 句柄自洽），其生死
//!    管理归 berry-term 自身；svcmgr app_supervise 监督的是 app 进程
//!    本体的崩溃重启——两层监督对象不同，不翻「app 监督归 svcmgr」裁决。
//! 4. 启动管道自检：帧循环前 spawn 一次性子 bsh（stdin_feed 静态
//!    `echo berryterm\nuname\nclear\n\x03`）——term_collect 式早退谓词
//!    收齐五断言（提示符 ×2/命令回显行/berryterm 结果行/BerryOS aarch64
//!    结果行/`^C\n`），收集流另整流喂一份 TermCore 断言（clear 序列消化
//!    后旧内容不在可见行集、`^C` 中断行在册、尾段以 prompt 收尾——哑
//!    终端解释器端到端自执法），过则 `berry-term: pipe ok`；不过 park
//!    （诊断行写明缺哪个断言）。自检子 terminate(130)+reap 收口
//!    （term_selftest 先例形）。
//!
//! **稳态 wait_many 至多 3 路**：帧回调 hw + 输入 hi（条件）+ 子 stdout
//! out_s（条件）——settings n≤2 面的第三路扩展（子输出即醒即收，不赌
//! IDLE 帽）；deadline = min(next_wakeup, IDLE 帽, respawn 到期点〔待
//! 重启死亡在席时〕）。
//!
//! **输出面**：manifest console=false → stdout 走 svcmgr 私有对（drain 归
//! app_supervise）；全部 println/eprintln 为诊断面，不构成串口标记。
//! 首帧呈现回执打一行 `berry-term: first frame`。
//!
//! **v1 降解面**：每键一通道往返（哑终端固有——子回显即醒即收，无本地
//! 即时反馈）；prompt 接续自然成立（子 prompt 与回显输入同驻尾段，输入
//! 行 = 尾段末块 + 光标——「输入行独立成行＋Enter 回流自证」旧观感已随
//! 哑终端化退役）；`\r`/UTF-8 多字节序列不 special-case（域外字节渲染
//! 等宽缺省空格面）；ESC 序列词汇表外一律吞（通用 ANSI 屏模型已砍单）；
//! 尾段满 320B 丢后续字节（bsh 词汇表上限 261B，正常路径不可达）；无
//! 指针交互（纯键盘页——pointer 帧照 push 进引擎，无命中目标零效果）。

use berry_exec::{channel_pair, cpio_find, spawn_rootfs_elf, ChildHandles, SpawnSpec};
use berry_gfx::cmd::{Color, CommandEncoder, GlyphPos, Rect as GfxRect};
use berry_gfx::color::{TextureDesc, TextureFormat};
use berry_gfx::font::FontSize;
use berry_gfx::pass::{LoadOp, PassDescriptor, StoreOp};
use berry_gfx::queue::Queue;
use berry_guiproto as gp;
use berry_libc::io::{fs_h, input_h, rootfs_h, self_vmar_h, wm_present_h};
use berry_sys::{
    berry_channel_recv, berry_channel_send, berry_clock_read, berry_handle_close,
    berry_handle_duplicate, berry_object_wait_many, berry_object_wait_one,
    berry_process_terminate, berry_vmar_map, berry_vmo_commit, berry_vmo_create, Handle,
    BERRY_ERR_AGAIN, BERRY_ERR_BUSY, BERRY_ERR_TIMEDOUT, CLOCK_MONOTONIC, RIGHT_READ,
    RIGHT_RECV, RIGHT_SEND, RIGHT_TRANSFER, RIGHT_WRITE, SIGNAL_READABLE, SIGNAL_WRITABLE,
};
use berry_termio::{TermCore, LINE_MAX, TERM_ROWS};
use berry_ui::desc::{ColProps, TextProps, TextStyle};
use berry_ui::hit::PtrEvent;
use berry_ui::layout::CrossAlign;
use berry_ui::{
    NodeId, ProportionalTextMetrics, Props, TextBuf, UiError, UiMain, ViewKind, ViewSpec,
    ROLE_TEXT,
};

// 链接种子（settings/berry-launcher 同款）：rlib/LTO 按需保留——无引用
// 则 berry-libc 的 crt0（含 `_start`）不进链接；`#[used]` 防死码消除。
#[used]
static CRT0_SEED: unsafe extern "C" fn() = berry_libc::berry_crt0_anchor;

/// RPC 收侧 READABLE 帽（settings 同值 300s：首 RPC 距 boot 稳态可远）。
const RPC_CAP_NS: u64 = 300 * 1_000_000_000;

/// 帧缓冲映射 VA（settings/berry-launcher frameloop 同位）。
const FRAME_VA: usize = 0x5100_0000;

/// 默认 vsync 周期（60Hz；S2 食谱默认，settings 同值）。
const VSYNC_NS: u64 = 16_666_666;

/// IDLE 稳态等待帽（S1 判词：Idle 相撤定时器 → 10s 周期兜底醒）。
const IDLE_CAP_NS: u64 = 10 * 1_000_000_000;

/// glyph scratch 容量（堆置——paint 对超容 run 截断尾字形；满屏 24×64
/// 字形的极端帧 1536，本帽照 appshell pump 档 1024（截尾视觉面非正确性
/// 面，终端常态行 ≪ 64B）。
const SCRATCH_GLYPHS: usize = 1024;

/// 终端整帧清屏底色（黑底——settings CLEAR_BG 同位先例的终端档）。
const CLEAR_BG: (u8, u8, u8) = (0x00, 0x00, 0x00);

// 行环行数/单行字节帽单源 = berry-termio（TERM_ROWS=24——节点预算
// 24 行 + 输入行 + chrome ≈ 27 ≪ MAX_NODES=512；LINE_MAX=64 = berry-ui
// `TEXT_INLINE` 现行值，TextBuf loud 拒面；另 TAIL_MAX=320 尾段帽）——
// 显示核纯面与本表消费同源，不两套。

/// 行环总收容帽（自检收集环同值——子输出整场字节上界，防无界 Vec）。
const COLLECT_MAX: usize = 32 * 1024;

/// 收集环活动帽（term_collect 同值：收包刷新、帽尽即收场）。
const COLLECT_TIMEOUT_NS: u64 = 10 * 1_000_000_000;

/// 子 bsh 栈页数（term_selftest 先例值：svcmgr 拉起 bsh 现值 16 页——
/// musl std 大程序口径）。
const CHILD_STACK_PAGES: u64 = 16;

/// 子死自动 respawn 冷却（T5：Dead 态 2s 后自动重启；respawn 失败同值
/// 续退避——pending 重臂不杀页面）。
const RESPAWN_COOLDOWN_NS: u64 = 2 * 1_000_000_000;

/// 子「存活 <2s 即死」判定窗（T5：快速死亡记一击——三击熔断的击数口径）。
const CHILD_MIN_LIFE_NS: u64 = 2 * 1_000_000_000;

/// 熔断击数（T5：三击——停自动重启，Enter 手动恢复）。
const RESPAWN_BREAK_STRIKES: u32 = 3;

/// 行环文本前景（近白微绿；对 CLEAR_BG 高对比）。
const ROW_FG: Color = Color { r: 0xD8, g: 0xE6, b: 0xDA };
/// 输入行前景（终端绿；与行环文本视觉分档）。
const INPUT_FG: Color = Color { r: 0x5E, g: 0xE8, b: 0x7A };
/// 自绘光标字符（下划线——经典终端形；0x7f/块字符在字形域外）。
const CURSOR: u8 = b'_';

/// 诊断 + park 收口（settings `fail` 同形——app 常驻不被 reap；崩溃监督
/// 归 svcmgr app_supervise 域，本进程不自救）。
fn fail(what: &str) -> ! {
    eprintln!("berry-term: {what}");
    berry_libc::park()
}

/// now = clock_read；失败按 0 → deadline 即刻到期 = fail-fast（settings
/// `now` 同口径）。
fn now() -> u64 {
    let mut t = 0u64;
    if berry_clock_read(CLOCK_MONOTONIC, &mut t) != 0 {
        return 0;
    }
    t
}

/// 即关（RELEASE 归还别名——关失败 = handle 表泄漏，FAIL loud）。
fn close_h(h: Handle) {
    if berry_handle_close(h) != 0 {
        fail("handle close");
    }
}

/// 严格串行 RPC **发送半程**（settings `rpc_send` 同形）：-EBUSY → 阻塞等
/// WRITABLE（帽重臂，雷 2 纪律不自旋）；send 原子性——EBUSY/错码时随行
/// handle 零迁移。**send 即 take**。
fn rpc_send(ch: Handle, req: &[u8], attach: &[Handle]) {
    let (hptr, hn) = if attach.is_empty() {
        (core::ptr::null(), 0)
    } else {
        (attach.as_ptr(), attach.len())
    };
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(ch, req.as_ptr(), req.len(), hptr, hn);
        if s == 0 {
            return;
        }
        if s != BERRY_ERR_BUSY {
            fail(&format!("rpc send err={s}"));
        }
        if deadline == 0 {
            deadline = now() + RPC_CAP_NS;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            fail("rpc send wait timeout");
        }
        deadline = 0; // 重臂（命中后仍满则续等——公平排队）
    }
}

/// 严格串行 RPC **收侧半程**（settings `rpc_recv` 同形）。
fn rpc_recv(ch: Handle, rep: &mut [u8; 256], rhs: &mut [Handle; 8]) -> (usize, usize) {
    loop {
        let mut nb = rep.len();
        let mut nh = rhs.len();
        let s = berry_channel_recv(ch, rep.as_mut_ptr(), &mut nb, rhs.as_mut_ptr(), &mut nh);
        if s == 0 {
            if nb == 0 {
                fail("rpc empty reply");
            }
            return (nb, nh);
        }
        if s != BERRY_ERR_AGAIN {
            fail(&format!("rpc recv err={s}"));
        }
        let mut obs = 0u64;
        if berry_object_wait_one(ch, SIGNAL_READABLE, now() + RPC_CAP_NS, &mut obs) != 0 {
            fail("rpc reply timeout");
        }
    }
}

/// 严格串行 RPC 单趟（settings `rpc` 同形）。
fn rpc(
    ch: Handle,
    req: &[u8],
    attach: &[Handle],
    rep: &mut [u8; 256],
    rhs: &mut [Handle; 8],
) -> (usize, usize) {
    rpc_send(ch, req, attach);
    rpc_recv(ch, rep, rhs)
}

// —— 建树小件（settings 同形）——

/// Text 叶 spec 模板（超 `TEXT_INLINE`=64B loud Err——行环内容已按帽截）。
fn text_tp(text: &[u8], fg: Color, scale: FontSize) -> Result<ViewSpec, UiError> {
    let tp = TextProps {
        style: TextStyle { fg, bg: None, scale },
        text: TextBuf::new(text)?,
    };
    Ok(ViewSpec { kind: ViewKind::Text, key: 0, props: Props::Text(tp) })
}

/// 定宽/定高占位 Col（settings `box_spec` 同形——顶留白推位）。
fn box_spec(w: u32, h: u32) -> ViewSpec {
    ViewSpec {
        kind: ViewKind::Col,
        key: 0,
        props: Props::Col(ColProps { gap: 0, w: Some(w), h: Some(h), cross: CrossAlign::Center }),
    }
}

/// 终端页运行期句柄（建树一次落定；帧循环只 set_text 换行 + 消费事件）。
struct Page {
    /// 行环 24 行的 Text 节点（row 0 = 屏顶）。
    rows: [NodeId; TERM_ROWS],
    /// 输入行 Text 节点（行缓冲 + 自绘光标）。
    input: NodeId,
}

/// 建终端页树：根 Col（左对齐、零行距——终端行贴行形态）→ 顶留白 →
/// 24 行占位 Text → 输入行。档位判别同 settings/files：OH phone
/// 1080×1920 走 big 档（S16x32），800×600 调试档紧凑（S8x16——25 行 ×
/// 16px = 400px 收敛）。Err = 文本域违例 → 调用方 fail 收口。
fn build(ui: &mut UiMain) -> Result<(Page, FontSize), UiError> {
    let (vp_w, vp_h) = ui.viewport();
    let big = vp_w >= 600 && vp_h >= 1800;
    let scale = if big { FontSize::S16x32 } else { FontSize::S8x16 };
    let pad_v: u32 = if big { 48 } else { 16 };

    let root = ui.root();
    let page_col = ui.add(
        root,
        ViewSpec {
            kind: ViewKind::Col,
            key: 0,
            props: Props::Col(ColProps {
                gap: 0,
                w: Some(vp_w),
                h: Some(vp_h),
                cross: CrossAlign::Start,
            }),
        },
    )?;
    // 顶留白（Col 游标从顶起）。
    ui.add(page_col, box_spec(1, pad_v))?;
    // NodeId 无公开零值构造——首行 add 值播种数组（NodeId: Copy），余行
    // 循环覆写。
    let mut rows = [ui.add(page_col, text_tp(b"", ROW_FG, scale)?)?; TERM_ROWS];
    for r in rows.iter_mut().skip(1) {
        // 空行占位（空 Text 内在 = 0×cell_h——行高恒 cell_h，空行不塌行）。
        *r = ui.add(page_col, text_tp(b"", ROW_FG, scale)?)?;
    }
    let input = ui.add(page_col, text_tp(b"_", INPUT_FG, scale)?)?;
    // a11y 只登输入行（行环内容高频换文——动态 churn 不占 A11Y_MAX=40 槽）。
    ui.set_a11y(input, ROLE_TEXT, b"terminal input")?;
    Ok((Page { rows, input }, scale))
}

/// 终端状态（显示核纯面 + 上帧渲染副本——core 归 berry-termio〔行环/
/// 尾段/回显字节解释器/软换行/视图计算，T7 host 单测护栏〕，drawn 归
/// 本表 UI 胶水）。
struct Term {
    /// 行环/尾段/feed 解释器/视图计算（哑终端显示核）。
    core: TermCore,
    /// 上帧渲染副本（24 行 + 输入行——同文跳过 set_text，零冗余脏帧）。
    drawn: [[u8; LINE_MAX]; TERM_ROWS + 1],
    drawn_len: [usize; TERM_ROWS + 1],
}

impl Term {
    fn new() -> Term {
        Term {
            core: TermCore::new(),
            drawn: [[0u8; LINE_MAX]; TERM_ROWS + 1],
            drawn_len: [0usize; TERM_ROWS + 1],
        }
    }

    /// 完成行入环（诊断行直入；软换行切块在纯面）。
    fn push_line(&mut self, bytes: &[u8]) {
        self.core.push_line(bytes);
    }

    /// 子输出喂流（回显字节解释器——`\n`/`\b`/`\r`/ESC 状态机全在纯面）。
    fn feed(&mut self, bytes: &[u8]) {
        self.core.feed(bytes);
    }

    /// 行环 + 尾段块 + 输入行 → 树同步（变化行 set_text；drawn 副本去重）。
    /// 返回是否有变更（调用方据此 wake——醒源在子回显路径，此处返值为
    /// 防御面）。行内容先拷本地再比对（view_row 借 self——与 drawn 写
    /// 互斥，借毕即放）。
    fn sync(&mut self, ui: &mut UiMain, page: &Page) -> bool {
        let mut dirty = false;
        for vi in 0..TERM_ROWS {
            let mut tmp = [0u8; LINE_MAX];
            let n = {
                let b = self.core.view_row(vi);
                let n = b.len();
                tmp[..n].copy_from_slice(b);
                n
            };
            if tmp[..n] != self.drawn[vi][..self.drawn_len[vi]] {
                if let Err(e) = ui.set_text(page.rows[vi], &tmp[..n]) {
                    fail(&format!("row set_text err={e:?}"));
                }
                self.drawn[vi][..n].copy_from_slice(&tmp[..n]);
                self.drawn_len[vi] = n;
                dirty = true;
            }
        }
        // 输入行 = 尾段末块 + 自绘光标（哑终端：子 prompt 与回显输入同驻
        // 尾段——prompt 接续自然成立；末块 + 光标 ≤ 64B 恒成立〔K 构造律〕）。
        let mut buf = [0u8; LINE_MAX];
        let blk = self.core.input_block();
        buf[..blk.len()].copy_from_slice(blk);
        buf[blk.len()] = CURSOR;
        let n = blk.len() + 1;
        if buf[..n] != self.drawn[TERM_ROWS][..self.drawn_len[TERM_ROWS]] {
            if let Err(e) = ui.set_text(page.input, &buf[..n]) {
                fail(&format!("input set_text err={e:?}"));
            }
            self.drawn[TERM_ROWS][..n].copy_from_slice(&buf[..n]);
            self.drawn_len[TERM_ROWS] = n;
            dirty = true;
        }
        dirty
    }
}

/// fs 礼物双 dup（bsh `fs_gifts` 同参——term_selftest 装配形）：gift[2] =
/// rootfs VMO READ dup、gift[3] = fs 通道 SEND|RECV dup。spawn 成功则两
/// dup 随 env send 全被 take（零泄漏）；spawn Err 早退点全在 env send 前
/// = 未 take，调用方显式补关双 dup。None = fs 服务缺席或 dup 失败。本
/// app 自身零 fs RPC 消费（rootfs VMO 直读 + 输入/wm 面）——严格串行
/// fs 会话纪律不破（子 bsh 是 fs 通道唯一活动客户端）。
fn fs_gifts() -> Option<(Handle, Handle)> {
    let mut rootfs_dup = 0 as Handle;
    let mut fs_dup = 0 as Handle;
    // TRANSFER 保位：两 dup 随 env 块 channel_send 迁交子进程。
    if berry_handle_duplicate(rootfs_h(), RIGHT_READ | RIGHT_TRANSFER, &mut rootfs_dup) != 0
        || berry_handle_duplicate(fs_h(), RIGHT_SEND | RIGHT_RECV | RIGHT_TRANSFER, &mut fs_dup)
            != 0
    {
        let _ = berry_handle_close(rootfs_dup);
        let _ = berry_handle_close(fs_dup);
        return None;
    }
    Some((rootfs_dup, fs_dup))
}

/// 子串在场判定（自检断言用——term_selftest `has_sub` 同形）。
fn has_sub(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// 子串计数（自检提示符 ×2 断言——term_selftest `count_sub` 同形）。
fn count_sub(hay: &[u8], needle: &[u8]) -> usize {
    hay.windows(needle.len()).filter(|w| *w == needle).count()
}

/// 子串首现位置（None = 不在席——`^C` 后 prompt 重显判据用）。
fn find_sub(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// `^C\n` 之后 prompt 已重显在流（审查轮补：`^C\n` 与其后的 `bsh> ` 可能
/// 分属两条通道消息——谓词只看 `^C\n` 会在重显消息抵达前早退收场，⓪b
/// 探针三「尾段以 prompt 收尾」恒假 → boot 期误 park。判据 = 首现
/// `^C\n` 之后再有一处 `bsh> `）。
fn prompt_after_ctrl_c(d: &[u8]) -> bool {
    match find_sub(d, b"^C\n") {
        None => false,
        Some(i) => has_sub(&d[i + 3..], b"bsh> "),
    }
}

/// 自检早退谓词（term_selftest `term_ready` 同形）：五断言物证全在席
/// （第二提示符在第一结果行之后——谓词必经两命令全链；T6 增 `^C\n` 后
/// prompt 重显在流——必经 clear + Ctrl-C 全链，且消 `^C\n` 与重显分属
/// 两条消息的早退竞速——重显不入流则 ⓪b 探针三恒假误 park）。
fn term_ready(d: &[u8]) -> bool {
    count_sub(d, b"bsh> ") >= 2
        && has_sub(d, b"echo berryterm")
        && has_sub(d, b"\nberryterm\n")
        && has_sub(d, b"BerryOS aarch64")
        && prompt_after_ctrl_c(d)
}

/// 自检收集环（term_selftest `term_collect` 同形）：四断言齐即收的早退
/// 谓词环——recv/wait 原语、活动帽收场全同；差异仅本表定容（32KiB 帽
/// 截尾，无 Vec）。
fn term_collect(h: Handle, data: &mut [u8; COLLECT_MAX]) -> usize {
    let mut len = 0usize;
    let mut msg = [0u8; 256];
    while len < data.len() && !term_ready(&data[..len]) {
        let mut nb = msg.len();
        let mut nh = 0usize;
        let s =
            berry_channel_recv(h, msg.as_mut_ptr(), &mut nb, core::ptr::null_mut(), &mut nh);
        if s == 0 {
            if nb == 0 {
                break; // EOF：子退出（异常面——断言自然红）
            }
            let keep = nb.min(data.len() - len);
            data[len..len + keep].copy_from_slice(&msg[..keep]);
            len += keep;
            continue;
        }
        if s != BERRY_ERR_AGAIN {
            break; // 对端异常（同款收场）
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_READABLE, now() + COLLECT_TIMEOUT_NS, &mut obs) != 0 {
            break; // 活动帽尽（断链面——断言自然红）
        }
    }
    len
}

/// 启动管道自检（模块头注件 4——term_selftest 镜像）：一次性子 bsh 喂
/// `echo berryterm\nuname\nclear\n\x03`，五断言齐即收 → terminate(130)+
/// reap 收口；收集流另整流喂一份 TermCore 断言（T6 扩探针——哑终端解释
/// 器端到端自执法）。过则 `berry-term: pipe ok`；不过 park（诊断行写明
/// 缺哪个断言）。
fn pipe_selftest() {
    let full = b"bin/bsh";
    let Some((data_off, data_len)) = cpio_find(rootfs_h(), full) else {
        fail("pipe selftest: bin/bsh not found");
    };
    let (rootfs_dup, fs_dup) = match fs_gifts() {
        Some(g) => g,
        None => fail("pipe selftest: fs gift dup failed"),
    };
    let (out_s, out_c) = channel_pair();
    let args: [&[u8]; 1] = [b"--no-selftest"];
    let spec = SpawnSpec {
        name: full,
        data_off,
        data_len,
        stdout: out_c,
        stack_pages: CHILD_STACK_PAGES,
        stdin_feed: Some(b"echo berryterm\nuname\nclear\n\x03"),
        stdin_handle: None,
        args: &args,
        extra_gifts: &[rootfs_dup, fs_dup],
        mmio_vmo: None,
        mmio_va: 0,
        intent: 0,
        force_eager: false,
    };
    let child = match spawn_rootfs_elf(rootfs_h(), &spec) {
        Ok(c) => c,
        Err(_) => {
            // spawn Err 早退点全在 env send 前 = 双 dup 未 take——显式补关
            //（fs_gifts 头注契约；out 对随关）。
            let _ = berry_handle_close(rootfs_dup);
            let _ = berry_handle_close(fs_dup);
            let _ = berry_handle_close(out_c);
            let _ = berry_handle_close(out_s);
            fail("pipe selftest: bsh spawn failed");
        }
    };
    let mut data = [0u8; COLLECT_MAX];
    let len = term_collect(out_s, &mut data);
    let data = &data[..len];
    // ⓪ 四断言（提示符 ×2 + 回显行 + 两结果行——混流同一 stdout 字节流）。
    let mut fails: Vec<&str> = Vec::new();
    if count_sub(data, b"bsh> ") < 2 {
        fails.push("prompt <2");
    }
    if !has_sub(data, b"echo berryterm") {
        fails.push("echo line missing");
    }
    // 独立行判据（前导 \n 消回声歧义）——只能被真结果行满足。
    if !has_sub(data, b"\nberryterm\n") {
        fails.push("berryterm result missing");
    }
    if !has_sub(data, b"BerryOS aarch64") {
        fails.push("uname result missing");
    }
    // ⓪b T6 扩探针：收集流整流喂一份 TermCore——哑终端解释器端到端自
    // 执法。喂流解剖（clear 链定序）：…「bsh> clear」行 → ESC[2J 清屏
    // （含该行）→ ESC[H 吞 → prompt 驻尾段 → `^C` 追加成「bsh> ^C」行 →
    // prompt 重显驻尾段。三断言：旧内容不在可见行集、`^C` 中断行在册、
    // 尾段以 prompt 收尾。纯面计算，零额外通道面。
    let mut core = TermCore::new();
    core.feed(data);
    if (0..TERM_ROWS).any(|vi| has_sub(core.view_row(vi), b"berryterm")) {
        fails.push("stale rows after clear");
    }
    if !(0..TERM_ROWS).any(|vi| core.view_row(vi) == b"bsh> ^C") {
        fails.push("^C line missing");
    }
    if !core.tail().ends_with(b"bsh> ") {
        fails.push("tail not at prompt");
    }
    // ① EOF 收口：子恒驻（feeder EOF 后空读续读不退）——terminate 非 0 =
    // 已退（哑终端契约破）；reap 真码 130（POSIX 128+SIGINT 惯例）。
    if berry_process_terminate(child.hp, 130) != 0 {
        fails.push("terminate != 0 (child exited early?)");
    }
    match child.reap() {
        Ok(130) => {}
        Ok(_) => fails.push("reap code != 130"),
        Err(()) => fails.push("reap failed"),
    }
    if berry_handle_close(out_c) != 0 || berry_handle_close(out_s) != 0 {
        fail("pipe selftest: handle close"); // 泄漏面 loud 死
    }
    if !fails.is_empty() {
        eprintln!("berry-term: pipe selftest bad: {:?}", fails);
        berry_libc::park();
    }
    println!("berry-term: pipe ok");
}

/// 持久子 bsh 句柄（帧循环的通道面）。
struct Child {
    /// 子 stdin 通道本端（发送面：行 + `\n`；Enter 键路消费）。
    in_s: Handle,
    /// 子 stdout 私有收端（帧循环第三等待路；0 = 子已退/面失效）。
    out_s: Handle,
    /// spawn 产物（子退出时 best-effort reap——不回收 = 有界泄漏）。
    proc: Option<ChildHandles>,
    /// 子死标记（EOF/挂断后置位——发送路静默丢弃）。
    dead: bool,
}

/// 持久子 bsh spawn（模块头注件 3；T5 抽 Result 变体——boot 首次 Err 仍
/// fail 收口〔main 调用方〕、respawn 路径 Err 打诊断行续退避不杀页面
///〔`try_respawn` 调用方〕）：stdin_handle 通道端直传（env send
/// take 迁交——子 STDIN_H recv 面；本表留对端 in_s）；stdout 私有对端
/// （spawn 内部 dup RIGHT_SEND 交付，原值 out_c 留本表即关——term_
/// selftest 收口同款）；`--no-selftest` + fs 双礼物 + stack 16 页。Err
/// 文案 = boot fail 面现行三态（bsh not found / fs gift dup failed /
/// bsh spawn failed）。
fn try_spawn_child() -> Result<Child, &'static str> {
    let full = b"bin/bsh";
    let Some((data_off, data_len)) = cpio_find(rootfs_h(), full) else {
        return Err("bsh not found");
    };
    let (rootfs_dup, fs_dup) = match fs_gifts() {
        Some(g) => g,
        None => return Err("fs gift dup failed"),
    };
    let (in_s, in_c) = channel_pair();
    let (out_s, out_c) = channel_pair();
    let args: [&[u8]; 1] = [b"--no-selftest"];
    let spec = SpawnSpec {
        name: full,
        data_off,
        data_len,
        stdout: out_c,
        stack_pages: CHILD_STACK_PAGES,
        stdin_feed: None,
        stdin_handle: Some(in_c), // 被 env send take（迁移语义——本表留 in_s）
        args: &args,
        extra_gifts: &[rootfs_dup, fs_dup],
        mmio_vmo: None,
        mmio_va: 0,
        intent: 0,
        force_eager: false,
    };
    let child = match spawn_rootfs_elf(rootfs_h(), &spec) {
        Ok(c) => c,
        Err(_) => {
            let _ = berry_handle_close(rootfs_dup);
            let _ = berry_handle_close(fs_dup);
            let _ = berry_handle_close(in_s);
            let _ = berry_handle_close(in_c); // Err 面 in_c 未被 take（显式补关）
            let _ = berry_handle_close(out_c);
            let _ = berry_handle_close(out_s);
            return Err("bsh spawn failed");
        }
    };
    // out_c 原值留本表（stdout 内部 dup 交付子表）——spawn 后即关；in_c
    // 已迁移（子表 STDIN_H），本表零副本。
    let _ = berry_handle_close(out_c);
    Ok(Child { in_s, out_s, proc: Some(child), dead: false })
}

/// 子监督态（T5：自动 respawn + 三击熔断——bsh 是 berry-term 的内部实现
/// 资源，生死管理归本 app；svcmgr app_supervise 监督 app 进程本体，两层
/// 监督对象不同）。
struct Super {
    /// 上次 spawn 时点（存活 <CHILD_MIN_LIFE_NS 即死记一击）。
    spawned_at: u64,
    /// 待重启死亡时点（None = 子活 / 熔断态；Some = 2s 冷却后自动
    /// respawn）。
    pending: Option<u64>,
    /// 快速死亡连击（≥RESPAWN_BREAK_STRIKES 熔断；自然寿命死亡清零）。
    strikes: u32,
    /// 熔断态（停自动重启；Enter 手动重启清零恢复）。
    broken: bool,
}

/// respawn 尝试（冷却到自动 / 熔断态 Enter 手动共用）：成功打
/// `berry-term: respawn` 诊断行、刷新 spawned_at、清 pending；失败打失败
/// 行、pending 重臂 now（2s 后再试——续退避不杀页面；手动路径熔断已先
/// 清，失败后回落自动退避节奏——pending 态 Enter 静默，键路只在熔断态
/// 响应 Enter，再试须待自动 respawn 或再熔断）。strikes/broken 不动——
/// 连击记账归 mark_child_dead（自动 respawn 成功不清零陈账，三击熔断
/// 语义才成立）；熔断恢复的清零归手动路径调用方。
fn try_respawn(term: &mut Term, sup: &mut Super) -> Option<Child> {
    match try_spawn_child() {
        Ok(c) => {
            term.push_line(b"berry-term: respawn");
            sup.spawned_at = now();
            sup.pending = None;
            Some(c)
        }
        Err(why) => {
            let msg = format!("berry-term: respawn failed: {why}");
            term.push_line(msg.as_bytes());
            sup.pending = Some(now());
            None
        }
    }
}

/// 通道发送（子 stdin 写点——rpc_send 的通道形：-EBUSY 阻塞等 WRITABLE
/// 帽重臂；其余错 = 链断返 false 调用方标记子死）。
fn child_send(h: Handle, bytes: &[u8]) -> bool {
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(h, bytes.as_ptr(), bytes.len(), core::ptr::null(), 0);
        if s == 0 {
            return true;
        }
        if s != BERRY_ERR_BUSY {
            return false;
        }
        if deadline == 0 {
            deadline = now() + COLLECT_TIMEOUT_NS;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            return false; // 满队超时（子不读——视同链断）
        }
        deadline = 0;
    }
}

fn main() {
    // —— 降级门：wm 客户端端未交付 → 一行诊断 + park 常驻（files 同形
    //    ——wants_fs GUI app 的 wm 礼物落 extras[2]，wm_present_h 同槽
    //    访问器；窗口义务语义不折衷成 exit(0) 秒退）。
    let hw = wm_present_h();
    if hw == 0 {
        eprintln!("berry-term: wm absent (no gui gift)");
        berry_libc::park();
    }

    // —— WM_GETINFO 一次握手（几何真值源；parse 失败 = 诊断 + park）。
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = gp::encode_wm_getinfo_req(&mut req);
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let info = match gp::parse_wm_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("wm getinfo rep bad st={}", rep[0])),
    };
    eprintln!(
        "berry-term: wm v={} {}x{} input={} fs={}",
        info.version_wm,
        info.disp_w,
        info.disp_h,
        input_h(),
        if fs_h() == 0 { 0 } else { 1 }
    );

    // —— UI arena 堆置（settings 同形：UiMain 构造中间体 ~29KB 落栈——
    //    stack_pages=64 前提；Box::leak = 常驻面无回收语义）。
    let ui: &'static mut UiMain =
        Box::leak(Box::new(UiMain::new(info.disp_w as u32, info.disp_h as u32)));
    static TEXT_METRICS: ProportionalTextMetrics = ProportionalTextMetrics;
    ui.set_text_metrics(&TEXT_METRICS);

    // —— 建树（24 行 + 输入行占位；首帧黑底空屏）。
    let (page, _scale) = match build(ui) {
        Ok(p) => p,
        Err(e) => fail(&format!("page build err={e:?}")),
    };

    // —— 启动管道自检（帧循环前一次性子 bsh——模块头注件 4；阻塞秒级，
    //    首帧前黑屏为 v1 已知观感，自检行/pipe ok 走诊断面）。
    pipe_selftest();

    // —— 持久子 bsh（自检过后 spawn 一次——模块头注件 3；boot 首 spawn
    //    Err 仍 fail 收口〔现行语义〕，respawn 路径见 try_respawn）。
    let child = match try_spawn_child() {
        Ok(c) => c,
        Err(why) => fail(why),
    };

    // —— 帧循环（常驻义务——永不返回）。
    run(hw, input_h(), ui, page, child);
}

/// 帧循环 + 输入 + 子 stdout 收割（settings `run` 的第三路扩展形：wait
/// many 至多 3 路 [hw, hi, out_s]；渲染/SUBMIT/回调/RELEASE/vote 全照
/// settings 食谱）。键帧（DEV_KEYBOARD）整帧字节透传子 bsh stdin（哑终
/// 端化 T1——本地编辑器已退役，pump.rs 分流点同形）；指针帧照 push
/// （无命中目标，零效果——预留面）。
fn run(hw: Handle, hi_in: Handle, ui: &'static mut UiMain, page: Page, child_in: Child) -> ! {
    // —— present GETINFO 轻断言（settings 同形）。
    let mut req = [0u8; 256];
    let mut rep = [0u8; 256];
    let mut rhs = [0 as Handle; 8];
    let m = match gp::encode_present_req(&mut req, &gp::PresentReq::Getinfo) {
        Some(m) => m,
        None => fail("present getinfo encode"),
    };
    let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    let pinfo = match gp::parse_getinfo_rep(&rep[..n]) {
        Some(i) => i,
        None => fail(&format!("present getinfo bad st={}", rep[0])),
    };
    if pinfo.version_present != gp::VER_PRESENT
        || pinfo.format_bits & gp::FMT_XRGB8888 == 0
        || pinfo.max_credits != gp::CREDITS_MAX
    {
        fail(&format!(
            "present getinfo vp={} fmt={:#x} credits={}",
            pinfo.version_present, pinfo.format_bits, pinfo.max_credits
        ));
    }
    let (vp_w, vp_h) = ui.viewport();
    if pinfo.display_w != vp_w || pinfo.display_h != vp_h {
        fail(&format!(
            "disp mismatch wm {}x{} present {}x{}",
            vp_w, vp_h, pinfo.display_w, pinfo.display_h
        ));
    }

    // —— 跨代清账（settings R2 修同款：前代实例孤儿别名全量排空）。
    let m = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
        Some(m) => m,
        None => fail("release encode"),
    };
    let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
    if gp::parse_release_rep(&rep[..n]).is_none() {
        fail(&format!("boot release bad st={}", rep[0]));
    }
    for h in &rhs[..nh] {
        close_h(*h);
    }

    // —— 帧缓冲 VMO 三连（settings 同形：stride = w*4 tight；len 页取整）。
    let (w, h) = (pinfo.display_w as usize, pinfo.display_h as usize);
    let stride = w.checked_mul(4).unwrap_or_else(|| fail("stride overflow"));
    let frame_bytes = stride.checked_mul(h).unwrap_or_else(|| fail("frame overflow"));
    let len = frame_bytes
        .checked_add(0xFFF)
        .map(|v| v & !0xFFF)
        .unwrap_or_else(|| fail("page round overflow"));
    let mut vmo: Handle = 0;
    if berry_vmo_create(len as u64, 0, &mut vmo) != 0 || vmo == 0 {
        fail("vmo create");
    }
    if berry_vmo_commit(vmo, 0, len as u64) != 0 {
        fail("vmo commit");
    }
    let hv = self_vmar_h();
    if hv == 0 {
        fail("no self vmar gift");
    }
    let mut va = FRAME_VA;
    if berry_vmar_map(hv, 0, vmo, 0, &mut va, len, RIGHT_READ | RIGHT_WRITE) != 0 || va != FRAME_VA
    {
        fail("vmar map");
    }
    let desc = match TextureDesc::new(w, h, stride, TextureFormat::Rgbx) {
        Ok(d) => d,
        Err(e) => fail(&format!("texture desc {e:?}")),
    };
    // SAFETY: [FRAME_VA, FRAME_VA+len) 刚由 vmar_map 落位且整段已 commit
    //（settings 同形）；run 永不返回——fb 终身持有。
    let fb: &mut [u8] = unsafe { core::slice::from_raw_parts_mut(va as *mut u8, len) };

    // —— glyph scratch 堆置（DisplayList 'a 钉调用方 scratch——paint.rs
    //    头注契约；堆置免栈穿）。
    let scratch: &'static mut [GlyphPos] =
        Box::leak(vec![GlyphPos { ch: 0, x: 0, y: 0 }; SCRATCH_GLYPHS].into_boxed_slice());

    // —— vsync 协商 + 起振（Idle→Run + 条件 FULL——首帧整帧 damage）。
    ui.set_vsync(VSYNC_NS, 0);
    ui.wake(now());

    let mut term = Term::new();
    let mut child = child_in;
    // T5 监督态（boot 子 spawn 毗邻 run 入口——spawned_at 就近取 now）。
    let mut sup =
        Super { spawned_at: now(), pending: None, strikes: 0, broken: false };
    let mut last_vote: Option<berry_ui::frame::VoteBallot> = None; // 票面变化才发
    let mut presented: u64 = 0; // FRAME_CALLBACK 过链计数（首帧诊断门）
    let mut buf_id: u32 = 1; // SUBMIT buffer 代际
    let mut hi = hi_in; // input 端（失效面置 0——纯键盘页续活降级）
    let mut input_pending = false; // INPUT_POLL 在途
    let mut input_buf = [0u8; gp::INPUT_REP_MAX];
    let mut input_hs = [0 as Handle; 8];
    let mut out_msg = [0u8; 256]; // 子 stdout 收割窗

    loop {
        let t_now = now();

        // —— T5 自动 respawn：Dead 态 2s 冷却到（熔断态除外——Enter 手动
        //    恢复）；失败续退避（try_respawn 内 pending 重臂）。
        if child.dead && !sup.broken {
            if let Some(died_at) = sup.pending {
                if t_now >= died_at.saturating_add(RESPAWN_COOLDOWN_NS) {
                    if let Some(c) = try_respawn(&mut term, &mut sup) {
                        child = c;
                    }
                }
            }
        }

        // —— 真实输入（settings 段④ 同形）：先收上一轮 INPUT_POLL 应答，
        //    再挂下一轮 poll。键帧（DEV_KEYBOARD）分流进行透传（appshell
        //    pump.rs 分流点同形——x = 翻译后字节）；指针帧 push 进引擎。
        if hi != 0 && input_pending {
            let mut nb = input_buf.len();
            let mut nh = input_hs.len();
            let s = berry_channel_recv(
                hi,
                input_buf.as_mut_ptr(),
                &mut nb,
                input_hs.as_mut_ptr(),
                &mut nh,
            );
            if s == 0 {
                input_pending = false;
                for h in &input_hs[..nh] {
                    close_h(*h);
                }
                match gp::parse_input_events_rep(&input_buf[..nb]) {
                    Some(Ok(view)) => {
                        for i in 0..view.count() {
                            if let Some(ev) = view.event(i) {
                                if ev.device_id == gp::DEV_KEYBOARD {
                                    // 键帧：哑终端透传（T1）——任意字节一
                                    // 视同仁发子 stdin（0x03/0x08/0x0A/0x09/
                                    // 0x1B/可打印零本地语义；空行 Enter 照发
                                    // `\n`——子 bsh 空行仅重显 prompt；遇
                                    // EBUSY 等可写再发——child_send 契约）。
                                    // 显示变更的醒源全在子回显（out_s 即醒
                                    // 即收）——键自身不 wake。
                                    let b = ev.x as u8;
                                    if child.dead {
                                        // 熔断态 Enter = 手动重启（T5 恢复
                                        // 路径——清零计数后重试；失败回落
                                        // 自动退避节奏）；其余键静默丢弃。
                                        if sup.broken && b == b'\n' {
                                            sup.strikes = 0;
                                            sup.broken = false;
                                            if let Some(c) =
                                                try_respawn(&mut term, &mut sup)
                                            {
                                                child = c;
                                            }
                                        }
                                    } else if !child_send(child.in_s, &[b]) {
                                        mark_child_dead(&mut term, &mut child, &mut sup);
                                    }
                                    continue;
                                }
                                // 指针帧：无命中目标的预留面（settings 同形）。
                                let pe = PtrEvent::from_wire(
                                    ev.phase,
                                    ev.pointer_id,
                                    ev.x,
                                    ev.y,
                                    ev.timestamp_ns,
                                    ev.buttons,
                                    ev.pressure,
                                );
                                if let Err(e) = ui.push_pointer(pe) {
                                    eprintln!("berry-term: input push err={e:?}");
                                }
                            }
                        }
                    }
                    Some(Err(gp::ST_EAGAIN)) => {} // 无事件常态
                    Some(Err(st)) => eprintln!("berry-term: input poll st={st}"),
                    None => eprintln!("berry-term: input poll rep bad len={nb}"),
                }
            } else if s != BERRY_ERR_AGAIN {
                eprintln!("berry-term: input recv err={s}");
                hi = 0;
                input_pending = false;
            }
        }
        if hi != 0 && !input_pending {
            if input_arm(hi) {
                input_pending = true;
            } else {
                // 输入面失效（arm 失败）——置 0 永久降级（页面常驻义务不受
                // 输入面拖累；settings 同形）。
                hi = 0;
            }
        }

        // —— 输入泵（referee——引擎单口；本页无 widget 事件消费者，组件
        //    事件排空即弃）。
        let _ = ui.pump_input_at(t_now);
        let _ = ui.take_events();

        // —— 子 stdout 收割（wait_many 命中 READABLE 或兜底醒后）：排空
        //    队列，喂行环。EOF（零长消息）= 子退出。
        if child.out_s != 0 {
            loop {
                let mut nb = out_msg.len();
                let mut nh = 0usize;
                let s = berry_channel_recv(
                    child.out_s,
                    out_msg.as_mut_ptr(),
                    &mut nb,
                    core::ptr::null_mut(),
                    &mut nh,
                );
                if s == 0 {
                    if nb == 0 {
                        // EOF：子退出（libc send_stdout_eof 收口约定）。
                        mark_child_dead(&mut term, &mut child, &mut sup);
                        break;
                    }
                    term.feed(&out_msg[..nb]);
                    continue; // 排空（队列深 8×256B——一次醒全收）
                }
                if s != BERRY_ERR_AGAIN {
                    eprintln!("berry-term: child stdout err={s}");
                    mark_child_dead(&mut term, &mut child, &mut sup);
                    break;
                }
                break; // EAGAIN = 排空毕
            }
        }

        // —— 行环 + 输入行 → 树同步（变化行 set_text；去重零冗余脏帧）。
        if term.sync(ui, &page) {
            ui.wake(now());
        }

        // —— 渲染（damage 空集零提交——跳过帧零 RPC）。
        let mut enc = CommandEncoder::new();
        let out = ui.render_frame(t_now, scratch, &mut enc);
        if out.drew {
            let dmg = out.damage;
            let full = dmg.full || dmg.count == 0; // K>0 xor FULL 立法兜底
            let pass = if full {
                PassDescriptor::clear(CLEAR_BG.0, CLEAR_BG.1, CLEAR_BG.2)
            } else {
                // 增量帧：Load+Store + K 矩形并集包络（settings 同形）。
                PassDescriptor::new(LoadOp::Load, StoreOp::Store, Some(damage_envelope(&dmg)))
            };
            let dl = enc.finish();
            if let Err(e) = Queue::new().submit(&dl, fb, &desc, &pass) {
                fail(&format!("raster submit {e:?}"));
            }

            // SUBMIT：fresh READ dup 别名随行（send 即 take；TRANSFER 保位）。
            let mut alias: Handle = 0;
            if berry_handle_duplicate(vmo, RIGHT_READ | RIGHT_TRANSFER, &mut alias) != 0
                || alias == 0
            {
                fail("dup alias");
            }
            let (flags, rects, k) = if full {
                (
                    gp::SUBMIT_F_DAMAGE_FULL,
                    [gp::Rect { x: 0, y: 0, w: 0, h: 0 }; gp::DAMAGE_MAX],
                    0usize,
                )
            } else {
                (0u8, dmg.rects, dmg.count as usize)
            };
            let sreq = gp::PresentReq::Submit {
                surface_id: 0,
                buffer_id: buf_id,
                flags,
                rate_hint: out.rate_hint_hz,
                target_present_ns: out.expected_present_ns,
                rects,
                rect_count: k,
            };
            let m = match gp::encode_present_req(&mut req, &sreq) {
                Some(m) => m,
                None => fail("submit encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[alias], &mut rep, &mut rhs);
            let token = match gp::parse_submit_ack(&rep[..n]) {
                Some(t) => t,
                None => fail(&format!("submit ack bad st={}", rep[0])),
            };
            ui.note_submitted();

            // FRAME_CALLBACK：blit-at-SUBMIT 形下帧已呈现——断 PRESENTED。
            let m = match gp::encode_present_req(
                &mut req,
                &gp::PresentReq::FrameCallback { frame_token: token },
            ) {
                Some(m) => m,
                None => fail("frame callback encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            let done = match gp::parse_frame_done_rep(&rep[..n]) {
                Some(d) => d,
                None => fail(&format!("frame done bad st={}", rep[0])),
            };
            if done.status != gp::FRAME_PRESENTED {
                fail(&format!("frame status={}", done.status));
            }
            ui.note_presented((ui.telemetry().frames() - 1) as u32, now(), done.status, done.credits);

            // RELEASE（归还别名即关——在途 ≤1 结构性背压；RETURN-ALL 语义）。
            let m = match gp::encode_present_req(&mut req, &gp::PresentReq::ReleaseWait) {
                Some(m) => m,
                None => fail("release encode"),
            };
            let (n, nh) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            let rel = match gp::parse_release_rep(&rep[..n]) {
                Some(r) => r,
                None => fail(&format!("release rep bad st={}", rep[0])),
            };
            if rel.count < 1 || nh != rel.count {
                fail(&format!("release count={} nh={}", rel.count, nh));
            }
            for h in &rhs[..nh] {
                close_h(*h);
            }

            presented += 1;
            buf_id = buf_id.wrapping_add(1);
            if buf_id == 0 {
                buf_id = 1;
            }
            if presented == 1 {
                // 首帧呈现回执过链（诊断面——console=false 走私有对）。
                println!("berry-term: first frame");
            }
        }

        // —— vote（S2 食谱：票面变化才发——省流；幂等面在服务端）。
        let ballot = ui.vote_hint();
        if last_vote != Some(ballot) {
            let vreq = gp::PresentReq::Vote {
                sticky: ballot.sticky,
                scene: ballot.scene,
                min_hz: ballot.min_hz,
                max_hz: ballot.max_hz,
                preferred_hz: ballot.preferred_hz,
            };
            let m = match gp::encode_present_req(&mut req, &vreq) {
                Some(m) => m,
                None => fail("vote encode"),
            };
            let (n, _) = rpc(hw, &req[..m], &[], &mut rep, &mut rhs);
            match gp::parse_status_rep(&rep[..n]) {
                Some(gp::ST_OK) => last_vote = Some(ballot),
                Some(st) => fail(&format!("vote st={st}")),
                None => fail(&format!("vote rep bad len={n}")),
            }
        }

        // —— 稳态等待（settings n≤2 面的第三路扩展）：wait_many 至多 3
        //    路 [hw READABLE, (hi READABLE), (out_s READABLE)] + deadline =
        //    min(next_wakeup, now+IDLE 帽)；子输出即醒即收不赌 IDLE 帽。
        //    条件路紧凑装填（n = 有效前缀——0 句柄禁入等待集）。
        let cap = now() + IDLE_CAP_NS;
        let mut deadline = ui.next_wakeup().map_or(cap, |t| t.min(cap));
        // respawn 到期点并入（T5）：待重启死亡在席时 min 加冷却到期点
        //（冷却到即醒走自动 respawn，不赌 IDLE 帽/next_wakeup）。
        if !sup.broken {
            if let Some(died_at) = sup.pending {
                deadline = deadline.min(died_at.saturating_add(RESPAWN_COOLDOWN_NS));
            }
        }
        let mut hs = [0 as Handle; 3];
        let ss = [SIGNAL_READABLE, SIGNAL_READABLE, SIGNAL_READABLE];
        hs[0] = hw;
        let mut n = 1usize;
        if hi != 0 && input_pending {
            hs[n] = hi;
            n += 1;
        }
        if child.out_s != 0 {
            hs[n] = child.out_s;
            n += 1;
        }
        let mut idx = 0usize;
        let mut obs = 0u64;
        let wst = berry_object_wait_many(hs.as_ptr(), ss.as_ptr(), n, deadline, &mut idx, &mut obs);
        if wst != 0 && wst != BERRY_ERR_TIMEDOUT {
            fail(&format!("wait err={wst}"));
        }
    }
}

/// 子死收口（EOF/挂断/发送链断共用）：行环加退出行 + best-effort reap +
/// 通道面收口（out_s 置 0 出等待集；in_s 关——后续键静默丢弃）+ T5 监督
/// 记账：存活 <2s 的快速死亡记一击（自然寿命死亡清零陈账——长命证明
/// 电路健康，防跨小时陈账误熔断）；三击熔断——停自动重启、打常驻诊断行
/// （Enter 手动恢复）；否则记 pending 死亡时点（2s 冷却后自动 respawn）。
fn mark_child_dead(term: &mut Term, child: &mut Child, sup: &mut Super) {
    if child.dead {
        return; // 幂等（收割/发送两路可能先后触发）
    }
    child.dead = true;
    let t = now();
    term.push_line(b"berry-term: bsh exited");
    // 通道收口：out_s 即关（EOF 后队列空——关不丢数据）；in_s 即关（子
    // 已死，后续透传字节静默丢弃）。
    let _ = berry_handle_close(child.out_s);
    child.out_s = 0;
    let _ = berry_handle_close(child.in_s);
    child.in_s = 0;
    // best-effort reap（EOF 先行于 TERMINATED 落账——wait 瞬回；Err = 有界
    // 泄漏，诊断不杀页面）。
    if let Some(proc) = child.proc.take() {
        match proc.reap() {
            Ok(_) => {}
            Err(()) => eprintln!("berry-term: child reap failed (bounded leak)"),
        }
    }
    // T5 监督记账。
    if t.saturating_sub(sup.spawned_at) < CHILD_MIN_LIFE_NS {
        sup.strikes += 1;
    } else {
        sup.strikes = 0;
    }
    if sup.strikes >= RESPAWN_BREAK_STRIKES {
        sup.broken = true;
        sup.pending = None;
        term.push_line(b"berry-term: respawn circuit broken (enter to restart)");
    } else {
        sup.pending = Some(t);
    }
}

/// K 矩形并集包络（增量 pass 的 dirty 域；settings 同形）。
fn damage_envelope(dmg: &berry_ui::damage::DamageSet) -> GfxRect {
    let mut x0 = u32::MAX;
    let mut y0 = u32::MAX;
    let mut x1 = 0u32;
    let mut y1 = 0u32;
    for r in &dmg.rects[..dmg.count as usize] {
        x0 = x0.min(r.x);
        y0 = y0.min(r.y);
        x1 = x1.max(r.x.saturating_add(r.w));
        y1 = y1.max(r.y.saturating_add(r.h));
    }
    let (ew, eh) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    GfxRect::new(
        i32::try_from(x0).unwrap_or(0),
        i32::try_from(y0).unwrap_or(0),
        i32::try_from(ew).unwrap_or(0).max(0),
        i32::try_from(eh).unwrap_or(0).max(0),
    )
}

/// 挂一轮 INPUT_POLL（settings `input_arm` 同形）：inputd 阻塞批拉 ≤3
/// 事件；-EBUSY = 通道满等 WRITABLE 后重试；其余错 = 输入面失效。
fn input_arm(h: Handle) -> bool {
    let mut req = [0u8; 4];
    let n = gp::encode_input_poll_req(&mut req, gp::INPUT_EVENTS_MAX as u8);
    let mut deadline = 0u64;
    loop {
        let s = berry_channel_send(h, req.as_ptr(), n, core::ptr::null(), 0);
        if s == 0 {
            return true;
        }
        if s != BERRY_ERR_BUSY {
            eprintln!("berry-term: input send err={s}");
            return false;
        }
        if deadline == 0 {
            deadline = now() + 10_000_000_000;
        }
        let mut obs = 0u64;
        if berry_object_wait_one(h, SIGNAL_WRITABLE, deadline, &mut obs) != 0 {
            eprintln!("berry-term: input send wait timeout");
            return false;
        }
        deadline = 0;
    }
}
