# Veda consent 机制深读：分类学 · 执行链 · 声明面 · 最小权限 · BerryOS 映射

> 调研日期 2026-11-08 · 方法：Veda 本地全量 clone（/tmp/os-hist/Veda，HEAD `fbf1763`，2026-11-04）逐文件实读——documentation/AGENT.md 全文（333 行）+ ARCHITECTURE.md §The voice agent（1072–1104 行）为纲，下探 agent 服务全部 25 个源文件中的 8 个关键件（approvals/policy/worker/server/admin/files/store/registrar）+ init 注册身份 + filesystem 访问控制 + Settings/Terminal 应用侧声明；对照基准 = research/24 号（原 12）§2.2 三条记录 + `docs/能力模型设计-v1.0.md`（§1–§8）+ `docs/AI调度设计-v1.0.md`（§3 权限节）。
>
> 本文为 21 号专题，是 24 号 §2.2 三条（consent by identity / 锁屏抑制 / presence 语义）的下探补全；结论均注出处（Veda 路径相对仓根，`文件:行号`），不自造事实。
>
> **许可纪律（先于一切结论）**：Veda 为 **GPL-3.0-only**（15 号专题经 SPDX 头实证，`bkl.rs`/本系列文件头同标）。本文全部内容为**机制语义参照**，BerryOS（Apache-2.0）对其执行 GPL 纪律：**语义可借、代码一字不进**（规则 13 同口径），§5 吸收清单均以此口径给出。

## 1. 同意的完整分类学（问题①）

**三档，判据轴是「可撤销性 × 影响范围」，不是概率风险。** `Risk` 枚举三值且有序（`Routine = 1 / Sensitive = 2 / Destructive = 3`，`implementation/libraries/proto/source/agent.rs:33-46`），文档注释直接给出判据：Routine = 读或易于撤销的常规动作（open/play/scroll/create）不问；Sensitive = 改动或发送要紧之物（覆写文件/加入网络/跑命令）要问、除非用户勾了 always allow；Destructive = 删数据/弃工作/影响全系统（删文件/杀任务/关机）每次都问（AGENT.md:113-117 同义表格）。

**可撤销性是主判据，且粒度到操作语义而非文件语义。** 系统函数的集中风险表（`implementation/services/agent/source/tools.rs:308-329`）：`files.delete` 归 Sensitive 的理由写在代码注释里——删掉的东西可从 Trash 恢复，直到清空；`empty_trash` 才是 Destructive；`write` 只在目标路径**已存在**（替换）时 Sensitive，写新文件 Routine；copy/move/rename 因「文件系统拒收已存在的目的地」而永不替换、move 回去即撤销，全 Routine；`wifi disconnect/off` Sensitive（断网可能断掉 agent 自己）；`memory.forget` 忘一条 Routine、一次忘多条 Destructive（tools.rs:323-324）；`system restart/shutdown`、`tasks end` Destructive。**这与 24 号 §2.2 的粗记录一致，但「单条遗忘不需同意」是个初看反直觉的细节**：依据是 memory 只记用户明说的话、遗忘也只遗忘用户点名的条目（"forget that" forgets nothing，AGENT.md:197），单条遗忘被视作用户显式意图的执行而非代理决策。

**「批准的即执行的」（made definite before classified）是分类学的前置纪律。** 调用在判档前先被钉死成确定形态：`memory.forget` 先由 `Memory::pin_forget` 把「要忘的东西」解析成 id 清单（worker/mod.rs:352-357，审批单上逐条列出）；move/copy/rename 的目的地由 `destination()` 按「实际执行时的规则」预演出来（裸名 rename 留在本目录、目的地是目录则收到自己名下，`implementation/services/agent/source/files.rs:54-75`）——用户在审批单上看到的目标就是之后真正执行的目标（AGENT.md:119-121）。

**参数依赖的风险用「双动作拆分」表达，而非运行时猜测。** Terminal 把跑命令拆成 `run_command`（Sensitive，可 always allow，且自拒删/替换/移动类命令）与 `run_risky_command`（Destructive）：`implementation/applications/terminal/source/agent.rs:60-66` 及头部注释（分类由 `terminal_core::classify` 做）；Text Editor 同法拆 `save`（只覆写 agent 自己存的）与 `save_as`（可覆写任何文件）（AGENT.md:122-125）。

**always allow 只开放给中档。** `Permissions::allow_always` 对 `risk != Sensitive` 一律拒绝（policy.rs:104-110），审批单的 always 复选框也只在 `risk == Sensitive` 时出现（approvals.rs:46）——Destructive 永不可常开，Routine 无需常开，档位与可授权性闭环。

**Risk 表之外还有四道独立门，共同构成完整分类面**：① 无人值守会话（提醒/通知触发的对话）不能 `timer set`，防通知自唤醒循环（tools.rs:352-354 `allowed_unattended`；AGENT.md:153-155）；② 锁屏即断：对话结束、停止听名，锁屏期间谁也问不动它（AGENT.md:156-164）；③ 防火墙对 agent 零可达——Settings 无此动作、网络服务只听 Settings、问题只有 shell 能答，三重独立锁（AGENT.md:137-140）；④ agent 改自身名字/换模型也走同意——Settings 把 `set_agent_model`/`rename_agent` 以 `Risk::Sensitive` 注册为 agent-compatible 动作（`implementation/applications/settings/source/control.rs:53-59`），API key 则根本不提供动作（AGENT.md:209-210）。

## 2. consent-by-identity 执行链（问题②）

**发起**：语言模型的函数调用经 Deepgram WebSocket 到达会话线程，包装成 `Job{id, name, args, approved: false}` 入 worker 队列（worker/mod.rs:66-73、150-153）。worker 是独立线程，一切慢操作（文件/应用调用/launcher/shell/网络）都在它上面，不占会话线程（worker/mod.rs:15-22）。

**拦截发生在 worker 的分类步，晚于「钉死」、早于「执行」**：`Ctx::call` 先 pin（见 §1），再定风险——系统函数查集中表 `tools::risk`（判 `write` 替换与否时用 `exists` 回调查真实文件系统，worker/mod.rs:302-303）；`use_app` 则从**持久化的应用自述**里取该动作的 `spec.risk`（worker/apps.rs:217-224）。然后 `needs_approval(key, risk)` 查 Permissions：Routine 恒 false、Sensitive 看是否 always-allowed、Destructive 恒 true（policy.rs:95-101）。需要同意就返回 `Outcome::Approval(Pending{name, 定档后的 args, app, action, detail, risk, key})`——**该调用绝不先跑**（worker/mod.rs:305-322；模块文档 "nothing that needs the user's consent runs before the user gave it"，worker/mod.rs:9-10）。

**主循环 `ask()` 挂起并告知模型**：分配自增 approval id，推入待批列表，经 Ui link 向 shell 发 `AgentEvent::Approval{request}`，同时立即给语言模型回 JSON `"waiting_for_approval": true`（approvals.rs:128-152）——会话不阻塞，结果之后以 notice 补达（AGENT.md:36-39）。

**shell 批：唯一收件人是身份验证过的 desktop shell。** 审批请求显示在 agent 窗口（窗口关闭时降级为通知），展示将发生什么、作用于什么，3 分钟未答自动过期（`APPROVAL_TTL_NS = 180s`，approvals.rs:28、182-196；AGENT.md:130-135；shell 侧组件 `implementation/shell/components/desktop/source/agent.rs` 头注释）。回答走 `agent::decide(id, allow, always)`，server 端先过 `Client::shell()`：`who.service && who.name == "shell"` 否则 `Denied`（server.rs:79-82、109-112）。**身份不是自称的**：init 的 registry 给每条连接附着 `ClientIdentity{koid, name, service, app, requested}`——由 init 从连接属主进程反查自己启动的子进程表得出（`implementation/services/init/source/main.rs:191-205`），随 `LISTENER_CONNECT` 事件随连接一并投递（init/main.rs:386-393）；服务名由 init 分配，别的程序抢不走（AGENT.md:126-129）。主循环文档直言分工：只有 shell 能接界面/答审批/唤醒/催眠/静音，只有 Settings 能改配置（main.rs:27-30）。

**批准后的重放与记账**：`decide` 把请求移出列表、记控制台日志（"approval N allowed/declined"）；`always && Sensitive` 时写入 Permissions 并持久化；然后以 `Job{id: "approved-{id}", approved: true, args 原样}` 重新入队执行（approvals.rs:156-179）。执行结果不再回 Deepgram 的函数调用（早已答过），而是以 Notice 告知会话：成功 `Done`、失败 `Failed`、没跑成 `NotDone`（approvals.rs:60-79）。

**打断语义**：用户再说话触发 `FunctionCallCancelled`——未开始的调用直接出队；已开始的跑完；若 `changes_something`（tools.rs:359-374 的白名单判定）则发 `RanAnyway` notice 告知「用户已走开但它还是做了什么」；被取消且本会 asking 的调用什么都不做（approvals.rs:9-14、81-88、113-124）。

**审计形态（诚实记录：没有独立审计日志）**。落地为三处分散记录：① 每步 `println!` 控制台日志（审批请求/允许/拒绝、函数结果截断 200 字符、"ran although the user had moved on"，approvals.rs:70、85、104、118、135、164）；② 会话内 Notice 六种（Done/Failed/NotDone/Declined/Expired/RanAnyway，notice.rs:28-44）——给用户听、给模型看，不落盘；③ `permissions.json` 只记 always-allow 授予项（policy.rs:122-127）；另 memory.json 存「最近对话的最后几轮」（AGENT.md:191-192）算弱旁证。**没有「谁在何时批准了什么」的持久 append-only 账本**——这是与「consent 机制最深」印象的主要落差，BerryOS 吸收时应补上（§5）。

## 3. 权限的声明与变更面（问题③）

**应用动作：纯动态自述注册，无静态 manifest 参与。** 任何程序都可 `register_app` 交出一条 serving `agentapp` 协议的 channel（registrar.rs:6-8）；registrar 独立线程向它 `describe` 要 `AppAgentInfo{summary, actions[ActionSpec{name, description, params, risk}], has_state}`（AGENT.md:214-218、226-240）。**自述先过 `sanitize` 边界再入内**：至多 64 动作 × 16 参数、名字仅限字母数字 `_`/`-` 且不重名、描述压成至多 600 字符的单行、剥除 `[System]` 标记（tools.rs:503-541；AGENT.md:289-294）——防的是 prompt 注入与目录爆破，不是权限虚报（见 §6 信任边界）。消毒后的目录持久化进 `apps.json`（agent 重启后不丢，shared.rs:222-235），并在给模型的动作清单里逐条标注 `needs_approval`（tools.rs:544-558）。注册有配额：同进程每分钟至多 4 次（超者该分钟拒收）、排队待询问至多 16 个、每进程一条（新替旧）（registrar.rs:31-35；policy.rs:47-80；AGENT.md:294）。

**系统函数：风险表是集中静态代码**（tools.rs 的 `functions()` 定义 + `risk()` 判定），应用改不了（policy.rs 模块文档 "The decision is the agent service's, enforced before anything runs; the language model cannot skip it"，policy.rs:11-12）。

**变更面按连接身份三向互斥**：① shell 服务连接（`service && name=="shell"`）独占 attach_ui/decide/wake/sleep/mute（server.rs:93-150），对 config/key/permissions 类请求一律 `Denied`（server.rs:152-194）；② Settings 连接（`app && name=="settings"`）在 accept 时即被分流到 admin 专用线程（server.rs:31-39），独占 set_config/set_api_key/check_key/catalog/preview_voice/memories/forget/forget_all/**permissions 列表与 revoke**（admin.rs:224-373），对 decide/wake 等一律 `Denied`（admin.rs:200-221）；③ 其余任意程序只能 status 与 register_app（server.rs:5-9）。Settings 界面上的 "Always allowed" 页就是 `permissions()/revoke()` 的皮（AGENT.md:207；policy.rs:112-114）。

**持久授权的生命周期以「声明源」为准**：`permissions.json` 读回时逐 key 过 `known_key` 白名单——应用动作必须仍在当前 apps 目录里且风险档为 Sensitive，系统函数操作必须过 `may_always_allow`（policy.rs:39-45；shared.rs:154-157）；测试实证「种植的 permissions 文件授不出任何不该有的权」（policy.rs:226-230）。推论：应用下架或撤回动作声明，对应的 always-allow 授权自动失效——授权不能脱离声明源存活。

## 4. 私有存储与放弃访问：最小权限模型全貌（问题④）

24 号只记了半句「worker 先放弃私有访问再逐路径检查」，全貌是**通道无能 + 调用点校验的双层结构**：

**底座：per-service 私有目录 + 按注册身份执法。** `/home/.private/<service>` 归同名系统服务：只有属主服务能进自己的目录、只有服务能进 `/home/.private` 本身、目录列表对客户端隐藏他人私有项——全部由文件系统服务按 registry 身份（`Client{service, name}`）在 `may_reach/may_change/sees` 三查里强制（`implementation/services/filesystem/source/access.rs:44-83`）。`/home`、用户主目录、`/tmp`、`/home/.private`、`/home/packages` 是不可移动的固定位置（防「把私有目录搬出守卫辖区」），`/home/packages` 只有 package-manager 能改（access.rs:17-24、64-89）。

**第一层（通道无能）：双连接 + 不可逆放弃。** agent 持两条独立 fs 连接：Store 一条，是唯一保留私有访问的连接，管 `/home/.private/agent` 下的五件东西（config.json / deepgram-key / memory.json / permissions.json / timers.json，store.rs:21-33）；worker 一条专跑用户文件动作，建连即调 `renounce_private()` **永久放弃**私有访问——放弃后该连接被文件系统视同应用连接（access.rs:9-11），且**放弃失败则整条连接作废、文件动作面全禁**（worker/mod.rs:210-227 的 `user_files()`：宁可没有文件功能，不可留有特权通道）。设计理由写在模块注释：worker 处理的路径来自语言模型、文档和**任何程序都能写的 Trash 记录**，这条连接根本不需要私有访问（worker/mod.rs:24-26）。

**第二层（调用点校验）：逐路径四函数。** 即便通道已无能，agent 侧仍对每个路径过 `agent::files`：`resolve` 规范化后拒绝一切落入 `/home/.private` 的拼法（`~/../.private`、`..` 变体全挡，files.rs:31-37 + 测试 96-111）；`writable` 进一步把创建/修改限定在 home 或 `/tmp` 之内（files.rs:41-48）；`destination` 按 §1 所述预演移动目的地且结果必过 `writable`；`restore_target` 把 Trash 记录里的原位置当**不可信输入**——必须是 home 内的绝对路径且非根非空，否则拒绝恢复（files.rs:76-90）。

**配套两件**：私有文件的崩溃原子替换（先写 `*.new` 临时名再一步 `replace`，崩溃只会留下旧或新、绝不两无；读侧还能从残留临时文件恢复，store.rs:14-16、54-98）；Deepgram key 只存私有目录、永不回显（Settings 只见尾字符 hint，admin.rs:148-162）、永不进 prompt、只经 TLS 发 Deepgram 自家服务（AGENT.md:179-185）。ARCHITECTURE 的 §The voice agent 四条 bullet 与此完全一致（ARCHITECTURE.md:1097-1100：private store / 独立连接已放弃私有访问 / checks every path）。

## 5. 与 BerryOS 能力模型的映射（问题⑤）

对照物：`docs/能力模型设计-v1.0.md` §1–§8（纯能力、handle=(对象,代号)、duplicate 收紧、Job revoke、§8 应用层是皮）+ `docs/AI调度设计-v1.0.md` §3（权限节：复用 `RIGHTS_SET_POLICY` 不新开特权类、Job 级 handle、作用域受能力约束）与 §4 末条（注入权本身也是能力，进委托链审计）。

### 5.1 已有等价物（Veda 机制 → BerryOS 现状）

| Veda 机制 | 出处 | BerryOS 等价物 | 出处 | 判语 |
|---|---|---|---|---|
| init registry 按连接附身份、服务名不可冒取 | init/main.rs:191-205；AGENT.md:126-129 | 引导通道能力递归派生 + 持证身份非特权身份（连接握的 handle 即授权，名字查证都不需要） | 能力模型 §5、§8 | 结构等价且更强：handle 不可伪造 vs 名字不可冒取 |
| 只有 shell 能答审批 | server.rs:79-82 | 审批端点 channel handle 在派生时定向只交接给 shell 服务 | 能力模型 §2（transfer）、§5（init 派生） | 等价，BerryOS 表达更省（免 identity 查询） |
| only Settings 改 agent 设置 | server.rs:31-39；admin.rs | 配置服务的写 handle 不发给 agent 以外者 | 能力模型 §1（无全局权限位） | 等价 |
| always allow 仅 Sensitive、可收回 | policy.rs:104-114 | duplicate 单调收紧、close 吊销（授权永不增权） | 能力模型 §1、§2；AI调度 §3 | 语义同型：中档可委托、高档不可常开 ↔ 权限只能收不能放 |
| permissions.json 读回过 known_key 白名单（防种植、随声明源失效） | policy.rs:39-45；shared.rs:154-157 | 委托链审计：设置服务能列「谁把调权能力给了谁」，账本在内核 handle 表 | AI调度 §4 末条；能力模型 §4 | BerryOS 更强：账本内核全知、用户态不可种植 |
| worker 双连接 + renounce_private | worker/mod.rs:210-227；access.rs:9-11 | per-process handle 表天然按进程隔离；「放弃」= 该连接根本不发私有目录 handle；Job 树 revoke 批量回收 | 能力模型 §4 | 结构等价；Veda 的「连接粒度放弃」在 BerryOS 即 close/不发 |
| 防火墙零可达（三重锁） | AGENT.md:137-140 | 不发 handle 即无面（一重即够） | 能力模型 §1 | BerryOS 单机制覆盖 Veda 三重防御 |
| intent 六档 / band 词汇表 / SET_POLICY Job 作用域 | —— | ADR #42/#58 | AI调度 §3、§5 | 正交对照：那是调度侧分级，同意面 Risk 三档是动作侧，两表不合并 |

### 5.2 P1-c 同意面应吸收的语义增量（只借语义、永不搬码）

1. **三档分类学及其判据语义**：以可撤销性为主轴（可从 Trash 恢复的删除 = 中档、清空 = 高档、写新文件 = 低档、替换 = 中档）——比「风险概率」更可判定，适合做进 P1-c 的动词权限词表。
2. **made-definite-before-classification**：批准单展示的参数即执行参数（id 钉死、destination 预演）——防「批准 A 执行 B」的 TOCTOU 语义。
3. **always allow 仅中档 + 授权以声明源为根 + 读回白名单**：委托随声明消失而失效、种植数据授不出权——与 SET_POLICY 委托链审计对接时照抄此生命周期语义。
4. **审批时效与打断语义**：TTL 过期即失效、已启动的跑完不假装没发生、`changes_something` 判定后**事后告知**（RanAnyway）——「诚实的代理」最小集。
5. **双动作拆分模式**（run_command/run_risky_command、save/save_as）：参数依赖的风险不要运行时猜，拆成两个不同档位的声明动作——typed action 协议设计可直接吸收。
6. **无人值守会话禁自唤醒动作**：提醒触发的对话不能设 timer——防 notice 自循环，BerryOS agent 服务同型场景直接适用。
7. **注入防护三件**：应用自述进 prompt 标记为「它自己的话」、notice 数据剥 `[System]` 标记、prompt 明写 information-never-instructions（prompt.rs:84-87、124；AGENT.md:141-146；notice.rs:24）。
8. **持久审计账本（补 Veda 的缺口）**：Veda 无 append-only 审批日志（§2 诚实记录）；BerryOS 委托链审计已有「谁给谁能力」的账，P1-c 应把「何时批准了什么档的动作」并进同一账本，而非复制 Veda 的分散记录。
9. **逐路径校验含不可信恢复目标**：Trash 记录任意程序可写 → 恢复目标当不可信输入校验（files.rs:76-90）——BerryOS 文件 agent 面同款威胁，语义直接适用。

Veda 其余件已有归属：agentapp describe/state/invoke 三动词 → berryui typed action 协议（24 号已列 P2）；锁屏抑制 + presence 语义 → 移动档交互（24 号已列 P2）；`display::user_active` 仅 agent 可调的独占授权（ARCHITECTURE.md:696-700）→ 等价于 BerryOS 专属 handle 单发，无需新增量。

## 6. 与预期不符处与信任边界（诚实记录）

- **无持久审计日志**：预期「同意机制最深」应带账本，实际审计分散于 println/Notice/permissions.json 三处（§2）——P1-c 勿照抄。
- **应用风险档是自报的**：`sanitize` 只消毒文本与数量边界，不改报的 `risk`（tools.rs:508-541 无 risk 改写分支）；恶意应用可把破坏性动作低报为 Routine 跳过同意。缓解是结构性的：动作在应用自己进程的主线程上以其自身访问执行（AGENT.md:293；worker/apps.rs:235-244 只转发 invoke），agent 不是提权通道——低报只让「问用户」这一环失效，不让应用获得新权限。BerryOS 吸收时应用动作的风险档应**对照静态 manifest 声明**而非纯运行时自报（应用模型「持证身份」可承载）。
- **24 号 §2.2 三条与源码全部相符**，无出入：consent by identity（server.rs:79-82 + init/main.rs:191-205）、锁屏抑制（AGENT.md:156-164；lock.rs 独立文件在册）、presence 语义（AGENT.md:165-178，谁在场四条白名单：用户点的/说进来的/agent 答话中的算，听名阶段的、提醒触发的对话不算）。
- **Terminal 语义比文档表格更保守**：AGENT.md:116 把「不丢东西的终端命令」归 Sensitive 已算全命令要问；源码实证 `run_command` 自报 Sensitive 且**自拒**删/替换/移动类命令、只有 `run_risky_command`（Destructive）能跑（terminal/agent.rs:10-14、60-66）——「应用自我设限 + 档位声明」双管齐下，这是 §5.2 第 5 条的实证依据。
- **agent 改自身配置的同意走的是 Settings 应用注册**，不是特权旁路：Settings 以普通 agent-compatible 应用身份注册 `set_agent_model`/`rename_agent`（Sensitive，settings/control.rs:53-59），admin.rs:195-198 注释明说 "Settings offers the agent its own actions like any application"——同意机制对 agent 自改闭环，此点文档未直说、源码可证。

## 7. 出处清单

- Veda 本地 clone：/tmp/os-hist/Veda @ `fbf1763`（2026-11-04）；主文档 documentation/AGENT.md（333 行全文）+ documentation/ARCHITECTURE.md §The voice agent（1072-1104 行）。
- Veda 源码（路径相对仓根）：implementation/services/agent/source/{approvals,policy,shared,tools,files,store,server,admin,registrar,notice,prompt,main}.rs、source/worker/{mod,apps}.rs、implementation/services/init/source/main.rs、implementation/services/filesystem/source/access.rs、implementation/libraries/proto/source/agent.rs、implementation/applications/{settings/source/control.rs, terminal/source/agent.rs}、implementation/shell/components/desktop/source/agent.rs。
- BerryOS：docs/能力模型设计-v1.0.md（§1–§8）、docs/AI调度设计-v1.0.md（§3、§4、§5）。
- 姊妹篇：research/24-2026新OS全景与四家深读.md §2.2 与吸收表、research/15-三家源码深读-moss-urthr-veda.md（GPL-3.0-only 实证）、research/17（三层收敛判据）。
