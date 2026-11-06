# SDK/API 历史教训深研证据档（wf_82fdb90d 全量）

> 2026-11-07 ｜ 证据档（工作底稿）｜ 判词收拢版 = `research/SDK-03-系统API历史教训与一次设计定谳-v1.0.md`

一轮 workflow 深研（11 agents 八面：Android/iOS/Windows/OpenHarmony/
兼容层与Web/演化纪律/AI原生/berry-repo 盘点）的全量原始结果，
自 /tmp 收拢入库（/tmp 易失；本档为 SDK-2 立法工作底稿的永久居所）。

| 档 | 内容 | workflow |
|---|---|---|
| `wf-result-full.json` | 63 课全量（外课 41 + berry 盘点 22）+ top3 课对抗复核记录 | `wf_82fdb90d-bcd` |
| `落库刀-spec.md` | 落库刀 spec 原稿（f4 拟，出处随档） | — |

## 用途与读法（两层纪律，与内核定名证据档同款）

- **课程层**：`result.all_lessons[*]`——每课含 `claim`（史实判词）、
  `berry_map`（BerryOS 落点：已定面引用/立法点）、`source`（出处 URL）。
- **复核层**：`result.verdicts[*]`——三课（target 门控/废弃纪律/细粒度资源）
  独立对抗复核，判定均「成立但需修正表述」；**`fixed_claim` 为修正后表述，
  引用立法条文时以本层为准**——只读 all_lessons 层必漏修正。
- 判词收拢版（五原则/三不可/41 外课速览/三张清单/SDK-2 交刀序）以
  SDK-03 报告为准；本目录只存证据不存结论。
- 落库时 `wf-result-full.json` 与 f4 交接区
  `/tmp/k3-review/sdk-api/wf-result-full.json` 逐字节一致（f4 cmp 复验）。
