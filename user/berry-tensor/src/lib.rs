//! berry-tensor —— 三阶异构张量计算的 Tier 0 量化算子核（ai-runtime
//! 总体设计契约二.1「确定性全平台量化软核」首刀：
//! `.agents/plans/ai-runtime/design.md`）。Tier 0 定位：纯 Rust 无堆
//! 分配软核，保证无独立加速器的穿戴/基础虚拟机环境可用，且作为 Tier 1
//! （宿主 GPU 直通）/Tier 2（真机 NPU）的**精度与语义基线**——SIMD 与
//! 卸载后端以本 crate 的确定性结果对拍（验收标准 2：Tier 0↔Tier 1 余弦
//! 相似度 ≥ 0.999）。「解析器即产品」家族形态（berry-aiproto/
//! berry-partition 同款）：零 unsafe、零依赖、零分配；
//! `cfg_attr(not(test), no_std)`（workspace none-target 全量构建可编），
//! test 构建放开 std 仅为测试挂具。host 测试经 justfile test-host 显式
//! 行跑。
//!
//! ## 本刀覆盖（首片）
//!
//! - **块布局单源**：Q4_0/Q8_0 的字节形在本 crate 常量与解析函数钉死
//!   （llama.cpp 兼容——生态工装可直接产 `.bmod` 张量段）；
//!   `Q4_K_M` 等其余 `.bmod` 量化格式（berry-aiproto::QuantFormat）
//!   的算子核归后续刀。
//! - **f16 解码**：手写位拆（零依赖无 half crate），含次正规域。
//! - **去量化**：整块 → `f32 × 32`（写入调用方缓冲，库零分配）。
//! - **量化×量化点积**：`dot_q4_q8`——块内整数内积、块尾 scale 后乘、
//!   顺序累积（确定性：同输入恒同输出，无重排无并行——SIMD 改写保序
//!   对拍的前置）。
//!
//! ## 字段布局（全 LE）
//!
//! - `Q4_0` 块（[`Q4_0_BLOCK_BYTES`] = 18B，32 权重）：`{f16 scale[2],
//!   packed[16]}`——packed 字节 j 携权重 2j（低半字节）与 2j+1（高半
//!   字节），存储值 = 量化值 + 8（量化域 [-8, 7]）；权重 =
//!   scale × (存储值 − 8)。
//! - `Q8_0` 块（[`Q8_0_BLOCK_BYTES`] = 34B，32 权重）：`{f16 scale[2],
//!   q[32] × i8}`；权重 = scale × q。
//!
//! ## 契约纪律
//!
//! - 块数与缓冲长度的匹配是调用方程序员错误面：`assert!` 即时响（零
//!   静默截断）——与 berry-aiproto 的「外部字节流 loud 拒」分层：本
//!   crate 输入是已通过 `.bmod` 结构执法的合法张量段。
//! - 算术域：块内整数内积走 `i32`（32 × 7 × 127 上界 28k，i32 富余），
//!   scale 乘与跨块累积走 `f32`——Tier 0 基线的确定性来源。

#![cfg_attr(not(test), no_std)]

/// Q4_0 块权重数（分组量化粒度）。
pub const Q4_0_BLOCK: usize = 32;
/// Q4_0 块字节数（2B f16 scale + 16B 4-bit packed）。
pub const Q4_0_BLOCK_BYTES: usize = 18;
/// Q8_0 块权重数。
pub const Q8_0_BLOCK: usize = 32;
/// Q8_0 块字节数（2B f16 scale + 32B i8）。
pub const Q8_0_BLOCK_BYTES: usize = 34;
/// 块内整数的 4-bit 存储偏置（存储域 [0,15] ↔ 量化域 [-8,7]）。
pub const Q4_BIAS: i32 = 8;

/// f16（IEEE 754 binary16，LE 字节对）→ f32 位级解码（含次正规域）。
pub fn f16_to_f32(lo: u8, hi: u8) -> f32 {
    let bits = ((hi as u16) << 8) | lo as u16;
    let sign = ((bits >> 15) & 1) as u32;
    let exp = ((bits >> 10) & 0x1f) as u32;
    let frac = (bits & 0x3ff) as u32;
    let out = match (exp, frac) {
        (0, 0) => sign << 31,                       // ±0
        // 次正规：f16 次正规（frac × 2^-24）在 f32 是正规域（f32 正规最小
        // 2^-126），位形直搬必错（R1 实测教训）——算术构造精确且无分支。
        (0, _) => {
            const F16_SUBNORM_SCALE: f32 = 1.0 / 16777216.0; // 2^-24（精确二次幂）
            let v = frac as f32 * F16_SUBNORM_SCALE;
            if sign == 1 { -v } else { v }.to_bits()
        }
        (0x1f, 0) => sign << 31 | 0x7f80_0000,      // ±inf
        (0x1f, _) => sign << 31 | 0x7f80_0000 | 0x0040_0000 | frac << 13, // NaN（尾数搬移 + 置 f32 quiet 位保传播）
        _ => {
            // 正规：f16 指数 e（偏置 15）→ f32 指数（偏置 127），尾数左移 13
            let e32 = exp + 112;
            sign << 31 | e32 << 23 | frac << 13
        }
    };
    f32::from_bits(out)
}

/// Q4_0 块去量化（32 权重写入 `out`；`block` 为 18B 合法块）。
pub fn dequant_q4_0(block: &[u8], out: &mut [f32]) {
    assert!(block.len() >= Q4_0_BLOCK_BYTES, "Q4_0 块截断：{}", block.len());
    assert!(out.len() >= Q4_0_BLOCK, "Q4_0 输出缓冲不足：{}", out.len());
    let scale = f16_to_f32(block[0], block[1]);
    for j in 0..16 {
        let b = block[2 + j];
        // 单乘式（scale × (stored − 8)）：与 dot_q4_q8 的整数路径舍入一致。
        out[2 * j] = scale * ((b & 0x0f) as i32 - Q4_BIAS) as f32;
        out[2 * j + 1] = scale * ((b >> 4) as i32 - Q4_BIAS) as f32;
    }
}

/// Q8_0 块去量化（32 权重写入 `out`；`block` 为 34B 合法块）。
pub fn dequant_q8_0(block: &[u8], out: &mut [f32]) {
    assert!(block.len() >= Q8_0_BLOCK_BYTES, "Q8_0 块截断：{}", block.len());
    assert!(out.len() >= Q8_0_BLOCK, "Q8_0 输出缓冲不足：{}", out.len());
    let scale = f16_to_f32(block[0], block[1]);
    for i in 0..32 {
        out[i] = scale * block[2 + i] as i8 as f32;
    }
}

/// 量化×量化点积：Q4_0 向量 `a`（`blocks` × 18B）· Q8_0 向量 `b`
/// （`blocks` × 34B）。块内 i32 整数内积、块尾 scale 后乘、f32 顺序
/// 累积——Tier 0 确定性基线（SIMD/卸载后端对拍锚）。
pub fn dot_q4_q8(a: &[u8], b: &[u8], blocks: usize) -> f32 {
    assert!(a.len() >= blocks * Q4_0_BLOCK_BYTES, "Q4_0 段截断：{} < {}", a.len(), blocks * Q4_0_BLOCK_BYTES);
    assert!(b.len() >= blocks * Q8_0_BLOCK_BYTES, "Q8_0 段截断：{} < {}", b.len(), blocks * Q8_0_BLOCK_BYTES);
    let mut acc = 0f32;
    for blk in 0..blocks {
        let aq = &a[blk * Q4_0_BLOCK_BYTES..(blk + 1) * Q4_0_BLOCK_BYTES];
        let bq = &b[blk * Q8_0_BLOCK_BYTES..(blk + 1) * Q8_0_BLOCK_BYTES];
        let sa = f16_to_f32(aq[0], aq[1]);
        let sb = f16_to_f32(bq[0], bq[1]);
        let mut dot: i32 = 0;
        for j in 0..16 {
            let byte = aq[2 + j];
            let lo = (byte & 0x0f) as i32 - Q4_BIAS;
            let hi = (byte >> 4) as i32 - Q4_BIAS;
            dot += lo * bq[2 + 2 * j] as i8 as i32;
            dot += hi * bq[2 + 2 * j + 1] as i8 as i32;
        }
        acc += sa * sb * dot as f32;
    }
    acc
}

/// f32 参考点积（`a`、`b` 等长；顺序累积）——量化核的语义对拍基准。
pub fn dot_f32(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len(), "dot_f32 长度失配：{} vs {}", a.len(), b.len());
    let mut acc = 0f32;
    for i in 0..a.len() {
        acc += a[i] * b[i];
    }
    acc
}

/// Q4_0 权重矩阵 × Q8_0 激活向量（量化域 GEMV 基元）：`w_q4` 为
/// `rows × cols_quant` 的 Q4_0 权重行堆叠（每行 `cols/32` 块），`x_q8`
/// 为同长 Q8_0 激活；输出 `out[i] = dot(权重行 i, x)`。Tier 0 确定性
/// （行内 [`dot_q4_q8`] 语义、行间顺序写）——解码层 MatMul 的行基元。
pub fn matvec_q4_q8(
    w_q4: &[u8],
    x_q8: &[u8],
    rows: usize,
    blocks: usize,
    out: &mut [f32],
) {
    assert!(
        w_q4.len() >= rows * blocks * Q4_0_BLOCK_BYTES,
        "Q4_0 权重段截断：{} < {}", w_q4.len(), rows * blocks * Q4_0_BLOCK_BYTES
    );
    assert!(
        x_q8.len() >= blocks * Q8_0_BLOCK_BYTES,
        "Q8_0 激活段截断：{} < {}", x_q8.len(), blocks * Q8_0_BLOCK_BYTES
    );
    assert!(out.len() >= rows, "输出缓冲不足：{} < {}", out.len(), rows);
    for r in 0..rows {
        let row = &w_q4[r * blocks * Q4_0_BLOCK_BYTES..(r + 1) * blocks * Q4_0_BLOCK_BYTES];
        out[r] = dot_q4_q8(row, x_q8, blocks);
    }
}

/// 1/√x（纯 core 实现：0x5f3759df 位黑客种子 + 3 轮牛顿精确化——种子
/// 相对误差 ~2e-2，三轮二次收敛至 f32 半 ULP 下，确定性无环境依赖；
/// `x > 0` 由调用域保证）。
fn inv_sqrt(x: f32) -> f32 {
    let half = 0.5 * x;
    let mut y = f32::from_bits(0x5f37_59df - (x.to_bits() >> 1));
    y *= 1.5 - half * y * y;
    y *= 1.5 - half * y * y;
    y *= 1.5 - half * y * y;
    y
}

/// RMSNorm（Transformer 归一化层基元）：`out[i] = w[i] × x[i] / rms(x)`，
/// rms = sqrt(mean(x²) + eps)。顺序累积确定性（与 [`dot_f32`] 同纪律）；
/// eps 防全零行除零。`x`、`w`、`out` 等长。
pub fn rmsnorm(x: &[f32], w: &[f32], out: &mut [f32], eps: f32) {
    assert_eq!(x.len(), w.len(), "rmsnorm x/w 长度失配：{} vs {}", x.len(), w.len());
    assert_eq!(x.len(), out.len(), "rmsnorm x/out 长度失配：{} vs {}", x.len(), out.len());
    let mut ss = 0f32;
    for v in x {
        ss += v * v;
    }
    let inv = inv_sqrt(ss / x.len() as f32 + eps);
    for i in 0..x.len() {
        out[i] = w[i] * x[i] * inv;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_decode_regular_and_signs() {
        assert_eq!(f16_to_f32(0x00, 0x3c), 1.0); // 0x3C00
        assert_eq!(f16_to_f32(0x00, 0x40), 2.0); // 0x4000
        assert_eq!(f16_to_f32(0x00, 0x38), 0.5); // 0x3800
        assert_eq!(f16_to_f32(0x00, 0x80), 0.0); // +0（次正规全零）
        assert_eq!(f16_to_f32(0x00, 0xbc), -1.0); // 0xBC00
        assert_eq!(f16_to_f32(0x00, 0xc0), -2.0); // 0xC000
        // 0x3555 = (1 + 0x155×2^-10) × 2^(13-15)：f16 位形精确搬（exp 13 → 2^-2）
        let v = f16_to_f32(0x55, 0x35);
        assert_eq!(v, (1.0 + 0x155 as f32 / 1024.0) / 4.0);
    }

    #[test]
    fn f16_decode_subnormal_and_special() {
        // 最小次正规 0x0001 → 2^-24（f32 次正规直搬语义）
        assert_eq!(f16_to_f32(0x01, 0x00), 2f32.powi(-24));
        // 最大次正规 0x03FF → 1023 × 2^-24
        assert_eq!(f16_to_f32(0xff, 0x03), 1023.0 * 2f32.powi(-24));
        // ±inf
        assert_eq!(f16_to_f32(0x00, 0x7c), f32::INFINITY);
        assert_eq!(f16_to_f32(0x00, 0xfc), f32::NEG_INFINITY);
        // NaN：静默位已置（位域判，禁 == NaN）
        let nan = f16_to_f32(0x01, 0x7c);
        assert!(nan.is_nan() && (nan.to_bits() & 0x0040_0000) != 0);
    }

    #[test]
    fn dequant_q4_0_nibble_order_and_bias() {
        // scale=1.0；存储域 = 量化值 + 8：字节 0x08 → 低/高半均存 8 → 量化 0/0；
        // 字节 0x17 → 低半存 7（量化 -1）、高半存 1（量化 -7）。
        let mut block = [0u8; Q4_0_BLOCK_BYTES];
        block[0] = 0x00;
        block[1] = 0x3c; // f16 1.0
        block[2] = 0x08; // 低半存 8（w0=0）、高半存 0（w1=-8）
        block[3] = 0x17; // 低半存 7（w2=-1）、高半存 1（w3=-7）
        let mut out = [0f32; Q4_0_BLOCK];
        dequant_q4_0(&block, &mut out);
        assert_eq!(out[0], 0.0);
        assert_eq!(out[1], -8.0);
        assert_eq!(out[2], -1.0);
        assert_eq!(out[3], -7.0);
        assert_eq!(out[4], -8.0); // 尾部未置字节 → 存储 0 → 量化 -8（偏置域下界）
        // scale=0.5：减半
        block[1] = 0x38; // f16 0.5
        dequant_q4_0(&block, &mut out);
        assert_eq!(out[2], -0.5);
        assert_eq!(out[3], -3.5);
    }

    #[test]
    fn dequant_q8_0_values_and_scale() {
        let mut block = [0u8; Q8_0_BLOCK_BYTES];
        block[1] = 0x3c; // scale 1.0
        block[2] = 1;
        block[3] = 0xff; // -1
        block[4] = 0x7f; // 127
        block[5] = 0x80; // -128
        let mut out = [0f32; Q8_0_BLOCK];
        dequant_q8_0(&block, &mut out);
        assert_eq!(out[0], 1.0);
        assert_eq!(out[1], -1.0);
        assert_eq!(out[2], 127.0);
        assert_eq!(out[3], -128.0);
        assert_eq!(out[4], 0.0);
        block[1] = 0x38; // scale 0.5
        dequant_q8_0(&block, &mut out);
        assert_eq!(out[2], 63.5);
        assert_eq!(out[3], -64.0);
    }

    /// 手构 Q4_0 块（量化值 q[0..32]，域 [-8,7]；scale 指定 f16 字节）。
    fn q4_block(q: &[i32; 32], scale_lo: u8, scale_hi: u8) -> [u8; Q4_0_BLOCK_BYTES] {
        let mut b = [0u8; Q4_0_BLOCK_BYTES];
        b[0] = scale_lo;
        b[1] = scale_hi;
        for j in 0..16 {
            let lo = (q[2 * j] + Q4_BIAS) as u8;
            let hi = (q[2 * j + 1] + Q4_BIAS) as u8;
            b[2 + j] = lo | (hi << 4);
        }
        b
    }

    /// 手构 Q8_0 块（量化值 q[0..32]，i8 域）。
    fn q8_block(q: &[i32; 32], scale_lo: u8, scale_hi: u8) -> [u8; Q8_0_BLOCK_BYTES] {
        let mut b = [0u8; Q8_0_BLOCK_BYTES];
        b[0] = scale_lo;
        b[1] = scale_hi;
        for i in 0..32 {
            b[2 + i] = q[i] as i8 as u8;
        }
        b
    }

    #[test]
    fn dot_q4_q8_single_block_exact() {
        // qa = [-8..7 交错简化]，qb = [1,-1,…]；sa=1.0、sb=2.0
        let mut qa = [0i32; 32];
        let mut qb = [0i32; 32];
        let mut expect = 0i32;
        for i in 0..32 {
            qa[i] = (i as i32 % 16) - 8; // -8..7 循环
            qb[i] = if i % 2 == 0 { 1 } else { -3 };
            expect += qa[i] * qb[i];
        }
        let a = q4_block(&qa, 0x00, 0x3c); // scale 1.0
        let b = q8_block(&qb, 0x00, 0x40); // scale 2.0
        let got = dot_q4_q8(&a, &b, 1);
        assert_eq!(got, 2.0 * expect as f32);
    }

    #[test]
    fn dot_q4_q8_multi_block_accumulation() {
        let qa1 = [7i32; 32];
        let qb1 = [2i32; 32];
        let qa2 = [-4i32; 32];
        let qb2 = [-5i32; 32];
        let a: Vec<u8> = [q4_block(&qa1, 0x00, 0x3c), q4_block(&qa2, 0x00, 0x38)]
            .concat();
        let b: Vec<u8> = [q8_block(&qb1, 0x00, 0x3c), q8_block(&qb2, 0x00, 0x40)]
            .concat();
        // 块 1：7×2×(1.0×1.0)=14/权 → 448；块 2：(-4)×(-5)×(0.5×2.0)=20/权 → 640
        assert_eq!(dot_q4_q8(&a, &b, 2), 448.0 + 640.0);
        assert_eq!(dot_q4_q8(&a, &b, 0), 0.0);
    }

    #[test]
    fn dot_parity_with_f32_reference() {
        // 量化核 vs 去量化后 f32 点积：整数与二次幂 scale 域内逐位一致。
        let mut qa = [0i32; 32];
        let mut qb = [0i32; 32];
        for i in 0..32 {
            qa[i] = (i as i32 % 7) - 3;
            qb[i] = ((i * 5 + 2) % 11) as i32 - 5;
        }
        let a = q4_block(&qa, 0x00, 0x38); // sa=0.5
        let b = q8_block(&qb, 0x00, 0x3c); // sb=1.0
        let mut da = [0f32; 32];
        let mut db = [0f32; 32];
        dequant_q4_0(&a, &mut da);
        dequant_q8_0(&b, &mut db);
        assert_eq!(dot_q4_q8(&a, &b, 1), dot_f32(&da, &db));
    }

    #[test]
    fn dot_f32_basics() {
        assert_eq!(dot_f32(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]), 32.0);
        assert_eq!(dot_f32(&[], &[]), 0.0);
    }

    #[test]
    #[should_panic(expected = "长度失配")]
    fn dot_f32_len_mismatch_loud() {
        let _ = dot_f32(&[1.0], &[1.0, 2.0]);
    }

    #[test]
    #[should_panic(expected = "Q4_0 段截断")]
    fn dot_q4_truncated_loud() {
        let a = [0u8; Q4_0_BLOCK_BYTES - 1];
        let b = [0u8; Q8_0_BLOCK_BYTES];
        let _ = dot_q4_q8(&a, &b, 1);
    }

    #[test]
    #[should_panic(expected = "Q8_0 段截断")]
    fn dot_q8_truncated_loud() {
        let a = [0u8; Q4_0_BLOCK_BYTES];
        let b = [0u8; Q8_0_BLOCK_BYTES - 1];
        let _ = dot_q4_q8(&a, &b, 1);
    }

    #[test]
    fn q4_full_range_roundtrip_through_dequant() {
        // 全量化域 [-8,7] 逐一（i%16 循环取域内值——夹具越域会静默绕
        // nibble，域纪律归调用方）：编码 → dequant 还原（scale=1.0 整域精确）。
        let mut q = [0i32; 32];
        for i in 0..32 {
            q[i] = (i % 16) as i32 - 8;
        }
        let blk = q4_block(&q, 0x00, 0x3c);
        let mut out = [0f32; 32];
        dequant_q4_0(&blk, &mut out);
        for i in 0..32 {
            assert_eq!(out[i], q[i] as f32);
        }
    }

    #[test]
    fn matvec_q4_q8_rows_and_parity() {
        // 2 行 × 1 块：行基元 = 逐行 dot_q4_q8；与 f32 去量化参考逐位一致。
        let qa1 = [1i32; 32];
        let qa2 = [-2i32; 32];
        let qb = [3i32; 32];
        let w: Vec<u8> = [q4_block(&qa1, 0x00, 0x3c), q4_block(&qa2, 0x00, 0x38)]
            .concat();
        let x = q8_block(&qb, 0x00, 0x40); // sx=2.0
        let mut out = [0f32; 2];
        matvec_q4_q8(&w, &x, 2, 1, &mut out);
        // 行 1：Σ(1×3)=96 × (1.0×2.0) = 192；行 2：Σ(-2×3)=-192 × (0.5×2.0) = -192
        assert_eq!(out, [192.0, -192.0]);
        // f32 参考对拍（去量化后逐行 dot_f32）。
        let mut dw = [0f32; 64];
        dequant_q4_0(&w[..Q4_0_BLOCK_BYTES], &mut dw[..32]);
        dequant_q4_0(&w[Q4_0_BLOCK_BYTES..], &mut dw[32..]);
        let mut dx = [0f32; 32];
        dequant_q8_0(&x, &mut dx);
        assert_eq!(out[0], dot_f32(&dw[..32], &dx));
        assert_eq!(out[1], dot_f32(&dw[32..], &dx));
    }

    #[test]
    #[should_panic(expected = "Q4_0 权重段截断")]
    fn matvec_truncated_weight_loud() {
        let w = [0u8; Q4_0_BLOCK_BYTES];
        let x = [0u8; Q8_0_BLOCK_BYTES];
        let mut out = [0f32; 2];
        matvec_q4_q8(&w, &x, 2, 1, &mut out);
    }

    #[test]
    fn rmsnorm_exact_domain() {
        // x=[2,4]，mean(x²)=10，+eps=6 → rms²=16 → inv=0.25；w=[1,2]
        // → out≈[0.5, 2.0]（inv_sqrt 牛顿值容差断言——收敛至 1e-6 内）。
        let mut o = [0f32; 2];
        rmsnorm(&[2.0, 4.0], &[1.0, 2.0], &mut o, 6.0);
        assert!((o[0] - 0.5).abs() < 1e-6, "o[0]={}", o[0]);
        assert!((o[1] - 2.0).abs() < 1e-6, "o[1]={}", o[1]);
        // 全零行：eps 兜底不除零（inv 有限，x=0 → out 恒 0 精确）。
        let mut z = [0f32; 2];
        rmsnorm(&[0.0, 0.0], &[1.0, 1.0], &mut z, 4.0);
        assert_eq!(z, [0.0, 0.0]);
        // inv_sqrt 自身：收敛至 1e-6 内（牛顿落点可差 1 ULP，位精确不保）。
        assert!((inv_sqrt(16.0) - 0.25).abs() < 1e-6);
        assert!((inv_sqrt(2.0) - 0.70710678).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "长度失配")]
    fn rmsnorm_len_mismatch_loud() {
        let mut o = [0f32; 2];
        rmsnorm(&[1.0], &[1.0, 2.0], &mut o, 1.0);
    }
}
