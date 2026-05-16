//! Issue #39: 文字列 → 潜在ベクトル → curion ラベル の対称パイプライン
//!
//! curion 生成を「文字列から直接 noun を引く」方式ではなく、
//!
//! ```text
//! seed bytes  →  16次元 latent vector  →  最も近い noun prototype  =  curion 名
//!                       │
//!                       ├──→  rarity     (dims 0..4 を投影)
//!                       ├──→  interest   (dims 8..12 を投影)
//!                       └──→  beauty     (dims 12..16 を投影)
//! ```
//!
//! 「curion の本体は noun ではなく潜在ベクトル」「noun名はラベルにすぎない」
//! という世界観に寄せた対称パイプライン。
//! 同じ seed からは常に同じ latent vector が得られ、rarity / interest / beauty / noun
//! の全てがそこから派生する。将来 Issue #38 (装備/消費効果) の効果ベクトルも
//! 同じ latent から導出する想定。

use sha2::{Digest, Sha256};

/// 潜在ベクトルの次元数。
///
/// 16 を採用した理由:
/// - SHA-256 (32 byte) を 2 回回せば 64 byte = 16 × u32 が一意に得られる
/// - 9 カテゴリ × 50 名詞程度の弁別には十分な解像度
/// - noun prototype のキャッシュサイズが軽量 (16 × f32 = 64 byte/noun)
pub const LATENT_DIM: usize = 16;

/// 固定長 latent vector。各次元は [-1.0, 1.0] の擬似一様乱数 (deterministic)。
pub type LatentVector = [f32; LATENT_DIM];

/// SHA-256 を 2 ラウンド回して 64 byte の擬似乱数列を得るドメインタグ。
///
/// `latent_from_seed` と `prototype_for_noun` で異なるタグを使い、
/// 「同じ文字列 = noun 名」を seed として渡したときに偶然 latent と prototype が
/// 一致しないようにする。
const TAG_SEED: &[u8] = b"curion:latent:seed:v1";
const TAG_NOUN: &[u8] = b"curion:latent:noun_prototype:v1";

/// 任意バイト列を 64 byte の deterministic 擬似乱数列に展開する。
///
/// 内部実装: `SHA-256(tag || seed)` でブロック A を、
/// `SHA-256(tag || seed || [0x01])` でブロック B を得て連結。
/// 衝突を避けるためタグは固定文字列を渡す。
fn expand_to_64_bytes(tag: &[u8], seed: &[u8]) -> [u8; 64] {
    let mut out = [0u8; 64];

    let mut h1 = Sha256::new();
    h1.update(tag);
    h1.update(seed);
    let block_a = h1.finalize();

    let mut h2 = Sha256::new();
    h2.update(tag);
    h2.update(seed);
    h2.update([0x01u8]);
    let block_b = h2.finalize();

    out[..32].copy_from_slice(&block_a);
    out[32..].copy_from_slice(&block_b);
    out
}

/// 64 byte の擬似乱数列を 16 次元の f32 ベクトルに展開する。
///
/// 各次元は連続する 4 byte を u32 (LE) として読み、
/// `(u as f64 / u32::MAX as f64) * 2.0 - 1.0` で [-1.0, 1.0] に正規化。
fn bytes_to_latent(bytes: &[u8; 64]) -> LatentVector {
    let mut out = [0f32; LATENT_DIM];
    for (i, slot) in out.iter_mut().enumerate() {
        let off = i * 4;
        let u = u32::from_le_bytes([bytes[off], bytes[off + 1], bytes[off + 2], bytes[off + 3]]);
        let normalized = (u as f64 / u32::MAX as f64) * 2.0 - 1.0;
        *slot = normalized as f32;
    }
    out
}

/// seed バイト列を hash し、16 次元の f32 latent vector に展開する。
///
/// 出力: 各次元が [-1.0, 1.0] の擬似一様乱数 (deterministic)。
/// 同じ `seed` には常に同じベクトルが返る。
pub fn latent_from_seed(seed: &[u8]) -> LatentVector {
    let bytes = expand_to_64_bytes(TAG_SEED, seed);
    bytes_to_latent(&bytes)
}

/// noun 名から prototype vector を導出する。
///
/// 「意味空間における noun の位置」を表現するベクトル。
/// 同じ noun は常に同じ prototype を返す。
/// 注意: prototype はあくまで noun 名から deterministic に作られる擬似的な意味空間で、
/// 真の意味的近接性 (例: 「魚」と「鯨」が近い) は保証しない。
/// noun 単位で固定されていることだけが本実装の保証範囲。
pub fn prototype_for_noun(noun: &str) -> LatentVector {
    let bytes = expand_to_64_bytes(TAG_NOUN, noun.as_bytes());
    bytes_to_latent(&bytes)
}

/// 2 つの latent vector のコサイン類似度を返す。
///
/// 値域: `[-1.0, 1.0]` (1.0 = 完全に同方向、-1.0 = 真逆)。
/// どちらかが零ベクトル (全次元 0) なら 0.0 を返す (零除算回避)。
pub fn cosine_similarity(a: &LatentVector, b: &LatentVector) -> f32 {
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..LATENT_DIM {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = na.sqrt() * nb.sqrt();
    if denom < 1e-12 {
        0.0
    } else {
        dot / denom
    }
}

/// latent の指定次元から f64 を取り出し、`[0.0, 1.0]` に正規化する。
///
/// `dim_indices` が指す次元の平均を取り、`(avg + 1.0) / 2.0` で
/// [-1.0, 1.0] → [0.0, 1.0] に変換。
/// `dim_indices` が空の場合は 0.5 を返す (中央値)。
pub fn project_unit(latent: &LatentVector, dim_indices: &[usize]) -> f64 {
    if dim_indices.is_empty() {
        return 0.5;
    }
    let mut sum = 0f64;
    for &i in dim_indices {
        debug_assert!(i < LATENT_DIM, "dim index {i} out of range");
        sum += latent[i] as f64;
    }
    let avg = sum / dim_indices.len() as f64;
    ((avg + 1.0) / 2.0).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #39: 同じ seed からは同じ latent vector が得られる (deterministic)。
    #[test]
    fn test_latent_from_seed_deterministic() {
        let a = latent_from_seed(b"hello world");
        let b = latent_from_seed(b"hello world");
        assert_eq!(a, b);
    }

    /// Issue #39: 異なる seed は異なる latent vector を生む。
    #[test]
    fn test_latent_from_seed_different_seeds_differ() {
        let a = latent_from_seed(b"hello world");
        let b = latent_from_seed(b"hello world!"); // 1 byte 違い
        assert_ne!(a, b);
        // 完全一致しないことだけでなく、距離があることを確認
        let sim = cosine_similarity(&a, &b);
        assert!(
            sim < 0.99,
            "very close seeds should not produce identical-direction latents (sim={sim})"
        );
    }

    /// Issue #39: 各次元が [-1.0, 1.0] の範囲に収まる。
    #[test]
    fn test_latent_range_normalized() {
        let seeds: [&[u8]; 3] = [b"a", b"long seed string here", &[0u8; 32]];
        for seed in seeds {
            let v = latent_from_seed(seed);
            for (i, &x) in v.iter().enumerate() {
                assert!(
                    (-1.0..=1.0).contains(&x),
                    "dim {i} out of range: {x} (seed={seed:?})"
                );
            }
        }
    }

    /// Issue #39: 同じ noun は常に同じ prototype を返す。
    #[test]
    fn test_prototype_for_noun_deterministic() {
        let a = prototype_for_noun("魚");
        let b = prototype_for_noun("魚");
        assert_eq!(a, b);
    }

    /// Issue #39: 異なる noun は異なる prototype を返す。
    #[test]
    fn test_prototype_for_different_nouns_differ() {
        let a = prototype_for_noun("魚");
        let b = prototype_for_noun("鯨");
        assert_ne!(a, b);
    }

    /// Issue #39: latent と prototype はドメインタグが違うので
    /// 同じ文字列を渡しても一致しない (= 「seed が偶然 noun と被ったときに
    /// その noun が常に勝つ」事故を防ぐ)。
    #[test]
    fn test_latent_and_prototype_use_different_domains() {
        let a = latent_from_seed("魚".as_bytes());
        let b = prototype_for_noun("魚");
        assert_ne!(a, b);
    }

    /// Issue #39: コサイン類似度は自分自身に対して 1.0。
    #[test]
    fn test_cosine_similarity_self_is_one() {
        let v = latent_from_seed(b"some seed");
        let sim = cosine_similarity(&v, &v);
        assert!(
            (sim - 1.0).abs() < 1e-5,
            "self similarity should be ~1.0, got {sim}"
        );
    }

    /// Issue #39: 直交ベクトル (片方が +1 軸、もう片方が +2 軸) で 0.0 になる。
    #[test]
    fn test_cosine_similarity_orthogonal() {
        let mut a = [0f32; LATENT_DIM];
        let mut b = [0f32; LATENT_DIM];
        a[0] = 1.0;
        b[1] = 1.0;
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "orthogonal sim should be 0, got {sim}");
    }

    /// Issue #39: 零ベクトルとの類似度は 0 (NaN/Inf 回避)。
    #[test]
    fn test_cosine_similarity_zero_vector_is_zero() {
        let zero = [0f32; LATENT_DIM];
        let v = latent_from_seed(b"x");
        assert_eq!(cosine_similarity(&zero, &v), 0.0);
        assert_eq!(cosine_similarity(&v, &zero), 0.0);
    }

    /// Issue #39: project_unit は [0.0, 1.0] の範囲に収まる。
    #[test]
    fn test_project_unit_in_range() {
        let seeds: [&[u8]; 3] = [b"a", b"hello", b"another seed"];
        for seed in seeds {
            let v = latent_from_seed(seed);
            for dims in [
                &[0usize][..],
                &[0, 1, 2, 3][..],
                &[8, 9, 10, 11][..],
                &[12, 13, 14, 15][..],
            ] {
                let u = project_unit(&v, dims);
                assert!(
                    (0.0..=1.0).contains(&u),
                    "project_unit out of range: {u} (seed={seed:?}, dims={dims:?})"
                );
            }
        }
    }

    /// Issue #39: project_unit は dim_indices 空のとき 0.5 を返す (中央値)。
    #[test]
    fn test_project_unit_empty_returns_center() {
        let v = latent_from_seed(b"x");
        assert_eq!(project_unit(&v, &[]), 0.5);
    }
}
