use crate::curion::{Category, Curion, Rarity};
use crate::i18n::Language;
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
    /// Issue #71 Phase 4: 英語版レシピ名。空文字なら `name` (JA) にフォールバックする。
    #[serde(default)]
    pub name_en: String,
    /// Issue #71 Phase 4: 英語版説明文。空文字なら `description` (JA) にフォールバックする。
    #[serde(default)]
    pub description_en: String,
    pub ingredients: Vec<IngredientRequirement>,
    pub result: SynthesisResult,
    pub discovery_rate: f64, // 初見成功率（0.0〜1.0）
    pub recipe_type: RecipeType,

    /// Issue #35: 発見済みレシピでも適用される「実行時成功率」。
    /// `discovery_rate` とは別軸で、`is_discovered == true` でも毎回 roll される。
    /// 既存レシピ (フィールド省略) は 1.0 (= 100% 成功) として扱われる。
    #[serde(default = "default_success_rate")]
    pub success_rate: f64,

    /// Issue #35: 失敗時にどう振る舞うか。デフォルトは `NoLoss` (保険) で、
    /// 既存レシピの挙動 (素材は消費しない / 副作用なし) と一致する。
    #[serde(default)]
    pub failure_mode: FailureMode,

    /// Issue #37: レシピの公開状態。プレイヤーがまだ発見していない時の
    /// 表示制御に使う。`#[serde(default)]` で `Public` 扱いになるので、
    /// 既存レシピ JSON は変更不要で完全公開のまま残る。
    ///
    /// - `Public`:  材料も結果も最初から完全に見える
    /// - `Partial`: 一部の材料だけ見える ("水 + ? → ?")
    /// - `Unknown`: 存在だけ分かる ("未確認レシピ #07")
    ///
    /// 発見済みになったレシピは `visibility` に関わらず常に完全表示する。
    #[serde(default)]
    pub visibility: RecipeVisibility,
}

/// Issue #37: レシピの公開状態。未発見時のヒント量を 3 段階で制御する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum RecipeVisibility {
    /// 材料も結果も最初から完全公開。既存レシピの挙動。
    #[default]
    Public,
    /// 材料の一部だけ見える。最低 1 つの材料は名前表示され、残りと結果は `?` で隠す。
    Partial,
    /// 存在しか分からない。全てが `???`、レシピ自体は番号で識別。
    Unknown,
}

/// Issue #37: プレイヤーが手元のキュリオンでこのレシピの材料要件を
/// どこまで満たしているかを表す進捗。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IngredientProgress {
    /// 充足した `IngredientRequirement` の数
    pub satisfied: usize,
    /// 必要な `IngredientRequirement` の数
    pub total: usize,
    /// 全要件を満たしているか (= `satisfied == total`)
    pub all_satisfied: bool,
}

fn default_success_rate() -> f64 {
    1.0
}

/// Issue #35: 高リスク合成のリスクしきい値。
/// `success_rate` がこれより小さい場合に "RISKY" として扱う。
pub const HIGH_RISK_THRESHOLD: f64 = 0.95;

/// Issue #35: 失敗時の挙動パターン。
/// `Default = NoLoss` で既存レシピ互換 (失敗ロスなし) を担保する。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(tag = "type")]
pub enum FailureMode {
    /// 何も失わない (保険)。素材は手元に残り、結果も出ない。
    #[default]
    NoLoss,
    /// 素材を全て失う (デフォルトの高リスク失敗)。
    LoseAll,
    /// 素材を失い、代わりに残骸 curion を 1 個得る。
    /// `fallback_rarity` のレアリティで、材料のうち最初のものを名詞元として残骸を生成する。
    Salvage { fallback_rarity: Rarity },
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
        let base = if is_discovered {
            1.0
        } else {
            self.discovery_rate.clamp(0.0, 1.0)
        };
        // Issue #35: 発見済みでも success_rate < 1.0 なら毎回 roll する。
        // 表示用の確率は discovery と success_rate の AND (積) で表現する。
        base * self.success_rate.clamp(0.0, 1.0)
    }

    /// Issue #35: 高リスクレシピ (`success_rate < HIGH_RISK_THRESHOLD`) か判定する。
    pub fn is_high_risk(&self) -> bool {
        self.success_rate < HIGH_RISK_THRESHOLD
    }

    /// Issue #37: プレイヤーの所持キュリオン `collection` で、このレシピの
    /// 材料要件を何個満たしているかを返す。`count > 1` の要件も「個数まで揃って 1 充足」とカウントする。
    ///
    /// 内部実装は `RecipeDatabase::can_synthesize` と同じ材料一致ロジックを使うが、
    /// 「途中で打ち切らずに全要件を見る」点だけが異なる。
    pub fn ingredient_progress(&self, collection: &[Curion]) -> IngredientProgress {
        let mut remaining = collection.to_vec();
        let total = self.ingredients.len();
        let mut satisfied = 0;

        for requirement in &self.ingredients {
            let mut found_count = 0;
            remaining.retain(|curion| {
                if found_count < requirement.count && requirement.matches(curion) {
                    found_count += 1;
                    false
                } else {
                    true
                }
            });
            if found_count >= requirement.count {
                satisfied += 1;
            }
        }

        IngredientProgress {
            satisfied,
            total,
            all_satisfied: satisfied == total && total > 0,
        }
    }

    /// Issue #37: あと何種類の材料要件を満たせば合成できるかを返す。
    /// (= `total - satisfied`)
    pub fn remaining_categories(&self, collection: &[Curion]) -> usize {
        let p = self.ingredient_progress(collection);
        p.total.saturating_sub(p.satisfied)
    }

    /// Issue #71 Phase 4: 言語別のレシピ名。`name_en` が空なら JA `name` にフォールバック。
    pub fn name_for(&self, lang: Language) -> &str {
        match lang {
            Language::Ja => &self.name,
            Language::En => {
                if self.name_en.is_empty() {
                    &self.name
                } else {
                    &self.name_en
                }
            }
        }
    }

    /// Issue #71 Phase 4: 言語別のレシピ説明。`description_en` が空なら JA にフォールバック。
    pub fn description_for(&self, lang: Language) -> &str {
        match lang {
            Language::Ja => &self.description,
            Language::En => {
                if self.description_en.is_empty() {
                    &self.description
                } else {
                    &self.description_en
                }
            }
        }
    }

    /// Issue #37: レシピ一覧に出す 1 行表示用ラベルを返す。
    ///
    /// - `is_discovered == true` なら `visibility` に関係なく完全表示 ("水 + 火 → 蒸気")
    /// - `Public` も完全表示
    /// - `Partial` は最初の材料を名前で見せ、残り材料と結果を `?` で隠す
    /// - `Unknown` は全部 `???` + `未確認レシピ #{index:02}` で識別
    ///
    /// `index` は呼び出し側 (UI) が一覧上の表示順から渡す 0-origin の値。
    /// `Unknown` のラベルは `#01` から表示されるよう内部で +1 する。
    ///
    /// Issue #71 Phase 4: `lang` を受け取り「未確認レシピ」を言語別に表示する。
    pub fn display_label(
        &self,
        _collection: &[Curion],
        is_discovered: bool,
        index: usize,
        lang: Language,
    ) -> String {
        // 発見済みは常に完全表示。
        if is_discovered || self.visibility == RecipeVisibility::Public {
            return format!(
                "{} → {}",
                join_ingredient_labels(&self.ingredients, &[]),
                self.result.noun
            );
        }

        match self.visibility {
            RecipeVisibility::Public => unreachable!("handled above"),
            RecipeVisibility::Partial => {
                // 最初の材料だけ名前表示、残りは `?`、結果も `?` で隠す。
                // 「最低 1 つは見せて残りを隠す」戦略。
                let mut parts: Vec<String> = Vec::with_capacity(self.ingredients.len());
                for (i, req) in self.ingredients.iter().enumerate() {
                    if i == 0 {
                        parts.push(ingredient_label(req));
                    } else {
                        parts.push("?".to_string());
                    }
                }
                format!("{} → ?", parts.join(" + "))
            }
            RecipeVisibility::Unknown => match lang {
                Language::Ja => format!("未確認レシピ #{:02}", index + 1),
                Language::En => format!("Unrecorded recipe #{:02}", index + 1),
            },
        }
    }
}

/// `IngredientRequirement` 1 つを人間向けに 1 トークンで表示する。
/// `specific_noun` があれば名詞、それ以外はカテゴリ / レアリティの大枠を出す。
fn ingredient_label(req: &IngredientRequirement) -> String {
    let core = if let Some(noun) = &req.specific_noun {
        noun.clone()
    } else if let Some(cat) = &req.category {
        format!("{cat:?}")
    } else if let Some(rar) = &req.rarity {
        format!("{rar:?}")
    } else {
        "?".to_string()
    };
    if req.count > 1 {
        format!("{} ×{}", core, req.count)
    } else {
        core
    }
}

/// 材料要件のリストを `A + B + C` 形式で連結する。
/// 第二引数 `_hidden_mask` は将来 partial の細粒度制御を入れるための予約だが、
/// 今は使わない (Public / 発見済みは全部見せる)。
fn join_ingredient_labels(reqs: &[IngredientRequirement], _hidden_mask: &[bool]) -> String {
    reqs.iter()
        .map(ingredient_label)
        .collect::<Vec<_>>()
        .join(" + ")
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

    /// 合成を試みる (旧 API)。
    ///
    /// 既定で `Language::Ja` を返すため、多言語対応コードでは
    /// [`try_synthesize_lang`](Self::try_synthesize_lang) を使うこと。
    #[deprecated(
        since = "0.4.0",
        note = "Use try_synthesize_lang. This wrapper defaults to Language::Ja and will be removed."
    )]
    #[allow(dead_code)]
    pub fn try_synthesize(&mut self, ingredients: Vec<Curion>) -> Result<SynthesisAttemptResult> {
        self.try_synthesize_lang(ingredients, Language::Ja)
    }

    /// Issue #71 Phase 4: 言語指定付きで合成を試みる。
    ///
    /// `recipe_name` を `lang` に従って組み立てるため、UI 側で英語化されたレシピ名を
    /// `Success` / `HighRiskFailure` に直接乗せられる。`try_synthesize` は JA を
    /// 既定として委譲する (後方互換)。
    pub fn try_synthesize_lang(
        &mut self,
        ingredients: Vec<Curion>,
        lang: Language,
    ) -> Result<SynthesisAttemptResult> {
        let discovery_roll: f64 = rand::random();
        let risk_roll: f64 = rand::random();
        self.try_synthesize_with_rolls_lang(ingredients, discovery_roll, risk_roll, lang)
    }

    /// Issue #35: テスト用に乱数を外部注入できる内部 API。
    ///
    /// - `discovery_roll`: 未発見レシピの `discovery_rate` 判定に使う。
    ///   `discovery_roll > discovery_rate` で `DiscoveryFailed`。
    /// - `risk_roll`: `success_rate` 判定に使う。`risk_roll > success_rate` で `HighRiskFailure`。
    ///
    /// `try_synthesize` の薄いラッパで本実装。
    #[cfg(test)]
    pub fn try_synthesize_with_rolls(
        &mut self,
        ingredients: Vec<Curion>,
        discovery_roll: f64,
        risk_roll: f64,
    ) -> Result<SynthesisAttemptResult> {
        self.try_synthesize_with_rolls_lang(ingredients, discovery_roll, risk_roll, Language::Ja)
    }

    /// Issue #71 Phase 4: `try_synthesize_with_rolls` の言語対応版。
    pub fn try_synthesize_with_rolls_lang(
        &mut self,
        ingredients: Vec<Curion>,
        discovery_roll: f64,
        risk_roll: f64,
        lang: Language,
    ) -> Result<SynthesisAttemptResult> {
        // マッチするレシピを検索
        let matching_recipes = self.recipe_db.find_matching_recipes(&ingredients);

        if matching_recipes.is_empty() {
            return Ok(SynthesisAttemptResult::NoRecipe);
        }

        // 複数マッチした場合は最初のレシピを使用
        let recipe = matching_recipes[0];

        // 必要なデータを先にコピー
        let recipe_id = recipe.id.clone();
        let recipe_name = recipe.name_for(lang).to_string();
        let discovery_rate = recipe.discovery_rate;
        let success_rate = recipe.success_rate;
        let failure_mode = recipe.failure_mode.clone();
        let result_curion = recipe.result.to_curion();

        // 発見済みか確認
        let is_discovered = self.is_discovered(&recipe_id);

        // 未発見の場合、discovery_rateで成功判定
        if !is_discovered {
            if discovery_roll > discovery_rate {
                let hint = match lang {
                    Language::Ja => "何かが起こりそうだが、まだ完全には理解できていない...",
                    Language::En => "Something almost happens, but you don't yet understand it...",
                };
                return Ok(SynthesisAttemptResult::DiscoveryFailed {
                    hint: hint.to_string(),
                });
            }
            // 発見成功！
            self.discover_recipe(recipe_id);
        }

        // Issue #35: success_rate < 1.0 の場合は実行時 roll を毎回行う。
        // discovery 判定を通過した後に行うため、未発見初回でも risk 判定が走る。
        if success_rate < 1.0 && risk_roll > success_rate {
            return Ok(self.build_high_risk_failure(recipe_name, ingredients, failure_mode));
        }

        // 合成成功
        Ok(SynthesisAttemptResult::Success {
            curion: result_curion,
            recipe_name,
            first_discovery: !is_discovered,
        })
    }

    /// Issue #35: failure_mode に応じた `HighRiskFailure` を組み立てる。
    fn build_high_risk_failure(
        &self,
        recipe_name: String,
        ingredients: Vec<Curion>,
        failure_mode: FailureMode,
    ) -> SynthesisAttemptResult {
        match &failure_mode {
            FailureMode::NoLoss => SynthesisAttemptResult::HighRiskFailure {
                recipe_name,
                lost_ingredients: Vec::new(),
                salvage: None,
                failure_mode,
            },
            FailureMode::LoseAll => SynthesisAttemptResult::HighRiskFailure {
                recipe_name,
                lost_ingredients: ingredients,
                salvage: None,
                failure_mode,
            },
            FailureMode::Salvage { fallback_rarity } => {
                // 残骸は最初の材料の名詞・カテゴリを引き継ぎつつ、レアリティを fallback まで落とす。
                // 「残骸」というイメージなので interest/beauty は低めに固定。
                let salvage_curion = ingredients.first().map(|src| {
                    Curion::new(
                        Uuid::new_v4(),
                        format!("{}の残骸", src.noun),
                        src.category.clone(),
                        *fallback_rarity,
                        0.3,
                        0.3,
                    )
                });
                SynthesisAttemptResult::HighRiskFailure {
                    recipe_name,
                    lost_ingredients: ingredients,
                    salvage: salvage_curion,
                    failure_mode,
                }
            }
        }
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
    /// Issue #35: 高リスクレシピで `success_rate` 判定に失敗したケース。
    HighRiskFailure {
        recipe_name: String,
        /// 失われた材料 (UI 側で collection から削除するために使う)。
        /// `failure_mode == NoLoss` の場合は空。
        lost_ingredients: Vec<Curion>,
        /// 残骸 curion (`failure_mode == Salvage` の場合のみ Some)。
        salvage: Option<Curion>,
        failure_mode: FailureMode,
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
            name_en: String::new(),
            description_en: String::new(),
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
            success_rate: 1.0,
            failure_mode: FailureMode::NoLoss,
            visibility: RecipeVisibility::Public,
        }
    }

    /// Issue #35: 「水 + 火」のような 2 材料レシピを動的に作る。テスト専用。
    fn make_pair_recipe(
        id: &str,
        result_noun: &str,
        ingredient_a: &str,
        ingredient_b: &str,
        success_rate: f64,
        failure_mode: FailureMode,
    ) -> SynthesisRecipe {
        SynthesisRecipe {
            id: id.to_string(),
            name: result_noun.to_string(),
            description: "test pair recipe".to_string(),
            name_en: String::new(),
            description_en: String::new(),
            ingredients: vec![
                IngredientRequirement {
                    specific_noun: Some(ingredient_a.to_string()),
                    category: None,
                    rarity: None,
                    count: 1,
                },
                IngredientRequirement {
                    specific_noun: Some(ingredient_b.to_string()),
                    category: None,
                    rarity: None,
                    count: 1,
                },
            ],
            result: SynthesisResult {
                noun: result_noun.to_string(),
                category: Category::Concept,
                rarity: Rarity::Epic,
                synthesis_only: true,
                special_attributes: HashMap::new(),
            },
            discovery_rate: 1.0,
            recipe_type: RecipeType::Advanced,
            success_rate,
            failure_mode,
            visibility: RecipeVisibility::Public,
        }
    }

    fn manager_with_recipes(recipes: Vec<SynthesisRecipe>) -> SynthesisManager {
        let db = RecipeDatabase { recipes };
        SynthesisManager::new(db)
    }

    fn make_curion(noun: &str) -> Curion {
        Curion::new(
            Uuid::new_v4(),
            noun.to_string(),
            Category::Element,
            Rarity::Common,
            0.5,
            0.5,
        )
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

    // ---------- Issue #35: 高リスク合成 ----------

    /// Issue #35: JSON で `success_rate` 省略時はデフォルト 1.0 になる。
    #[test]
    fn test_synthesis_recipe_default_success_rate_is_1() {
        // success_rate / failure_mode を持たない旧レシピ JSON を再現
        let legacy_json = r#"{
            "id": "legacy_recipe",
            "name": "レガシー",
            "description": "old style",
            "ingredients": [],
            "result": {
                "noun": "x",
                "category": "Concept",
                "rarity": "Common",
                "synthesis_only": false,
                "special_attributes": {}
            },
            "discovery_rate": 0.8,
            "recipe_type": "Intuitive"
        }"#;
        let recipe: SynthesisRecipe =
            serde_json::from_str(legacy_json).expect("legacy recipe parse");
        assert!((recipe.success_rate - 1.0).abs() < 1e-9);
        assert_eq!(recipe.failure_mode, FailureMode::NoLoss);
    }

    /// Issue #35: 既存レシピは high-risk 扱いにならない (= 100% 成功)。
    #[test]
    fn test_is_high_risk_threshold() {
        // success_rate がしきい値より上なら non-high-risk
        let mut r = sample_recipe(1.0);
        r.success_rate = 1.0;
        assert!(!r.is_high_risk());

        r.success_rate = 0.95;
        assert!(!r.is_high_risk(), "0.95 はしきい値ちょうど (非リスク)");

        r.success_rate = 0.94;
        assert!(r.is_high_risk(), "0.94 未満は high risk");

        r.success_rate = 0.30;
        assert!(r.is_high_risk());
    }

    /// Issue #35: success_rate 100% なら、risk_roll に関係なく Success を返す。
    #[test]
    fn test_try_synthesize_legacy_recipe_always_succeeds() {
        let recipe = make_pair_recipe("legacy", "結果", "水", "火", 1.0, FailureMode::LoseAll);
        let mut mgr = manager_with_recipes(vec![recipe]);
        let ingredients = vec![make_curion("水"), make_curion("火")];

        // risk_roll が 0.99 (= 普通なら高リスクで失敗するレベル) でも、
        // success_rate=1.0 なら成功する。discovery_roll も成功側にしておく。
        let result = mgr
            .try_synthesize_with_rolls(ingredients, 0.0, 0.99)
            .expect("synthesize");
        assert!(matches!(result, SynthesisAttemptResult::Success { .. }));
    }

    /// Issue #35: risk_roll が success_rate 以下なら成功する。
    #[test]
    fn test_try_synthesize_high_risk_success() {
        let recipe = make_pair_recipe("risky", "禁断", "水", "火", 0.30, FailureMode::LoseAll);
        let mut mgr = manager_with_recipes(vec![recipe]);
        let ingredients = vec![make_curion("水"), make_curion("火")];

        // discovery 成功 + risk 成功 (roll 0.10 <= 0.30)
        let result = mgr
            .try_synthesize_with_rolls(ingredients, 0.0, 0.10)
            .expect("synthesize");
        assert!(matches!(result, SynthesisAttemptResult::Success { .. }));
    }

    /// Issue #35: risk_roll が success_rate を超えると HighRiskFailure を返す。
    /// LoseAll モードでは lost_ingredients に全材料が入る。
    #[test]
    fn test_try_synthesize_high_risk_failure_loseall() {
        let recipe = make_pair_recipe("risky", "禁断", "水", "火", 0.30, FailureMode::LoseAll);
        let mut mgr = manager_with_recipes(vec![recipe]);
        let water = make_curion("水");
        let fire = make_curion("火");
        let water_id = water.id.clone();
        let fire_id = fire.id.clone();
        let ingredients = vec![water, fire];

        // risk_roll 0.80 > 0.30 → 失敗
        let result = mgr
            .try_synthesize_with_rolls(ingredients, 0.0, 0.80)
            .expect("synthesize");
        match result {
            SynthesisAttemptResult::HighRiskFailure {
                lost_ingredients,
                salvage,
                failure_mode,
                ..
            } => {
                assert_eq!(failure_mode, FailureMode::LoseAll);
                assert_eq!(lost_ingredients.len(), 2);
                let lost_ids: Vec<String> = lost_ingredients.iter().map(|c| c.id.clone()).collect();
                assert!(lost_ids.contains(&water_id));
                assert!(lost_ids.contains(&fire_id));
                assert!(salvage.is_none());
            }
            other => panic!("expected HighRiskFailure, got {other:?}"),
        }
    }

    /// Issue #35: NoLoss モードでは失敗しても素材は失われない (lost_ingredients が空)。
    #[test]
    fn test_try_synthesize_failure_mode_noloss_no_lost_ingredients() {
        let recipe = make_pair_recipe("insured", "保険", "水", "火", 0.30, FailureMode::NoLoss);
        let mut mgr = manager_with_recipes(vec![recipe]);
        let ingredients = vec![make_curion("水"), make_curion("火")];

        let result = mgr
            .try_synthesize_with_rolls(ingredients, 0.0, 0.80)
            .expect("synthesize");
        match result {
            SynthesisAttemptResult::HighRiskFailure {
                lost_ingredients,
                salvage,
                failure_mode,
                ..
            } => {
                assert_eq!(failure_mode, FailureMode::NoLoss);
                assert!(lost_ingredients.is_empty(), "NoLoss は素材を失わない");
                assert!(salvage.is_none());
            }
            other => panic!("expected HighRiskFailure, got {other:?}"),
        }
    }

    /// Issue #35: Salvage モードは lost_ingredients + 残骸 curion を返す。
    #[test]
    fn test_try_synthesize_failure_mode_salvage_produces_salvage_curion() {
        let recipe = make_pair_recipe(
            "salvage",
            "残骸テスト",
            "水",
            "火",
            0.30,
            FailureMode::Salvage {
                fallback_rarity: Rarity::Common,
            },
        );
        let mut mgr = manager_with_recipes(vec![recipe]);
        let ingredients = vec![make_curion("水"), make_curion("火")];

        let result = mgr
            .try_synthesize_with_rolls(ingredients, 0.0, 0.95)
            .expect("synthesize");
        match result {
            SynthesisAttemptResult::HighRiskFailure {
                lost_ingredients,
                salvage,
                failure_mode,
                ..
            } => {
                assert!(matches!(failure_mode, FailureMode::Salvage { .. }));
                assert_eq!(lost_ingredients.len(), 2);
                let salvage = salvage.expect("salvage curion should be produced");
                assert_eq!(salvage.rarity, Rarity::Common);
                assert!(salvage.noun.contains("残骸"));
            }
            other => panic!("expected HighRiskFailure, got {other:?}"),
        }
    }

    /// Issue #35: discovery 判定に失敗した場合は risk 判定まで到達せず、
    /// `DiscoveryFailed` が返ることで素材ロストの誤発火が起きない。
    #[test]
    fn test_try_synthesize_discovery_failure_does_not_trigger_risk_failure() {
        // discovery_rate を低く設定したいので make_pair_recipe を上書きする
        let mut recipe = make_pair_recipe("risky", "禁断", "水", "火", 0.50, FailureMode::LoseAll);
        recipe.discovery_rate = 0.10;
        let mut mgr = manager_with_recipes(vec![recipe]);
        let ingredients = vec![make_curion("水"), make_curion("火")];

        // discovery_roll 0.50 > 0.10 → 発見失敗
        let result = mgr
            .try_synthesize_with_rolls(ingredients, 0.50, 0.0)
            .expect("synthesize");
        assert!(matches!(
            result,
            SynthesisAttemptResult::DiscoveryFailed { .. }
        ));
    }

    /// Issue #35: 発見済みレシピで success_rate < 1.0 の場合、毎回 risk roll が走る。
    #[test]
    fn test_try_synthesize_discovered_still_rolls_risk() {
        let recipe = make_pair_recipe("risky", "禁断", "水", "火", 0.30, FailureMode::LoseAll);
        let recipe_id = recipe.id.clone();
        let mut mgr = manager_with_recipes(vec![recipe]);
        mgr.discover_recipe(recipe_id);

        let ingredients = vec![make_curion("水"), make_curion("火")];
        // 発見済みでも risk_roll 0.99 > 0.30 → 失敗
        let result = mgr
            .try_synthesize_with_rolls(ingredients, 0.0, 0.99)
            .expect("synthesize");
        assert!(matches!(
            result,
            SynthesisAttemptResult::HighRiskFailure { .. }
        ));
    }

    /// Issue #35: 表示用の `success_probability` は discovery × success_rate の積。
    #[test]
    fn test_success_probability_combines_discovery_and_success_rate() {
        let mut recipe = sample_recipe(0.50);
        recipe.success_rate = 0.40;

        // 未発見: 0.50 * 0.40 = 0.20
        let undiscovered = recipe.success_probability(false);
        assert!((undiscovered - 0.20).abs() < 1e-9);

        // 発見済み: 1.0 * 0.40 = 0.40
        let discovered = recipe.success_probability(true);
        assert!((discovered - 0.40).abs() < 1e-9);
    }

    // ---------- Issue #37: 部分公開レシピ ----------

    /// Issue #37: `RecipeVisibility` のデフォルトは `Public` (既存挙動を維持)。
    #[test]
    fn test_recipe_visibility_default_is_public() {
        let r = sample_recipe(1.0);
        assert_eq!(r.visibility, RecipeVisibility::Public);
        assert_eq!(RecipeVisibility::default(), RecipeVisibility::Public);
    }

    /// Issue #37: JSON で `visibility` フィールドを 3 値とも往復できる。
    /// 省略時は `Public` にフォールバックする (後方互換)。
    #[test]
    fn test_recipe_visibility_serde_roundtrip() {
        // visibility 省略 → Public
        let legacy_json = r#"{
            "id": "legacy",
            "name": "legacy",
            "description": "",
            "ingredients": [],
            "result": {
                "noun": "x",
                "category": "Concept",
                "rarity": "Common",
                "synthesis_only": false,
                "special_attributes": {}
            },
            "discovery_rate": 1.0,
            "recipe_type": "Intuitive"
        }"#;
        let r: SynthesisRecipe = serde_json::from_str(legacy_json).expect("legacy parse");
        assert_eq!(r.visibility, RecipeVisibility::Public);

        // visibility を 3 値とも逐次 deserialize できる
        for (raw, expected) in [
            ("\"public\"", RecipeVisibility::Public),
            ("\"partial\"", RecipeVisibility::Partial),
            ("\"unknown\"", RecipeVisibility::Unknown),
        ] {
            let v: RecipeVisibility = serde_json::from_str(raw).expect("visibility parse");
            assert_eq!(v, expected, "raw={raw}");
        }

        // round trip: serialize → deserialize で同一値
        for v in [
            RecipeVisibility::Public,
            RecipeVisibility::Partial,
            RecipeVisibility::Unknown,
        ] {
            let s = serde_json::to_string(&v).expect("ser");
            let back: RecipeVisibility = serde_json::from_str(&s).expect("de");
            assert_eq!(v, back);
        }
    }

    /// Issue #37: 全要件を満たす場合、satisfied == total かつ all_satisfied == true。
    #[test]
    fn test_ingredient_progress_full() {
        let recipe = make_pair_recipe("p_full", "結果", "水", "火", 1.0, FailureMode::NoLoss);
        let collection = vec![make_curion("水"), make_curion("火"), make_curion("土")];
        let p = recipe.ingredient_progress(&collection);
        assert_eq!(p.total, 2);
        assert_eq!(p.satisfied, 2);
        assert!(p.all_satisfied);
    }

    /// Issue #37: 一部だけ揃っている場合、satisfied < total。
    #[test]
    fn test_ingredient_progress_partial() {
        let recipe = make_pair_recipe("p_part", "結果", "水", "火", 1.0, FailureMode::NoLoss);
        let collection = vec![make_curion("水")]; // 火 が無い
        let p = recipe.ingredient_progress(&collection);
        assert_eq!(p.total, 2);
        assert_eq!(p.satisfied, 1);
        assert!(!p.all_satisfied);
    }

    /// Issue #37: 何も持っていない場合、satisfied == 0。
    #[test]
    fn test_ingredient_progress_empty() {
        let recipe = make_pair_recipe("p_empty", "結果", "水", "火", 1.0, FailureMode::NoLoss);
        let collection: Vec<Curion> = vec![];
        let p = recipe.ingredient_progress(&collection);
        assert_eq!(p.total, 2);
        assert_eq!(p.satisfied, 0);
        assert!(!p.all_satisfied);
    }

    /// Issue #37: `remaining_categories` は total - satisfied と一致する。
    #[test]
    fn test_remaining_categories_calculation() {
        let recipe = make_pair_recipe("p_remain", "結果", "水", "火", 1.0, FailureMode::NoLoss);

        // 何もない: 2 種残り
        assert_eq!(recipe.remaining_categories(&[]), 2);

        // 1 つ持っている: 1 種残り
        assert_eq!(recipe.remaining_categories(&[make_curion("水")]), 1);

        // 全部揃ってる: 0
        assert_eq!(
            recipe.remaining_categories(&[make_curion("水"), make_curion("火")]),
            0
        );
    }

    /// Issue #37: Public レシピは未発見でも完全表示される。
    #[test]
    fn test_display_label_public_uses_full_names() {
        let recipe = make_pair_recipe("p_pub", "蒸気", "水", "火", 1.0, FailureMode::NoLoss);
        // visibility は make_pair_recipe で Public がデフォルト
        let label = recipe.display_label(&[], false, 0, Language::Ja);
        assert!(label.contains("水"), "label={label}");
        assert!(label.contains("火"), "label={label}");
        assert!(label.contains("蒸気"), "label={label}");
        assert!(label.contains("→"));
    }

    /// Issue #37: Partial レシピは最初の材料は名前、残りと結果は `?` で隠す。
    #[test]
    fn test_display_label_partial_hides_inputs_after_first() {
        let mut recipe = make_pair_recipe(
            "p_part_lbl",
            "黒い太陽",
            "光",
            "影",
            0.5,
            FailureMode::NoLoss,
        );
        recipe.visibility = RecipeVisibility::Partial;

        let label = recipe.display_label(&[], false, 5, Language::Ja);
        assert!(
            label.contains("光"),
            "first ingredient should be shown: {label}"
        );
        assert!(
            !label.contains("影"),
            "second ingredient should be hidden: {label}"
        );
        assert!(
            !label.contains("黒い太陽"),
            "result must be hidden: {label}"
        );
        assert!(label.contains("?"));
    }

    /// Issue #37: Unknown レシピは存在のみ。中身は `??? + ??? = ???` ではなく
    /// 「未確認レシピ #NN」形式で index 表示する。
    #[test]
    fn test_display_label_unknown_shows_index_only() {
        let mut recipe =
            make_pair_recipe("p_unk", "禁断", "混沌", "秩序", 0.25, FailureMode::LoseAll);
        recipe.visibility = RecipeVisibility::Unknown;

        // index 6 → "#07" (0-origin +1, 2 桁 0 埋め)
        let label = recipe.display_label(&[], false, 6, Language::Ja);
        assert!(label.contains("未確認レシピ"), "label={label}");
        assert!(
            label.contains("#07"),
            "should be 0-padded 2 digits: {label}"
        );
        assert!(
            !label.contains("混沌"),
            "ingredient names must be hidden: {label}"
        );
        assert!(!label.contains("秩序"), "label={label}");
        assert!(!label.contains("禁断"), "result must be hidden: {label}");
    }

    /// Issue #37: 発見済みレシピは visibility に関わらず完全表示。
    /// Partial / Unknown でも is_discovered=true なら全部見える。
    #[test]
    fn test_display_label_discovered_recipe_always_public() {
        for vis in [RecipeVisibility::Partial, RecipeVisibility::Unknown] {
            let mut recipe =
                make_pair_recipe("p_d", "黒い太陽", "光", "影", 0.5, FailureMode::NoLoss);
            recipe.visibility = vis;
            let label = recipe.display_label(&[], true, 9, Language::Ja);
            assert!(label.contains("光"), "vis={vis:?} label={label}");
            assert!(label.contains("影"), "vis={vis:?} label={label}");
            assert!(label.contains("黒い太陽"), "vis={vis:?} label={label}");
            assert!(
                !label.contains("未確認レシピ"),
                "discovered should not be Unknown: {label}"
            );
        }
    }

    // ---------- Issue #71 Phase 4: lang gate ----------

    /// Issue #71 Phase 4: `name_for(Ja)` は JA `name` をそのまま返す。
    #[test]
    fn test_recipe_name_for_ja_matches_legacy() {
        let db = RecipeDatabase::load_embedded().expect("recipes load");
        for r in db.all_recipes() {
            assert_eq!(r.name_for(Language::Ja), r.name);
        }
    }

    /// Issue #71 Phase 4: `name_for(En)` は 17 件すべて非空かつ JA と異なる。
    #[test]
    fn test_recipe_name_for_en_is_translated_for_all_recipes() {
        let db = RecipeDatabase::load_embedded().expect("recipes load");
        assert_eq!(db.all_recipes().len(), 17);
        for r in db.all_recipes() {
            let en = r.name_for(Language::En);
            assert!(!en.is_empty(), "{}: name_en is empty", r.id);
            assert_ne!(en, r.name, "{}: name_en equals name (JA)", r.id);
        }
    }

    /// Issue #71 Phase 4: `description_for(En)` も 17 件すべて非空。
    #[test]
    fn test_recipe_description_for_en_is_translated_for_all_recipes() {
        let db = RecipeDatabase::load_embedded().expect("recipes load");
        for r in db.all_recipes() {
            let en = r.description_for(Language::En);
            assert!(!en.is_empty(), "{}: description_en is empty", r.id);
            assert_ne!(en, r.description, "{}: description_en equals JA", r.id);
        }
    }

    /// Issue #71 Phase 4: `name_en`/`description_en` が空のレガシーデータは JA フォールバック。
    #[test]
    fn test_recipe_for_lang_fallbacks_when_en_empty() {
        let mut r = sample_recipe(1.0);
        r.name = "テスト".to_string();
        r.description = "JA only".to_string();
        r.name_en = String::new();
        r.description_en = String::new();
        assert_eq!(r.name_for(Language::En), "テスト");
        assert_eq!(r.description_for(Language::En), "JA only");
    }

    /// Issue #71 Phase 4: Unknown レシピの "未確認レシピ" 表示が言語別に切り替わる。
    #[test]
    fn test_display_label_unknown_lang_gate() {
        let mut recipe = make_pair_recipe(
            "p_unk_lang",
            "禁断",
            "混沌",
            "秩序",
            0.25,
            FailureMode::LoseAll,
        );
        recipe.visibility = RecipeVisibility::Unknown;
        let ja = recipe.display_label(&[], false, 6, Language::Ja);
        let en = recipe.display_label(&[], false, 6, Language::En);
        assert!(ja.contains("未確認レシピ"), "ja={ja}");
        assert!(ja.contains("#07"));
        assert!(en.contains("Unrecorded recipe"), "en={en}");
        assert!(en.contains("#07"));
        assert!(!en.contains("未確認"), "en should not include JA: {en}");
    }
}
