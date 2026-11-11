# SDK-2 错误域与机器可读面设计 v1.0

> **性质**：错误即数据（枚举+附因+两舱）、拒因三元组、类型化不透明 ID、schema 三出口与封闭值域纪律的接口设计（立法档）。
> **溯源**：SDK-03 原则 4 + AI 原生面课（gRPC/MCP/RFC 9457 三源同向；Stripe 类型化 ID；RFC-0190 单源）；史实见 SDK-03 §二/§四。
> **状态**：设计定稿 · **P0 已开工 2026-11-11**（P0-1 信封 TLV/信任账本 + P0-2 自研 ed25519 原语两刀入 base，沿革二百三十一/二百二十三；错误域前置小刀已先落 2026-11-08）· 预 ADR（§八）；错误域与拒因面**可前置独立小刀**（SDK-4 拍板明言拒因三元组可前置）。
> **基座**：ADR #54（host 测编解码先行 / SDK-4 四 verb / SDK-1 dump round-trip）· ADR #58 intent 六档封闭词表（封闭值域范本）· ADR #48（机器可读 SDK 面）· POSIX errno 语义保留。

## 一、错误 = 稳定枚举 + 结构化附因

**总则**：跨边界错误 = 机器枚举 + 附因结构；**裸字符串错误不许穿过服务边界**
（人读文案由消费端从枚举+附因渲染）。

```rust
// 形状示意（berry-err 或 berry-sys 内模块，落位随刀定）
pub struct BerryErr {
    pub code: ErrCode,          // 稳定枚举，封闭值域，只加不改义
    pub scope: ErrScope,        // 两舱：Actionable（可自纠）/ Protocol（请求结构坏）
    pub detail: ErrDetail,      // 附因结构，按 code 分型
}
pub enum ErrDetail {
    BadParam  { which: u8, expected: &'static str, got: &'static str },
    MissingCap{ select: &'static str, why: CapWhy },   // 未声明/未授予/已撤销
    Retry     { retryable: bool, after_ms: u32 },
    // …按需分型追加，字段只加不改义（同 wire 纪律）
}
```

- **两舱**（MCP 先例）：`Actionable` = 调用方改参数/补能力可自纠；
  `Protocol` = 请求结构坏（编码错/版本不识），重试无意义。消费端（含
  agent）按舱分支：前者给修法，后者给上报。
- **syscall 面（L0 冻结不破）**：35 号签名零改动；立法落在**返回值语义**
  ——errno 域保留 POSIX 子集，BerryOS 自有扩展码走 BerryErr 结构经
  berry-sys 层暴露（rich error 在库层，不在 ABI 层）。
- **IPC 拒因面**：服务 wire 拒绝消息按同形状（code+scope+detail 的 TLV
  编码，进各协议 crate 黄金字节测试）。

## 二、拒因三元组（编译期/工具面，可前置）

现状：mkapp/berry-manifest 拒因 = 一行中文（loud 但不可编程消费）。立：

1. 拒因三元组 = **位置（行/列）+ 期望 + 修法**，结构化先行、中文文案后渲染。
2. 双出口：人读（现役一行中文，由结构渲染）+ 机读（`--json` 输出同结构）。
3. SDK-4 已拍板可前置——**独立小刀位**：berry-manifest 校验器拒因结构化 +
   mkapp `--json` 双读，不必等 SDK-2 全案。

## 三、类型化不透明 ID（语法护栏）

**总则**：dump/遥测/bsh --json/日志中所有跨边界 ID 一律**类型化前缀不透明
串**，禁裸整数裸指针（LLM 会自信张冠李戴，语法护栏是它唯一必然踩中的护栏）。

| 前缀 | 对象 | 例 |
|---|---|---|
| `hdl_` | 内核 handle | `hdl_3f2a` |
| `pid_` | 进程/任务 | `pid_11` |
| `srf_` | surface/视图 | `srf_07` |
| `chn_` | IPC channel | `chn_c4` |

- 规则：前缀闭集（berry-err/遥测 crate 单源定义）；同对象跨 dump 注入
  round-trip 前后稳定（SDK-1 dump 纪律延伸）；数值部分编码不承诺语义
  （防 Hyrum 依赖），类型信息全在前缀。
- 落点：SDK-1 DFX（dump/截图/对拍）与遥测环输出统一改造；bsh --json 同步。

## 四、schema 三出口（接口即数据）

**总则**：每个系统面从**单一 schema 源**生成三个出口，禁止任何手写副本：

```
schema 源（abi.toml / manifest 定义 / 工具 verb 定义 / 错误码表）
  ├─ Rust 绑定（现役 gen 产物路线，RFC-0190 教训）
  ├─ 人读文档（marker 块生成，现役四产物之一）
  └─ LLM tool 定义（JSON Schema 导出——新增出口）
```

- LLM tool 定义出口：syscall 面/工具 verb 面/错误码表导出为 JSON Schema
  （OpenAI Structured Outputs / MCP tool 形态兼容），进 SDK-4 agent 面
  消费；**schema 变更 = abi-diff 的一部分**（章程 §六）。
- **封闭值域纪律**：凡进入 schema 且 AI 参与读写的字段，一律封闭枚举或
  有限文法（intent 六档为范本）；开放自由文本字段不进 AI 写路径。

## 五、验收线

1. berry-err 形状定稿 + errno↔ErrCode 映射表（进「不承诺面」清单的是
   **映射次序**，枚举值本身是承诺面）。
2. 协议 crate 拒因 TLV 编解码 host 测 + 黄金字节钉板。
3. 拒因三元组前置小刀：berry-manifest 拒因结构化 + mkapp --json 双读
   host 测（人读渲染不回归）。
4. ID 前缀闭集单源 + dump round-trip 前后稳定断言。
5. 错误码表 JSON Schema 导出一次走通（三出口同源 diff 为零）。

## 六、ADR 落册点

- #75（候）：错误域立法（枚举+附因+两舱 + 禁裸字符串过边界 + 拒因三元组）。
- #76（候）：类型化 ID + schema 三出口（机器可读面一揽子）。与 ADR #48/#54
  关系：#48 定「ABI 单源+协议 crate 家族」方向，本 ADR 定错误/ID/schema
  三件具体形状。
