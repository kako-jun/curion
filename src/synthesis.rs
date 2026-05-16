use crate::curion::{Category, Curion, Rarity};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

const RECIPES_JSON: &str = include_str!("../data/recipes/basic_recipes.json");

/// 合成レシピ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisRecipe {
    pub id: String,
    pub name: String,
    pub description: String,
    pub ingredients: Vec<IngredientRequirement>,
    pub result: SynthesisResult,
    pub discovery_rate: f64, // 初見成功率（0.0〜1.0）
    pub recipe_type: RecipeType,
}

impl SynthesisRecipe {
    /// レシピの成功確率を返す (Issue #28)
    ///
    /// - `is_discovered == false`: 初見成功率 (`discovery_rate`) をそのまま返す
    /// - `is_discovered == true`: 100% (1.0) — 発見済みレシピは確定成功
    ///
    /// 計算はロジック層に閉じており、UI 層は本メソッドを呼ぶだけで
    /// 表示用の確率を得られる。
    pub fn success_probability(&self, is_discovered: bool) -> f64 {
        if is_discovered {
            1.0
        } else {
            self.discovery_rate.clamp(0.0, 1.0)
        }
    }
}

/// レシピタイプ
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecipeType {
    Intuitive,  // 直感的（水+火→蒸気）
    Conceptual, // 概念的（夢+光→希望）
    Biological, // 生物的（狼+月→月光狼）
    Cooking,    // 料理系（米+火→ご飯）
    Abstract,   // 抽象概念（愛+美→美愛）
    ChaosMix,   // ごった煮（猫+愛→愛猫）
    Advanced,   // 複雑（3つ以上の材料）
}

/// 材料要求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngredientRequirement {
    /// 特定の名詞を要求（例: "水"）
    pub specific_noun: Option<String>,
    /// カテゴリで要求（例: Category::Animal）
    pub category: Option<Category>,
    /// レアリティで要求（例: Rarity::Rare）
    pub rarity: Option<Rarity>,
    /// 必要な個数
    pub count: usize,
}

impl IngredientRequirement {
    /// 指定されたキュリオンが要件を満たすか
    pub fn matches(&self, curion: &Curion) -> bool {
        // 特定の名詞が指定されている場合
        if let Some(ref noun) = self.specific_noun {
            if &curion.noun != noun {
                return false;
            }
        }

        // カテゴリが指定されている場合
        if let Some(ref category) = self.category {
            if &curion.category != category {
                return false;
            }
        }

        // レアリティが指定されている場合
        if let Some(ref rarity) = self.rarity {
            if &curion.rarity != rarity {
                return false;
            }
        }

        true
    }
}

/// 合成結果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynthesisResult {
    pub noun: String,
    pub category: Category,
    pub rarity: Rarity,
    pub synthesis_only: bool, // 合成限定かどうか
    pub special_attributes: HashMap<String, f64>,
}

impl SynthesisResult {
    /// 合成結果からキュリオンを生成
    pub fn to_curion(&self) -> Curion {
        let guid = Uuid::new_v4();
        let mut curion = Curion::new(
            guid,
            self.noun.clone(),
            self.category.clone(),
            self.rarity,
            0.8, // 合成品は高い興味度
            0.8, // 合成品は高い美しさ
        );

        // 特別な属性があればinterestやbeautyに加算
        if let Some(interest_bonus) = self.special_attributes.get("interest") {
            curion.interest = (curion.interest + interest_bonus).min(1.0);
        }
        if let Some(beauty_bonus) = self.special_attributes.get("beauty") {
            curion.beauty = (curion.beauty + beauty_bonus).min(1.0);
        }

        curion
    }
}

/// レシピデータベース
#[derive(Debug)]
pub struct RecipeDatabase {
    recipes: Vec<SynthesisRecipe>,
}

impl RecipeDatabase {
    /// 埋め込みデータからレシピデータベースを構築
    pub fn load_embedded() -> Result<Self> {
        let recipes: Vec<SynthesisRecipe> =
            serde_json::from_str(RECIPES_JSON).context("Failed to parse embedded recipe data")?;
        Ok(Self { recipes })
    }

    /// 全レシピを取得
    pub fn all_recipes(&self) -> &[SynthesisRecipe] {
        &self.recipes
    }

    /// 指定された材料で作れるレシピを検索
    pub fn find_matching_recipes(&self, ingredients: &[Curion]) -> Vec<&SynthesisRecipe> {
        self.recipes
            .iter()
            .filter(|recipe| self.can_synthesize(recipe, ingredients))
            .collect()
    }

    /// レシピが作成可能かチェック
    fn can_synthesize(&self, recipe: &SynthesisRecipe, available: &[Curion]) -> bool {
        let mut remaining = available.to_vec();

        for requirement in &recipe.ingredients {
            let mut found_count = 0;

            // 要件を満たすキュリオンを探す
            remaining.retain(|curion| {
                if found_count < requirement.count && requirement.matches(curion) {
                    found_count += 1;
                    false // このキュリオンは使用済みなので削除
                } else {
                    true // 残す
                }
            });

            // 必要な数が揃わなかった
            if found_count < requirement.count {
                return false;
            }
        }

        true
    }
}

/// 合成マネージャー
#[derive(Debug)]
pub struct SynthesisManager {
    recipe_db: RecipeDatabase,
    discovered_recipes: HashMap<String, bool>, // recipe_id -> discovered
}

impl SynthesisManager {
    /// 新しい合成マネージャーを作成
    pub fn new(recipe_db: RecipeDatabase) -> Self {
        Self {
            recipe_db,
            discovered_recipes: HashMap::new(),
        }
    }

    /// レシピが発見済みか
    pub fn is_discovered(&self, recipe_id: &str) -> bool {
        self.discovered_recipes
            .get(recipe_id)
            .copied()
            .unwrap_or(false)
    }

    /// レシピを発見済みにする
    pub fn discover_recipe(&mut self, recipe_id: String) {
        self.discovered_recipes.insert(recipe_id, true);
    }

    /// 合成を試みる
    pub fn try_synthesize(&mut self, ingredients: Vec<Curion>) -> Result<SynthesisAttemptResult> {
        // マッチするレシピを検索
        let matching_recipes = self.recipe_db.find_matching_recipes(&ingredients);

        if matching_recipes.is_empty() {
            return Ok(SynthesisAttemptResult::NoRecipe);
        }

        // 複数マッチした場合は最初のレシピを使用
        let recipe = matching_recipes[0];

        // 必要なデータを先にコピー
        let recipe_id = recipe.id.clone();
        let recipe_name = recipe.name.clone();
        let discovery_rate = recipe.discovery_rate;
        let result_curion = recipe.result.to_curion();

        // 発見済みか確認
        let is_discovered = self.is_discovered(&recipe_id);

        // 未発見の場合、discovery_rateで成功判定
        if !is_discovered {
            let roll: f64 = rand::random();
            if roll > discovery_rate {
                return Ok(SynthesisAttemptResult::DiscoveryFailed {
                    hint: "何かが起こりそうだが、まだ完全には理解できていない...".to_string(),
                });
            }

            // 発見成功！
            self.discover_recipe(recipe_id);
        }

        // 合成成功
        Ok(SynthesisAttemptResult::Success {
            curion: result_curion,
            recipe_name,
            first_discovery: !is_discovered,
        })
    }

    /// 発見済みレシピの数
    pub fn discovered_count(&self) -> usize {
        self.discovered_recipes.values().filter(|&&v| v).count()
    }

    /// 全レシピ数
    pub fn total_recipe_count(&self) -> usize {
        self.recipe_db.all_recipes().len()
    }

    /// 発見状態を取得（セーブ用）
    pub fn get_discovered_state(&self) -> HashMap<String, bool> {
        self.discovered_recipes.clone()
    }

    /// 発見状態を設定（ロード用）
    pub fn set_discovered_state(&mut self, state: HashMap<String, bool>) {
        self.discovered_recipes = state;
    }

    /// 1つ目の材料から可能な2つ目の候補を検索
    pub fn find_possible_second_ingredients(
        &self,
        first: &Curion,
        available_curions: &[Curion],
    ) -> Vec<PossibleSecondIngredient> {
        let mut candidates: HashMap<String, PossibleSecondIngredient> = HashMap::new();

        // 全レシピをチェック
        for recipe in self.recipe_db.all_recipes() {
            // 2材料のレシピのみ対象
            if recipe.ingredients.len() != 2 {
                continue;
            }

            // 1つ目の材料がマッチするか確認
            let first_req = &recipe.ingredients[0];
            let second_req = &recipe.ingredients[1];

            let mut matched_as_first = false;
            let mut matched_as_second = false;
            let mut other_req = None;

            // 1つ目の要求として一致
            if first_req.matches(first) {
                matched_as_first = true;
                other_req = Some(second_req);
            }

            // 2つ目の要求として一致（対称的なレシピの場合）
            if second_req.matches(first) {
                matched_as_second = true;
                other_req = Some(first_req);
            }

            if !matched_as_first && !matched_as_second {
                continue;
            }

            let other_requirement = other_req.unwrap();

            // 利用可能なキュリオンからマッチするものを探す
            for curion in available_curions {
                // 自分自身は除外
                if curion.id == first.id {
                    continue;
                }

                if !other_requirement.matches(curion) {
                    continue;
                }

                // このキュリオンは候補
                let key = curion.noun.clone();
                let is_discovered = self.is_discovered(&recipe.id);

                candidates
                    .entry(key.clone())
                    .or_insert_with(|| PossibleSecondIngredient {
                        noun: key.clone(),
                        category: curion.category.clone(),
                        available_count: 0,
                        result_preview: if is_discovered {
                            Some(recipe.result.noun.clone())
                        } else {
                            None
                        },
                        is_discovered,
                    })
                    .available_count += 1;
            }
        }

        candidates.into_values().collect()
    }

    /// レシピデータベースへの参照を取得
    pub fn recipe_db(&self) -> &RecipeDatabase {
        &self.recipe_db
    }

    /// 指定レシピの成功確率を返す (Issue #28)
    ///
    /// `SynthesisRecipe::success_probability` の薄いラッパー。
    /// 発見状態を `SynthesisManager` 側で判定して返すので、
    /// 呼び出し側 (UI) は発見済みかどうかを意識せず確率を取れる。
    pub fn success_probability_for_recipe(&self, recipe: &SynthesisRecipe) -> f64 {
        let is_discovered = self.is_discovered(&recipe.id);
        recipe.success_probability(is_discovered)
    }
}

/// 可能な2つ目の材料候補
#[derive(Debug, Clone)]
pub struct PossibleSecondIngredient {
    pub noun: String,
    pub category: Category,
    pub available_count: usize,
    pub result_preview: Option<String>, // 発見済みなら結果、未発見ならNone
    pub is_discovered: bool,
}

/// 合成試行結果
#[derive(Debug)]
pub enum SynthesisAttemptResult {
    Success {
        curion: Curion,
        recipe_name: String,
        first_discovery: bool,
    },
    NoRecipe,
    DiscoveryFailed {
        hint: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ingredient_matching() {
        let curion = Curion::new(
            Uuid::new_v4(),
            "水".to_string(),
            Category::Element,
            Rarity::Common,
            0.5,
            0.5,
        );

        // 特定の名詞でマッチ
        let req1 = IngredientRequirement {
            specific_noun: Some("水".to_string()),
            category: None,
            rarity: None,
            count: 1,
        };
        assert!(req1.matches(&curion));

        // カテゴリでマッチ
        let req2 = IngredientRequirement {
            specific_noun: None,
            category: Some(Category::Element),
            rarity: None,
            count: 1,
        };
        assert!(req2.matches(&curion));

        // 不一致
        let req3 = IngredientRequirement {
            specific_noun: Some("火".to_string()),
            category: None,
            rarity: None,
            count: 1,
        };
        assert!(!req3.matches(&curion));
    }

    fn sample_recipe(discovery_rate: f64) -> SynthesisRecipe {
        SynthesisRecipe {
            id: "test_recipe".to_string(),
            name: "テストレシピ".to_string(),
            description: "for unit test".to_string(),
            ingredients: vec![],
            result: SynthesisResult {
                noun: "結果".to_string(),
                category: Category::Concept,
                rarity: Rarity::Rare,
                synthesis_only: true,
                special_attributes: HashMap::new(),
            },
            discovery_rate,
            recipe_type: RecipeType::Intuitive,
        }
    }

    /// Issue #28: 未発見レシピの成功確率は `discovery_rate` と一致する
    #[test]
    fn test_success_probability_undiscovered_matches_discovery_rate() {
        let recipe = sample_recipe(0.42);
        assert!((recipe.success_probability(false) - 0.42).abs() < 1e-9);
    }

    /// Issue #28: 発見済みレシピの成功確率は常に 1.0 (確定成功)
    #[test]
    fn test_success_probability_discovered_is_one() {
        let recipe = sample_recipe(0.10);
        assert!((recipe.success_probability(true) - 1.0).abs() < 1e-9);
    }

    /// Issue #28: SynthesisManager 経由でも確率が一貫
    #[test]
    fn test_manager_success_probability_for_recipe() {
        let recipe_db = RecipeDatabase::load_embedded().expect("recipes load");
        let mut manager = SynthesisManager::new(recipe_db);
        let recipe_id = manager.recipe_db().all_recipes()[0].id.clone();
        let recipe = manager.recipe_db().all_recipes()[0].clone();

        // 未発見: discovery_rate と一致
        let undiscovered_p = manager.success_probability_for_recipe(&recipe);
        assert!((undiscovered_p - recipe.discovery_rate).abs() < 1e-9);

        // 発見済み: 1.0
        manager.discover_recipe(recipe_id);
        let discovered_p = manager.success_probability_for_recipe(&recipe);
        assert!((discovered_p - 1.0).abs() < 1e-9);
    }
}
