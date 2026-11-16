# PM 双服务骨架:berry-power 与 berry-thermal 落件

Status: implemented

## Problem
调度彻底计划三大接缝之一:调度-PM 契约空转——ADR #66/#68 立法完备但 berry-power/berry-thermal 服务本体未建,util/EM/thermal 三件有产无消,省电支柱调度侧仅 tickless 兜底。

## Decision
双服务骨架以「lib 纯函数核(host 可测)+bin 壳(留窗)」形态落地:berry-power=频率帽聚合点(多源 min 唯一书写面)+充电限流票仲裁+util EWMA(复用 berry-emface 零第二实现)+pwproto 会话机;berry-thermal=step_wise+迟滞仲裁引擎+JEITA 五档第一张系统热表+thermal_level 四档+失败安全(热服务亡→帽保持 OutputLatch)。常驻/通道/挂接全窗注释,接线归 CP-08/CP-10。

## Alternatives considered
直接常驻接线(否:svcmgr/justfile/rootfs 均 owner 占用,骨架先行零撞面);单服务合体(否:ADR #66 明裁两服务同级——失败域隔离)。

## Related
docs/调度彻底计划-v1.0.md CP-08;复核 04 号 CP-08 接线窗 13 件(F1 mA-pct 票桥/F2 sim 消费切分须裁)。
