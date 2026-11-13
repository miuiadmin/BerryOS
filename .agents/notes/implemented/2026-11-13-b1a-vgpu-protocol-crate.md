# B1-a:virtio-gpu 协议 crate 纯面先行

Status: implemented

## Problem
GPU 加速路径(案 β)guest 半第一刀:协议面需先冻结(字节面钉板),传输接线(B1-c)与 guifbd 后端(B1-b)才有单源可依。

## Decision
user/berry-gpuproto 纯协议面(零传输依赖/零 unsafe/零分配,no_std 双面):2D 九命令 encode/parse+控制头+响应面+config 事件面+BGRX↔ramfb fmt=1 映射;uapi 常量七组两源对照断言冻结字节面。S1 三陷阱(PCI 别名/force-legacy=false/INDIRECT 默认 on)头注存证。

## Alternatives considered
传输层一起做(否:owner 占 kernel/drivers 域,纯面先行零撞面);直接写 guifbd 后端(否:无协议单源则后端字节面会漂)。

## Related
docs/显示与加速体系设计-v1.0.md 刀序 B1;research/显示与加速体系设计/05-08 号;后续 B1-b(guifbd)/B1-c(接线)。
