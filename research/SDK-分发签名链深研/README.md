# SDK 分发签名链深研证据档

> **溯源**：2026-11-07 用户批「继续深挖」，对 `docs/SDK-2-分发与签名链设计草案-v1.0.md`
> 三决策点起 workflow `wf_d1f4b61e-8a1`（11 agents：7 镜头并行挖掘 + 4 决策点
> 对抗复核；51 课 = 7 镜头 findings，4 裁决均「成立但需修正」）。
> **收拢去向**：四裁决修正 + 用户拍板（不外发/支柱 2 推迟 v2/自建 berry-crypto/
> sideload 系统开关）已全部吸收进 `docs/SDK-2-分发与签名链设计-v1.1.md`
> （设计定稿）——**判词以该档为准，本目录只存证据**。

## 读法（两层）

- `wf-result-full.json → all_findings`：51 课课程层（7 镜头各带 findings，
  每课含 claim/evidence/出处链接）——**背景素材**，未逐课复核。
- `wf-result-full.json → verdicts`：四裁决层（D1 密钥体系 / D2 更新与增量 /
  D3 分发形态 / 范围裁剪），每案含 fixed_recommendation（修正后推荐——
  **引用时以本层为准**）/ risks / open_questions / evidence_checked
  （复核时重取的一手出处 4–7 条）。

## 镜头一览（7）

Android 签名演化（v3 proof-of-rotation/Janus/Play 托管）· Apple 全链
（Team ID/公证/Gatekeeper/吊销语义）· TUF 族（防回滚防冻结/root rotation/
Uptane/pip 与 APT 的十年差距）· 更新与增量（Omaha/A-B/bsdiff/Fedora F40
弃 delta/整包增量 50MB 分界）· 用户与攻击者（核验仪式实证/社工/警告疲劳）·
密钥运维事故（APT 换钥史/Arch 过渡/Homebrew CI 信任根/HPKP）· berry-repo
实施约束盘点（appd 单槽/netd 4 op/零依赖纪律/TCG 落盘时延）。

## 与定稿的映射

| 裁决 | 定稿落点 |
|---|---|
| D1 五修正 | 设计 v1.1 §二/§三（信封/账本/机器裁决/护栏/berry-crypto 门） |
| D2 四补 | §四（水位/回滚边界/delta 闸门三条件/诚实化） |
| D3 五修正 | §五（镜像内置静态源/签名索引/两层分离/账本/双签窗） |
| 范围裁剪 | §〇拍板②/§六分期/§八关门触发器 |
