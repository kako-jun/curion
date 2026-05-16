use crate::curion::{Category, Curion, Rarity};
use anyhow::{Context, Result};
use rand::distributions::{Distribution, WeightedIndex};
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use uuid::Uuid;

// コンパイル時にデータをバイナリに埋め込む
const NOUNS_ANIMALS: &str = include_str!("../data/nouns/animals.json");
const NOUNS_PLANTS: &str = include_str!("../data/nouns/plants.json");
const NOUNS_COLORS: &str = include_str!("../data/nouns/colors.json");
const NOUNS_OBJECTS: &str = include_str!("../data/nouns/objects.json");
const NOUNS_CONCEPTS: &str = include_str!("../data/nouns/concepts.json");
const NOUNS_ELEMENTS: &str = include_str!("../data/nouns/elements.json");
const NOUNS_FOODS: &str = include_str!("../data/nouns/foods.json");
const NOUNS_PHENOMENA: &str = include_str!("../data/nouns/phenomena.json");
const NOUNS_ABSTRACTS: &str = include_str!("../data/nouns/abstracts.json");

/// 名詞エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NounEntry {
    pub name: String,
    pub reading: String,
    pub english: String,
    pub weight: f64,
}

/// 名詞データベース
pub struct NounDatabase {
    entries: HashMap<Category, Vec<NounEntry>>,
}

impl NounDatabase {
    /// 埋め込みデータから名詞データベースを構築
    pub fn load_embedded() -> Result<Self> {
        let mut entries = HashMap::new();

        let embedded: &[(Category, &str)] = &[
            (Category::Animal, NOUNS_ANIMALS),
            (Category::Plant, NOUNS_PLANTS),
            (Category::Color, NOUNS_COLORS),
            (Category::Object, NOUNS_OBJECTS),
            (Category::Concept, NOUNS_CONCEPTS),
            (Category::Element, NOUNS_ELEMENTS),
            (Category::Food, NOUNS_FOODS),
            (Category::Phenomenon, NOUNS_PHENOMENA),
            (Category::Abstract, NOUNS_ABSTRACTS),
        ];

        for (category, content) in embedded {
            let nouns: Vec<NounEntry> = serde_json::from_str(content)
                .with_context(|| format!("Failed to parse noun data for {category:?}"))?;

            if nouns.is_empty() {
                anyhow::bail!("No nouns found for {category:?}");
            }

            entries.insert(category.clone(), nouns);
        }

        Ok(Self { entries })
    }

    /// カテゴリに対応する名詞リストを取得
    pub fn get_nouns(&self, category: &Category) -> Option<&Vec<NounEntry>> {
        self.entries.get(category)
    }

    /// 統計情報を取得
    #[cfg(test)]
    pub fn stats(&self) -> HashMap<Category, usize> {
        self.entries
            .iter()
            .map(|(cat, nouns)| (cat.clone(), nouns.len()))
            .collect()
    }
}

/// キュリオン生成器
pub struct CurionGenerator {
    noun_db: NounDatabase,
}

impl CurionGenerator {
    /// 埋め込みデータから生成器を作成
    pub fn new() -> Result<Self> {
        let noun_db = NounDatabase::load_embedded()?;
        Ok(Self { noun_db })
    }

    /// GUIDからキュリオンを生成（バーコードバトラー的）
    ///
    /// マッピングルール：
    /// - ハッシュの0バイト目: カテゴリ決定
    /// - ハッシュの1〜4バイト目: レアリティ決定
    /// - ハッシュの5〜8バイト目: 名詞インデックス決定（重み付き）
    /// - ハッシュの9〜12バイト目: 興味度
    /// - ハッシュの13〜16バイト目: 美しさ
    pub fn generate_from_guid(&self, guid: Uuid) -> Result<Curion> {
        // GUIDをSHA-256でハッシュ化
        let mut hasher = Sha256::new();
        hasher.update(guid.as_bytes());
        let hash_result = hasher.finalize();

        // カテゴリを決定（ハッシュの0バイト目）
        let category = self.determine_category_from_hash(hash_result[0]);

        // レアリティを決定（ハッシュの1〜4バイト目）
        let rarity_seed = u32::from_le_bytes([
            hash_result[1],
            hash_result[2],
            hash_result[3],
            hash_result[4],
        ]);
        let rarity = self.determine_rarity_from_seed(rarity_seed);

        // 名詞を選択（ハッシュの5〜8バイト目、重み付き）
        let noun_seed = u32::from_le_bytes([
            hash_result[5],
            hash_result[6],
            hash_result[7],
            hash_result[8],
        ]);
        let noun = self.select_noun_from_category(&category, noun_seed)?;

        // 属性値を生成（ハッシュの9〜16バイト目）
        let interest_seed = u32::from_le_bytes([
            hash_result[9],
            hash_result[10],
            hash_result[11],
            hash_result[12],
        ]);
        let interest = (interest_seed as f64) / (u32::MAX as f64);

        let beauty_seed = u32::from_le_bytes([
            hash_result[13],
            hash_result[14],
            hash_result[15],
            hash_result[16],
        ]);
        let beauty = (beauty_seed as f64) / (u32::MAX as f64);

        Ok(Curion::new(guid, noun, category, rarity, interest, beauty))
    }

    /// ハッシュバイトからカテゴリを決定
    fn determine_category_from_hash(&self, hash_byte: u8) -> Category {
        let categories = [
            Category::Animal,
            Category::Plant,
            Category::Color,
            Category::Object,
            Category::Concept,
            Category::Element,
            Category::Food,
            Category::Phenomenon,
            Category::Abstract,
        ];

        let index = (hash_byte % 9) as usize;
        categories[index].clone()
    }

    /// シード値からレアリティを決定
    fn determine_rarity_from_seed(&self, seed: u32) -> Rarity {
        // シードを0.0〜1.0に正規化
        let roll = (seed as f64) / (u32::MAX as f64);

        // 累積確率でレアリティを決定
        let mut cumulative = 0.0;
        for rarity in &[
            Rarity::Legendary,
            Rarity::Epic,
            Rarity::Rare,
            Rarity::Common,
        ] {
            cumulative += rarity.probability();
            if roll < cumulative {
                return *rarity;
            }
        }

        Rarity::Common
    }

    /// カテゴリから名詞を選択（重み付き）
    fn select_noun_from_category(&self, category: &Category, seed: u32) -> Result<String> {
        let nouns = self
            .noun_db
            .get_nouns(category)
            .context("Category not found in noun database")?;

        if nouns.is_empty() {
            anyhow::bail!("No nouns available for category {category:?}");
        }

        // 重みの配列を作成
        let weights: Vec<f64> = nouns.iter().map(|n| n.weight).collect();

        // シードからRNGを作成
        let mut rng = StdRng::seed_from_u64(seed as u64);

        // 重み付き選択
        let dist =
            WeightedIndex::new(&weights).context("Failed to create weighted distribution")?;
        let index = dist.sample(&mut rng);

        Ok(nouns[index].name.clone())
    }

    /// 名詞データベースへの参照を取得
    pub fn database(&self) -> &NounDatabase {
        &self.noun_db
    }

    /// 統計情報を取得
    #[cfg(test)]
    pub fn database_stats(&self) -> HashMap<Category, usize> {
        self.noun_db.stats()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_curion() {
        let generator = CurionGenerator::new().expect("Failed to load noun database");
        let guid = Uuid::new_v4();
        let curion = generator
            .generate_from_guid(guid)
            .expect("Failed to generate curion");

        assert_eq!(curion.source_guid, guid);
        assert!(!curion.noun.is_empty());
        assert!(curion.interest >= 0.0 && curion.interest <= 1.0);
        assert!(curion.beauty >= 0.0 && curion.beauty <= 1.0);
    }

    #[test]
    fn test_deterministic_generation() {
        let generator = CurionGenerator::new().expect("Failed to load noun database");
        let guid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();

        let curion1 = generator
            .generate_from_guid(guid)
            .expect("Failed to generate curion 1");
        let curion2 = generator
            .generate_from_guid(guid)
            .expect("Failed to generate curion 2");

        // 同じGUIDからは同じキュリオンが生成される
        assert_eq!(curion1.noun, curion2.noun);
        assert_eq!(curion1.category, curion2.category);
        assert_eq!(curion1.rarity, curion2.rarity);
        assert_eq!(curion1.interest, curion2.interest);
        assert_eq!(curion1.beauty, curion2.beauty);
    }

    #[test]
    fn test_database_stats() {
        let generator = CurionGenerator::new().expect("Failed to load noun database");
        let stats = generator.database_stats();

        // 全カテゴリにデータが存在することを確認
        assert!(!stats.is_empty());
        for (category, count) in stats {
            assert!(count > 0, "Category {category:?} has no nouns");
        }
    }
}
