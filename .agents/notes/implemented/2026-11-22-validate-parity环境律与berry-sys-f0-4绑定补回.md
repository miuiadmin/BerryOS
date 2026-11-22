# validate_parity 真源基座拒腿环境律固化 + berry-sys F0-4 缓存维护绑定补回

Status: implemented

## 背景

W3-38 F1 全套（分支 hermes/w3-202611221720）在 host 四门验证时 `just test-host` 于 `berryassemble validate_parity` 假红。同时发现 base tip（3aa4eabfb）上 `berry_vmo_cache_op` 的 berry-sys fn 绑定缺席——vnet 消费 14 处却无定义，`just build`（含 vnet）在冷 target 下必红。

## 根因一：validate_parity 三腿环境敏感（host 假红）

- 腿⑤（--check 真源基座拒）/ 腿⑦（--dry-run 真源基座拒）/ 腿⑩（--emit 真源基座拒）隐式假设「host 构建树冷——`user/userbins`（gitignored）缺席 → resolve_entries 咬『构建树缺席』拒 → 双侧 rc=1 对拍」。
- 开发树 `just build-user` 后构建树热 → 基座表条目全在 → 双侧 rc=0 → 断言 `r.rc == 1` 假红。本机 2026-11-22 18:07 build 刷热 userbins 后 18:11/18:12 两连红实锤。
- CI 冷树不踩（ci.yml:161 挂账注——重道不跑 just test-host），reviewer 树冷亦不踩；唯「先 build 后 test-host」的开发树踩。

## 根因二：berry-sys `berry_vmo_cache_op` fn 绑定跨 lane 重放丢失

- F0-4 刀（2026-11-18 implemented）件 1 落了 abi_gen.rs 常量与 nr 号（0x1C）、kernel 侧 handler、vnet/vblk/gui-inputd 消费，但 berry-sys `lib.rs` 的 `#[no_mangle] pub extern "C" fn berry_vmo_cache_op` 绑定在跨 lane 重放/合流中丢失——base tip 树 `git grep -c "fn berry_vmo_cache_op"` 零命中。
- CI 侧靠共享 target 缓存（berry-sys 源未变则 vnet 不重编）掩盖；任何冷 clone / 缓存清空跑 `just build` 即炸 E0432。
- 本刀补回定义（签名与 vnet 消费面 14 处、abi_gen.rs 单源常量逐位一致）。

## 修形

1. [validate_parity.rs](../../../tools/berryassemble/tests/validate_parity.rs) 腿⑤/⑦/⑩加环境律门：双侧 rc=0（构建树热）= 明示 eprintln skip（对拍纪律保留——其余腿照跑；CI 冷树照咬），rc!=0 才走原断言链。拒面文案断言保留在冷树臂内，勿删。
2. [lib.rs](../../../user/berry-sys/src/lib.rs) 补 `berry_vmo_cache_op(vmo, op, offset, len)` 绑定（nr::VMO_CACHE_OP 单源，五面一致由 abi-check 执法）。
3. 随批：svcface REGISTRY 0x03 handle_replace TABLE→SQ_RQ_TABLE 升档（replace 内含 close 臂——末步 release_handle 触 wake 面，与 0x01 同律；F1 dispatch 触面计数不变）；[SMP-S3 草案](../../../docs/SMP-S3实施设计草案-v1.0.md) 排除面同步补注；sync-public.sh WHITELIST 补 vmarface（F1 合入 vmarface crate 后白名单笔未随刀，#299/#307 判例族）；tools/*.sh 8 件 shvarcheck 花括号清偿（bash 3.2 吞名守卫门）；facts.txt tests_rust 3389→3393。

## 验证

- `just test-host` 全绿（EXIT=0，351 test-result ok 组；validate_parity 1 passed——腿⑤⑦⑩热树明示 skip，其余腿全咬）。
- docs-guard PASS 0 警；abi-check PASS（NR v1 42 号五面单源）；writable_globals ok 92 条；shvarcheck ok；facts PASS 11 键。
- base CI 红实测为 facts 4 键漂移（W1/W2 lane 债，run 45012305803 light 面），非本件。
