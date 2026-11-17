//! berry-term —— GUI 终端应用（TERMINAL；settings/files 后第三件真 GUI
//! app，GUI 终端首件）。骨架 = settings 镜像，四件终端面叠加。
//!
//! **使命**：黑底终端页 + 持久子 bsh——用户键盘行经 GUI 输入面抵达
//! （inputd 键帧出向覆载：device_id = DEV_KEYBOARD、x = 翻译后字节），
//! 本地行编辑（退格/提交），Enter 整行 + `\n` 发子 stdin 通道；子 stdout
//! 私有对端回流进 24 行定容行环，滚动 = 行环推进 + 变化行 set_text 重排。
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
//! 放不下），DEV_KEYBOARD 帧分流点内联（pump.rs :346 同形）。
//!
//! **终端四件**：
//! 1. 渲染：app 掌整帧底色（黑底 CLEAR_BG——settings 同位先例）；定容
//!    行环 24 行 × 每行 64B（= berry-ui TEXT_INLINE 现行值，逐行 Text
//!    节点 Col 叠行；节点预算 24 行 + 输入行 + chrome ≈ 27 ≪
//!    MAX_NODES=512）；滚动 = 行环推进后对变化行 set_text 重排（本表
//!    drawn 副本去重——同文跳过零冗余脏帧）。
//! 2. 行编辑器（app 自持约 60 行）：字节缓冲 + 0x08 退一格 + 0x0A 提交；
//!    Esc(0x1B)/Tab(0x09) 透传进流；其余控制码忽略（Ctrl 组合字节入忽略
//!    档——inputd translate 的 c&0x1F 形态，v1 无消费者）；箭头等扩展码
//!    inputd 表外翻译 None 不出向，天然忽略。**超长行截断策略**：行环单
//!    行超 64B 截前 64B（v1 无软换行——行宽预算 64 × cell_w 恰满 800px
//!    调试档/1080px 大档）；输入缓冲帽 63B（+自绘光标 1B = 64B 恰满）。
//!    **历史环 v2 挂账不做**（上/下箭头取回历史——等 inputd 扩展码出向）。
//! 3. 持久子 bsh：启动 spawn 一次（`--no-selftest` 跳自验链直进交互循环
//!    ——term_selftest 铁律：不带旗 = TCG 分钟级自验 + term 链无界递归）；
//!    stdin = `stdin_handle` 通道端直传（feeder 模型反面——本 app 长命
//!    持续发键，非一次性泵尽；端被 env send take，本表留对端 in_s 发送）；
//!    stdout = 私有对端；stack_pages=16（svcmgr 拉起 bsh 现值先例）；
//!    extra_gifts = rootfs/fs 双 dup（fs_gifts——term_selftest 同参）。
//!    子退出（EOF 零长消息/对端异常）→ 行环加一行
//!    `berry-term: bsh exited`，v1 不重启（监督重启归 svcmgr app 域）。
//! 4. 启动管道自检：帧循环前 spawn 一次性子 bsh（stdin_feed 静态
//!    `echo berryterm\nuname\n`）——term_collect 式早退谓词收齐四断言
//!    （提示符 ×2/命令回显行/berryterm 结果行/BerryOS aarch64 结果行），
//!    过则 `berry-term: pipe ok`；不过 park（诊断行写明缺哪个断言）。
//!    自检子 terminate(130)+reap 收口（term_selftest 先例形）。
//!
//! **稳态 wait_many 至多 3 路**：帧回调 hw + 输入 hi（条件）+ 子 stdout
//! out_s（条件）——settings n≤2 面的第三路扩展（子输出即醒即收，不赌
//! IDLE 帽）。
//!
//! **输出面**：manifest console=false → stdout 走 svcmgr 私有对（drain 归
//! app_supervise）；全部 println/eprintln 为诊断面，不构成串口标记。
//! 首帧呈现回执打一行 `berry-term: first frame`。
//!
//! **v1 降解面**：输入行独立成行（子 prompt「bsh> 」在上一行——本地
//! 编辑行不接续 prompt 后渲染；Enter 后子回显行回流自证）；`\r`/UTF-8
//! 多字节序列不 special-case（域外字节渲染等宽缺省空格面）；无指针交互
//! （纯键盘页——pointer 帧照 push 进引擎，无命中目标零效果）。

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
use berry_ui::desc::{ColProps, TextProps, TextStyle};
use berry_ui::hit::PtrEvent;
use berry_ui::layout::CrossAlign;
use berry_ui::theme::Density;
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

/// 行环可见行数（节点预算：24 行 + 输入行 + chrome ≈ 27 ≪ MAX_NODES=512；
/// 25 行 × cell_h 在 800×600 调试档（S8x16）= 400px、1080×1920 大档
/// （S16x32）= 800px，双档 viewport 内收敛）。
const TERM_ROWS: usize = 24;

/// 单行字节帽 = berry-ui `TEXT_INLINE` 现行值 64（TextBuf loud 拒面）。
/// 超长行截前 64B（v1 无软换行——见模块头注「超长行截断策略」）。
const LINE_MAX: usize = 64;

/// 输入缓冲帽（63B + 自绘光标 1B = 64B 恰满单 Text 帽）。
const EDIT_MAX: usize = LINE_MAX - 1;

/// 行环总收容帽（自检收集环同值——子输出整场字节上界，防无界 Vec）。
const COLLECT_MAX: usize = 32 * 1024;

/// 收集环活动帽（term_collect 同值：收包刷新、帽尽即收场）。
const COLLECT_TIMEOUT_NS: u64 = 10 * 1_000_000_000;

/// 子 bsh 栈页数（term_selftest 先例值：svcmgr 拉起 bsh 现值 16 页——
/// musl std 大程序口径）。
const CHILD_STACK_PAGES: u64 = 16;

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
    let big = Density::for_viewport(vp_w, vp_h) == Density::Product;
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

/// 键动作（on_key 的返程——帧循环侧执行通道发送）。
enum KeyAct {
    /// 无操作（忽略的控制码/空缓冲退格）。
    Ignored,
    /// 显示面变更（编辑缓冲进/退——需重排输入行）。
    Edited,
    /// 透传字节进流（Esc/Tab）。
    Pass(u8),
    /// 提交（编辑缓冲成行——帧循环侧组 `行+\n` 发通道）。
    Submit,
}

/// 终端状态（行环 + 尾段 + 行编辑器 + 上帧渲染副本）。
struct Term {
    /// 行环内容（`head` 起逻辑序；满环后 head 推进顶掉最老行）。
    rows: [[u8; LINE_MAX]; TERM_ROWS],
    /// 各行有效长。
    lens: [usize; TERM_ROWS],
    /// 最老行物理下标。
    head: usize,
    /// 在册行数（0..=TERM_ROWS）。
    used: usize,
    /// 无 `\n` 尾段（活动行——子 prompt「bsh> 」等无换行输出的驻留位）。
    tail: [u8; LINE_MAX],
    tail_len: usize,
    /// 行编辑器字节缓冲（Enter 提交发通道）。
    ed: [u8; EDIT_MAX],
    ed_len: usize,
    /// 上帧渲染副本（24 行 + 输入行——同文跳过 set_text，零冗余脏帧）。
    drawn: [[u8; LINE_MAX]; TERM_ROWS + 1],
    drawn_len: [usize; TERM_ROWS + 1],
}

impl Term {
    fn new() -> Term {
        Term {
            rows: [[0u8; LINE_MAX]; TERM_ROWS],
            lens: [0usize; TERM_ROWS],
            head: 0,
            used: 0,
            tail: [0u8; LINE_MAX],
            tail_len: 0,
            ed: [0u8; EDIT_MAX],
            ed_len: 0,
            drawn: [[0u8; LINE_MAX]; TERM_ROWS + 1],
            drawn_len: [0usize; TERM_ROWS + 1],
        }
    }

    /// 行环推进（单行入环；超 64B 截前 64B——见模块头注截断策略）。
    fn push_line(&mut self, bytes: &[u8]) {
        let n = bytes.len().min(LINE_MAX);
        if self.used < TERM_ROWS {
            let idx = (self.head + self.used) % TERM_ROWS;
            self.rows[idx][..n].copy_from_slice(&bytes[..n]);
            self.lens[idx] = n;
            self.used += 1;
        } else {
            // 满环：head 位即被顶掉的最老行（覆写 + 推进）。
            self.rows[self.head][..n].copy_from_slice(&bytes[..n]);
            self.lens[self.head] = n;
            self.head = (self.head + 1) % TERM_ROWS;
        }
    }

    /// 子输出喂流：按 `\n` 切行入环；无换行残余驻尾段（活动行续写）。
    fn feed(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if b == b'\n' {
                let (t, n) = (self.tail, self.tail_len);
                self.push_line(&t[..n]);
                self.tail_len = 0;
            } else if self.tail_len < LINE_MAX {
                self.tail[self.tail_len] = b;
                self.tail_len += 1;
            }
            // 尾段超 64B 丢弃后续字节（截断策略同push_line——前 64B 保真）。
        }
    }

    /// 键事件消费（HostHooks::on_key 同签名——appshell pump.rs 键帧分流
    /// 点的内联消费面；`code` = inputd 翻译后字节）：可打印进缓冲、
    /// 0x08 退一格、0x0A 提交、Esc/Tab 透传、其余控制码忽略。显示面变
    /// 更即 wake（键响应零等待下一拍）。
    fn on_key(&mut self, ui: &mut UiMain, code: u8, _ts_ns: u64) -> KeyAct {
        match code {
            0x0A => {
                // Enter 提交：缓冲成行（调用方组 `\n` 发通道）；子回显行
                // 回流后输入行清空即见（bsh 交互编辑器自带回显）。
                if self.ed_len == 0 {
                    return KeyAct::Ignored; // 空行 Enter：不发（bsh 空行仅重显 prompt）
                }
                ui.wake(now());
                KeyAct::Submit
            }
            0x08 => {
                if self.ed_len > 0 {
                    self.ed_len -= 1;
                    ui.wake(now());
                    KeyAct::Edited
                } else {
                    KeyAct::Ignored
                }
            }
            0x1B | 0x09 => KeyAct::Pass(code), // Esc/Tab 透传（子编辑器的补全/中断面）
            c if c >= 0x20 => {
                if self.ed_len < EDIT_MAX {
                    self.ed[self.ed_len] = c;
                    self.ed_len += 1;
                    ui.wake(now());
                    KeyAct::Edited
                } else {
                    KeyAct::Ignored // 缓冲满：丢弃（帽 63B 见模块头注）
                }
            }
            _ => KeyAct::Ignored, // 其余控制码（Ctrl 组合字节等）忽略
        }
    }

    /// 可见行内容（屏顶起第 `vi` 行）：逻辑行序 = 完整行 + 尾段居末；满环
    /// 且有尾段时顶掉最老行保尾段可见（prompt 恒在屏）。
    fn view_line(&self, vi: usize) -> &[u8] {
        let start = if self.used == TERM_ROWS && self.tail_len > 0 { 1 } else { 0 };
        let li = vi + start;
        if li < self.used {
            let idx = (self.head + li) % TERM_ROWS;
            &self.rows[idx][..self.lens[idx]]
        } else if li == self.used && self.tail_len > 0 {
            &self.tail[..self.tail_len]
        } else {
            &[]
        }
    }

    /// 行环 + 输入行 → 树同步（变化行 set_text；drawn 副本去重）。返回
    /// 是否有变更（调用方据此 wake——feed/on_key 路已各自 wake，此处返值
    /// 为防御面）。行内容先拷本地再比对（view_line 借 self——与 drawn 写
    /// 互斥，借毕即放）。
    fn sync(&mut self, ui: &mut UiMain, page: &Page) -> bool {
        let mut dirty = false;
        for vi in 0..TERM_ROWS {
            let mut tmp = [0u8; LINE_MAX];
            let n = {
                let b = self.view_line(vi);
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
        // 输入行：缓冲 + 自绘光标（帽 63+1=64 恰满）。
        let mut buf = [0u8; LINE_MAX];
        buf[..self.ed_len].copy_from_slice(&self.ed[..self.ed_len]);
        buf[self.ed_len] = CURSOR;
        let n = self.ed_len + 1;
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

/// 自检早退谓词（term_selftest `term_ready` 同形）：四断言物证全在席
/// （第二提示符在第一结果行之后——谓词必经两命令全链）。
fn term_ready(d: &[u8]) -> bool {
    count_sub(d, b"bsh> ") >= 2
        && has_sub(d, b"echo berryterm")
        && has_sub(d, b"\nberryterm\n")
        && has_sub(d, b"BerryOS aarch64")
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
/// `echo berryterm\nuname\n`，四断言齐即收 → terminate(130)+reap 收口。
/// 过则 `berry-term: pipe ok`；不过 park（诊断行写明缺哪个断言）。
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
        stdin_feed: Some(b"echo berryterm\nuname\n"),
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

/// 持久子 bsh spawn（模块头注件 3）：stdin_handle 通道端直传（env send
/// take 迁交——子 STDIN_H recv 面；本表留对端 in_s）；stdout 私有对端
/// （spawn 内部 dup RIGHT_SEND 交付，原值 out_c 留本表即关——term_
/// selftest 收口同款）；`--no-selftest` + fs 双礼物 + stack 16 页。
fn spawn_child() -> Child {
    let full = b"bin/bsh";
    let Some((data_off, data_len)) = cpio_find(rootfs_h(), full) else {
        fail("bsh not found");
    };
    let (rootfs_dup, fs_dup) = match fs_gifts() {
        Some(g) => g,
        None => fail("fs gift dup failed"),
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
            fail("bsh spawn failed");
        }
    };
    // out_c 原值留本表（stdout 内部 dup 交付子表）——spawn 后即关；in_c
    // 已迁移（子表 STDIN_H），本表零副本。
    let _ = berry_handle_close(out_c);
    Child { in_s, out_s, proc: Some(child), dead: false }
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

    // —— 持久子 bsh（自检过后 spawn 一次——模块头注件 3）。
    let child = spawn_child();

    // —— 帧循环（常驻义务——永不返回）。
    run(hw, input_h(), ui, page, child);
}

/// 帧循环 + 输入 + 子 stdout 收割（settings `run` 的第三路扩展形：wait
/// many 至多 3 路 [hw, hi, out_s]；渲染/SUBMIT/回调/RELEASE/vote 全照
/// settings 食谱）。键帧（DEV_KEYBOARD）分流进行编辑器（pump.rs 分流点
/// 同形）；指针帧照 push（无命中目标，零效果——预留面）。
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

        // —— 真实输入（settings 段④ 同形）：先收上一轮 INPUT_POLL 应答，
        //    再挂下一轮 poll。键帧（DEV_KEYBOARD）分流进行编辑器（appshell
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
                                    // 键帧：on_key 消费（编辑器进/退/提交/
                                    // 透传——通道发送在本块执行）。
                                    let act = term.on_key(ui, ev.x as u8, ev.timestamp_ns);
                                    match act {
                                        KeyAct::Submit => {
                                            // 行 + \n 发通道（遇 EBUSY 等可写
                                            // 再发——child_send 契约）。
                                            let mut line = [0u8; EDIT_MAX + 1];
                                            line[..term.ed_len]
                                                .copy_from_slice(&term.ed[..term.ed_len]);
                                            line[term.ed_len] = b'\n';
                                            if !child.dead
                                                && !child_send(child.in_s, &line[..term.ed_len + 1])
                                            {
                                                mark_child_dead(&mut term, &mut child);
                                            }
                                            term.ed_len = 0;
                                        }
                                        KeyAct::Pass(b) => {
                                            if !child.dead && !child_send(child.in_s, &[b]) {
                                                mark_child_dead(&mut term, &mut child);
                                            }
                                        }
                                        KeyAct::Edited | KeyAct::Ignored => {}
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
                        mark_child_dead(&mut term, &mut child);
                        break;
                    }
                    term.feed(&out_msg[..nb]);
                    continue; // 排空（队列深 8×256B——一次醒全收）
                }
                if s != BERRY_ERR_AGAIN {
                    eprintln!("berry-term: child stdout err={s}");
                    mark_child_dead(&mut term, &mut child);
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
        let deadline = ui.next_wakeup().map_or(cap, |t| t.min(cap));
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
/// 通道面收口（out_s 置 0 出等待集；in_s 关——后续键静默丢弃）。
/// v1 不重启（模块头注件 3）。
fn mark_child_dead(term: &mut Term, child: &mut Child) {
    if child.dead {
        return; // 幂等（收割/发送两路可能先后触发）
    }
    child.dead = true;
    term.push_line(b"berry-term: bsh exited");
    // 通道收口：out_s 即关（EOF 后队列空——关不丢数据）；in_s 即关（子
    // 已死，后续行/透传字节静默丢弃）。
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
