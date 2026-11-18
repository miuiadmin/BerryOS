# export 内建与 spawn env 面刀（processargs v1.1）

Status: implemented

一句话说明：owner 令「继续实施」（2026-11-17 深夜）v2 系列首刀——规格轮 workflow（wf_f488dffa-aaf：spawn 链/bsh 与消费面/契约碰撞三域侦察 + 编码 ABI/语义价值/风险半径三视角设计 + 综合）+ 实施轮 workflow（wf_bed6b85b-6c4：S1∥S3 → S2 → S4 → 两域对抗审查）+ 主线修复集成三段式；交付 export 第 21 内建（全导出语义）＋processargs v1.1 env 第二消息（通道级 add-only：消息一恒形、env 仅非空且组装非零时发送、子侧 -EAGAIN 探测缺席）＋berry-libc environ/getenv/setenv/unsetenv 真身＋bsh 三产点接线与 environ 播种；内核零改、七面零滚新（CLI ok n=10→11 前缀式）、零新 bin 零新 crate；补齐「赋值→覆盖→撤销→传递子进程」变量生命周期最后一环。

## 背景

完善刀挂账 #2 明文「export 随 spawn env 面契约刀整刀落」——SpawnSpec 13 字段零 env 是物质根源，berry-libc stubs.rs 的 environ=NULL/getenv 恒 null 空桩是消费面缺口。规格轮三视角合议 15 裁决：注入通道选 bootstrap 通道**第二消息**（blob 同消息追加段三罪：hello 64B 窄窗/berry-verify 严格 parse_argv/192B 空间互噬；gift 槽 8=CHAN_HND_MAX 已满；抬 CHAN_MSG_MAX 违内核零改）；缺席探测选条件发送＋-EAGAIN（产侧 send#1→[send#2]→process_start 全序先行、子进程 start 后才执行首指令——EAGAIN 是「确定无」非「未到」，零竞速）；export 语义选**全导出**（v1「所有变量等价环境变量」三重证据在册：shell设计 P5 行、bsh「vars 表即源」头注、uu env 空表自认）。

## 交付

- **S1 berry-argv**：env_blob 组装律（ENV_BLOB_SZ=256＝通道消息帽恰满、KEY=VALUE\0 续装、整项或零项单调截断、空项跳过、内嵌 NUL debug_assert 预检全输入面）——与 argv_blob 同法度单源不立第二套；crate 头注升格 processargs wire 格式单源（argv v1＋env v1.1 并陈）；host 测试 5→11（argv 既有 5 零变化＝字节面零漂移实证 + env 六族含 should_panic）。
- **S3 berry-libc**（dormant 形态与空桩观察等价论证入头注）：boot 尾第二收（同窗复用、非阻塞；-EAGAIN/其余错误同桶防御降级空环境——env 非生存面不杀进程）＋ENV_STR[256]/ENV_PTR[17] 静态区（environ 恒指 NULL 终结数组，post-boot 永非 NULL 不变式）＋stubs 三符号真身（getenv 线性扫；setenv/unsetenv 摘出 ok! 谎报桶写静态区紧致重建——environ 基指针恒定故 std::env::vars_os 迭代器安全，满 16 项/256B 返 -1）。
- **S2 berry-exec**：SpawnSpec 尾追加 `env: &[&[u8]]`（空片=现行为零漂，args 同款惯例）＋P2 条件发送（审查收紧：组装 0 字节不压空消息）＋三处陈旧文档面滚新（crate 头注/extra_gifts 字段注/spawn 协议头注——两代旧 gift 布局文→现行布局＋v1.1 尾随消息）；33 处构造点机械 `env: &[]`（svcmgr 25/berry-term 2/bsh 6，rg 新旧计数对账恒等；结构体无 Default 漏改即 E0063 构建红 loud）。
- **S4 bsh**：export 第 21 内建三形（K=V 赋值形——值域可含 |/>/ 与 `A=1 > f` 同形记面；K 幂等确认形；裸形/`-p` 落 render_cmd 与 env 同臂同渲染）＋build_export_set 全导出集（rev-unique 同名最新胜出、KEY= 尾界判等防前缀误撞）＋三产点接线（run_output 增参两调用点/pipe_capture 增参全段同集/run_capture_gifts_cap 增参＋新包装 run_capture_env；int/sig/term 三自验点恒 &[]）＋interactive 入口 rc_load 前 environ 播种（继承链闭环——rc 尾置覆盖胜出由 rev 语义自然成立）＋cli 步⑩（CLI ok n=10→11）＋u4 步② 常量翻 `b"U4V=zz\n"` 精确等值（头注预埋契约兑现）。
- **治理**：API治理章程补 L3 登记例·processargs v1.1（薄指针——详律单源在代码头注不双写）；uu env/which 两消费头注随 environ 真身滚新；justfile berry-argv 行计数注释 5→11。

## 裁决与判例

- **审查 critical 判例（主线修复）**：`export > f`（含 `-p`/`>>` 形）被 export 前缀臂当杂形拒 usage 2——违 B1「与 `env > f` 同走 do_redirect」承诺。修法：assignment_of 先行保值域，列显形与含 `>`/`|` 形不入臂直落后续检测（render_cmd 的 export/export -p 精确匹配产列显字节）；⑩a 补两枚钉板（`export -p` 渲染逐字节同 + `export > __exf` RAM 树落列显读回含 `__X=7` 清场）。附带修 `export  -p` 多空格 127 形（rest.trim 后 -p 匹配）。判例：**前缀臂内建的「杂形拒」必须让位于操作符检测层**——含操作符的形先归管道/重定向臂，再谈内建参数面。
- **__environ 别名盲区定谳（技术残验）**：S3 探针实证三源收窄——musl std rlib 未定义符号表仅 environ 一枚、全像零 __environ 引用、bsh+uu 23 成员实链全过——双静态预案不需要；U4 ② 端到端精确等值为兜底执法。
- **G1 零漂移论证**：空 env 下产侧 send 序列逐调用恒等（消息一原样、无第二条 send——审查收紧后连 0 字节消息也不发）、子侧第二收走内核 EAGAIN 早退支零变异、environ 空 NULL 终结数组与 NULL 桩观察等价（vars_os 零行/getenv 恒 NULL/uu env 无命令面输出逐字节同）。
- **挂账不做**（10 项全在刀谱）：bootstrap 端句柄卫生、svcmgr/launcher/berry-term env 转发链（27 点恒 &[]——PATH 消费闭环实跑定收益后另刀）、BSHRC 写者悬空、ENV 帽扩容（>256B 走 VMO 挂 v3）、A5 草案翻新、七面注释陈旧搭车、uu env 全 POSIX 面、PATH/hexyl 辅证步、setenv 动态扩容、ENV 多线程化。

## 验证

- **宿主链**：berry-argv host 11 测绿（argv 5 零变化+env 6）；berry-libc none-target 纯编绿＋探针符号面钉板（environ(B)/getenv/setenv/unsetenv(T) 全由 berry-libc 定义、唯一未定义=弱符号 gettid、__environ 零引用）；berry-exec+svcmgr 构建绿（svcmgr 1 条预存警非本刀）；bsh/berry-term/uu 家族 23 成员 musl 构建（berry-ld 链）全绿零本刀警告；rg 计数对账 33 构造点零遗漏。
- **B6 机查**：__E/__X/__NOPE（⑦⑧ 同形复用）对 245 现役标记串＋49 grep 断言模式正反两向零碰撞。
- **客机 smoke**：boot 相与 U4 ②/CLI ⑩ 端到端留 owner 稳定窗复跑（当夜 smoke 环境五连拦：echod 竞速 ×2/LOADGEN 竞速/SKT 兄弟域回归/BENCH 期望在途漂移——全部带证归档，非本刀面）。
- **G2/G3 自证（主线口径）**：本刀提交路径零 kernel/ 与七面文件（工作树中 tools/ 脏面系并行 lane 在途——BENCH n=8 期望滚新与 smoke-image 计数，归属他方）；u4 ② 翻转属 bsh 单文件（CLI ok n=11 步数单源在 bsh）。

## Related

- [export 内建与 spawn env 面刀实施设计](../../plans/2026-11-17-export内建与spawn-env面刀实施设计.md)（本刀刀谱——契约 P/A/L/B、裁决 15 条、砍单汇总）
- [2026-11-17-命令行终端与shell完善刀](2026-11-17-命令行终端与shell完善刀.md)（挂账 #2 兑现源）
- [shell设计-v1.1](../../../docs/shell设计-v1.1.md) P5 行后记（export 兑现注）
