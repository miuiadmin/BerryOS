# berry-term 超时会流分诊刀（Major-3 清偿：异步背压＋双信号分诊＋terminate 先行＋respawn 收口门）

Status: implemented

一句话说明：owner 令「彻底实施…review…合入」续——规格轮 wf_9e3d886c＋实施轮 wf_24a1e45e＋主线修复三段式清偿跨刀 review Major-3：键路满队 10s 帽与链断折叠误杀（sleep 期间敲 9 键杀健康子→冻结 40s→活孤儿占 64 帽→respawn 双 bsh 共享 fs 通道禁例形）四面根除——①键路异步保序背压（child_send 拆非阻塞三态 SendFeed；Full 转 triage Backlog 本地 FIFO 256B 溢出丢新计数；帧循环顶排空步＋wait_many 5 路含 in_s WRITABLE 背压臂——EBUSY 从死判面除名）；②真死双信号分诊（L1 stdout EOF＞L2 hp TERMINATED 恒挂臂＞L3 错误打标——DeathEntry 四入口经 converge() 单路收敛）；③收口漏斗（mark_child_dead 重构：排空残留→文案零滚→陪葬计数→关双端→无条件 terminate(130)（pipe_selftest 先例·-37 幂等）→reap 瞬回 30s 兜底→Err 熔断）；④respawn 收口门（proc.is_none() 结构断言双路——双活 bsh 禁例结构性不可达）；berry-termio 新增 triage 纯面模块（28 host 测含保序负证/收敛表）；TermCore/kernel/berry-exec/bsh/berry-libc 零改；审查 1 minor 修（忙段中途死亡丢键计数并入陪葬行）。

## 裁决与判例

- **EBUSY≠死信号立法**：满队是流控不是故障——死判唯 L1/L2 双信号（对端关闭检测 v1 不存在的诚实缺口下 TERMINATED wait 是非配合死唯一权威窗）。
- **保序不变式结构性成立**：pending 非空一律入缓冲不旁路（误喂 Sent 亦然）——排空步是满队期唯一发送者，帧循环单任务零竞态；triage_sim bypass 负证测试证明 FIFO 断言真在咬。
- **terminate 先行收口**：reap 的 30s 假设（EOF 先行）在误判场景破产——无条件 terminate(130) 前置使 wait TERMINATED 瞬回；pipe_selftest 先例同码。
- **收口门>运行时检查**：respawn 前置结构断言使禁例形状不可达（自动路注释立法防刷屏/手动路一次性诊断）。

## 验证

berry-termio 28 host 测全绿（17 旧零扰动＋7 新 triage 族＋2 集成＋2 pipe_sim）；berry-term musl 零本刀警；kernel/berry-exec/bsh/berry-libc/berry-sys/tools 零 diff；pipe_selftest/term_collect 函数体零字符改；rg 自查键路/排空路零带帽 wait_one。e2e（sleep 30＋连敲 9 键＋exit 双场景）挂账低载窗——判据在刀谱风险节。

## Related

- [berry-term 超时会流分诊刀实施设计](../../plans/2026-11-20-berry-term超时会流分诊刀实施设计.md)
- [2026-11-20-跨刀整体review清偿批](2026-11-20-跨刀整体review清偿批.md)（Major-3 移交源——本刀清偿）
