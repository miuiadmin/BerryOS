# export 内建与 spawn env 面刀实施设计（processargs v1.1）

Type: plan
Status: draft
Updated: 2026-11-17

## 一句话说明

owner 令（2026-11-17 深夜「继续实施」）v2 系列首刀的终稿切法——env 走 bootstrap 通道第二条消息（processargs v1.1 通道级 add-only：消息一 argv blob+gifts 在 env 有无两态下逐字节恒形、env 串独立成消息且仅非空时在 process_start 前压队、子侧以非阻塞 recv 的 -EAGAIN 探测缺席），berry-libc 建 ENV_STR[256]/ENV_PTR[17] 静态区并在 boot 填真 environ/getenv、setenv/unsetenv 摘出 ok! 谎报桶写真身，bsh 落 export 第 21 内建采全导出语义（vars 单表即导出集、env 内建显示面零漂移）＋启动自 environ 播种＋run/裸名派发/管道三 spawn 面接线，补齐「赋值→覆盖→撤销→传递子进程」变量生命周期最后一环；内核零改、七面标记零滚新（新步吸收进 CLI ok n=10→11 前缀式、步数单源在 bsh）、零新 bin 零新 crate，u4 步② 常量按头注预埋契约同批翻转。

## 背景

（锚点全部 2026-11-17 深夜工作树现场复核成立）

1. export 挂账在册且 v1 历史语义=全导出：bsh 头注（user/bins/bsh/src/main.rs:74-76）与完善刀挂账 #2（.agents/plans/2026-11-17-命令行终端与shell完善刀实施设计.md:76）均明文「随 spawn env 面契约刀整刀落」；shell 变量面是单一 Vec<(String,String)>（main.rs:203），get_var 用 iter().rev().find() 取最新（:3351-3354），赋值在原始 trimmed 段检测 assignment_of（:3360-3374）、exec_one 命中后 expand 值再 push 尾置覆盖（:3504-3507），unset 同层全删同名（:3514-3523，头注明文「env 显示面零改动（vars 表即源）」），管道检测在其后（:3529 起 raw.contains('|')→do_pipe）；env 内建回显全表含覆盖史（render_cmd 精确匹配 :3592-3602，「面向人不做去重」）；rc_load 从 BSHRC.CFG 静默注入同表（:375-424，push :420），装载点 interactive() 入口（:436-437）。三重证据（docs/shell设计-v1.0.md:113 P5 行、bsh:3514 头注、uu env harness 空表自认）钉死「所有变量等价环境变量」是 v1 既定语义。
2. spawn 链无 env 面是挂账的物质根源：SpawnSpec 13 字段零 env（user/berry-exec/src/lib.rs:337-394）；产侧 channel_pair（:736）组装 gift 8 槽前缀紧致布局（[0]stdout/[1]stdin/[2]/[3]自描述对/[4..]extras≤4，nhandles∈{1,4..=8}，:739-796）后单发 channel_send(env_s, blob, plen, gift, nhandles)（:805）→ process_start(bootstrap=env_c)（:808）→ 两侧 close 自证（:811-816）；argv blob 192B 组装律单源在 berry_argv::argv_blob（user/berry-argv/src/lib.rs:26-54，整参或零参单调截断）；子侧 berry_libc_boot 恰一次 recv（user/berry-libc/src/io.rs:92）解析 argv 落 ARGV_STR[256]/ARGV_PTR[17]（:55-56、:119-143）；environ=NULL/getenv 恒 null（user/berry-libc/src/stubs.rs:750-756）、setenv/unsetenv 在 ok! 桶（:304-305）全树零写点。
3. 第二消息通道的机制基础现场核实成立（本刀选型的三根桩）：sys_channel_recv 空队非阻塞返 -EAGAIN（kernel/src/syscall.rs:3498「None => return ERR_AGAIN——非阻塞：空即返」）；send_args_legal 仅上界判据 nbytes≤CHAN_MSG_MAX=256 且 nhandles≤CHAN_HND_MAX=8、无下界——nhandles=0 合法（chanface/src/lib.rs:70-72），hello 以 null 指针+0 句柄 channel_send 是现役先例（user/bins/hello/src/main.rs:47-48）；内核 send/recv 对字节恒不透明搬运（syscall.rs:3189-3304 copy_from_user 原样入 Message.bytes 零解析）。CHAN_DEPTH=8（chanface:63）容两票压队零 -EBUSY；产侧消息本就全在 process_start 之前压队（lib.rs:725「确定性先行」）、子进程 start 后才执行首指令——recv#2 的 -EAGAIN 是「确定无」而非「未到」，无竞速无死锁。
4. 消费面已就绪且有一处硬耦合：uu env（bin/env，rootfs manifest 在档）无命令面经 uucore print_all_env_vars 遍历 environ、`env K=V` 经 set_var 落 setenv；which 缺席 PATH 回落 /bin（user/bins/which/src/main.rs:22）、hexyl NO_COLOR（:369）是真 getenv 消费者；bsh U4 步② run_capture("env",None,&["U4V=zz"]) 现弱断言 o.data.is_empty()（main.rs:6094-6098），头注 :6069-6071 明文预埋「setenv/environ 落真时本步常量改 "U4V=zz\n"」——本刀落 environ 真身则该常量必须同批翻，漏改＝boot smoke 红（loud）。
5. 契约网零滚新通路成立：CLI ok n=/TERM ok n= 四断言面全开放前缀（tools/markers.sh:33、smoke-image.sh ECO_MARKERS:360、bcts@2/@3 冻结件 :544/:546），步数单源在 bsh println（main.rs:5243 现值 10，d922f5d3a 6→10 零滚先例同构）；现役 RX 断言串零 KEY=VALUE 形态，新步字面量 __E/__X/__NOPE 形与现役标记两两互非子串（canon 纪律）；cli_selftest ④ BUILTINS 探针表（:5078，现 7 名）与 help↔BUILTINS 互钉（:5084-5089）自动执法第 21 内建的文案面。
6. 文件面与并行 lane：本刀六域文件（berry-argv/berry-libc/berry-exec/svcmgr/berry-term/bsh）当前全干净零在途；工作树脏面以实施轮开工时 git status 快照为准（落稿时刻 15 M＋3 ??：.githooks 三件、docs 四件、.agents 笔记与 research 各一、tools/svg2icons.py、user/berry-appshell/icons.rs、user/bins/berry-launcher/syspages.rs、kernel/src/bench.rs、user/berry-tcp 两件、disk-qw1.img 等——kernel bench lane 与 tcp lane 系契约侦察后新入场，均与本刀域零交集）；唯一流程活险＝docs/阶段沿革.md 工作树截断于五百五十二（553/554 已入库）——第四次同型自覆写窗口，主线提交轮必须以 HEAD 重同步后追加。SpawnSpec 构造点实数 33 处（svcmgr 25＋bsh 6＋berry-term 2，rg 现场复核，berry-exec 内多出的 1 处命中为结构体定义本身）；两处陈旧文档面（crate 头注 lib.rs:18-21 与 extra_gifts 字段注 :361-366 写「gift[2..]≤3」两代旧布局，现行是 [2]/[3] 自描述对＋extras [4..]≤4）本刀顺手同批滚新。

## 契约

### 全局契约（六域共守）

G1 零漂移数学论证（硬约束兑现面）：既有 33 处 SpawnSpec 构造点全部机械补 env: &[]——空导出集产侧零发第二消息，channel_send 调用序列与现行为逐调用一致（消息一字节串＝argv_blob(name,args) 恒等、gift 槽位/nhandles 不变、无第二条 send）；子侧新 boot 路径为「recv→-EAGAIN→空环境」，environ 由 NULL 变为指向空 NULL 终结静态数组（迭代零行、getenv 恒 NULL——与空桩观察等价）；hello/berry-verify 自定入口不经 berry-libc 零感知（env 消息对永不二次 recv 的收面结构性不可见，通道随子进程退出销毁）。产侧序恒为 send#1→[send#2]→process_start——子进程 start 后才执行首指令，消息序 FIFO 确定。boot 自验链全部既有标记（CLI/TERM/U4/PIPE/FS/BSH/EXIT/ECO…）按构造零漂。

G2 内核零改：kernel/、chanface/、tools/abi.toml 零触碰（EAGAIN 在册、send(≤256,0) 合法、CHAN_DEPTH=8 容两票——三桩皆现役）；抬 CHAN_MSG_MAX 与统一升收窗两条路违本约束封存（裁决 15／砍单）。实施毕 git diff --stat kernel/ tools/ 为空自证。

G3 七面零 diff 含注释：markers.sh／smoke.sh／smoke-smp.sh／smoke-image.sh／bcts 冻结件／bcts-run.sh／gui8-smoke.py 完全不触（含注释——注释陈旧是小债、七面出 diff 是约束解释空间大债，挂账 6 搭车捎带）；新步吸收进 CLI ok n=10→11（步数单源在 bsh println :5243 与 bsh 内步数注释 :333/:4973-4975/:5246 同批滚——bsh 不在七面）；u4 ② 常量翻转属 bsh 单文件（:6094-6098 期望＋:6069-6071 头注同批）；实施毕 git diff --stat tools/ 为空自证。

G4 脏文件禁碰与并行纪律：git status 现脏全列（以开工快照为准，落稿时刻 15 M＋3 ??）禁碰——含 docs/阶段沿革.md（沿革五百五十五由主线提交轮以 HEAD 重同步后追加，拆第四次自覆写窗）与 docs/索引.md（新笔记登记线主线后补）；实施轮禁 git 提交，主线统一验证提交（显式路径暂存）；各域构建走隔离 CARGO_TARGET_DIR；kernel/src/bench.rs 与 berry-tcp 两件在途 lane 不碰。

G5 体积账：零新 bin 零新 app 零新 crate 零 manifest 改动（env 组装并入 berry-argv、解析内联 berry-libc——user/Cargo.toml 与 justfile 零触碰）；增量预算＝bsh +2-3KiB、berry-libc 代码 +1KiB＋bss +~400B（ENV_STR 256B＋ENV_PTR 17×8B）、berry-exec +~0.5KiB、uu 家族经 berry-libc 重链各 +~1KiB——rootfs1 余约 21MiB 九牛一毛；主验证核对 vols=2（smoke.sh:1032-1038 形态断言不动）。

G6 通道语义账：env 消息 0 句柄、载荷 ≤256B 单发、仅非空时发送；boot 后 bootstrap 端子侧悬挂未关与现况同形（产侧 env_s 照旧 send 后即关 lib.rs:814-816；通道随子进程退出整体消亡，未读消息无害——挂账 1 句柄卫生刀）；env 与 stdin 是不同通道零互扰。

### 契约 P：processargs v1.1 wire 协议（域间文字接口，S1/S2/S3 三域同读）

P1 消息一恒形：payload＝argv0\0[+arg\0]*（berry_argv::argv_blob 现行编码不动）＋gift 槽位/nhandles 域零改；env 有无两态下逐字节恒形——无哨兵、无版本域、无长度域（裁决 2/3）。

P2 消息二（env 消息）：产侧在 argv send 之后、process_start 之前，spec.env 非空则组 env_blob 并 channel_send(env_s, ebuf, elen, null, 0)；payload＝KEY=VALUE\0 续装串，帽 ENV_BLOB_SZ=256（＝CHAN_MSG_MAX 恰满帽），「整项或零项」单调截断（装不下即停、其后不再装），空项跳过（编码面禁 \0\0 歧义），每项为完整预拼 KEY=VALUE 字节串（'=' 语义校验不在编码层——归产侧构造纪律与收侧解析防御）；nhandles=0。空导出集零发送。

P3 收侧（新 libc）：berry_libc_boot 在 argv 解析落位后复用同窗二次 recv（blob[256]/hs[8]）——返非 0（-EAGAIN＝确定无 env 消息；其余错误码同桶防御降级）→空环境继续 boot（env 非生存面不杀进程——与 argv 破缺 argc=0＋自旋死的语义分档记面）；返 0→NUL 串逐项解析（无 '=' 项跳过）落 ENV_STR[256]/ENV_PTR[17]（16 项帽＋NULL 终结、整项或零项单调截断同律）；environ＝ENV_PTR.as_mut_ptr() 恒置位（空表＝合法 NULL 终结数组——post-boot environ 永非 NULL 不变式；ARGV_STR/ARGV_PTR 静态落位同款先例 io.rs:54-56）。

P4 产销适用面：env 只对 berry-libc boot 收面子有语义（全 musl 件随重链自动升级）；对自定入口（hello 64B 窗只消费 hs[0]、berry-verify 自带 parse_argv）env 消息结构性不可见（永不二次 recv、通道随退出销毁）——per-bin 产销配对纪律对本刀不再必要，窄窗陷阱仅在「blob 追加段」形态复活时才相关（治理登记记档，裁决 15）。

### 契约 A：产侧（S1 berry-argv＋S2 berry-exec 与 33 点机械）

A1 berry-argv 增 env_blob 组装律（与 argv_blob 同法度、单源不立第二套）：pub const ENV_BLOB_SZ=256；pub fn env_blob(out: &mut [u8], entries: &[&[u8]]) -> usize——每项完整 KEY=VALUE\0 续装、整项或零项单调截断、空项跳过、含内嵌 NUL 项 debug_assert 拦；host 测试就地扩（tests/argv.rs：空集返 0／单项多项／整项截断边界／空项跳过／256B 恰满／无内嵌 NUL 断言六族）；crate 头注升格 wire 格式单源（argv v1＋env v1.1 两律并陈）；justfile:887 既有测试行自动覆盖零接线改动；argv_blob 字节面与既有测试零变化。

A2 SpawnSpec 尾追加 pub env: &'a [&'a [u8]]（空片＝无 env——args 字段同款惯例；intent/force_eager 后第三例尾扩）：spawn_rootfs_elf 解构同步；argv send（:805）之后、process_start（:808）之前插 P2 逻辑——spec.env.len()>0 则栈上 ebuf[256] 组 env_blob 并 channel_send(env_s, ebuf.as_ptr(), elen, null, 0)，失败照 trap 纪律；同批滚新两处陈旧文档面（crate 头注 :18-21 与 extra_gifts 字段注 :361-366→现行 [2]/[3] 自描述对＋extras [4..]≤4＋env v1.1 尾随消息面）＋协议头注（:396-408）与 io.rs:62-71 对偶滚新为两条消息制。

A3 33 处构造点机械 env: &[]（结构体字面无 Default 全点必改、漏改即构建红 loud）：svcmgr 25（run_rootfs_child 15＋ps_selftest/bc_run_sub/dpgx/x5/uu_run/p3_sig_guard/app_launch/tomb×2/launcher_assemble 等）、berry-term 2（pipe_selftest／try_spawn_child）、bsh 6（int:807／sig:3238／run_output:3800／term:5301／pipe_capture:5561／run_capture_gifts_cap:6344）——bsh 六点先落 &[] 占位保零漂，S4 再对 run_output/pipe_capture 两点接真值、run_capture_gifts_cap 走显式 env 参、int/sig/term 三自验点恒 &[]（自验面确定性优先；sig 拉的 berry-verify 是自定入口）。

### 契约 L：berry-libc 收侧（S3，可与 S1 并行先行——dormant 形态零观察面变化）

L1 io.rs boot 尾增第二收：按 P3 落地；ENV 帽 256B/16 项如实文档化（无堆无增长，静态纪律；ARGV 手法镜像）；协议头注 :62-71 滚新。

L2 stubs.rs 三符号真身：getenv＝ENV_PTR 线性扫（键前缀＋'=' 比对，命中返值内指针，缺席 NULL——:753-756 翻真）；setenv/unsetenv 摘出 ok! 桶（:304-305 删行——桩册变动，api-diff 通告随主线）写真身——静态区紧致重建（同名按 overwrite 语义重建、缺席追加空间足则入、不足返 -1、unsetenv 剔除；environ 指针恒定不动故 std 迭代器安全；现项计数用 ENV_LEN 静态或扫至 NULL 终结）。musl std 符号对接：std::env::var/vars_os 经 libc environ 读本符号；若实测 std 链 __environ 别名则同批补第二静态并 boot/setenv 双置位（砍刀重议触发 ② 的收窄预案）。

### 契约 B：bsh 面（S4 单文件域 user/bins/bsh/src/main.rs）

B1 export 第 21 内建（三形，exec 分层与 env 内建同构）：exec_one 在 unset 臂后、管道检测前（:3523/:3529 之间）增 export 前缀臂（原始 trimmed 段上检测——值域可含 |/>，与赋值同纪律，`export K=V > f` 与 `A=1 > f` 同形记面）：`export K=V`＝赋值（assignment_of 同法切名值、值 expand 后 vars.push 尾置覆盖——全导出按构造成立）；`export K`（合法名 [A-Za-z_][A-Za-z0-9_]*）＝幂等确认 0（未赋值保持未赋值——POSIX「标记导出」在全导出语义下坍缩，偏差记面）；裸 `export`/`export -p` 不进 exec 臂、落 render_cmd 精确匹配臂＝与 env 内建同渲染路径（全表回显含覆盖史；`export > f` 与 `env > f` 同走 do_redirect）；非法名/缺参 usage 2。BUILTINS 20→21（:3550-3553，env 邻位插入）、HELP_LINES 帽内补名（:3556-3563，单行 ≤192B 字节帽现场复核、超帽三行再平衡）、cli ④ 探针表加名（:5078）——help↔BUILTINS 互钉（:5084-5089）自动执法文案面。

B2 全导出语义（方案 C）：vars 单表即导出集、无局部/导出二分——env 内建显示面零改动（:3592 头注「面向人不做去重」保持）；spawn 导出集＝build_export_set(vars)（同名最新值胜出＝rev-unique 去重，序按最后赋值先后——get_var rev 语义同源）；POSIX 偏差三笔记面（无 shell 局部变量、export K 标记面坍缩、export 列显无前缀）；裸 env（shell 全表）与 run env（子件 environ＝去重导出集）常态收敛、覆盖史行差异记面。BSHRC 注入自动导出按构造成立（rc_load 进唯一 vars 表 :420）。

B3 spawn 接线：run_output 增 env 参（两调用点——render_cmd `run ` 臂（:3762 附近 run_output(rest.trim(), &[], out)）与裸名 PATH 派发臂——均在 vars 作用域内），内部 build_export_set 组 KEY=VALUE 喂 spec.env；pipe_capture 产点（:5561）两段同集接线（POSIX 同形；do_pipe 持 vars（do_pipe(raw, vars, status, out) 已核），段装配处预组传入取最小 diff）；run_capture_gifts_cap 增 env 参（S2 机械占位的既有调用点保持 &[]）＋新便捷包装 run_capture_env(name, feed, args, env) 供新自验步与后续刀复用；int/sig/term 三自验 SpawnSpec 点恒 &[]（零漂）。装配小面：Vec<Vec<u8>> 预拼 K=V＋Vec<&[u8]> 借用切片喂 spec（局部生命周期覆盖 spawn 调用；String 无内嵌 NUL 面，A1 断言按构造不触）。

B4 启动播种：interactive() 入口（:436）rc_load 之前增 environ→vars 播种（std::env::vars_os 首个 '=' 切分、无 '=' 跳过、插入序保持）——继承链闭环（子 bsh `echo $K`/`env` 可见、which 的 PATH 来源链通）；rc 后载同名尾置覆盖胜出（rev 语义自然成立）；boot 自验链零播种副作用（svcmgr 未传 env→environ 空→零行，且自验链不进 interactive）。

B5 自验双改：cli_selftest 增步⑩（CLI ok n=10→11，:5243 与 bsh 内步数注释同批）——⑩a 语义纯面（局部 vv：exec_one("export __X=7") 后 expand("$__X",&vv,0)=="7" 赋值等价；render_cmd("export",&vv,…) 与 render_cmd("env",&vv,…) 输出逐字节相同——同渲染执法；exec_one("export 9BAD=x") 返 2 非法名拒）＋⑩b 显式端到端（run_capture_env("env",None,&[],&["__E=1"]) 断 o.data==b"__E=1\n" 精确等值——传递链全通证：bsh 组装→berry-exec 发送→libc boot 解析→environ→std vars_os→uucore print_all）＋⑩c 真实路径端到端（vv 含 __E=1 后 do_pipe("env | grep -q __E",&vv,…)==0 且 do_pipe("env | grep -q __NOPE",&vv,…)==1——run/管道产点接线活证，⑨ do_pipe 先例零新机器）；u4 步② 常量翻转 :6095 期望 o.data==b"U4V=zz\n"（run_capture 恒空 env＋setenv 真身追加＝environ 恰一行，精确等值成立）＋头注 :6069-6071 同批滚＋bsh 头注 :74-76 defer 句翻新。

B6 文案纪律：新字面量 __E/__X/__NOPE 形与现役标记及 RX 断言族（hellq/f42t/d127x/i130t 等）两两互非子串（canon 机查复核记档）；断言只建立在子件应答上（「展开造内容消回声」纪律不破）。

### 契约 D：治理与主线收口（S5）

D1 治理登记：docs/API治理章程-v1.0.md 补 processargs v1.1 登记节（L3 系内 API：两条消息制／env 消息形态与 256B/16 项帽／通道级 add-only 纪律＝消息一恒形＋旧消费者零义务＋自定入口结构性不可见论证／收窗帽注记——详律单源在 berry-argv/berry-exec/berry-libc 头注对偶，登记只做薄指针不双写）。

D2 挂账翻态与文档四件：docs/shell设计-v1.0.md:113 P5 行滚新（export 已落＋全导出语义＋三笔 POSIX 偏差＋步数钉板句随 CLI n=11）；.agents/plans/2026-11-17-命令行终端与shell完善刀实施设计.md 挂账 2 行尾追加落地指回；.agents/notes/implemented/2026-11-17-命令行终端与shell完善刀.md :26 同批翻态；新笔记 .agents/notes/implemented/2026-11-17-export内建与spawn-env面刀.md（裁决与砍单全文＋验证账＋体积账）；node .agents/tools/check-docs.mjs 绿；docs/索引.md 现脏禁碰——笔记登记线主线收口后补。

D3 主线统一验证与提交：分层验证链（见步骤 5／verification）；提交显式路径暂存；docs/阶段沿革.md 以 HEAD 重同步追加五百五十五（工作树截断于五百五十二的陈旧缓冲禁直接改——第四次自覆写窗拆弹）；七面与 kernel/ 的 git diff 为空自证。

## 步骤

1. [ ] S1 berry-argv 域（可即启）：A1 落地 user/berry-argv/（src/lib.rs＋tests/argv.rs）；host 测试绿（隔离 CARGO_TARGET_DIR）。
2. [ ] S3 berry-libc 域（与 S1 并行——文件互斥、契约 P 文字接口已定）：L1＋L2 落地 user/berry-libc/（io.rs＋stubs.rs）；dormant 形态（无产侧发送→recv#2 EAGAIN→空环境，观察面与空桩等价）；musl 单件构建绿。
3. [ ] S2 产侧域（dependsOn S1）：A2＋A3 落地 berry-exec＋svcmgr/berry-term/bsh 机械 &[]（bsh 仅占位）；批收口全树 musl 构建绿。
4. [ ] S4 bsh 域（dependsOn S2＋S3）：B1-B6 落地 user/bins/bsh/src/main.rs 单文件；musl 单件构建绿＋cli/u4 自验逻辑自核（实跑留主线 QEMU）。
5. [ ] S5 主线（dependsOn 全部）：D1-D3；分层验证链全绿；统一提交＋沿革重同步追加。

## 挂账移交（本刀显式不做）

1. bootstrap 端句柄卫生（boot 后 env 通道端子侧悬挂未关——现况同形；随句柄泄漏卫生刀）。
2. env 链路贯穿 svcmgr/launcher/berry-term（ECO 链/app_launch/持久 bsh 转发 env——本刀只接 bsh 产侧三点，其余 27 点恒 &[]；待 BSHRC/PATH 消费面实跑定收益后另刀）。
3. BSHRC.CFG 写者与宿主生成面（树内无写者，PATH 变量唯一来源悬空——which 真值端到端待此）。
4. ENV 容量帽 256B/16 项扩容路（BSHRC 64 行帽 vs 传递 16 项帽的截断已文档化，无现实压力；>256B 走 VMO 路挂 v3）。
5. docs/事件循环A5设计草案-v1.0.md §1.1 锚点陈旧翻新（lib.rs:240 已漂、gift ≤3→≤4——草案整体翻新另刀）。
6. tools 七面注释陈旧（smoke.sh:949 起「六内建/CLI 十步」枚举注释块、markers.sh:47-48、smoke-image.sh:314——G3 七面零 diff 硬约束下不触，搭下次合法触面刀捎带）。
7. uu env 全 POSIX 面（env -i/-u/env K=V cmd 组合行为）与 exec/posix_spawn 真身（现 ok! 桶族——`env K=V cmd` 带命令形依赖此，独立刀）。
8. export PATH 后 which 命中路径变化与 hexyl NO_COLOR 关色的辅证自验步（真消费面但断言脆弱——本刀以 ⑩b/⑩c/u4② 端到端为准）。
9. setenv 容量满动态扩容（ENV 帽 256B/16 项足够 bsh 量级，满即 -1 POSIX ENOMEM 形档）。
10. ENV 静态面多线程化（bsh 单线程现状绑定；随 pthread 生态 v2 议）。

## 裁决记录

1. 注入通道＝env 通道第二消息（三稿共识）：blob 同消息追加段三罪（env ≤64B 窘迫余量＋消息一变形触碰 berry-verify 严格 parse_argv 与 hello 64B 窄窗＋argv 192B/env 空间互噬）；抬 CHAN_MSG_MAX 违 G2 内核零改；gift 槽 8=CHAN_HND_MAX 已满（bsh arm 四 extras 满配）；随行 VMO 是 >256B 载荷方案而 env 量级 KB 内。第二消息使消息一恒形（零漂最强形态）、env 独享 256B 预算、通道帽/内核/ABI 文档全零滚、对自定入口结构性不可见。
2. 发送时机与缺席探测＝条件发送＋EAGAIN 探测（encoding 案），否决 semantics 恒发空消息案与 risk 空串哨兵案：恒发案使旧调用方通道多一票 0B/0 句柄消息（队列态可观察），违「不传 env 的 spawn 调用方字节面零变化」硬约束字面；其死锁论证（「子侧无法区分没有与未到」）在本架构不成立——产侧 send#1→[send#2]→process_start 全序完成、子进程 start 后才执行首指令，recv#2 之 -EAGAIN 是确定无而非未到（lib.rs:725 确定性先行论证延伸，内核 EAGAIN 现场核实 syscall.rs:3498）；哨兵案必改消息一（env 在场时）＋空参理论歧义面＋为 EAGAIN 已可判之事引入第二信号机制。代价承认：错误码与缺席同桶（防御降级为空环境——协议破缺另有 argc=0 自旋死 loud 死法，可受）。
3. 版本名＝processargs v1.1（非 v2）：纯 add-only 追加一票可选消息、消息一恒形，语义上是次版本扩展非代际更替。
4. env 帽＝256B＝CHAN_MSG_MAX 恰满（encoding/risk 案），否决 semantics 192B 设计帽案：argv 的 192B 帽动因是 berry-verify 192B 收窗；env 唯一收面是新 libc 256B 窗，无窄窗约束——64B 余量纪律对 env 无意义，满帽用满。
5. export 语义＝方案 C 全导出（三稿共识）：v1「所有变量等价环境变量」三重证据在册（shell设计 :113、bsh:3514「vars 表即源」、uu env 空表自认）；案 A 翻 16 处 Vec 类型点＋同名多行标记位规则复杂、案 B 双表腐化面，均无对应用户收益；代价＝无 shell 局部变量（POSIX 偏差三笔记面）。
6. 裸 export 列显＝与 env 内建同渲染路径（semantics/risk 案），否决 encoding rev-unique 去重集案：一真相零新代码；「env 列覆盖史、export 列去重集」的双头记忆负担无收益；子件实收面（去重集）是 spawn 装配语义（B3 build_export_set）不属显示面；⑩a 的 render 逐字节同断言把此裁决钉进自验。`export -p` 采裸形别名保留（encoding 裹挟）：一匹配臂换 POSIX 习惯兼容；砍 POSIX 可重用列显形（无 source/脚本回读面——BSHRC 是 KEY=VALUE 注入非命令序列，产出的行零消费者）。
7. u4 ② 断言＝精确等值 o.data==b"U4V=zz\n"（encoding/semantics 案），否决 risk contains 案：run_capture 恒空 env 使子件输出与 bsh 全局表状态解耦——risk 的「锁全等绑死 boot 链变量注入面」论证在恒空 env 下不成立；精确等值还执法「setenv 真身恰好追加一行」的构造事实（若红即暴露 std 符号别名残验点，loud）。
8. env 参数传递＝显式参＋新 run_capture_env 包装（encoding 案），否决 risk ENV_ENTRIES 全局缓存案与 semantics 全员穿参案：全局缓存引「四变更点漏重建＝子件见旧环境」静默腐化面＋static mut 多线程债；全员穿参动 run_capture 族数十调用点零收益；显式参使存量 run_capture 签名与调用点零漂（与裁决 7 的确定性解耦同一构造）。
9. berry-term 持久 bsh 恒 &[]（encoding/risk 案），否决 semantics 转发 vars_os 案：上游（svcmgr/launcher）恒空使转发今日为不可测代码；bsh→bsh 继承经 run/pipe 接线已活；svcmgr→term→bsh 继承链挂账 2 独立刀。
10. setenv/unsetenv 真身随刀收（三稿共识）：u4 ② 头注预埋翻转条件即「setenv/environ 落真」——只落传递不落写面则 `env K=V` 仍空转半语义面；实现＝静态区紧致重建、environ 指针恒定（std 迭代器安全、无堆）。
11. 播种收入（三稿共识）：不播种则 export 只惠及非 shell 子件、交互面 `echo $PATH` 断链（BSHRC 的 PATH 消费闭环断在最后一米——which 是 PATH 唯一设计消费者）；播种在 interactive 入口 rc 前，rc 尾置覆盖胜出由 rev 语义自然成立。
12. 治理登记＝API治理章程薄指针节（semantics 案），否决 risk 独立标准文档案：独立文档制造第二副本，正是本刀要治的 extra_gifts 注释两代漂移病；一事实一归属——wire 详律家在 berry-argv/berry-exec/berry-libc 头注（本刀同批滚新），章程只登记分级＋纪律＋锚点。
13. A5 草案翻新挂账（encoding 案），否决 risk 本刀加指链案：草案态整翻另刀；单加指链不修锚点漂移本身。
14. 七面零 diff 含注释（encoding 案），否决 semantics/risk 注释随刀滚案：owner 硬约束「smoke/bcts/markers 七面零改」字面执行；tools/ 注释陈旧挂账 6 搭车；bsh 单文件内步数注释随刀（bsh 不在七面）。
15. hello/berry-verify 不升窗且产配纪律不再必要（encoding 裁决补强）：条件发送制下 env 消息对自定入口结构性不可见（永不二次 recv、通道随退出销毁）；窄窗陷阱仅在 blob 追加段形态复活时才相关——治理登记记档备查。

## 风险

最大回归面＝berry-libc boot 改动波及全部 musl 件：护栏＝空 env 下消息流逐字节恒等论证（G1）＋environ 空表与 NULL 观察等价＋boot smoke 32/32 全量回归（任何 bin 环境面异常即红）。
musl std→environ 符号别名残验（本刀唯一盲区）：std::env::vars_os/var 经 libc environ 读 berry-libc #[no_mangle] 符号（其余符号族同法实证在册），但 musl 内部若有 __environ 别名路径首跑即暴露——U4 ② 精确等值断言当场红（loud 非静默）；预案＝补第二静态双置位（单行修），不可解则按砍刀重议触发 ② 收窄刀面。
setenv 静态区边界（满 16 项/256B 返 -1）：uu env 单变量面远低于帽；帽行为文档化（挂账 9）。
HELP_LINES 192B 字节帽（CJK 3B/字）：export 名排版以实施时字节数为准，超帽三行再平衡；④ 互钉只查在场不查行位。
RX 断言撞面：__E/__X/__NOPE 形与现役标记/RX 断言族两两互非子串已设计期规避＋实施毕机查复核（canon 纪律无脚本执法——记档「机查已复核」）。
并行 lane：本刀域文件全净；kernel/src/bench.rs 与 berry-tcp 系侦察后新入场 lane（禁碰快照已收）；docs/阶段沿革.md 第四次自覆写窗由主线提交轮 HEAD 重同步拆弹，实施轮全周期禁碰；disk-qw1.img 与构建残留勿误 add。

## 砍单汇总（killList）

落选封存（设计裁决，复苏需翻本刀谱）：blob 同消息追加段（三罪——裁决 1）；msg1 空串哨兵与带内版本域/magic（裁决 2/3）；恒发空 env 消息（裁决 2）；抬 CHAN_MSG_MAX 或统一升收窗（违 G2）；gift 槽传 env（8 槽满配）；随行 VMO（>256B 载荷方案）；方案 A 三列/方案 B 平行标记表（裁决 5）；env 内建改列去重导出集（裁决 6）；export POSIX 可重用列显形（裁决 6）；hello/berry-verify 升窗（裁决 15）；新 rootfs 标记/bcts/markers 七面滚新（G3）；setenv 动态扩容（挂账 9）；新 bin/新 app/新 crate/user/Cargo.toml members/manifest 改动（G5）；ENV_ENTRIES 全局缓存（裁决 8）；berry-term/svcmgr env 转发（裁决 9/挂账 2）；u4 ② contains 形（裁决 7）；独立协议标准文档（裁决 12）；内核 spawn_user_env 合流（boot 进程 env 是 sched 独立机制）。
砍刀重议触发（实发即停刀重议）：① 内核对 bootstrap 通道载荷存在解析面或句柄数下界执法（本次现场核实不成立——syscall.rs:3281-3304 纯搬运、chanface:70-72 无下界、hello null+0 句柄现役先例 main.rs:47-48），实施期实证相反即停刀重议注入通道选型；② musl std 环境读面符号经 berry-libc shim 不可达（environ/__environ 别名双静态预案均无效）→刀面收窄重议（保 bsh 全导出显示面、回滚 libc environ 真身，u4 ② 常量回翻）；③ boot 自验链任一既有标记红且归因到 env 面漂移（G1 零漂移论证被实证推翻）→停刀归因，禁带红推进；④ 并行 lane 在本刀提交轮前整档提交 docs/阶段沿革.md 抹掉已入库 553/554 条目（第四次同型自覆写实发）→暂停提交轮，先以 HEAD 重同步拆弹再重启，沿革五百五十五顺延。

## 待裁项（owner）

1. 裸 export 列显含覆盖史行（与 env 同渲染）——若 owner 要「子件实收面」去重列显需翻裁决 6（改 render 去重＋⑩a 断言随改）。
2. `export -p` 别名保留（POSIX 习惯兼容）——是否砍掉由 owner 定（成本一匹配臂）。
3. u4 ② 精确等值——若 owner 预期 uu_env 输出面未来变化可降 contains（本稿裁精确等值，翻案随 ⑩ 步同批）。
4. berry-term/svcmgr 继承链转发时点（本刀恒空挂账 2）——PATH 消费闭环实跑定收益后排期。
5. BSHRC.CFG 写者/宿主生成面排期（树内零写者，PATH 唯一来源悬空——which 真值端到端待此）。
6. processargs 登记形态（本稿裁治理章程薄指针节）——若 owner 要独立标准文档（docs/进程参数块协议-v1.1.md）翻裁决 12。
7. musl std environ/__environ 别名形态——实施轮客机实测定谳（技术残验非裁决，U4 ② 端到端兜底＋双静态预案在册）。