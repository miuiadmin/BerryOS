# Agent Note: berry console 串口直连——宿主终端进 guest bsh

Status: implemented

## Problem

CLI-first 定位下 berry 的主接口是命令行，但命令行体验有一处断层：guest 内 bsh 的唯一交互面是串口，而宿主侧串口是单向的（`-serial file:serial.log`）——用户在 Mac 终端里没有任何入口「进到」客机 shell；要看客机输出只能 `berry logs` 读文件，要交互则无路（VNC 桌面在 GUI 形态、且客机 GUI 无终端应用——键盘链三墙挡着）。命令行体验刀把它挂为件 4「宿主 berry console 串口直连」，等 tools/berry 脏面落库；本轮（2026-11-16）缝解锁后兑现。

## Decision

三层落地（2026-11-16，workflow 补完 + 实跑验证全绿）：

1. **传输层 = chardev socket + logfile**（qemu.rs machine_args 单点改造，run/shot/device 三路接线）：`-chardev socket,id=ser0,path=<run>/serial.sock,server=on,wait=off,logfile=<serial.log>` + `-serial chardev:ser0`。关键性质（侦察实证 QEMU 11.0.3）：serial.log 仍由 QEMU 写且零客户端期 boot 早期字节完整落盘——logs/boot tail/GUI tail 五个读方**零改动**；`device restart_in_place` 前清残留 socket 文件（vnc.sock/qmp.sock 先例）；logfile 缺省 logappend=off 与既有截断语义天然一致。
2. **命令层 = 顶层 `berry console [--run]`**（commands.rs + main.rs 挂线，logs/screenshot attach 面先例）：serial.sock 缺席报「旧 -serial file: 形态」+ restart hint（rc=3）；run 目录 console.lock flock 独占（FFI，第二会话 rc=6 带持有方提示——把 QEMU 单活客户端的「静默僵尸连接」升格为显式拒绝）；连接 0.5s×8 重试（chardev backlog=1 拒连窗，实证 5 连 4 即成）；TTY raw（tcgetattr/tcsetattr FFI 零新依赖，windows_console.rs 房内先例；清 ICANON|ECHO|**ISIG**——ISIG 不清则 0x03 被 tty 截成 SIGINT 杀 berry 进程，与「Ctrl-C 转发客机」语义直接矛盾）；双向泵 UnixStream try_clone 双线程；Ctrl-D(0x04)/EOF 断开恢复终端，run 继续跑。
3. **验证**：142 测全绿（+4 新：锁互斥/旧形态 hint/重试参数/活 listener）；实跑端到端——起 run → 持开管道 `(sleep 5; printf 'rx\n'; sleep 6; printf '\x04') | berry console` → stdout 中继矩阵洪流并出现 `RX ok n=3 lost=0 fdrop=0`（传输环双向闭环，当前镜像 ROOTFS absent 断 bsh 故用内核 rx token 验）→ Ctrl-D → disconnected 行 + `device status` 仍 running + serial.log 持续增长；logs 零变化；`berry run --marker`/`berry shot` 回归绿（chardev 路 Tail 消费不变）。

## Alternatives considered

- **host 代理 tee**（host 进程连 socket 双写 serial.log + 转发）：落选——host 未连前的早期字节丢失、logs 面写者换人改动大；chardev logfile= 让 QEMU 自己落盘，host 只做纯转发客户端。
- **crossterm/termios crate 做 raw 终端**：落选——berry 依赖树干净（无 termios/crossterm），libc 仅传递依赖；`#[link] extern` FFI 是 windows_console.rs 房内既有先例，零新依赖。
- **EOF linger 语义**（管道 EOF 后等应答静默窗再拆，让 `printf 'cmd\n' | berry console` 一步式可用）：暂缓挂 v2——要动读泵超时面（SO_RCVTIMEO FFI + 线程收尾交互），风险与 UX 收益不成比例；管道形态文档化持开写法（README 在册），传输环已双向实证非功能缺口。

## Consequences

- 命令行体验闭环补全：宿主终端 `berry console` 即进 bsh（prompt/行编辑/Tab/历史/Ctrl-C 杀链全在客机侧 editor——宿主只是哑管道）；`berry run`/`shot` 路径同获 chardev 面（marker/断言零回归）。
- 挂账移交：Windows chardev TCP 化（serial.port 文件 + host=127.0.0.1——非 Unix 平台现诚实拒「not supported yet」）；EOF linger v2；bsh 交互面实机验收待 rootfs 在场的镜像态（当前 sched lane 镜像 ROOTFS absent）；背压文档化（bsh 忙窗输入 128B fring + 通道 8 深，prompt 未出前超发丢新计 fdrop）。
- 多客户端事实登记：QEMU 单活 + 第二连接僵尸 + FIFO 晋升——berry 侧以 console.lock 独占语义覆盖；非合作客户端（socat 直连）的排队行为是 QEMU 原生。
- 旧 run（file: 形态起）无 serial.sock——报错带 restart hint 而非静默失败。

## Related

- [CLI-first 设计 v1.9](../../../docs/Berry设备平台-CLI-first设计-v1.9.md)（§8 命令树/§8.2 语义/§14 刀序）
- [命令行体验刀实施设计](../../plans/2026-11-15-命令行体验刀实施设计.md)（契约 4 件 4 兑现）
- [shell 设计 v1.0](../../../docs/shell设计-v1.0.md) §2.5（管道驱动契约的宿主消费面）
