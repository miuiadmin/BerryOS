# G2 svc 注册表五号漏同步 + tools/berry dmface 依赖漏登（双 base 债清偿）

Status: implemented
Type: note

## 事实

2026-11-21 轮（hermes/w3-202611212205，base b3729927d 系）清偿两件 base 债：

1. **G2 svc 债（reviewer R199 留债⑤）**：base G2 刀 7（0x4A/0x4B 受限态原语）与刀 10（0x77/0x78/0x79 Hypervisor MicroVM）五笔 `install()` reg() 落地未同步 `berry-svcface` `REGISTRY` 与 `tools/svc_registry_check.py` `EXPECT_ENTRIES`——三方对拍必红（install 52 行 vs REGISTRY 47 项），堵全部 PR test-host 门。修法 = #610 六联动面判例同款：REGISTRY 按号序补五号（全 SVC_CLASS_TABLE）+ 计数注释滚新 + EXPECT_ENTRIES 47→52。
2. **tools/berry dmface 依赖漏登**：base `tools/berry/src/commands.rs:2057` 消费 `berry_dmface::parse_display_mode` 但 `tools/berry/Cargo.toml` 漏登 path 依赖——独立小工作区 `cargo test` E0433 必红，堵 test-host 链。修法 = berry-cbox/berry-fatfs 跨 workspace path 依赖先例同款：补 `berry-dmface = { path = "../../dmface" }` + lock 滚新。

## 判例

- **G2/跨 lane 新增 syscall 号的三方联动义务**：install() 落号时 REGISTRY/EXPECT_ENTRIES/（若 v1/1.1 面）abi_gen 常量层必须同批滚——svc_registry_check 在 test-host 链执法，漏同步即堵全部 PR 门（#610「第六联动面」判例的再次实证）。
- **独立小工作区消费 workspace crate 须显式 path 依赖**：根 workspace members 对独立小工作区（自带 lock）不可见；消费面落笔时 Cargo.toml 与 lock 必须同刀（tools/berry 此前两次同病：berry-cbox/berry-fatfs 均为补刀）。

## 验证

svc_registry_check PASS 52 号；svcface 8 host 测绿；tools/berry 373 测绿；test-host 全链 EXIT=0（310 组 3319）；docs-guard 0 警；unsafe 台账 1233 对拍 ok（svcface 零 unsafe 零漂移）；abi-check PASS；check-docs --strict 716 篇 0 警。
