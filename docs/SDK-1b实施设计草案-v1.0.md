# SDK-1b 实施设计草案（六 crate crates.io publish-ready 工程面）

> 版本：**1.0**（v1.0）｜2026-11-06

> 【状态】已落地 2026-10-30（实施全记录 + hash 实录表见 §9 修复轮记录；收官追记见 `阶段沿革.md` 2026-10-30 条——后续演进不回写本文）
> 【溯源】ADR #57 拍板⑥（「berry-libc 及协议 crate 族现在上 crates.io」）+ SDK-1a
> 草案 §10 O6 挂账 + 双 agent 基线实录（2026-10-30：六 crate 工程面事实清单 +
> `/tmp/sdk1b-baseline/` 证据文件——hashes.txt / testhost.txt / pkg-*.log /
> pub-dryrun-*.log / `/tmp/sdk1b-dualwrite/` 双写沙箱）。
> 【产制】2026-10-30 workflow 编排（SDK-1b 实施设计草案合成员）。行号 =
> 2026-10-30 工作区实测（含兄弟会话在途改动）。
> 【补充实测】本草案合成员在 `/tmp/sdk1b-dualwrite/` 沙箱追加四则通道探针
> （§1.2 E1–E4，与基线实录分列标注——「实测说 X」均可回溯到对应实录/探针）。

---

## 0. 范围、红线与不做清单

### 0.1 本刀交付（= 刀形五件）

| # | 交付件 | 落点 |
|---|---|---|
| D1 | 六 crate publish 元数据齐备（description/repository/readme/keywords/categories；version/license 已有面保持或补齐） | 六份 `user/berry-*/Cargo.toml` + 六份新 `README.md` |
| D2 | 内部依赖 path+version 双写（fsclient 三条、libc 四条） | 同上 |
| D3 | crate 级 docs 面（lib.rs 头部第三方视角 Overview 块） | 六份 `user/berry-*/src/lib.rs`（**只增 `//!`，零代码改动**） |
| D4 | CI publish 守卫步 | `.github/workflows/ci.yml` +1 步（**勿动 justfile**） |
| D5 | 发布手顺文档 | `docs/应用开发指南.md` 文首双态改写 + 新增 §9 + §3.2 一行括注 |

### 0.2 红线（多会话纪律）

1. **兄弟会话在途禁写**（工作树现存未提交改动，一律禁碰禁改禁暂存）：
   `kernel/src/*`（含未跟踪 `kernel/src/mem.rs`）、`arch/aarch64/src/task.rs`、
   `justfile`、`.cargo/config.toml`、`research/GUI-*`、
   `docs/GUI里程碑规划-2026-10-v1.0.md`、`docs/架构决策.md`、`docs/阶段沿革.md`、
   `tools/smoke.sh`、`user/bins/bsh/src/main.rs`、`user/bins/netprobe/src/main.rs`。
2. **本刀改动面白名单**（越表 = 冲突，上报不硬抢）：六份
   `user/berry-{sys,tls,netdproto,fsproto,fsclient,libc}/Cargo.toml`、六份
   同目录新 `README.md`、六份 `src/lib.rs`（仅头注）、
   `.github/workflows/ci.yml`（仅 +1 步）、`docs/应用开发指南.md`。
3. **禁 git commit/push/add**（主会话统一提交）。
4. **不执行最终 `cargo publish`**（需用户 crates.io 账号——发布是用户动作，
   本刀交付到「publish-ready + 手顺文档」为止）。
5. smoke 零漂移：`tools/smoke.sh`/`justfile` 零改动；本刀全部改动为元数据/
   文档/CI 面，不进编译产物（§7 hash 验收即此承诺的执法面）。

### 0.3 不做清单（刀形原样 + 随刀补边界）

- **不做**最终 `cargo publish`（用户账号动作，§6 手顺交付给用户）。
- **不做** `berry-exec`/`berry-fstree`/`berry-manifest`/`berry-psproto` 等
  svcmgr 侧或工具侧 crate 入发布族（发布族恰六件，后续刀扩）。
- **不做**语义版本纪律细则（ADR #57 已有原则，抬级流程归版本纪律另刀）。
- **不做** net 词相关任何面（归兄弟会话 A5 线）。
- **不做** `deny.toml` 改动：cargo-deny CI 步的 `-A no-license-field` 豁免在
  berry-libc 补 license 后**可收紧**，但 deny.toml 不在本刀白名单——挂账 §8。
- **不做** LICENSE 文件随包；`[package.metadata.docs.rs]` 原列不做——修复轮
  （审查 F2，§8 挂账已解）撤回半项：三件 asm 面 crate 已配 targets 覆盖。
- **不做** bins（svcmgr/bsh/uu 家族…）对六 crate path 依赖的双写化——bins
  不发布，path-only 是正确形态（防 scope creep，明示排除）。

---

## 1. 事实基线与通道实测（设计依据全录）

### 1.1 现状元数据（基线实录 §1）

六 crate 均为 `user/` workspace 成员、version 全 `0.1.0`、edition 2021、
无 `[lib]`/`include`/`exclude`/`publish` 字段、无 tests/ 目录、无 build.rs、
无 README、共享 `user/Cargo.lock`（hash 前缀 `2f5fdfed`）。

| crate | description | license | repository/readme/keywords/categories | 内部依赖（实测，**非刀形卡片所述两条**） |
|---|---|---|---|---|
| berry-sys | 有 | 有 | 缺 | ——（叶子） |
| berry-tls | 有 | 有 | 缺 | ——（叶子） |
| berry-netdproto | 有 | 有 | 缺 | ——（叶子） |
| berry-fsproto | 有 | 有 | 缺 | ——（叶子） |
| berry-fsclient | 有 | 有 | 缺 | **三条**：berry-sys + berry-fsproto + **berry-netdproto**（Cargo.toml:19/20/24——A3-2 socket 面加的第三条，刀形卡片漏记，以实测为准） |
| berry-libc | **缺**（六件唯一） | **缺**（六件唯一） | 缺 | 四条：berry-sys + berry-fsclient + berry-tls + berry-netdproto（Cargo.toml:12/15/19/22） |

五件现存 description/license 随 0d275a8 入册；全部内部依赖为 path-only 无
version 键。`berry-sys/src/abi_gen.rs` 是生成文件（单源 `tools/abi.toml`，
`just abi-check` 守卫）——随 src/ 自动进包，**不需也不得手工处理**。

### 1.2 通道实测（设计每一档执法的依据）

基线实录（B 段，2026-10-30）：

- **B2**：`cargo package -p berry-sys` 全链 PASS——verification build 的
  target 是 `aarch64-unknown-none`（根 `.cargo/config.toml` 锁 build.target
  且 config 发现跨 workspace 边界命中——CLAUDE.md 既有 Gotcha 的 package
  版）；四叶子 `.crate` 已在 `~/.cargo-target/package/` 落档。
- **B5**：`cargo publish --dry-run -p berry-sys`（及 berry-tls）全链 PASS
  （index 更新 → Packaging → Verifying → Uploading → dry-run abort）；
  dry-run 不需要登录态。
- **B3/B4**：berry-libc / berry-fsclient 现状（path-only）`cargo package`
  exit 101——「all dependencies must have a version requirement specified
  when packaging」，manifest 校验层，**与 target 无关**。
- **B6**：沙箱双写（path+version）后 `cargo package -p crate-b` 仍 FAIL——
  tarball 生成即剥 path，**打包期就要依赖在 registry 可解析**；
  `--no-verify` 绕不过（失败在 Packaging 段非 Verifying 段）。⇒ **中/顶层
  的本地全链被发布顺序锁死**：叶子真发布到 crates.io 之前不可本地 package
  通过——「六连 publish 依赖叶子序」连 CI 守卫设计都必须吸收。

本草案补充探针（E 段，2026-10-30，`/tmp/sdk1b-dualwrite/` 沙箱，本合成员执行）：

- **E1**：双写形依赖（上游未发布）下 `cargo package --list -p crate-b`
  **exit 0**——`--list` 不做 registry 解析 ⇒ CI 对中/顶层可用 `--list`。
- **E2**：把双写**退回** path-only 再 `--list`——**照样 exit 0** ⇒ `--list`
  连「依赖必须带 version」的 lint 都不执法 ⇒ 双写形必须靠 grep 硬门。
- **E3**：叶子缺 description/license 全量 `cargo package --no-verify`——
  **warning 不拦，exit 0** ⇒ 元数据齐备性在 package 层无硬门（硬错在
  publish 上传时），grep 硬门是唯一执法点。
- **E4**：六个 keywords / 26 字符 keyword / 伪造 category 三态过
  `--list` 全静默 ⇒ keyword 数量·长度·category 合法性是 publish 服务端
  执法面，本地不查——**设计侧自守**（每件 ≤5 个、全 ASCII 小写短词，本
  设计最长 `thread-local-storage` = 20 字符；category 只用 §1.3 实测在册
  slug）。

### 1.3 分类合法值实测（2026-10-30 拉取 `crates.io/api/v1/category_slugs`）

本设计用到的 slug 全部在册：`api-bindings`、`no-std`、`os`、
`filesystem`、`network-programming`、`memory-management`（另核实
`os::linux-apis` 等子档存在但不用）。六名占用复核：带 User-Agent 头逐名
`GET /api/v1/crates/<name>` 全 **404**（2026-10-30 本草案合成员复核，与
基线查证一致；无 UA 头会得 403 反爬——预检命令须带 UA，§6 已写入）。

---

## 2. 六 crate 元数据定稿（D1）

### 2.1 逐 crate 定稿表

description 一句话（对外第三方视角，英文——crates.io 受众）；「动作」列
标明保持现值/改写/新增（改写理由 = 现值外泄仓内里程碑代号 P6 A-8/P6 A-1/
C-1 knife 1，第三方不可读）：

| crate | description 定稿 | 动作 | keywords（每件 ≤5 个短词） | categories（实测合法 slug） |
|---|---|---|---|---|
| berry-sys | `BerryOS raw syscall bindings (v1 FFI mirror)` | 保持现值 | `berryos, syscall, aarch64, no-std, os` | `api-bindings, no-std, os` |
| berry-tls | `BerryOS copy-style TLS placement pure functions (tls_place/find_ptls/template globals)` | 保持现值 | `berryos, tls, thread-local-storage, elf, no-std` | `memory-management, no-std` |
| berry-netdproto | `BerryOS network-service RPC wire protocol: pure no_std encoder/parser (DNS + TCP session ops)` | **改写**（原值尾缀 "for the P6 A-8 vnet DNS service"） | `berryos, rpc, dns, protocol, no-std` | `network-programming, no-std` |
| berry-fsproto | `BerryOS filesystem-service RPC wire protocol: pure no_std encoder/parser (10 ops, host-testable)` | **改写**（原值尾缀 "for the P6 A-1 RAM-fs service"） | `berryos, filesystem, rpc, protocol, no-std` | `filesystem, no-std` |
| berry-fsclient | `BerryOS filesystem client: RPC session core + pure fd-table/cwd/stat/dirent faces for the libc file layer` | **改写**（原值尾缀 "(C-1 knife 1)"） | `berryos, filesystem, rpc, client, no-std` | `api-bindings, filesystem` |
| berry-libc | `BerryOS libc shim: full-definition libc surface for Rust std (musl target) on the BerryOS microkernel (aarch64)` | **新增**（六件唯一缺 description） | `berryos, libc, posix, musl, aarch64` | `api-bindings, os` |

### 2.2 repository 占位策略

六件统一 `repository = "https://github.com/berryos/berryos"`——**占位地址**
（本仓公开镜像真地址未定）。发布前用户动作：六份 Cargo.toml repository 字段
+ 六份 README.md 内仓库链接同批替换（§6 手顺已列）。CI grep 只断言
`^repository = ` 在场（真地址人工把关——机器断不了真伪）。

### 2.3 readme 策略：每 crate 最简 README.md（非共享说明）

裁决：**每 crate 一份 ~15–25 行 README.md** + `readme = "README.md"` 显式
字段。理由：crates.io 包页直接渲染包内 README；跨包共享说明无法进 tarball
（`include` 面另开复杂度，不值）。六份同骨架、一段差异：

```markdown
# berry-sys

Raw syscall bindings for the [BerryOS](https://github.com/berryos/berryos)
microkernel (experimental, aarch64-first research OS): `#[repr(C)]` ABI
types, generated constants, and `svc #0` wrappers. No allocation, never
panics.

**Status: experimental (0.1.0).** The API is unstable and tracks the
BerryOS v1 syscall ABI. Builds for aarch64 targets only
(`aarch64-unknown-none` / `aarch64-unknown-linux-musl`); useful only on
BerryOS or its QEMU image.

BerryOS userland SDK stack (this crate in bold):

    berry-sys             <- raw syscall ABI (this crate)
    berry-tls             <- TLS placement pure functions
    berry-netdproto       <- network-service RPC wire protocol
    berry-fsproto         <- filesystem-service RPC wire protocol
    berry-fsclient        <- fs RPC session + fd-table faces
    berry-libc            <- libc shim for Rust std (musl target)

License: Apache-2.0. Design docs live in the repository (`docs/`,
mostly zh-CN).
```

per-crate 差异点（第 1–2 段与 stack 图高亮行；其余骨架逐字同）：

- **berry-tls**：一句 = "Copy-style TLS (thread-local storage) placement
  math: ELF `PT_TLS` walk, placement arithmetic, and template globals —
  pure functions, host-testable."；honesty 行 = "no_std core; builds and
  unit-tests on any host (no arch-specific code)"。
- **berry-netdproto**：一句 = "Wire protocol for the BerryOS network
  service RPC: strict request/reply byte layouts for DNS and TCP
  round-trip ops — pure no_std encoder/parser, host-testable."；
  honesty 行注明 "protocol-only: channel/wait semantics live at each end"。
- **berry-fsproto**：一句 = "Wire protocol for the BerryOS
  filesystem-service RPC (10 ops incl. v2 NUL-segment paths and STATFS)
  — pure no_std encoder/parser, host-testable."；同上 protocol-only 行。
- **berry-fsclient**：一句 = "Client side of the BerryOS
  filesystem-service RPC: session core (strict-serial, poisoning,
  EBUSY-retry) plus pure fd-table/cwd/stat/dirent faces used by the
  berry-libc file layer."；honesty 行 = "the syscall-touching session
  module is cfg(not(test)) — host tests cover the pure faces"。
- **berry-libc**：一句 = "Full-definition libc shim letting Rust std
  build for `aarch64-unknown-linux-musl` on BerryOS (malloc/write/
  pthread/futex/crt0 real, the rest stubbed loudly)."；honesty 行 =
  "aarch64 only; pairs with the in-repo `berry-ld` linker wrapper —
  see the repository's app guide"。

### 2.4 version

六件现值全 `0.1.0`——**无抬齐动作**（刀形「若现版本不齐则列出抬齐动作」
的答案：已齐）。统一性由 CI 硬门 grep `^version = "0.1.0"` 执法（§5）。

### 2.5 Cargo.toml 定稿块（六件 [package] 段终态）

现存注释行全部原位保留（历史/义务注记是仓内纪律，不因 publish 删）；新增
键插在 `license` 行之后（libc 则按序补齐）。终态（新键以 `+` 标）：

```toml
# berry-sys / berry-tls / berry-netdproto / berry-fsproto / berry-fsclient 五件同形：
[package]
name = "<crate>"
version = "0.1.0"
edition = "2021"
# <现存注释块原样>
description = "<§2.1 定稿>"          # 五件为现值（netdproto/fsproto/fsclient 为改写值）
license = "Apache-2.0"
+repository = "https://github.com/berryos/berryos"
+readme = "README.md"
+keywords = ["<§2.1>"]
+categories = ["<§2.1>"]

# berry-libc（六键补齐件）：
[package]
name = "berry-libc"
version = "0.1.0"
edition = "2021"
# <现存 G-eco-d 注释块原样（:6-9）>
+description = "BerryOS libc shim: full-definition libc surface for Rust std (musl target) on the BerryOS microkernel (aarch64)"
+license = "Apache-2.0"
+repository = "https://github.com/berryos/berryos"
+readme = "README.md"
+keywords = ["berryos", "libc", "posix", "musl", "aarch64"]
+categories = ["api-bindings", "os"]
```

---

## 3. 依赖双写（D2）——六份 Cargo.toml 逐行 before/after

形制：`{ path = "../<dep>", version = "0.1.0" }` 恰一行（**CI grep 按此
逐字节断言**，§5——格式漂移即红）。依赖行上方现存注释行原样不动。

### 3.1 berry-fsclient/Cargo.toml（三条，:19/:20/:24）

```toml
# before                                  # after
berry-sys = { path = "../berry-sys" }         berry-sys = { path = "../berry-sys", version = "0.1.0" }
berry-fsproto = { path = "../berry-fsproto" } berry-fsproto = { path = "../berry-fsproto", version = "0.1.0" }
berry-netdproto = { path = "../berry-netdproto" }   berry-netdproto = { path = "../berry-netdproto", version = "0.1.0" }
```

### 3.2 berry-libc/Cargo.toml（四条，:12/:15/:19/:22）

```toml
# before                                    # after
berry-sys = { path = "../berry-sys" }           berry-sys = { path = "../berry-sys", version = "0.1.0" }
berry-fsclient = { path = "../berry-fsclient" } berry-fsclient = { path = "../berry-fsclient", version = "0.1.0" }
berry-tls = { path = "../berry-tls" }           berry-tls = { path = "../berry-tls", version = "0.1.0" }
berry-netdproto = { path = "../berry-netdproto" }   berry-netdproto = { path = "../berry-netdproto", version = "0.1.0" }
```

（四叶子 `[dependencies]` 段为空，零改动。）

### 3.3 Cargo.lock 与 workspace 预期

- `user/Cargo.lock` 预期**零漂移**（path 依赖在 lock 中本就带 version 记录，
  加 req 不改锁定值；hash 前缀 `2f5fdfed` 不变——§7 验收执法）。若漂移：
  逐行 diff 人工审，不得静默接受。
- workspace 解析行为：`cargo metadata` 下 req 变 `^0.1.0` 且 path 保留，
  **仓内构建仍走 path**（基线 B6 沙箱 `cargo build --offline` 通过 = 零网络
  零 registry 依赖的实证）。六 crate 的 bins/驱动消费者（svcmgr、bsh、vnet
  等）全部经各自 path 依赖——**零改动零漂移**（§0.3 已明示不做 bins 双写）。

---

## 4. crate 级 docs 面（D3）——lib.rs 头部 Overview 块

**纪律**：只在每份 `src/lib.rs` 头部 `//!` 块的标题行之后插入一段英文
Overview（第三方视角：这是什么/什么状态/怎么和栈里其它件相处/在哪构建），
**既有中文正文一字不动**（它们是高质量合约注释，不需要重写）。`//!` 无
codegen——产物 hash 验收（§7）不受扰。

样板（berry-sys 全文，`REPO_URL` = §2.2 占位；插在
`//! # berry-sys —— …` 标题行与「合约源」段之间）：

```rust
//! ## Overview (for crates.io users)
//!
//! Raw syscall bindings for the [BerryOS](REPO_URL) microkernel — an
//! experimental, aarch64-first research OS. This crate mirrors the frozen
//! v1 syscall ABI: `#[repr(C)]` base types, generated constants (single
//! source: `tools/abi.toml` in the repo), and thin `svc #0` wrappers.
//! The raw layer never panics and never allocates; higher-level runtime
//! lives in berry-libc.
//!
//! Builds for **aarch64 targets only**; useful only on BerryOS or its
//! QEMU image. Status: experimental (0.1.0) — the API moves with the
//! kernel ABI. Repo docs are mostly zh-CN.
```

其余五件补强点（每件 2 句「是什么/边界」+ 1 句状态，同插入位）：

- **berry-tls**：copy-style TLS placement math（PT_TLS 走查/放置算术/模板
  四全局），主/子线程共用同一算术；纯函数零 arch 面、host 可测；消费方
  berry-libc（crt0 + pthread）。
- **berry-netdproto**：网络服务 RPC wire 协议（DNS + TCP 三 op + 会话化
  尾随字段）；只定字节布局与编解码，通道/等待语义在两端；严格恰长执法、
  编不出就拒绝。
- **berry-fsproto**：文件系统服务 RPC wire 协议（10 op 含 NUL 分段路径、
  STATFS）；同 protocol-only 边界；「解析器即产品」——host 单测全盖错误面。
- **berry-fsclient**：fs 服务客户端两层——纯函数面（fd 表/cwd/stat/dirent）
  + 会话机（cfg(not(test))，host 测试不链 syscall 面）；为 berry-libc 文件
  层供料；v1 诚实缺口（无 `.`/`..` 归一化、写打开截断语义）已在下方正文
  成文，Overview 一句带过链至正文。
- **berry-libc**：musl target 下 Rust std 的 libc shim——malloc/write 族
  真实现 + 桩面 loud（ENOSYS/ENOENT 按调用方白名单给码）；aarch64 only；
  配 berry-ld 链接器包装使用（repo app guide）。

---

## 5. CI publish 守卫步（D4）——`.github/workflows/ci.yml` 原文

**定位**：「berry-ksproto host 测试」步（ci.yml:76-77）之后、「构建（内核 +
用户程序产物链）」步（:83）之前——fail-fast 家族同位（零 QEMU 依赖、零
justfile 依赖）。冷缓存预计 +1–3 min（四叶子 verification build 各数秒级 +
index 更新），45 min 帽无虞。

**设计依据**（§1.2 实测逐条对号）：grep 硬门 ← E2/E3（package/--list 都不
执法元数据与 version lint）；四叶子全量 package ← B2（verification 对 none
target 实证 PASS）；中/顶层只 `--list` ← B6（registry 解析前置锁死）+ E1
（--list registry-free 实证 exit 0）。步骤原文：

```yaml
      # ---------- SDK-1b：发布族六 crate publish 守卫（fail-fast 位） ----------
      # 三档执法，档位由 2026-10-30 双轮实测定死（基线 B2/B3/B4/B6 + 沙箱
      # E1–E3，docs/SDK-1b实施设计草案-v1.0.md §1.2/§5）：
      # ① grep 硬门：cargo package/--list 对「内部依赖必须带 version」（path-only
      #    照样过 --list）与「description/license 等六键齐备」（缺键只 warning
      #    不 fail）都不执法——grep 是唯一硬门，防 path-only 回归静默过 CI、
      #    爆在用户 cargo publish 当场；
      # ② 四叶子（零内部依赖）完整 cargo package——verification build 对
      #    aarch64-unknown-none（根 config target 锁跨 workspace 边界命中）=
      #    真实发布面可打包（基线 B2 实证）；
      # ③ 中/顶层（fsclient/libc）双写依赖在叶子真发布到 crates.io 前结构性
      #    不可本地全链（tarball 生成即剥 path → registry 解析前置，--no-verify
      #    也不绕，B6 实证）——只跑 --list（registry-free，E1 实证）保文件清单
      #    干净面。叶子发布后本档升级为完整 package（另刀回填，勿现在改）。
      - name: sdk-publish-guard（SDK-1b 六 crate 发布面守卫）
        working-directory: user
        run: |
          set -eu
          # —— 硬门 1：六 crate 元数据齐备（version/description/license/
          #    repository/readme 字段 + README 本体在位）。
          for c in berry-sys berry-tls berry-netdproto berry-fsproto berry-fsclient berry-libc; do
            m="$c/Cargo.toml"
            grep -q '^version = "0.1.0"' "$m"      || { echo "::error file=$m::version 应为 0.1.0"; exit 1; }
            grep -q '^description = ' "$m"         || { echo "::error file=$m::缺 description"; exit 1; }
            grep -q '^license = "Apache-2.0"' "$m" || { echo "::error file=$m::license 应为 Apache-2.0"; exit 1; }
            grep -q '^repository = ' "$m"          || { echo "::error file=$m::缺 repository"; exit 1; }
            grep -q '^readme = "README.md"' "$m"   || { echo "::error file=$m::缺 readme 字段"; exit 1; }
            test -f "$c/README.md"                 || { echo "::error file=$m::README.md 缺席"; exit 1; }
          done
          # —— 硬门 2：内部依赖双写形恰一（path + version 同行；依赖行上方
          #    注释不拦，行本体逐字节钉死）。
          dw() {  # dw <manifest> <dep>
            grep -q "^$2 = { path = \"../$2\", version = \"0.1.0\" }$" "$1" \
              || { echo "::error file=$1::依赖 $2 应为 { path = \"../$2\", version = \"0.1.0\" } 双写形"; exit 1; }
          }
          dw berry-fsclient/Cargo.toml berry-sys
          dw berry-fsclient/Cargo.toml berry-fsproto
          dw berry-fsclient/Cargo.toml berry-netdproto
          dw berry-libc/Cargo.toml berry-sys
          dw berry-libc/Cargo.toml berry-fsclient
          dw berry-libc/Cargo.toml berry-tls
          dw berry-libc/Cargo.toml berry-netdproto
          # —— 档 ②：四叶子完整 package（含 verification build；rustflags
          #    -fp-armv8/-neon 弃用告警为已知噪声（基线 B2），非失败）。
          for c in berry-sys berry-tls berry-netdproto berry-fsproto; do
            echo "::group::cargo package -p $c"
            cargo package -p "$c"
            echo "::endgroup::"
          done
          # —— 档 ③：中/顶层 --list（registry-free 文件清单干净面）。
          cargo package --list -p berry-fsclient
          cargo package --list -p berry-libc
```

说明：CI checkout 干净——不带 `--allow-dirty`（顺带守住 VCS 清单面）；
`working-directory: user` 下根 `.cargo/config.toml` target 锁照常命中（与
本地 B2 同一形态）；`aarch64-unknown-none` target 已在 ci.yml toolchain 步
安装（:20）。**勿动 justfile**（刀形红线）。

---

## 6. 发布手顺（D5）——`docs/应用开发指南.md` 两处改写 + 新 §9

### 6.1 文首「v1 现实约束」双态改写（before/after 全文）

before（指南 :10-12）：

> 【v1 现实约束（诚实声明）】`berry-libc`（libc shim）与打包工具链
> （`tools/berry-ld` 链接器包装、`tools/mkapp.py`）目前都在 BerryOS 仓库
> 内——v1 开发模型 = 手边需要一份仓库 checkout（path 依赖 + 仓内工具）；
> crates.io 发布面另刀（ADR #57）。

after：

> 【v1 现实约束（诚实声明·双态）】SDK 六 crate（`berry-libc` 及其依赖族：
> berry-sys/berry-tls/berry-netdproto/berry-fsproto/berry-fsclient）自
> SDK-1b 起 publish-ready：**未发布态**（当前）= path 依赖，需一份仓库
> checkout（§3.2）；**已发布态**（维护者按 §9 六连 publish 后）= 仓外工程
> 可 `berry-libc = "0.1.0"` 版本依赖。但打包工具链（`tools/berry-ld`、
> `tools/mkapp.py`）仍在仓库内——**v1 完整构建链两态都需要一份 checkout**，
> 版本依赖免除的只是 crate 源码路径这一项。

### 6.2 新增 §9 全文（追加在 §8 之后，可逐字落盘）

```markdown
## 9. 发布与获取（SDK-1b：crates.io 面）

SDK 六 crate 与依赖序（叶子在前）：`berry-sys` / `berry-tls` /
`berry-netdproto` / `berry-fsproto`（四叶子，互不依赖）→ `berry-fsclient`
（→ sys+fsproto+netdproto）→ `berry-libc`（→ 四件全量）。仓内开发走
path 依赖（§3.2）；六件发布到 crates.io 后，仓外工程的依赖形态：

    [dependencies]
    berry-libc = "0.1.0"        # 取代 §3.2 的 path 形（其余五件同理）

（v1 边界见文首双态声明：构建链工具仍在仓内。）

### 9.1 维护者发布手顺（六连 publish——需 crates.io 账号）

    # 一次性：
    #   crates.io 账号（GitHub 登录）→ Account Settings → API Tokens
    #   创建 token（publish-new-update scope），然后：
    cargo login                # 粘贴 token（存 ~/.cargo/credentials.json）

    # 名占预检（六行应全 404；非 404 = 名被占，停下议事——2026-10-30 全
    # 空闲复核过；注意必须带 User-Agent，裸 curl 得 403 反爬不是 404）：
    for c in berry-sys berry-tls berry-netdproto berry-fsproto berry-fsclient berry-libc; do
      curl -s -A "berryos-release-check" -o /dev/null -w "$c %{http_code}\n" \
        "https://crates.io/api/v1/crates/$c"
    done

    # 发布前必做：六份 user/berry-*/Cargo.toml 的 repository 字段与六份
    # README.md 内仓库链接，从占位 https://github.com/berryos/berryos 换成
    # 公开镜像真地址（占位上线 = crates.io 页面死链）。

    # 六连 publish（叶子序；顺序错了当场 "no matching package" 报错——
    # tarball 生成即剥 path、打包期就要 registry 可解析。工作树脏则每条
    # 加 --allow-dirty）：
    cd user/
    cargo publish -p berry-sys
    cargo publish -p berry-tls
    cargo publish -p berry-netdproto
    cargo publish -p berry-fsproto
    cargo publish -p berry-fsclient        # 四叶子在册后才可解析
    cargo publish -p berry-libc            # fsclient 在册后才可解析

注意事项：

- **index 传播延迟**：连发中偶发 `no matching package named berry-sys` =
  crates.io sparse index 尚未收录上一发（秒级到分钟级）——等 30–60s 重试
  **当前这条**；已发成功的那条重试会报「already exists」，看到即跳过继续。
- **不可逆**：版本一旦上传不可删除（只能 yank——yank 只阻止新依赖解析，
  已锁定版本照常可用）。发布 = ADR #57 语义版本纪律生效起点（0.x 阶段
  minor 即 breaking）。
- 六件 version 统一 0.1.0；抬级时六份 Cargo.toml + 内部依赖 version req
  联动同抬——流程归版本纪律另刀，本指南不展开。
```

### 6.3 §3.2 一行括注

`Cargo.toml` 模板 path 依赖注释块（指南 :89-91）尾增一行：
`# （crates.io 发布后的 version 依赖形态见 §9）`。

---

## 7. 验证面（零漂移验收——实施线收尾必过）

前提说明：基线 hash 于 2026-10-30 在**含兄弟会话在途改动的当前工作树**上
取定（基线实录明示「git status 与兄弟会话快照完全一致」）——验收比对有效
的前提 = 兄弟面无新增改动；漂移时按 7.2 归因序排查，不得直接判本刀越界。

### 7.1 host 测试

`just test-host` exit 0，通过数 **609/0** 与基线（/tmp/sdk1b-baseline/
testhost.txt）同口径——本刀零新测试零改测试，计数恒定即执法。

### 7.2 产物零漂移（按 target 面分账——musl 面恒漂是预期，none 面恒定是执法面）

> **本节为修复轮改写（双镜头审查 F8，2026-10-30）**。原文（实施前）承诺
> 「Cargo.toml 元数据与 `//!` 头注均无 codegen——产物字节必须不变」，对
> musl 面**实测为假**：仅施加本刀增量（无任何兄弟线改动）即令 musl 下游
> 产物移位。隔离 worktree 同路径分相实验（逐相位冷启、相位内冷/热重建
> 逐字节互同——确定性无恙），证据 `/tmp/sdk1b-art{A,B,C,D1..D5,F}/hashes.txt`：

- **none 面（内核内嵌五件）恒定**：abuser/svcmgr/echo/talker/thrchild
  `.bin` 与基线逐字节相同——**svcmgr 实际消费 berry-sys +
  berry-netdproto**（path 依赖，消费事实取自 manifest、不做推断），其
  恒定恰是 none 面中性的证据、而非「不消费故不变」（审查 F9 更正）；
- **musl 面（appdemo/appquiet ELF + 双 .bap，rootfs.cpio 含 .bap 随动）
  恒漂**：二分定位——D1 纯 Cargo.toml 元数据==基线、D2 +libc 头注==基线、
  D3 +berry-sys 源（BERRY_ATTEST）==基线、D4 +fsclient/tls/netdproto
  头注≠基线、全量态又≠D4——即**传递依赖 crate 的 crate 级 doc 头注独立
  移动 musl fat-LTO 下游字节**（机制工作假设：`//!` 脱糖 `#![doc]` 属性
  进 crate 指纹/SVH 面、fat-LTO 内联符号携指纹移动；机制未深挖，实测
  事实独立成立）。

执法口径（替代原「三步归因序」，原第 3 步「两步排除后仍漂移=刀越界」
删除——对 musl 面不成立）：

1. **none 面五 hash 漂移** = 越界或兄弟混入信号——零改动复跑 `just
   build-user` 一次再 hash、`git status` 对基线时点快照排查；
2. **musl 面漂移 = 预期内**，但须与 §9 实录表 hash **一致**——不一致 =
   增量超出本刀面（新混入源），升级排查；
3. **gotcha（后续刀基线同样受扰）**：上游 crate 纯 doc 注释/元数据增量
   即可移动 musl fat-LTO 下游字节——零漂移基线必须**按 target 面分账**
   （none 面 / musl 面），「源码没动逻辑」不构成 musl 面零漂移的论据。

附带：`user/Cargo.lock` hash 前缀 `2f5fdfed` 不变（§3.3）。

### 7.3 打包通道

`cd user && cargo package --list --allow-dirty -p <六件逐一>`——清单干净：
每件 = `Cargo.lock` / `Cargo.toml` / `Cargo.toml.orig` / `README.md`（新）/
`src/**`（berry-sys 含生成件 abi_gen.rs；本地脏树无 `.cargo_vcs_info.json`
行属正常，干净树有）。出现 tests 残渣/编辑器垃圾/多余文件 = FAIL。

### 7.4 叶子全量 package

四叶子 `cargo package --allow-dirty -p <各>` 全 PASS（= 基线 B2 形态，含
verification build）。**berry-fsclient/berry-libc 全量 package 预期 FAIL**
（exit 101，Packaging 段 "no matching package" ——B6 实测的 registry 解析
前置）——这不是本刀失败，是该约束被 CI 档 ③（§5）正确吸收的证据；实施线
在执行结果里如实记录该 FAIL 形态即止。

### 7.5 红线自检

`git diff --stat` 核对：改动面 ⊆ §0.2 白名单（六 Cargo.toml + 六 README +
六 lib.rs 头注 + ci.yml + 应用开发指南）；§0.2 禁写清单零触碰；无
commit/add/publish。

---

## 8. 风险与挂账

**风险（发布时点面，手顺已吸收）**：

- **名占风险**：六名 2026-10-30 复核全 404——发布宜早不宜迟（占名者先到
  先得，被占 = 改名连锁 = 全部消费者文档作废）。
- **占位 repository 上线 = 死链**：发布前用户必做清单 §6.2 已列（六
  Cargo.toml + 六 README 同批替换）。
- **index 传播延迟**：连发重试手顺 §6.2（already exists = 已完成，跳过）。
- **不可逆**：yank 不删版本；首发 0.1.0 = 公开永久占名。

**挂账（后续刀/主会话）**：

- **docs.rs 面（修复轮已解——双镜头审查 F2，2026-10-30）**：原判「sys/libc
  含 aarch64 专用 asm → docs.rs 构建必失败、配 `[package.metadata.docs.rs]`
  也不救 sys/libc 两件」**不成立**——docs.rs 官方 targets 覆盖元数据生效，
  语义（docs.rs/about/metadata 实测核验）：`default-target` 未设时
  `targets` **首元素即默认构建目标**、其余附加构建、host x86_64 不再构建；
  目标可用面 = rustup 可装（两 target 均在册）。修复轮已给
  berry-sys/berry-fsclient/berry-libc 三件（自带或经传递带 asm）配
  `targets = ["aarch64-unknown-none", "aarch64-unknown-linux-musl"]`；
  四纯函数件零配置、全 tier-one 目标可建。**真发布后复核项**：
  docs.rs/berry-sys 首建绿即闭环（残留风险：docs.rs 环境差异，元数据面
  无法本地全预演）。
- **deny.toml 收紧**：berry-libc 补 license 后 cargo-deny 的
  `-A no-license-field` 豁免面缩小可收紧——deny.toml 非本刀白名单，留主
  会话。
- **CI 档 ③ 升级**：六件真发布后，fsclient/libc 的 CI 步从 `--list` 升为
  完整 `cargo package`（§5 注释已留位）。
- **版本抬级联动流程**：归 ADR #57 语义版本纪律另刀。
- **`cargo publish --dry-run` 全链预演**：六件元数据落定后可在实施线跑
  `cargo publish --dry-run -p berry-sys`（B5 实证免登录全链）作发布彩排
  ——非验收必需，实施线裁量。

---

## 9. 执行结果（实施回填 2026-10-30 + 修复轮（双镜头审查 12 findings）回填同日）

- [x] D1 六 crate 元数据 + README 落盘（commit 归主会话统一提交）。**提交
  纪律（审查 F4）：六 README.md 为新文件，必须与六 Cargo.toml 同 commit
  入库**——干净 clone / `git clean -fd` 后缺 README 的树上 `cargo package`
  在 readme 解析步即炸；指南 §9.1 已写明「从已提交的工作树发布」前提。
- [x] D2 双写七条（fsclient×3 + libc×4，第三边 netdproto 事实源 =
  berry-fsclient/Cargo.toml 依赖段）。CI 执法面两层：硬门 2 dw() 七条正向
  钉死 + **修复轮新增硬门 2b**（path-only 形零容忍 grep + path 行计数钉死
  fsclient=3/libc=4/四叶子=0——审查 F1；计数漂移即红，防改形绕过）。
- [x] D3 六 lib.rs Overview 块。
- [x] D4 CI 步挂线（首跑绿归主会话 push 后 CI；修复轮已本地复验守卫步
  grep 三门全过，见验收记录）。
- [x] D5 指南文首双态 + §9 + §3.2 括注。修复轮补 §9.1 三前提：发布机
  `rustup target add aarch64-unknown-none`（verification build 经根 config
  锁定该 target——审查 F5）、已提交态前提（F4）、占位 repository 换址后
  `grep -rn 'github.com/berryos/berryos' …` 零命中自检（F3）。

验收 7.1–7.5（按修复轮口径；7.2 已改写——musl 面恒漂为预期）：

产物 hash 实录（隔离 worktree 同路径分相；HEAD 基线 artA vs HEAD+本刀增量
artB；短 hash = sha256 前 8 位，全文见 `/tmp/sdk1b-art{A,B}/hashes.txt`）：

| 产物 | HEAD 基线 | +本刀增量 | 面 / 判定 |
|---|---|---|---|
| kernel/src/userbins/abuser.bin | 637d1d35 | 637d1d35 | none 恒定 |
| kernel/src/userbins/svcmgr.bin | 1b0d96d2 | 1b0d96d2 | none 恒定（消费 sys+netdproto 而字节中性——F9） |
| user/userbins/echo.bin | 165ae5ec | 165ae5ec | none 恒定 |
| user/userbins/talker.bin | c529de0c | c529de0c | none 恒定 |
| user/userbins/thrchild.bin | 1e5dcafc | 1e5dcafc | none 恒定 |
| …musl/release/appdemo | a5e79b09 | 826ca245 | musl 漂移（预期，§7.2） |
| …musl/release/appquiet | 27cdb08b | e4f68677 | musl 漂移（预期，§7.2） |
| APPDEMO.bap | 7d6a577c | cdb89129 | musl 漂移（预期，§7.2） |
| APPQUIET.bap | c3be9d5f | 0a2e2bea | musl 漂移（预期，§7.2） |

（rootfs.cpio 含双 .bap 随动；二分相位 D1–D5/F 详见 §7.2 与审查证据。）
实施线首验收曾按原 §7.2 判 musl 面 FAIL——修复轮定谳：非兄弟混入、非刀
越界，是本刀 D3 头注自身的 musl 面贡献（原「两步排除后仍漂移=刀越界」
归因逻辑已删）。§7.1 host 测试、§7.3 --list、§7.4 叶子 package、§7.5
红线自检由实施线过（testhost 计数与基线同口径；fsclient/libc 全量 package
按预期 FAIL 于 registry 解析前置 = 档 ③ 约束被正确吸收的证据）。

修复轮增量（12 findings：修 9 / 驳 0 / note 记录 3——逐条处置表随验收
报告归 workflow 记录）：

- F1 → ci.yml 硬门 2b（上述）；F2 → 三件 manifest + docs.rs targets +
  §8 挂账解除 + §0.3 撤半项；F3/F4/F5 → 指南 §9.1 三前提；F8 → §7.2
  改写 + 本节 hash 回填 + gotcha（**CLAUDE.md Gotchas 收录归主会话**——
  本轮多会话纪律禁碰）；F9 → 本节 svcmgr 表述更正；F10 → 指南四处
  justfile 行号引用改锚文本；F11 → 索引 :51 摘要补 §9 + 指南【状态】行
  抬 SDK-1b；F6（keywords 界限）/F7（完备性清面）/F12（mkcpio per-member
  hash 边车 + 仓根陈旧分裂卷 rootfs40/400/401.cpio 清理与自清覆盖面）=
  note 记录不修，F12 归 mkcpio 属主/主会话。
