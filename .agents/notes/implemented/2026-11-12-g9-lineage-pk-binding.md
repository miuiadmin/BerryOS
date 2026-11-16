# G9 lineage↔pk 绑定执法:零链面收口

Status: implemented

## Problem
OEM 演练 G9:admit TOFU 首见即锚,lineage↔pk 绑定未执法——同 lineage 换 pk(攻击者自签同 lineage 包)可混过。

## Decision
KeyMismatch(零链∧信封 pk∉锚集)升策略族拒,全档恒执法;带链信封=换钥仪式面走链机器裁决不受影响。取舍:逐字「验签后查 pk∈锚」会连仪式重叠双签窗一起拒,故按零链面收口(与 SDK-2 §二/§三一致)。

## Alternatives considered
仪式链全拒(否:与换钥仪式矛盾);仅 product 档执法(否:dev 档同 lineage 换 pk 同样是攻击面)。

## Related
admit.rs :459-469;42 号 F3;38 号 G9。
