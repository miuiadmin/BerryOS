# 刀 C 实施落账（r2·本会话直接实施——r1/r2 两轮子代理实施者均未产出，复盘见 §五）

基线：`bff81e891`（主树）。工作树：本 land-c。四件全落、零手拼、ASCII、guiproto 零改动（只读消费）。

## 一、berry-appshell（新 crate，user/berry-appshell）

Dialog/Menu/Toast v1 in-app overlay 形态（C-RECON §五裁决：不 cherry-pick wave3 抽取版——抽取版不含本三件且落地形态重；并主时抽取版骨架吸收本 widget 模块，纯增量）。要点：面板一律 Card kind（elevation 2/corner 24=@3x 8vp）+ 遮罩 Rect（scrim = bg_page 混 40% 黑整数预混）+ 按压一律 Button kind（五态查表单源）；Dialog 双钮（Outlined 取消/Filled 确认）、Menu 行 = Button Text 形（on_pick_base+i 差分路由）、Toast 定长 2s（`toast_expired` 宿主帧拍轮询）。a11y 全登记（ROLE_BUTTON/ROLE_TEXT）。host 测 6 件全绿（tests/appshell.rs：建树/a11y/事件 id/时序/loud 拒面/scrim 律）。

**色彩豁免台账**（暗色刀「豁免只减不增」同口径）：`TOAST_BG`(0x2C2C2C)/`TOAST_FG`(白) 一对中性恒色——OH toast 双主题同形（暗底白字为不随主题换档的不变量，非主题缩放色）；theme 出 on_scrim 类 token 时收编。当前 toast v1 视觉 = Card kind 默认底（bg_comp）+ 白字，TOAST_BG 对已引出留 API（恒色对齐注释在源）。

## 二、berry-softime（新 crate，user/berry-softime）

softimed 引擎半件（SERVICE 底窗渲染本体随波 3 并主接线——主树已提交面 `wm_layer_known` 仍拒 layer=2，C-RECON §一）：键位模型（QWERTY-lite 3 字母行 + Shift/Space/Backspace/Enter 动作行，26 字母闭包测试）；注入编码走 berry-guiproto `encode_input_inject_req` 单源（可打印 ASCII → DEV_TEXT 单字节、Backspace/Enter → DEV_KEYBOARD Linux input 码 14/28——gui-inputd EV_KEY 词汇同源）；Shift 单发大写档（字母注入后回落，OH/iOS 软键盘惯例）；显隐状态机 Face（focus ime_wanted 唯一真源，翻转返回值 = SERVICE 窗建/撤单源驱动位）。host 测 5 件全绿（tests/softime.rs）。

**挂账**：①softimed bin（SERVICE 底窗渲染 + OP_INPUT_INJECT 通道真实接线）随波 3 `wm_layer_known` 收宽落地，届时挂本引擎零改动；②镜像装配 spec：softime 非运行件 v1 不入 rootfs（bin 落地时随 justfile 装配批——justfile 当前在他会话脏面，本刀零触碰）。

## 三、simbattery 发布面（user/bins/simbattery/src/main.rs）

`Step::Frame(f)` ST_OK 分支增发布：`f.snap` 为 Battery 时 `print_batt(capacity_pct, status)` → 串口标记 `BATT ok pct=N state=S`（print_ok 同形整行单消息）。musl check 绿（justfile 配方 target）。

**挂账**：launcher/状态栏消费通道设计（svcmgr channel 礼物对：预拉 simbattery 时多赠一 SEND 端、launcher bootstrap 对应 RECV 端、每帧转发 8B 摘要）——本刀不实施（launcher 主树面与 wip/ui-wave3 重写漂移 224+/17−，先动必冲突），随波 3 launcher 重写同批接线；届时 `BATT ok pct=` 标记即真值在场的回归门。

## 四、smoke 门（tools/markers.sh + tools/smoke.sh）

RX_MARKERS 增 `"BATT ok pct="`（与 SIMBAT 同门控——门闭 EFI 相两者皆 0 行自洽）；smoke.sh SIMBAT 双正例断言后增 BATT 在场断言（fail 文案指认 print_batt 断裂面）；行 48 注释同步。互非子串自检：BATT ok pct= 对全 87 标记零违规（本会话脚本核——既有 DEMO/CLI 族命中为注释正则伪影非本批）。

## 五、r1/r2 复盘（一句话进笔记）

r1 = workflow 脚本漏 await（实施者孤儿 Promise，验收/审查跑了空树——两者「不通过」结论正确）；r2 = 实施子代理本体失败两次同点（`impl: null`，疑与并行会话抢全局子代理帽）——终局 = 本会话直接实施。教训：三步流水在撞帽环境下应首选单代理或直做。

## 六、提交面

显式路径：user/berry-appshell/、user/berry-softime/、user/bins/simbattery/src/main.rs、user/Cargo.toml（members +2）、tools/markers.sh、tools/smoke.sh。
