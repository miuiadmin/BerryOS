# bcts hostfwd 入向段预检：QEMU -S 冻结态监听探活（#290 REJECT 修面）

Status: implemented

## Problem

[bcts 步](../../../.github/workflows/ci.yml)（CI 全量回放）在 netns + mount-ns 内跑出 evsclient ×10 conn timed out（run 43616188197）；reviewer 分类器与 owner 第 6 次裁定把红面收敛在 SLIRP hostfwd 入向段（宿主 → 127.0.0.1:18081-83 → guest），但「QEMU/SLIRP 侧死」与「echod/环境侧死」在烧掉 16 分钟全相前无法区分——修复件诉求即本预检腿。

## Decision

`just bcts` 前两腿 5 秒定谳（`tools/bcts-hostfwdprobe.py`，dns preflight 后一行挂 ci.yml bcts 步）：

- 腿 A（echod 环境锚）：ns 内 127.0.0.1:18080 bind + 回显环回 8B——证 lo 起、互斥口可 bind、ns TCP 栈活。红 = 环境面（echod 同样起不来），与 QEMU 无关。
- 腿 B（hostfwd 监听面）：最小 QEMU `-S` 冻结态（无内核无盘，CPU 冻在复位向量）挂 smoke.sh 同款 netdev，宿主侧逐口 connect 18081-83 探活。SLIRP hostfwd 监听 socket 在 guest 未就绪时也收 host 侧 connect（evsclient.py 头注同源定谳）——accept 即监听面活证，与 guest 状态解耦。红 = QEMU/SLIRP hostfwd 侧。

关键实证（本机 QEMU 11.0.3）：`-S` 冻结态 hostfwd 监听对宿主 connect 即收（0.00s ×3，空闲口族 18091-93 复测排除本机 18080-83 占用干扰）。selftest 6 腿零 QEMU 钉板（netdev 单源对拍 smoke.sh 四处全等 / argv / 回显环回 / poll 慢起收敛 / 恒拒面超时 / 定谳行字面）挂 test-host。退出码 10=A 红 20=B 红——预检红即 loud 退，不烧全相。

## Notes

- netdev 单源对拍方向：本件常量与 smoke.sh 四处 launch 行互为镜像，改任一侧端口面由 selftest 腿 ① 咬住。
- 搜前查重：.agents/ 内无 hostfwd 预检先例（rg 实证），本件为首刀；与 [bcts-parkprobe](./2026-11-14-bcts-parkprobe.md) 互补（事后串口分类 ←→ 事前监听探活）。
