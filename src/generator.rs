use crate::curion::{Category, Curion, Rarity};
use crate::latent::{
    cosine_similarity, latent_from_seed, project_unit, prototype_for_noun, LatentVector,
};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
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

    /// Issue #63: noun の Japanese ID から English 表示名を引く。
    ///
    /// `data/nouns/*.json` の各エントリは Phase 1 時点で全て `english` フィールドを
    /// 持つが、合成限定 noun (`蒸気` / `残骸` 等、データ JSON に存在しない名詞)
    /// は引けないため `None` を返す。呼び出し側は `None` のとき JA noun をそのまま
    /// 表示する fallback を取る (Phase 2 でこれらにも英訳を入れる予定)。
    pub fn english_for(&self, noun_name: &str) -> Option<&str> {
        self.find_entry(noun_name).map(|e| e.english.as_str())
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

/// latent から得た [0, 1] の roll 値とクールダウンボーナス進捗から Rarity を決定する。
///
/// `bonus_progress > 0.0` のとき roll 値から `bonus_progress * 0.3` を引いて
/// レア以上に押し上げる (Issue #25 の roll-shift モデル)。
/// 累積確率は `Legendary -> Epic -> Rare -> Common` の順に判定するので、
/// roll が小さいほど高レアリティが出る。
fn determine_rarity_with_bonus(roll_unit: f64, bonus_progress: f64) -> Rarity {
    let shift = bonus_progress.clamp(0.0, 1.0) * 0.3;
    let roll = (roll_unit - shift).max(0.0);

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

    /// GUIDからキュリオンを生成（バーコードバトラー的）。
    ///
    /// 内部的には [`generate_from_seed_bytes`] (Issue #39 の対称パイプライン)
    /// に委譲する。GUID のバイト列を seed として渡すだけで、latent vector
    /// 経由で noun/rarity/interest/beauty が決まる。
    ///
    /// [`generate_from_seed_bytes`]: Self::generate_from_seed_bytes
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
        self.generate_from_seed_bytes_with_bonus(guid.as_bytes(), guid, bonus_progress)
    }

    /// Issue #39: 任意の seed 文字列から Curion を生成する公開 API。
    ///
    /// 内部的に `seed.as_bytes()` を [`latent_from_seed`] に通し、
    /// 16 次元 latent vector を経由して noun/rarity/interest/beauty を導出する。
    ///
    /// `source_guid` は Curion 構造体の `source_guid` フィールドに記録される
    /// (Player の重複判定や履歴表示で使う)。seed と guid を別に渡せるのは、
    /// 将来「同じ seed 文字列を別の generation 履歴で複数回 Curion 化したい」
    /// (= ID は別で、内容は同じ) ような拡張のため。
    ///
    /// 現状ではテストとライブラリ的な公開 API としてのみ参照されている
    /// (P2P 受信や任意文字列入力からの Curion 化など、将来の呼び出し元の前段)。
    #[allow(dead_code)]
    pub fn generate_from_seed(&self, seed: &str, source_guid: Uuid) -> Result<Curion> {
        self.generate_from_seed_bytes_with_bonus(seed.as_bytes(), source_guid, 0.0)
    }

    /// Issue #39: 任意の seed バイト列から Curion を生成する主要 API。
    ///
    /// `seed_bytes` を hash → 16 次元 latent vector → nearest noun prototype の
    /// 対称パイプラインで Curion を導出する。bonus_progress は rarity の roll-shift
    /// にのみ作用する (Issue #25 の roll-shift モデルを latent パイプライン上で再現)。
    pub fn generate_from_seed_bytes_with_bonus(
        &self,
        seed_bytes: &[u8],
        source_guid: Uuid,
        bonus_progress: f64,
    ) -> Result<Curion> {
        let latent = latent_from_seed(seed_bytes);

        // カテゴリ: latent の dim 0 (= [-1, 1]) を [0, 1] に展開し 9 カテゴリに振り分け。
        let category = self.determine_category(&latent);

        // レアリティ: dims 1..4 を [0, 1] に投影した roll に bonus_progress を反映。
        let rarity_roll = project_unit(&latent, &[1, 2, 3, 4]);
        let rarity = determine_rarity_with_bonus(rarity_roll, bonus_progress.clamp(0.0, 1.0));

        // 名詞: latent 全 16 次元と category 内 noun prototype のコサイン類似度 × weight
        //       が最大になる noun を選ぶ (nearest-neighbor)。
        let noun = self.nearest_noun_in_category(&category, &latent)?;

        // interest / beauty: 直交する次元帯から投影。
        let interest = project_unit(&latent, &[8, 9, 10, 11]);
        let beauty = project_unit(&latent, &[12, 13, 14, 15]);

        Ok(Curion::new(
            source_guid,
            noun,
            category,
            rarity,
            interest,
            beauty,
        ))
    }

    /// latent vector から category を決定する。
    ///
    /// dim 0 を [0, 1] に展開して `(u * 9).floor()` で 9 カテゴリに振り分け。
    /// dim 0 = 1.0 (= u = 1.0) のとき index = 9 にならないよう min クランプ。
    fn determine_category(&self, latent: &LatentVector) -> Category {
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
        let u = project_unit(latent, &[0]);
        let idx = ((u * categories.len() as f64) as usize).min(categories.len() - 1);
        categories[idx].clone()
    }

    /// category 内の全 noun について cosine_similarity(latent, prototype(noun)) × weight
    /// を計算し、最大値の noun 名を返す (Issue #39 nearest-neighbor)。
    ///
    /// weight は noun データに既に入っている「出やすさ」を継続使用。
    /// similarity は [-1, 1] なので weight と素直に掛け算するとマイナス側で
    /// 順序が壊れる (weight が大きいほど不利になる)。`(sim + 1.0) * 0.5` で
    /// [0, 1] に正規化してから weight を掛ける。
    fn nearest_noun_in_category(
        &self,
        category: &Category,
        latent: &LatentVector,
    ) -> Result<String> {
        let nouns = self
            .noun_db
            .get_nouns(category)
            .context("Category not found in noun database")?;
        if nouns.is_empty() {
            anyhow::bail!("No nouns available for category {category:?}");
        }

        let mut best_idx = 0usize;
        let mut best_score = f64::NEG_INFINITY;
        for (i, entry) in nouns.iter().enumerate() {
            let proto = prototype_for_noun(&entry.name);
            let sim = cosine_similarity(latent, &proto) as f64;
            let sim_unit = (sim + 1.0) * 0.5;
            let score = sim_unit * entry.weight;
            if score > best_score {
                best_score = score;
                best_idx = i;
            }
        }
        Ok(nouns[best_idx].name.clone())
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

    // -----------------------------------------------------------------
    // Issue #39 latent vector pipeline (generate_from_seed*)
    // -----------------------------------------------------------------

    /// Issue #39: 同じ seed bytes (+ 同じ source_guid + 同じ bonus) からは
    /// 同じ Curion (noun / category / rarity / interest / beauty) が決定論的に生成される。
    #[test]
    fn test_generate_from_seed_bytes_deterministic() {
        let generator = CurionGenerator::new().expect("Failed to load noun database");
        let guid = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let seed = b"my fixed seed";

        let a = generator
            .generate_from_seed_bytes_with_bonus(seed, guid, 0.0)
            .unwrap();
        let b = generator
            .generate_from_seed_bytes_with_bonus(seed, guid, 0.0)
            .unwrap();

        assert_eq!(a.noun, b.noun);
        assert_eq!(a.category, b.category);
        assert_eq!(a.rarity, b.rarity);
        assert!((a.interest - b.interest).abs() < 1e-12);
        assert!((a.beauty - b.beauty).abs() < 1e-12);
    }

    /// Issue #39: `generate_from_seed(str, guid)` も同じ seed 文字列で deterministic。
    #[test]
    fn test_generate_from_seed_str_deterministic() {
        let generator = CurionGenerator::new().expect("Failed to load noun database");
        let guid = Uuid::parse_str("12345678-1234-4234-9234-123456789abc").unwrap();

        let a = generator.generate_from_seed("hello curion", guid).unwrap();
        let b = generator.generate_from_seed("hello curion", guid).unwrap();
        assert_eq!(a.noun, b.noun);
        assert_eq!(a.category, b.category);
        assert_eq!(a.rarity, b.rarity);

        // 別 seed では (基本的には) 別の結果になる。
        // 偶然 noun が一致するケースを許容するため、何かしらの属性で差を確認する。
        let c = generator
            .generate_from_seed("totally different", guid)
            .unwrap();
        let any_differ = a.noun != c.noun
            || a.category != c.category
            || a.rarity != c.rarity
            || (a.interest - c.interest).abs() > 1e-9
            || (a.beauty - c.beauty).abs() > 1e-9;
        assert!(
            any_differ,
            "different seeds should produce different Curions in some dimension"
        );
    }

    /// Issue #39: `generate_from_guid(guid)` は GUID のバイト列を seed として
    /// `generate_from_seed_bytes_with_bonus` に委譲する。両者の結果は完全一致する
    /// (= 新しい latent パイプライン経由で動いていることの検証)。
    #[test]
    fn test_generate_from_guid_uses_latent_pipeline() {
        let generator = CurionGenerator::new().expect("Failed to load noun database");
        for s in &[
            "550e8400-e29b-41d4-a716-446655440000",
            "00000000-0000-0000-0000-000000000001",
            "ffffffff-ffff-4fff-bfff-ffffffffffff",
        ] {
            let guid = Uuid::parse_str(s).unwrap();
            let via_guid = generator.generate_from_guid(guid).unwrap();
            let via_seed = generator
                .generate_from_seed_bytes_with_bonus(guid.as_bytes(), guid, 0.0)
                .unwrap();
            assert_eq!(via_guid.noun, via_seed.noun);
            assert_eq!(via_guid.category, via_seed.category);
            assert_eq!(via_guid.rarity, via_seed.rarity);
            assert!((via_guid.interest - via_seed.interest).abs() < 1e-12);
            assert!((via_guid.beauty - via_seed.beauty).abs() < 1e-12);
        }
    }

    /// Issue #39: latent パイプライン経由でも全 9 カテゴリが
    /// (1000 サンプル中) 少なくとも 1 回は出現する (= 偏りで完全に出ないカテゴリがない)。
    #[test]
    fn test_latent_pipeline_covers_all_categories() {
        let generator = CurionGenerator::new().expect("Failed to load noun database");
        let mut seen: std::collections::HashSet<Category> = std::collections::HashSet::new();
        for i in 0..1000u32 {
            let mut bytes = [0u8; 16];
            bytes[..4].copy_from_slice(&i.to_le_bytes());
            let guid = Uuid::from_bytes(bytes);
            let c = generator.generate_from_guid(guid).unwrap();
            seen.insert(c.category.clone());
        }
        assert_eq!(
            seen.len(),
            9,
            "all 9 categories should appear; got {seen:?}"
        );
    }

    /// Issue #39: latent パイプライン経由で生成された noun は、必ず
    /// その Curion の category に属する noun データベースエントリと一致する
    /// (= nearest-neighbor が「他カテゴリの noun を引いてしまう」事故を起こさない)。
    #[test]
    fn test_latent_pipeline_noun_belongs_to_its_category() {
        let generator = CurionGenerator::new().expect("Failed to load noun database");
        for i in 0..200u32 {
            let mut bytes = [0u8; 16];
            bytes[..4].copy_from_slice(&i.to_le_bytes());
            let guid = Uuid::from_bytes(bytes);
            let c = generator.generate_from_guid(guid).unwrap();
            let nouns = generator
                .noun_db
                .get_nouns(&c.category)
                .expect("category should be present");
            assert!(
                nouns.iter().any(|n| n.name == c.noun),
                "noun {} should be in category {:?}",
                c.noun,
                c.category
            );
        }
    }
}
