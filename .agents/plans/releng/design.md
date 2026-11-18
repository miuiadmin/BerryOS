# releng（发布工程、位对位可复现构建与供应链安全）总体设计

Type: spec
Status: active
Updated: 2026-11-18
Version: 1.0.0

## 1. 模块定位与职责边界

Release Engineering (RelEng) 是 BerryOS 软件工程生命周期的严密品控防线与发版基建。它不直接运行在设备端用户态，但严格定义并管控从源码检出、密封工具链编译、软件物料清单（SBOM）生成、位对位确定性可复现构建（Bit-for-bit Reproducible Builds），到操作系统镜像签名与 OTA 差分载荷封装的全链条契约。它是 BerryOS 满足 SLSA Level 3/4 供应链安全与商业级工业交付的唯一权威源头。

```
+-------------------------------------------------------------------------+
|                  BerryOS 源码库 (Git Repo & 锁定清单)                     |
+-------------------------------------------------------------------------+
                                    |
                                    v (Hermetic Sandbox / SOURCE_DATE_EPOCH)
+-------------------------------------------------------------------------+
|                       密封构建环境 (Hermetic Toolchain)                  |
|   +-------------------+  +-------------------+  +-------------------+   |
|   | 路径抹平与固定时标 |  | 离线依赖锁定缓存  |  | 交叉编译器与链接器 |   |
|   +-------------------+  +-------------------+  +-------------------+   |
+-------------------------------------------------------------------------+
       |                                          |
       v (产物位对位完全一致)                      v (自动化元数据提取)
+-------------------------------+       +-------------------------------+
|    发布镜像包 (Release Images) |       |  供应链物料清单 (CycloneDX)    |
| - berryboot.efi / boot.img    |       | - SHA-256 密码学哈希签名      |
| - system.img (BerryROFS)      |       | - 开源许可证审计 (SPDX)       |
| - recovery.img / OTA payload  |       | - 漏洞 CVE 关联追踪           |
+-------------------------------+       +-------------------------------+
                                |       |
                                v       v
+-------------------------------------------------------------------------+
|                 硬件安全模块 (HSM / Ed25519 商业发版签名)                |
+-------------------------------------------------------------------------+
```

### 职责边界划分
- **归属 RelEng 的职责**：
  1. **位对位可复现构建（Reproducible Builds）**：通过强制注入 `SOURCE_DATE_EPOCH`、文件系统目录遍历排序、编译宏路径映射（`-ffile-prefix-map`）消除一切时间戳、编译机用户名、绝对路径及随机熵残留，确保两次独立构建生成的二进制镜像 SHA-256 严格一致。
  2. **密封构建沙箱（Hermetic Sandbox）**：构建过程全离线隔离网络，依赖版本通过 `Cargo.lock` / `vendor` 严格锁定，杜绝因外部源变动引入幽灵代码。
  3. **软件物料清单（SBOM）与许可证合规**：自动生成标准化 CycloneDX 1.5+ 及 SPDX 2.3 JSON 清单；对核心微内核、运行时与用户态服务执行开源许可证（MIT/Apache-2.0/BSD vs GPL）边界阻断审计。
  4. **全套发布镜像与 OTA 组装**：调用 `storage-image` 与 [bootloader](../bootloader/design.md) 工具，封装包含 dm-verity 散列树的只读 `system.img`、`boot.img` 以及高压缩比的 A/B 增量 OTA 差分包。
  5. **商业级签名门禁**：通过硬件安全模块（HSM）注入生产环境 Ed25519 签名与防回滚版本号（Rollback Index）。
- **严禁越界的范围**：
  1. 严禁构建脚本在编译中途执行未经锁定的网络抓取（如 `curl` 最新脚本或未固定 Commit 的依赖）。
  2. 严禁在生产镜像中打入开发者主机私有密钥或未剥离符号信息的私有文件。
  3. 严禁核心微内核组件（`meihe`, `mm`, `ipc`, `cbox`）污染带传染性协议（如 AGPL-3.0）。

## 2. 核心架构与数据流

### 2.1 确定性可复现构建验证流水线
```mermaid
flowchart TD
    SRC[Git Commit Tag / Source Tree] --> ENV1[构建节点 Alpha\nUbuntu 22.04 / macOS]
    SRC --> ENV2[构建节点 Beta\nArchLinux / Debian]
    
    subgraph AlphaNode [环境 Alpha]
        ENV1 --> H1[注入 SOURCE_DATE_EPOCH=1731931200\n-ffile-prefix-map=/workspace=.]
        H1 --> B1[密封编译 Build 1]
        B1 --> IMG1[产出镜像 system_a.img]
    end

    subgraph BetaNode [环境 Beta]
        ENV2 --> H2[注入 SOURCE_DATE_EPOCH=1731931200\n-ffile-prefix-map=/workspace=.]
        H2 --> B2[密封编译 Build 2]
        B2 --> IMG2[产出镜像 system_b.img]
    end

    IMG1 --> CMP[diffoscope 位对位哈希比对]
    IMG2 --> CMP
    CMP --> RES{SHA256(A) == SHA256(B) ?}
    RES -- 是 --> PASS[通过 G-REL-1 门禁\n放行 HSM 生产签名]
    RES -- 否 --> FAIL[拦截发布\n输出不确定性字节 Diff 报告]
```

### 2.2 镜像打包与分层大小配额管控
RelEng 严格约束各分区交付产物的物理体积边界，防止过度膨胀拖垮启动与 OTA 分发：

| 交付产物 | 格式 / 文件系统 | 大小硬预算 (Hard Budget) | 校验方式 |
| :--- | :--- | :--- | :--- |
| `berryboot.efi` | PE/COFF Stage 2 可执行体 | $\le 2\text{MiB}$ | Verified Boot RoT 校验 |
| `boot.img` | 微内核 + Initrd 静态镜像 | $\le 16\text{MiB}$ | Ed25519 签名头校验 |
| `system.img` | BerryROFS 压缩镜像 + dm-verity | $\le 128\text{MiB}$ | 根哈希 Merkle Tree 校验 |
| `recovery.img` | 极简自愈维护系统 | $\le 32\text{MiB}$ | 独立安全公钥校验 |

## 3. 接口契约与 SBOM 元数据格式

### 3.1 环境变量与编译器参数契约
所有 BerryOS 构建脚本（Makefile / `build.rs` / `cargo`）必须识别并尊重以下环境约束：

```bash
# 1. 规范化时间戳 (统一以 Git 提交秒级时间为准)
export SOURCE_DATE_EPOCH=$(git log -1 --pretty=%ct)

# 2. 规范化编译目录前缀抹平
export RUSTFLAGS="--remap-path-prefix=${PWD}=/berryos"
export CFLAGS="-ffile-prefix-map=${PWD}=/berryos -fno-canonical-system-headers"

# 3. 严格无歧义文件掩码与语言环境
umask 0022
export LC_ALL=C
export TZ=UTC
```

### 3.2 CycloneDX 1.5 SBOM 结构规范 (`sbom.json`)
```json
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.5",
  "serialNumber": "urn:uuid:67f47c2-1f92-466f-b4ad-3af2d1c7bf70",
  "version": 1,
  "metadata": {
    "timestamp": "2026-11-18T00:00:00Z",
    "tools": [{"vendor": "BerryOS", "name": "releng-bom-gen", "version": "1.0.0"}],
    "component": {
      "type": "operating-system",
      "name": "BerryOS",
      "version": "1.1.0-alpha",
      "hashes": [{"alg": "SHA-256", "content": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}]
    }
  },
  "components": [
    {
      "type": "library",
      "name": "meihe-kernel",
      "version": "1.1.0",
      "licenses": [{"license": {"id": "MIT"}}],
      "purl": "pkg:cargo/meihe-kernel@1.1.0"
    }
  ]
}
```

## 4. 故障模型、降级与安全边界

### 4.1 构建非确定性故障排查
1. **时间戳泄漏（Timestamp Contamination）**：
   - 使用 `diffoscope` 工具扫描未匹配字段；排查是否在宏中误用了 `__DATE__`、`__TIME__` 或 Rust 的 `built` 库；使用静态代码检测工具阻断此类用法。
2. **符号表排布伪随机（Linker Layout Jitter）**：
   - 在链接脚本中加入严格的 `SORT_BY_NAME` 指令，并配置链接器参数 `--sort-common --sort-section=alignment`，确保目标文件链接顺序严格固定。
3. **压缩字典波动（Archive Non-determinism）**：
   - 制作 tar/squashfs/verity 镜像时，必须传递 `--mtime="@$SOURCE_DATE_EPOCH"`、`--owner=0 --group=0` 及固定压缩算法字典。

### 4.2 供应链与分发安全防御
- **SLSA Level 3 来源验证**：每次官方发版必须产出加密防伪证明（In-toto Attestation），将二进制哈希与代码仓库 Commit ID 强绑定。
- **发布密钥零接触（HSM Enforcement）**：生产发版私钥托管在云端/物理 HSM 中，发版脚本只能提交镜像哈希触发签名，私钥永不落地到构建服务器磁盘。
- **开源许可证污染拦截**：CI 自动扫描第三方 crate 与 C 库的 `Cargo.toml` / `LICENSE`，严禁感染型许可证代码渗透进微内核与基础库。

## 5. 落地步骤与验收指标

### 5.1 实施推进三阶段
1. **第一阶段（确定性构建工具链标准化）**：
   - 编写 `tools/reproducible_build.sh` 统一构建包装器；
   - 消除镜像打包时文件创建顺序与修改时间引入的不一致。
2. **第二阶段（SBOM 生成与自动化双重构建比对）**：
   - 接入 `cargo-cyclonedx` 自动化生成整机依赖清单；
   - 在 CI 流水线部署「双机异构编译一致性比对」脚本。
3. **第三阶段（自动化 OTA 差分打包与签名发布）**：
   - 部署 `releng-ota-pack` 工具，实现自动从旧版本镜像生成 Block-level 增量差分补丁与元数据清单。

### 5.2 商业化验收门禁
- **G-REL-1（确定性可重现率）**：在两台异构主机（不同目录、不同用户名、不同构建时区）构建出的发布镜像 SHA-256 重现率 $100\%$。
- **G-REL-2（物料清单覆盖率）**：整机 SBOM 对内核与系统组件的依赖节点与开源协议覆盖率 $100\%$。
- **G-REL-3（镜像体积达标率）**：`berryboot.efi` $\le 2\text{MiB}$、`boot.img` $\le 16\text{MiB}$、`system.img` $\le 128\text{MiB}$ 达标率 $100\%$。
- **G-REL-4（许可证合规）**：核心分发包（内核/驱动/系统服务）GPL/AGPL 污染率为 $0\%$。
