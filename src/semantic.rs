//! Issue #38: 意味空間タグ抽出
//!
//! Issue #39 で導入した 16 次元 latent vector を「意味タグ」(熱 / 速度 / 秩序 ...)
//! に投影する。装備効果・消費効果はこの `SemanticProfile` から導出されるため、
//! 効果ロジックは個別 curion に手書きせず latent からの decompose に閉じる。
//!
//! ```text
//! source_guid → latent_from_seed(guid.bytes) → SemanticProfile (10 tags)
//!                                                      │
//!                                                      ├─→  装備効果 (常時)
//!                                                      └─→  消費効果 (一時、将来 Issue)
//! ```
//!
//! 各タグ強度は `[0.0, 1.0]` の f64。次元割り当ては「latent の dim i から
//! `(x + 1) / 2` で [0,1] 化したものを tag_i に使う」というシンプルな線形写像で、
//! 「魚」と「龍」が偶然同じ熱値になることはあっても、同じ curion が呼び出しごとに
//! 異なる値になることは無い (deterministic)。

use crate::curion::Curion;
use crate::latent::{latent_from_seed, LatentVector, LATENT_DIM};
use serde::{Deserialize, Serialize};

/// 意味空間の 10 タグ。
///
/// latent の 16 次元のうち先頭 10 次元を直接タグに割り当てる
/// (残り 6 次元は category / rarity / interest / beauty に既に使われている。
/// タグはそれらと「同じ latent から派生する別の見方」になる)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SemanticTag {
    Heat,     // 熱、攻撃性
    Speed,    // 速度、加速
    Order,    // 秩序、安定
    Chaos,    // 混沌、不安定
    Life,     // 生命、回復
    Machine,  // 機械、効率
    Dream,    // 幻想、夢
    Violence, // 暴力、破壊
    Luck,     // 幸運、レア率
    Purity,   // 純粋、SAN
}

impl SemanticTag {
    /// UI 表示用の短い日本語ラベル。
    pub fn label(&self) -> &'static str {
        match self {
            SemanticTag::Heat => "熱",
            SemanticTag::Speed => "速度",
            SemanticTag::Order => "秩序",
            SemanticTag::Chaos => "混沌",
            SemanticTag::Life => "生命",
            SemanticTag::Machine => "機械",
            SemanticTag::Dream => "夢",
            SemanticTag::Violence => "暴力",
            SemanticTag::Luck => "幸運",
            SemanticTag::Purity => "純粋",
        }
    }

    /// 全タグ (`SemanticProfile::from_latent` のループ用)。
    pub const ALL: [SemanticTag; 10] = [
        SemanticTag::Heat,
        SemanticTag::Speed,
        SemanticTag::Order,
        SemanticTag::Chaos,
        SemanticTag::Life,
        SemanticTag::Machine,
        SemanticTag::Dream,
        SemanticTag::Violence,
        SemanticTag::Luck,
        SemanticTag::Purity,
    ];
}

/// 10 タグの強度 (それぞれ `[0.0, 1.0]`)。
///
/// `SemanticProfile::from_latent` で deterministic に生成される。
/// 同じ latent からは常に同じ profile が返る。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SemanticProfile {
    pub heat: f64,
    pub speed: f64,
    pub order: f64,
    pub chaos: f64,
    pub life: f64,
    pub machine: f64,
    pub dream: f64,
    pub violence: f64,
    pub luck: f64,
    pub purity: f64,
}

impl SemanticProfile {
    /// latent vector からタグ強度を抽出する。
    ///
    /// 各タグ `tag_i` は `latent[i]` を `(x + 1) / 2` で `[0, 1]` に正規化する。
    /// `latent` の各次元は `[-1, 1]` の deterministic 擬似乱数なので、出力タグ強度も
    /// 同じ latent からは常に同じ値になる。
    pub fn from_latent(latent: &LatentVector) -> Self {
        // ALL の順序 = フィールド順序 = dim 0..10 の順序。
        // map で値を一つずつ取り出して構築する。
        let v = |i: usize| {
            debug_assert!(i < LATENT_DIM);
            let normalized = ((latent[i] as f64) + 1.0) * 0.5;
            normalized.clamp(0.0, 1.0)
        };
        Self {
            heat: v(0),
            speed: v(1),
            order: v(2),
            chaos: v(3),
            life: v(4),
            machine: v(5),
            dream: v(6),
            violence: v(7),
            luck: v(8),
            purity: v(9),
        }
    }

    /// Curion から SemanticProfile を導出する (`source_guid` を seed とする)。
    ///
    /// `Curion::source_guid.as_bytes()` を [`latent_from_seed`] に通し、得られた
    /// latent vector を [`SemanticProfile::from_latent`] に渡す。
    /// curion 生成パイプラインと同じ seed を使うので、同じ source_guid を持つ
    /// 別の Curion (= 内部的に同じ curion) からは必ず同じ profile が返る。
    pub fn from_curion(curion: &Curion) -> Self {
        let latent = latent_from_seed(curion.source_guid.as_bytes());
        Self::from_latent(&latent)
    }

    /// 指定タグの強度を取得する。
    pub fn get(&self, tag: SemanticTag) -> f64 {
        match tag {
            SemanticTag::Heat => self.heat,
            SemanticTag::Speed => self.speed,
            SemanticTag::Order => self.order,
            SemanticTag::Chaos => self.chaos,
            SemanticTag::Life => self.life,
            SemanticTag::Machine => self.machine,
            SemanticTag::Dream => self.dream,
            SemanticTag::Violence => self.violence,
            SemanticTag::Luck => self.luck,
            SemanticTag::Purity => self.purity,
        }
    }

    /// 強度が高い順に最大 `top_n` 個のタグを返す。
    ///
    /// 同点は `SemanticTag::ALL` の順 (= enum 宣言順) で先に来たものが残る (stable sort)。
    /// `top_n == 0` のときは空 Vec。
    pub fn dominant_tags(&self, top_n: usize) -> Vec<(SemanticTag, f64)> {
        let mut pairs: Vec<(SemanticTag, f64)> =
            SemanticTag::ALL.iter().map(|&t| (t, self.get(t))).collect();
        // stable sort descending by f64 score
        pairs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        pairs.truncate(top_n);
        pairs
    }

    /// レーダー風の表示行を返す (Collection 詳細ペイン用)。
    ///
    /// 各タグについて `★★★` (0.66 以上) / `★★` (0.33 以上) / `★` (それ未満) の
    /// 3 段階バッジを付ける。dominant 順 (強い順) に並べる。
    /// 現状の UI は dominant_tags(3) を直接組み立てているのでここは未使用だが、
    /// 将来 Stats タブ等で全タグレーダー表示する余地として残す。
    #[allow(dead_code, clippy::wrong_self_convention)]
    pub fn to_radar_lines(&self) -> Vec<String> {
        self.dominant_tags(SemanticTag::ALL.len())
            .into_iter()
            .map(|(tag, score)| {
                let stars = if score >= 0.66 {
                    "★★★"
                } else if score >= 0.33 {
                    "★★ "
                } else {
                    "★  "
                };
                format!("{} {}", stars, tag.label())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::latent::latent_from_seed;

    /// 各タグ強度が `[0.0, 1.0]` の範囲に収まる (latent の `[-1, 1]` → `[0, 1]` 写像)。
    #[test]
    fn test_profile_from_latent_in_unit_range() {
        let seeds: [&[u8]; 5] = [b"a", b"hello", b"the long seed", &[0u8; 16], &[0xFFu8; 16]];
        for seed in seeds {
            let latent = latent_from_seed(seed);
            let profile = SemanticProfile::from_latent(&latent);
            for tag in SemanticTag::ALL {
                let v = profile.get(tag);
                assert!(
                    (0.0..=1.0).contains(&v),
                    "tag {tag:?} out of range: {v} (seed={seed:?})"
                );
            }
        }
    }

    /// 同じ latent からは常に同じ profile が返る (deterministic)。
    #[test]
    fn test_profile_deterministic_from_same_latent() {
        let latent = latent_from_seed(b"hello world");
        let a = SemanticProfile::from_latent(&latent);
        let b = SemanticProfile::from_latent(&latent);
        assert_eq!(a, b);
    }

    /// `dominant_tags(N)` は強い順に N 個返す。
    #[test]
    fn test_dominant_tags_returns_top_n() {
        // 全タグ強度を手動で組んだ profile で検証
        let profile = SemanticProfile {
            heat: 0.9,
            speed: 0.1,
            order: 0.5,
            chaos: 0.8,
            life: 0.2,
            machine: 0.3,
            dream: 0.7,
            violence: 0.4,
            luck: 0.6,
            purity: 0.0,
        };
        let top3 = profile.dominant_tags(3);
        assert_eq!(top3.len(), 3);
        // 期待: heat (0.9) > chaos (0.8) > dream (0.7)
        assert_eq!(top3[0].0, SemanticTag::Heat);
        assert_eq!(top3[1].0, SemanticTag::Chaos);
        assert_eq!(top3[2].0, SemanticTag::Dream);

        // top_n = 0 は空
        assert!(profile.dominant_tags(0).is_empty());

        // top_n > 10 は最大 10 個に丸める
        let all = profile.dominant_tags(99);
        assert_eq!(all.len(), 10);
    }

    /// Curion から導出した profile も deterministic である (source_guid に依存)。
    #[test]
    fn test_profile_from_curion_deterministic() {
        use crate::curion::{Category, Curion, Rarity};
        use uuid::Uuid;
        let guid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let c1 = Curion::new(
            guid,
            "魚".to_string(),
            Category::Animal,
            Rarity::Common,
            0.5,
            0.5,
        );
        let c2 = Curion::new(
            guid,
            "別名".to_string(), // noun が違っても source_guid が同じなら profile も同じ
            Category::Concept,
            Rarity::Legendary,
            0.1,
            0.9,
        );
        assert_eq!(
            SemanticProfile::from_curion(&c1),
            SemanticProfile::from_curion(&c2)
        );
    }

    /// to_radar_lines は dominant 順、各行に `★` 表現とタグラベルを含む。
    #[test]
    fn test_to_radar_lines_format() {
        let profile = SemanticProfile {
            heat: 0.9,  // ★★★
            speed: 0.5, // ★★
            order: 0.1, // ★
            chaos: 0.0,
            life: 0.0,
            machine: 0.0,
            dream: 0.0,
            violence: 0.0,
            luck: 0.0,
            purity: 0.0,
        };
        let lines = profile.to_radar_lines();
        assert_eq!(lines.len(), 10);
        // 1 行目は heat (最強)、3 つ星
        assert!(lines[0].contains("熱"));
        assert!(lines[0].starts_with("★★★"));
        // 2 行目は speed、2 つ星
        assert!(lines[1].contains("速度"));
        assert!(lines[1].starts_with("★★ "));
        // 3 行目は order、1 つ星
        assert!(lines[2].contains("秩序"));
        assert!(lines[2].starts_with("★  "));
    }
}
