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
    /// SF 寓話風のフレーバーテキスト（Issue #22）。
    /// 既存 JSON との後方互換性のため `#[serde(default)]` で省略可能。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flavor: Option<String>,
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

    /// 名詞名から該当する NounEntry を検索する（フレーバー参照用）
    pub fn find_entry(&self, noun_name: &str) -> Option<&NounEntry> {
        self.entries
            .values()
            .flat_map(|v| v.iter())
            .find(|e| e.name == noun_name)
    }

    /// 名詞名から該当するフレーバーテキストを取得する（未設定なら None）
    pub fn flavor_for(&self, noun_name: &str) -> Option<&str> {
        self.find_entry(noun_name).and_then(|e| e.flavor.as_deref())
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
        self.generate_with_bonus(guid, 0.0)
    }

    /// GUIDからキュリオンを生成し、`bonus_progress` (0.0..=1.0) に応じてレア確率を引き上げる。
    ///
    /// Issue #25: レア出現予告クールダウンが満ちると `bonus_progress == 1.0` になり、
    /// レアリティ判定の roll 値を最大 0.3 だけ引き下げる (= レア以上に寄せる) ことで、
    /// 「収集後 X 時間でレア出現率が段階的に上がる」体験を提供する。
    ///
    /// `bonus_progress == 0.0` のときは [`generate_from_guid`] と完全に同じ Curion を返す
    /// (deterministic, 後方互換)。
    pub fn generate_with_bonus(&self, guid: Uuid, bonus_progress: f64) -> Result<Curion> {
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
        let rarity = self.determine_rarity_with_bonus(rarity_seed, bonus_progress.clamp(0.0, 1.0));

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

    /// シード値からレアリティを決定 (旧 API、テスト互換のため残置)
    #[allow(dead_code)]
    fn determine_rarity_from_seed(&self, seed: u32) -> Rarity {
        self.determine_rarity_with_bonus(seed, 0.0)
    }

    /// シード値 + クールダウンボーナス進捗からレアリティを決定する。
    ///
    /// `bonus_progress == 0.0` は従来挙動 (`determine_rarity_from_seed` と一致)。
    /// `bonus_progress > 0.0` のとき roll 値から `bonus_progress * 0.3` を引いて
    /// レア以上に押し上げる。`bonus_progress == 1.0` のとき最大 -0.3 シフト。
    fn determine_rarity_with_bonus(&self, seed: u32, bonus_progress: f64) -> Rarity {
        // シードを0.0〜1.0に正規化
        let mut roll = (seed as f64) / (u32::MAX as f64);

        // クールダウンボーナス: roll を引き下げることでレア以上の累積確率帯に入りやすくする。
        // 累積確率は `Legendary -> Epic -> Rare -> Common` の順に判定するので、
        // roll が小さいほど高レアリティが出る。最大 -0.3 で十分にレアが伸びる。
        let shift = bonus_progress.clamp(0.0, 1.0) * 0.3;
        roll = (roll - shift).max(0.0);

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

    /// Issue #22: 全 268 名詞にフレーバーテキストが付与されていることを確認。
    /// 空でなく、必ず日本語句点 `。` で終わる。
    #[test]
    fn test_flavor_field_loaded_for_all_nouns() {
        let db = NounDatabase::load_embedded().expect("Failed to load noun database");

        let mut total = 0usize;
        for (category, nouns) in &db.entries {
            for entry in nouns {
                total += 1;
                let flavor = entry
                    .flavor
                    .as_ref()
                    .unwrap_or_else(|| panic!("{category:?}/{} has no flavor", entry.name));
                assert!(
                    !flavor.is_empty(),
                    "{category:?}/{} flavor is empty",
                    entry.name
                );
                assert!(
                    flavor.ends_with('。'),
                    "{category:?}/{} flavor does not end with 。: {flavor:?}",
                    entry.name,
                );
            }
        }
        assert_eq!(total, 268, "Expected 268 nouns total, got {total}");
    }

    /// Issue #22: 既存 JSON（flavor フィールド無し）でも互換性を保つ。
    #[test]
    fn test_flavor_field_optional_for_serde_compat() {
        let json = r#"{
            "name": "テスト",
            "reading": "てすと",
            "english": "test",
            "weight": 1.0
        }"#;

        let entry: NounEntry =
            serde_json::from_str(json).expect("Failed to deserialize legacy NounEntry");
        assert_eq!(entry.name, "テスト");
        assert!(
            entry.flavor.is_none(),
            "flavor should default to None when omitted"
        );
    }

    /// Issue #22: flavor_for ヘルパは存在しない名詞には None を返す。
    #[test]
    fn test_flavor_for_helper() {
        let db = NounDatabase::load_embedded().expect("Failed to load noun database");
        assert!(db.flavor_for("魚").is_some(), "魚 should have a flavor");
        assert!(db.flavor_for("__not_exist__").is_none());
    }

    // -----------------------------------------------------------------
    // Issue #25 レア出現予告クールダウン (generate_with_bonus)
    // -----------------------------------------------------------------

    /// `bonus_progress = 0.0` のとき、`generate_with_bonus` は `generate_from_guid` と
    /// 完全に同じ Curion を返す (deterministic, 後方互換)。
    #[test]
    fn test_generate_with_bonus_zero_matches_existing_generate_from_guid() {
        let generator = CurionGenerator::new().expect("Failed to load noun database");
        for s in &[
            "550e8400-e29b-41d4-a716-446655440000",
            "00000000-0000-0000-0000-000000000001",
            "ffffffff-ffff-4fff-bfff-ffffffffffff",
            "12345678-1234-4234-9234-123456789abc",
        ] {
            let guid = Uuid::parse_str(s).unwrap();
            let a = generator.generate_from_guid(guid).unwrap();
            let b = generator.generate_with_bonus(guid, 0.0).unwrap();
            assert_eq!(a.noun, b.noun);
            assert_eq!(a.category, b.category);
            assert_eq!(a.rarity, b.rarity);
            assert!((a.interest - b.interest).abs() < 1e-12);
            assert!((a.beauty - b.beauty).abs() < 1e-12);
        }
    }

    /// `bonus_progress` を 0.0 → 1.0 と上げると、サンプル全体で
    /// レア以上の出現割合が単調 (非減少) に増える。
    ///
    /// 個別 GUID では「同じレアリティのまま」もあり得るが、
    /// 1000 サンプル単位で見ればレア確率の引き上げが効いていることを
    /// 反映できる (= 「Common→Rare/Epic 化することがある」)。
    #[test]
    fn test_generate_with_bonus_higher_progress_shifts_rarity() {
        let generator = CurionGenerator::new().expect("Failed to load noun database");

        let count_non_common = |progress: f64| -> usize {
            (0..1000u32)
                .filter(|i| {
                    // 決定論的な GUID を生成 (sha256 から uuid_v4 を作る代わりに
                    // 連番を埋める)
                    let mut bytes = [0u8; 16];
                    bytes[..4].copy_from_slice(&i.to_le_bytes());
                    let guid = Uuid::from_bytes(bytes);
                    let c = generator.generate_with_bonus(guid, progress).unwrap();
                    !matches!(c.rarity, Rarity::Common)
                })
                .count()
        };

        let zero = count_non_common(0.0);
        let half = count_non_common(0.5);
        let full = count_non_common(1.0);

        assert!(
            zero <= half,
            "progress 0.0 ({zero}) <= 0.5 ({half}) のレア以上数"
        );
        assert!(
            half <= full,
            "progress 0.5 ({half}) <= 1.0 ({full}) のレア以上数"
        );
        // 最終ボーナスは「明確に」効くこと。0.3 シフトで Common の確率帯がほぼ削れる。
        assert!(
            full > zero,
            "progress 1.0 のレア以上数 ({full}) > 0.0 のレア以上数 ({zero})"
        );
    }
}
