# tools/testkeys/ —— dev 签发密钥纪律（SDK-2 分发签名链 P3 前集成刀）

> 定位：现役签名链「在签」（`just sign-user` 九 app dev 签发一键化）的
> **密钥纪律件**。本文件入仓；密钥本体**永不入仓**（`.gitignore`
> `/tools/testkeys/dev/`）。
>
> 母法：`docs/SDK-2-分发与签名链设计-v1.2.md` §三（berry-sign CLI）；
> ADR #77（分发与签名链立法）；`user/berry-crypto/src/main.rs` 头注
> 「dev/test 面纪律」；24 号 `research/厂商定制化深研/24-SDK2签名链拆刀
> 侦察-首刀设计.md` §4。

## 目录内容

| 路径 | 入仓 | 说明 |
|---|---|---|
| `README.md` | ✅ | 本纪律件 |
| `dev/dev.sk` | ❌ gitignored | 32B 原始 ed25519 私钥（**权限 0600**——`berry-sign keygen` `write_secret` 落盘执法） |
| `dev/dev.pk` | ❌ gitignored | 32B 公钥（= `ed25519::public_key(sk)`；`gensourceindex --source-pk` 消费） |

## 生成命令

一键（推荐，随签发自足——钥缺则产、在则复用）：

```bash
just sign-user
```

手动等价（`berry-sign` 发现序 = `BERRY_SIGN` → `target-cargo` →
`/tmp/target-p{1,2}`，全缺则 `just sign-user` 现场构建）：

```bash
berry-sign keygen tools/testkeys/dev/dev.sk tools/testkeys/dev/dev.pk
# 熵源 = /dev/urandom；sk 0600；stdout 回显 pk hex
```

轮换/作废（dev 面）：`rm -rf tools/testkeys/dev && just sign-user` 即重产
——dev 面无账本仪式；**真签面**轮换走信封 BSIG-ROT 链 + 信任账本
（ADR #77，P3 设备侧落盘）。

## 保管纪律

1. **不入仓**：`.gitignore` 已挂 `/tools/testkeys/dev/`；勿 force-add、
   勿把钥复制到仓内任何路径（含 /tmp 之外的构建产物目录）。
2. **不外传**：dev.sk 只住本机；CI/他人环境各自 keygen 重产（索引
   `source_pk` 随钥变——两件套对拍天然暴露串钥）。
3. **权限面**：sk 恒 0600（keygen 执法；`berry-sign selftest` 钉板）。
   手滑 chmod 过宽 = 删了重产，不修补。

## 永不真签注记（dev/test 面纪律）

- 本目录钥**只用于开发/自检签发**：`just sign-user`（九 app `.bap.sig` +
   官方源索引两件套）、`berry-sign selftest`、host 冒烟验签。
- **永不用于真签**：真签面 = 平台/OEM 正式签发，随 ADR #77/P3（设备侧
   验签 + 账本落盘）另立仪式与钥管理（24 号 §5-Q3 归主会话）。
- dev 签发件（`.bap.sig`、`source-index.tlv`、`source-keys.txt`）是
  **构建树产物**（`user/userbins/` gitignored），不随仓分发、不进任何
  发布镜像基线——`mkimage --source-instance` 挂接属验证/预演面。

## 与现役树的配对事实

- 九 app manifest `[security]` 三键 = `profile="ed25519-main"` /
  `key_lineage="berryos-dev"` / `integrator="berryos"`——`key_lineage`
  是 `.sig` 信封回声源（`berry-sign sign` 拒收无 `[security]` 的包：
  `user/berry-crypto/src/main.rs:188-190`）；`gensourceindex` 对无
  `[security]` 的 `.bap` 同拒（`tools/gensourceindex.py:215-217`）。
- `source-keys.txt` 的 `source_id` 缺省 `berryos`（源身份）与 manifest
  `key_lineage=berryos-dev`（钥血脉）是两个字段，勿混。
