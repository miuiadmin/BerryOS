# 08 号：白皮书与 wgpu 复核——白皮书 v1.0→v1.1 升版收官与 wgpu 上屏 PoC 独立抽验

> 复核日期：2026-11-14（同机续轮，Apple M3 Pro / 18GB / rustc 1.97.0 实证）。复核对象：两 wave——Wave1 [调度器架构白皮书](../../docs/调度器架构白皮书-v1.1.md) v1.0→v1.1 回填收官（mv 升版＋抬头 v1.1/2026-11-14＋文末修订记录＋§8 性能表/SMPD-3/4/路线图节回填＋全仓引用随迁）；Wave2 wgpu 上屏 PoC（β-1 壳半卡尺，[15 号](../显示与加速体系设计/15-wgpu上屏PoC.md) 报告＋`/tmp/wgpu-poc/` 独立 crate）。任务：只验不修——①白皮书对 [01 号](01-调度现状全景与在途线收编.md) 逐项／数字双源／链改指；②wgpu 数据可信（stride 路径／damage 口径）；③门禁＋git 对拍（guifbd 零碰）。方法：v1.0@HEAD 全文提取与工作树 v1.1 全量 diff 核读；基线链三时点提交 json 直读（d709cc5a＝26、f7b5970a＝22）；SMPD 三提交＋sched 代码双核；wgpu 30.0.2 本地 registry 源码四行号直读；PoC 四模式独立复跑（探针双＋离屏＋gauge 全档，产物 `/tmp/wgpu-poc/gauge-verify.log`）；15 号表格对三轮留存日志逐格转录对拍。纪律：零修零碰（本件外零仓写）；禁 git add/commit/push；推断显式标注。

---

## §0 结论速览（六条）

1. **Wave1 白皮书复核通过**：mv 纯工作树（v1.0 deleted 未暂存＋v1.1 untracked，全仓 staged 0 笔）✓；抬头 v1.1/2026-11-14/文件名一致（v1.1 :3）✓；修订记录两行自记完整（:144-147）✓；01 号 §1.1 三漂移项（性能 21 过时／SMPD-3/4 未入／路线图节过时）逐项覆盖且数字双源实测吻合；冻结面（§1-§7/§9/§10/附录）除指定增量外零翻动（diff 全量核读）。
2. **性能数字双源坐实**：§8 表 pick=22cy（v1.1 :81）两源——`tools/sched_baseline.json` :2-5（pick.cycles=22, n=1000）＋基线刷新提交 f7b5970a「CI 基线刷新——pick 22cy(-15.4%)」（json@f7b5970a 同值）；基线链 21→26→22 三时点全 git 实证（d709cc5a json pick=26；21＝v1.0 §8 11-08 时点），与[调度器设计 v1.2](../../docs/调度器设计-v1.2.md) :425 基线链注记逐句同口径（双文档互证）；01 号「21→26」与任务书「现行 22」各为自身时点真值，v1.1 以带时点链收纳两说。
3. **Wave2 wgpu PoC 复核通过且独立复跑同带**：stride 路径三几何锚全对上（11 号 :45 ScanoutMap 1280×800/stride 5120/BGRX＋:58 lsof 4096000=5120×800 互证；09 号 :36 800×600/3200；03 号桌面档 1080×1920）；5120=1280×4 **紧凑**——任务书「非紧凑」措辞不成立，15 号 §6-97 如实对账（非对齐面由 GB 3200%256=128／GC 4320%256=224＋双探针覆盖）；damage 口径 K=8×(16×10) 与 guifbd DAMAGE_MAX（`user/berry-ui/src/damage.rs` :14/:207 K≤8）＋09 号 :79 实证形状同源，计时口径＝预写替身只计上传（main.rs :447-481 实读）✓；本轮四模式独立复跑全 PASS、数值与 15 号同带（§5）。
4. **wgpu 30.0.2 源码定谳四条全中**：write_texture 对齐豁免（wgpu-types `texture.rs` :845-849 文档原文＋wgpu-core `device/queue.rs` :887-892 `false /* alignment not required */` 双源）／对齐门仅编码器路径执法（`command/transfer.rs` :480-487 `aligned &&` 分支）／内部 staging 快路/chunked 双路（`queue.rs` :994-1040）／per-call TRANSIENT 缓冲（`resource.rs` :1264-1270）——本地 registry 直读行号逐条命中；双探针（bpr=3200／offset=51264）本轮复跑 PASS。
5. **门禁＋git 对拍通过、guifbd 零碰坐实**：check-docs 218 篇 0 告警（07 号时点的 12 条设计 v1.1 死链已随占用解除消失）；全仓 staged 0 笔；两 wave 仓内足迹＝白皮书 mv 簇（docs/索引.md :50＋调度彻底计划 5 处 live link 改指 v1.1）＋15 号（untracked）＋显示线 README :18 行；guifbd 两 M 件（lib.rs +352／guest.rs +319）diff 全文为 v2 帧批面（frame_flush_batch/per-rect flush＋异步发车），归因 v2 续做会话，与两 wave 零交集。
6. **瑕疵三条＋残留两条（均轻，不动定性）**：①15 号 §2.1 GA 首轮 p50 转录 419.1 vs gauge-run1.log 418.9（0.2µs 笔误级，27 格中唯一偏差格）；②白皮书链改指只换文件名未同步行锚——§8 两行基线链段插入使 :88 后 +2 漂移，调度彻底计划 `[白皮书]:109`（band kill/EAS 框架）落点漂至 SMP 条（band 今 :113／EAS 今 :112）、`:93-103` 跨度仍罩住对标表；③15 号 §2.3「离屏两轮」之另轮 4084.8µs 无留存日志（§7 产物清单如实只列 offscreen-run2.log，自洽但少一件）；残留＝13 号 §0-4-② 0.12ms 预估升版回填（15 号已声明挂起）＋present 节拍留 B1-b 有窗会话（任务书预授权如实裁）。

## §1 Wave1 升版形态——过

git 对拍：`D docs/调度器架构白皮书-v1.0.md`（未暂存）＋`?? docs/调度器架构白皮书-v1.1.md`＝纯工作树 mv，`git diff --cached` 全仓空（staged 0 笔，零暂存）✓。抬头（v1.1 :3）：版本 **1.1**｜2026-11-14｜文件名 `调度器架构白皮书-v1.1.md` 三元一致 ✓；定位句增「SMP 部署 v1.4 在途」与框架计划挂接（:4），证据口径句增「性能数字随基线链滚动、引用带时点」（:5）——与 §8 链注呼应。文末新增「修订记录」节（:144-147）：v1.1 行自记 mv 形态＋性能双源＋SMPD 收录＋路线图刷新＋引用随迁范围，v1.0 行保留首版史实 ✓。136 行→147 行，全量 diff 核读：改动面＝抬头三行＋§3.2 一句＋§3.4 一句＋§6 一句＋§8 表一行与链段两行＋§11 标题与条目＋参考一行＋修订记录——**无越界改动**（§1/§2/§4/§5/§7/§9/§10/附录名词表逐字未动，§3/§6 仅追加指定 SMPD 增量句）。

## §2 白皮书对 01 号逐项（01 §1.1 三漂移项＋汇总表 :98）

- **漂移①（§8 性能表 pick=21cy 过时，01 :19）→ 已回填**：v1.1 :81 `27 周期 | **22 周期**（−18.5%）| 13×`（v1.0 为 21/−22%/14× 的同构更新，算术 27→22=−18.5% ✓）；基线链段（:89）21→26→22 三时点＋「跨测量序列不直接互比」＋真机口径诚实声明收口。双源核验：`tools/sched_baseline.json` :2-5 pick.cycles=22/n=1000 ✓；f7b5970a（2026-11-13 15:28，九十一轮）commit 题名「pick 22cy(-15.4%)」且 json@该提交=22 ✓。链中段 26：d709cc5a（七十六轮）json pick=26 ✓，01 :19「22→26 归因同键链初始化 +4cy」被链段「非回归」注收纳。时序自洽：01 号写于 11-13 12:42（时点 26 为真），f7b5970a 15:28 刷 22——任务书「现行 22cy」与 01 号各自时点成立。互证：调度器设计 v1.2 :425 基线链注记（21 时点值/26 SLEEPQ 重测/22 现行两源）与白皮书 :89 逐句同口径。
- **漂移②（SMPD-3/4 未入，01 :20/:98）→ 已入四处**：§3.2 :38 divergence per-CPU 分账（64 字节对齐槽、写本核零争用、读求和）＝SMPD-4；§3.4 :46 band 预算热字全局原子账（fetch_add 单赢家、CAS 防双翻）＝SMPD-3；§6 :69 双核终验（`-smp 2` 60 项断言、四族回归）；§11 :109 SMPD 刀序实况（SMPD-3/4 已落＋0/1/2/5 余项）。代码实况吻合：`sched/src/policy.rs` :618-626（CountSlot 64B align/size 双断言＋DIVERGENCE_PER_CPU/EVAL_PER_CPU）与 :690-706（本核 fetch_add、读侧求和、清零环扫）；`sched/src/band.rs` 头注（单赢家/CAS 防双翻自述）与 `CONSUMED/DEADLINES: [AtomicU64]` 静态账。提交三笔 git 实证：1869412f（SMPD-3）/078073be（SMPD-4）/54fc9b64（双核终验 60 ok）。白皮书 §11 未展开 SMPD-0 的 X-6 阻塞细节——外宣总览粒度取舍，与 01 号 §1.6 判定不冲突。
- **漂移③（路线图节过时，01 :98「路线图节未含 SMPD-3/4」）→ 已刷新**：§11 :107-117 标题改「现状与方向」，新增 SMPD 实况／W3 观测面外迁／PM 双服务／总纲挂调度彻底计划四条，治理面改写（RIGHT_MANAGE 现行＋Job 树级挂 v2）。新增两条的事实核：W3「已并 22 个 crate」——01 号 :120 枚举 22 个 face 族 crate 与根 `Cargo.toml` workspace members（27 成员）逐一在席实测 ✓；PM 双服务「骨架已立、host 单测绿」——[04 号](04-PM双服务骨架复核.md) §0 判语（berry-power 28/28、berry-thermal 12/12 绿，`user/bins/` 两目录在席）在册，01 号 :143「服务本体未建」为 11-13 12:42 时点真值、其后 PM 波落骨架（04 号 15:57 复核），时间线自洽。
- **域外关联面**：v1.0 单源自认句在 01 号时点写「设计 v1.1」，HEAD 的 v1.0 已被轮三（098ca8bf，设计 v1.2 回填 19 文件同步）改指 v1.2——v1.1 :4/:138 沿用 v1.2 ✓（`docs/调度器设计-v1.2.md` 在席）。

## §3 链改指（引用随迁）

改指面实测：`docs/索引.md` :50 白皮书条目整行改写为 v1.1（描述含「pick 22cy 现行 CI 基线带时点注／SMPD 实况」——与正文一致）；`docs/调度彻底计划-v1.0.md` 五处 live link 改指 v1.1（:68 裁三、:74 裁六、:105 CP-09、:161 基线、:227 文档族清单，git diff 逐处确认）。死链扫描：全仓 `](…调度器架构白皮书-v1.0.md` 形 live link 0 条 ✓；v1.0 残留提及仅两类——research/调度彻底计划 01/02/03/07 号时点史料（行锚带日期口径，按修订记录「research 时点史料不回改」声明保留）与设计 v1.2 :436 修订记录内的反引号史实条目，均非活链、符合一事实一归属的时点引用惯例。check-docs 0 告警（链接全可达）机器坐实。**瑕疵②（行锚漂移，§0-6）**：§8 基线链段两行插入使 :88 之后 +2——`:81-83`（性能表行）在未漂移区仍精确；`:93-103`（对标表）跨度 :97-103 仍被罩住（偏 2 行）；`:109` 语义落点漂至 SMP 条（band 执法今 :113、EAS 今 :112）。按设计 v1.2 :436 自宣布的仓内惯例（「外部行锚引用随增行漂移、引用带时点、不回改」）可不回改，但白皮书修订记录「全仓引用随迁」若按含锚理解则欠半步——如实记，定性不动（无死链、门禁绿）。

## §4 wgpu stride 路径与 damage 口径——数据可信

**几何与格式锚（15 号 §1 逐项验源）**：GA 1280×800/5120＝[11 号](../显示与加速体系设计/11-B1宿主通道-dbus-p2p实证.md) :45 实收（`ScanoutMap fd=14 off=0 1280x800 stride=5120 pixman=0x20020888`）＋:52（0x20020888=PIXMAN_x8r8g8b8 BGRX 字节序）＋:58（lsof 4096000=5120×800 与几何精确互证、mmap 读通）——stride 5120 算术上＝1280×4 紧凑且恰 256 对齐，15 号 §1/§6-97 对任务书「非紧凑」措辞的纠偏成立，非对齐真实面由 GB（800×600/3200，09 号 :36 m1 GOP 实收，3200%256=128）与 GC（1080×1920/4320，03 号桌面档，4320%256=224）覆盖。BGRX 直配 `Bgra8Unorm` 判定与 11 号 :84 同判 ✓。**damage 口径**：K=8 注明「guifbd DAMAGE_MAX 同值（user/berry-ui/src/damage.rs K≤8）」——本轮实测该文件 :14 注释（K≤DAMAGE_MAX=8）与 :211 `rects: [Rect; berry_guiproto::DAMAGE_MAX]` ✓；16×10 字符格形状＝09 号 :79 VNC 面实证矩形族（16×10/16×7/32×10）同源 ✓。**计时口径**：main.rs :464-476 实读——damage 帧先 `fill_rect` 预写替身（模拟 QEMU 写共享图）后仅对 K 次 `upload_direct_bpr` 计时（预写在计时窗外），「壳半真实成本」口径成立；路径 A 切片形态 `&map[y*stride+x*4 .. +span]`（span=(h-1)*stride+w*4）＋bpr=stride＋offset=0 与 15 号 §5-1 形态建议逐字一致；readback 对拍走 `copy_texture_to_buffer` 256 对齐 bpr＋逐像素比较（:202-268），对拍基准为确定性图案（X 固定 0xFF 且 §5-3 如实注 QEMU X 字节实为未初始化、呈现着色器强制 alpha=1.0——main.rs :688 WGSL 实读）。**依赖面**：`/tmp/wgpu-poc/Cargo.toml` wgpu `=30.0.2`＋Cargo.lock 三锁（wgpu 30.0.2/winit 0.30.13/pollster 0.4.0）✓；255 包＝lock 256 条目−自身 ✓；壳 winit 先例 `tools/berry/Cargo.toml` :23（winit = "0.30"）实证——host 面外部依赖合法性先例成立。

## §5 独立复跑（本轮四模式，2026-11-14）

`CARGO_HOME=/tmp/wgpu-poc/cargo-home cargo run --release --`：`unaligned-bpr` PASS（write_texture bpr=3200 接受且 readback 逐字节一致）＋`offset-probe` PASS（layout.offset=51264＝64B 对齐、origin=(16,10) 矩形定位正确）＋`offscreen` PASS（blit 首帧 872.8µs、稳态编码+提交 p50=124.0µs——15 号记 886.7/126.1，同带）＋`gauge` 全档（adapter=Apple M3 Pro/Metal/IntegratedGpu 确认，`/tmp/wgpu-poc/gauge-verify.log`）：GA A p50 296.7µs（15 号三轮 303.3-419.1）、GB 157.7（154.3-173.6）、GC 590.4（606.0-645.3）、damage A 25.0（24.9-26.2）、e2e GA/GB/GC=299.5/151.2/616.0（15 号 305.8-334.6/148.0-159.2/617.2-671.5）、memcpy 地板 67.6-79.3GB/s（15 号 71.0-77.7）——全部落在 15 号三轮区间同带或略优，verify（full/damage）全 PASS；A>B>C 结构序、直配合法性、K=8 帧 ≈25µs=0.15% 预算、GC 全帧 ≈3.9% 预算四项定谳独立复现。结构性结论对本机复核者封闭，绝对值跨机外推谨慎（15 号 §6-93 同款声明）。

## §6 数字转录对拍（15 号表格 vs 三轮留存日志）

§2.1/§2.2/§2.3 全表逐格对 `gauge-run1/2/3.log`＋`offscreen-run2.log`＋`window-run1/2.log`：**26/27 格逐位一致**（含 GA 持续 452.5/309.4/313.1、GB A 173.6/154.3/156.8、GC B 2103.6/830.0/777.0、C 3774.2（持续 2621.7-4075.6）、GC B 首轮尾部 p95 5723.2µs/max 69450.5µs、e2e 三几何九值、damage A/B 六值、offscreen 886.7/126.1、window Occluded/Timeout ×600）；唯一偏差格＝GA A 首轮 p50 **419.1 vs 日志 418.9**（0.2µs，0.05%，笔误级——瑕疵①）。memcpy 地板表行取 run1 值（77.0/77.1/71.9）✓、§0-3「72-77GB/s」区间为四舍五入口径（run2 实测最低 71.04，边界格）。附带：`window-run1.log` 为旧版代码产物（裸 `Occluded` 无 ×600 有界重试，现行 main.rs 已改有界重试）——日志混版留档，轻瑕不计入偏差格。

## §7 门禁＋git 对拍（guifbd 零碰）

**门禁**：`node .agents/tools/check-docs.mjs`＝检查 218 篇／告警 0 条／退出 0（本件落盘后复跑同绿）——07 号 §3.3 时点的 12 条告警（调度彻底计划 12 处设计 v1.1 死链＝当时 owner 占用延迟面）已消失，占用解除后随迁收口。**git 对拍**：全仓 staged 0 笔（`git diff --cached` 空）✓；两 wave 仓内足迹清单＝`D docs/调度器架构白皮书-v1.0.md`＋`?? docs/调度器架构白皮书-v1.1.md`（mv 簇）＋`docs/索引.md`/`docs/调度彻底计划-v1.0.md` 白皮书链接改指部分（M 态与他线混批，diff 归属可分）＋`?? research/显示与加速体系设计/15-wgpu上屏PoC.md`＋显示线 README :18 索引行（M）——除此之外零足迹，PoC 本体全在 `/tmp`（零仓改声明成立）。**guifbd 零碰**：工作树 `M user/bins/guifbd/src/guest.rs`（+319）/`lib.rs`（+352）diff 全文核读＝v2 帧批面（`rects_clip_into`/`frame_flush_batch` K×TRANSFER＋1×FLUSH 包络/`batch_resps_ok` 回执桶——12 号 §4.3 模型的 v2 实现），PM/白皮书/wgpu 关键词零命中，mtime（lib 09:14/guest 03:28）与两 wave 产物（白皮书 09:21/15 号 09:38）同窗不同线——归因 v2 续做会话，两 wave 零碰 guifbd 坐实；kernel（dpg.rs mtime 11-12）/launcher（frameloop 11-13 04:38）的 M 件同为他线在途，sched/ 无 M 件。纪律面：本轮零 git add/commit/push，仓写仅本件。

## §8 瑕疵与残留清单

- **瑕疵①（数值，轻）**：15 号 §2.1 GA A 首轮 p50 转录 419.1，gauge-run1.log 实为 418.9——唯一偏差格，0.2µs 不动任何判读。
- **瑕疵②（链改指，轻）**：白皮书随迁只换文件名未同步行锚，`:88` 后 +2 漂移——调度彻底计划 `[白皮书]:109`（band kill/EAS 框架）今落 SMP 条（band :113/EAS :112）、`:93-103` 仍罩对标表；按仓内「行锚随增行漂移、带时点、不回改」惯例可豁免，建议下次触碰调度彻底计划时顺手对齐或补时点注。
- **瑕疵③（证据留档，轻）**：15 号 §2.3「离屏两轮」之另轮首帧 4084.8µs 无留存日志（§7 产物清单如实未列，自洽但少一件）；`window-run1.log` 为旧版无界重试代码产物，与现行行为不同版。
- **残留①（在册挂起，非本轮义务）**：13 号 §0-4-②「壳全帧 write_texture ≈0.12ms@1080×1920」算术预估待升版按 15 号实测修正 ≈0.62-0.65ms（×5，per-call TRANSIENT staging 税；15 号 §0-3/§5-6 已声明挂 13 号升版随 §5 清单回填；13 号工作树未改、:12 仍 0.12ms——挂起状态与声明一致）。
- **残留②（任务书预授权）**：present 节拍未测（无可见窗会话 `get_current_texture` 恒 Occluded，重试 600 次）——留 B1-b 壳半有窗会话首刀补测；vsync/Retina scale/desired_maximum_frame_latency 三参数同留。

## §9 复核方法与产物

v1.0 底本：`git show HEAD:docs/调度器架构白皮书-v1.0.md`（136 行，/tmp/wp-v1.0.md 快照）与工作树 v1.1 全量 diff。基线链：`git log --follow -- tools/sched_baseline.json`（b18cd13d→f651dfc5→d709cc5a→f7b5970a）＋各提交 json 直读。SMPD：1869412f/078073be/54fc9b64 `--stat` 核面＋`sched/src/policy.rs`/`band.rs` 现行实读。wgpu 源码：`/tmp/wgpu-poc/cargo-home/registry/src/index.crates.io-1949cf8c6b5b557f/`（wgpu-types-30.0.2/wgpu-core-30.0.2）四件行号直读。独立复跑：四模式各一，gauge 全档产物 `/tmp/wgpu-poc/gauge-verify.log`。转录对拍：15 号 §2 全表 vs 五份留存日志逐格。引用形态：本件路径:行号均为 2026-11-14 工作树时点（v1.1/15 号为未跟踪新件，行号随其落盘态；后续增行漂移按仓内惯例不回改）。

## 来源索引

**仓内文档（时点 2026-11-14 工作树）**：[01-调度现状全景与在途线收编.md](01-调度现状全景与在途线收编.md)（:19-20/:98 漂移三件/:120 face 22 crate 枚举/:143 PM 时点）；[04-PM双服务骨架复核.md](04-PM双服务骨架复核.md) §0（PM 骨架判语）；[07-轮三复核.md](07-轮三复核.md) §3（v1.2 副刀与 12 条告警时点、白皮书 21cy 域外残留记录）；[调度器设计-v1.2.md](../../docs/调度器设计-v1.2.md)（:425 基线链注记/:436 行锚惯例自宣布）；[15-wgpu上屏PoC.md](../显示与加速体系设计/15-wgpu上屏PoC.md) 全篇；[11-B1宿主通道-dbus-p2p实证.md](../显示与加速体系设计/11-B1宿主通道-dbus-p2p实证.md)（:45/:52/:58/:84 ScanoutMap 实收）；[09-B1bc复核.md](../显示与加速体系设计/09-B1bc复核.md)（:36 GB 几何/:79 damage 形状）；[13-GPU加速深化综合定谳.md](../显示与加速体系设计/13-GPU加速深化综合定谳.md) :12（0.12ms 待修项）；`docs/调度器架构白皮书-v1.1.md`（:3/:38/:46/:69/:81/:89/:107-117/:144-147）；`docs/索引.md` :50；`docs/调度彻底计划-v1.0.md`（:68/:74/:105/:161/:227 链改指五处）。

**git 实证**：f7b5970a（CI 基线刷新 pick 22cy）/d709cc5a（七十六轮重测 pick 26）/1869412f（SMPD-3）/078073be（SMPD-4）/54fc9b64（SMPD 双核终验 60 ok）/098ca8bf（轮三设计 v1.2 回填）；`git diff --cached` 空（零暂存）。

**代码实测**：`sched/src/policy.rs` :618-626/:690-706；`sched/src/band.rs`（头注＋CONSUMED/DEADLINES 静态账）；`user/berry-ui/src/damage.rs` :14/:211（K≤8）；`tools/berry/Cargo.toml` :23（winit 0.30 壳先例）；根 `Cargo.toml` workspace members（face 族 22 crate 在席）。

**本轮实测产物**：`/tmp/wgpu-poc/gauge-verify.log`（gauge 复跑全档）；`/tmp/wp-v1.0.md`（v1.0@HEAD 快照）；四模式复跑输出（探针双 PASS/offscreen PASS/gauge 同带）；wgpu registry 四件行号直读记录。

**PoC 留档（15 号 §7 清单复核）**：`/tmp/wgpu-poc/`——`Cargo.toml`/`Cargo.lock`（255 依赖）/`src/main.rs`（1161 行四模式）/`gauge-run1/2/3.log`/`offscreen-run2.log`/`window-run1/2.log`。
