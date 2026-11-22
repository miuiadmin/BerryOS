# ai-runtime §一.1 首刀：.bmod 容器格式件 berry-aiproto

Status: implemented

一句话说明：E lane 首跑第一刀（DAG buildable 首项 `ai-runtime §1` 的有界片段）——把契约一.1「零反序列化直接映射格式 `.bmod`」从文字落成机器执法面：新 crate `user/berry-aiproto`（「解析器即产品」家族，berry-partition/berry-fatfs 同款）定义 BMOD v1 二进制布局并全量结构执法（Ed25519 签名域 + 页 0 闭合有界 TLV 超参数表 + 4096B 页对齐张量段表，Q4_0/Q4_K_M/Q8_0/FP16/BF16 五格式枚举），零依赖零 IO 零分配、全接口零拷贝切片、畸形输入零 panic；host 19 测。契约一.2（airtd VMO 池）涉内核面留后续刀，design.md §一 已挂建造清单。

## 裁决与判例

- **格式先行于运行时（DAG 首项切片法）**：条款 1（格式）与条款 2（VMO 池）一体两面，但格式件零依赖纯新增、运行时件涉内核 mm/svcmgr——按「更可逆、更保守」序切条款 1 先落，条款 2 等边界声明。`A: (run decision, 2026-11-22) 格式件先行（纯新增 crate，不动消费面）. Reversal: 删 crate + 撤 members/test-host 两行即净退。`
- **TLV 有界性 = 页 0 闭合单帽**：契约要求「超参数有界 TLV 表」，本刀不给 TLV 第二套数字帽——`header_len ≤ 4096` 一条物理事实同时封顶 TLV、段表与头部区（单一事实源纪律的格式面投影），帽外即 `BadHeaderLen` loud 拒。
- **签名覆盖域单源 API**：Ed25519 校验本体归消费侧（berry-crypto/airtd），但「签的是什么」必须无歧义——`signed_span()` 单源给出覆盖域（前导外全文件：TLV + 段表 + 页 0 尾填充 + 全部张量页），消费侧禁自拼第二份偏移。
- **段间重叠不执法**：tied weights（嵌入与输出头共享）是合法别名形态，重叠归消费侧策略——解析层执法结构律（对齐/界内/格式域），不执法资源策略。
- **reserved 非零即拒**：段条目 reserved u16 与前导 reserved[6] 全零执法（前向兼容面显式为零，静默吞未知位 = 漂移温床）——与 GPT 头 CRC「不执法面」注记相对，零值域是结构律不是策略面，故执法。

## 验证

`cargo test --manifest-path user/Cargo.toml -p berry-aiproto --target aarch64-apple-darwin` 19/19 绿（roundtrip/签名域/十三坏样例腿：坏魔数/坏版本/坏算法/坏保留位/页闭合两腿/TLV 截断两腿含 1 字节尾/段表失配/零段/页下偏移/非对齐/越界两腿/域外格式/段保留位/加密旗透传/截尾镜像）；全量门禁见本轮 PR（test-host 全套件 + docs-guard + facts 对拍同批刷）。

## Related

- [ai-runtime 总体设计](../../plans/ai-runtime/design.md)（契约一.1 本刀兑现、一.2 建造清单挂账）
- [2026-11-18-整机十维缺失拼图全栈终态架构裁决](../proposed/2026-11-18-整机十维缺失拼图全栈终态架构裁决.md)（模块立项源）
