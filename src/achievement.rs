use crate::curion::{Category, Rarity};
use crate::i18n::Language;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 実績ID
pub type AchievementId = String;

/// 実績の種類
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AchievementType {
    /// 総数マイルストーン
    TotalCount(usize),

    /// レアリティ別収集
    RarityCount { rarity: Rarity, count: usize },

    /// カテゴリ別コンプリート
    CategoryComplete(Category),

    /// カテゴリ別収集数
    CategoryCount { category: Category, count: usize },

    /// 連続ログイン
    ConsecutiveLogin(usize),

    /// プレイ時間（分）
    PlayTime(usize),

    /// 特定名詞の収集
    SpecificNoun { noun: String, count: usize },

    /// 全カテゴリコンプリート
    AllCategoriesComplete,

    /// コンボ系（連続でレア以上）
    LuckyStreak(usize),

    /// 時間内獲得数
    HourlyAcquisition(usize),
}

/// 実績定義
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Achievement {
    pub id: AchievementId,
    pub name: String,
    pub description: String,
    /// Issue #71 Phase 4: 英語版実績名。空文字なら `name` (JA) にフォールバック。
    #[serde(default)]
    pub name_en: String,
    /// Issue #71 Phase 4: 英語版説明。空文字なら `description` (JA) にフォールバック。
    #[serde(default)]
    pub description_en: String,
    pub achievement_type: AchievementType,
    pub reward_xp: u32,
    pub reward_title: Option<String>,
    /// Issue #71 Phase 4: 英語版称号。`None` なら `reward_title` を返す。
    #[serde(default)]
    pub reward_title_en: Option<String>,
    pub icon: String,
}

impl Achievement {
    pub fn new(
        id: String,
        name: String,
        description: String,
        achievement_type: AchievementType,
        reward_xp: u32,
        reward_title: Option<String>,
        icon: String,
    ) -> Self {
        Self {
            id,
            name,
            description,
            name_en: String::new(),
            description_en: String::new(),
            achievement_type,
            reward_xp,
            reward_title,
            reward_title_en: None,
            icon,
        }
    }

    /// Issue #71 Phase 4: 英語訳を後付けする builder。
    ///
    /// 既存の `Achievement::new` を変えずに `.with_en(...)` で英訳を流し込めるので、
    /// `register_default_achievements` の各登録が縦並びのまま読める。
    pub fn with_en(
        mut self,
        name_en: impl Into<String>,
        description_en: impl Into<String>,
        reward_title_en: Option<String>,
    ) -> Self {
        self.name_en = name_en.into();
        self.description_en = description_en.into();
        self.reward_title_en = reward_title_en;
        self
    }

    /// Issue #71 Phase 4: 言語別の実績名。`name_en` が空なら JA にフォールバック。
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

    /// Issue #71 Phase 4: 言語別の説明文。`description_en` が空なら JA にフォールバック。
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

    /// Issue #71 Phase 4: 言語別の称号。`reward_title_en` が None なら JA にフォールバック。
    pub fn reward_title_for(&self, lang: Language) -> Option<&str> {
        match lang {
            Language::Ja => self.reward_title.as_deref(),
            Language::En => self
                .reward_title_en
                .as_deref()
                .or(self.reward_title.as_deref()),
        }
    }
}

/// 実績の進捗状態
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AchievementProgress {
    pub achievement_id: AchievementId,
    pub current: usize,
    pub target: usize,
    pub unlocked: bool,
    pub claimed: bool,
    pub unlocked_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl AchievementProgress {
    pub fn new(achievement_id: AchievementId, target: usize) -> Self {
        Self {
            achievement_id,
            current: 0,
            target,
            unlocked: false,
            claimed: false,
            unlocked_at: None,
        }
    }

    /// 進捗率を計算（0.0〜1.0以上）
    pub fn progress_ratio(&self) -> f64 {
        if self.target == 0 {
            return 1.0;
        }
        (self.current as f64) / (self.target as f64)
    }

    /// 進捗率（%）
    pub fn progress_percentage(&self) -> usize {
        ((self.progress_ratio() * 100.0) as usize).min(9999)
    }

    /// 残り数
    pub fn remaining(&self) -> usize {
        self.target.saturating_sub(self.current)
    }

    /// 達成可能かチェック
    pub fn is_achievable(&self) -> bool {
        self.current >= self.target && !self.unlocked
    }

    /// 進捗を更新
    pub fn update_progress(&mut self, current: usize) {
        self.current = current;
        if self.current >= self.target && !self.unlocked {
            self.unlocked = true;
            self.unlocked_at = Some(chrono::Utc::now());
        }
    }

    /// 報酬を受け取る
    pub fn claim_reward(&mut self) -> bool {
        if self.unlocked && !self.claimed {
            self.claimed = true;
            true
        } else {
            false
        }
    }
}

/// 実績マネージャー
#[derive(Debug)]
pub struct AchievementManager {
    achievements: HashMap<AchievementId, Achievement>,
    progress: HashMap<AchievementId, AchievementProgress>,
}

impl AchievementManager {
    pub fn new() -> Self {
        let mut manager = Self {
            achievements: HashMap::new(),
            progress: HashMap::new(),
        };

        manager.register_default_achievements();
        manager
    }

    /// デフォルトの実績を登録
    fn register_default_achievements(&mut self) {
        // Issue #32 きりの悪い数字設計:
        //   旧 [10, 50, 100, 250, 500, 1000] (キリのいい所で切り上げたくなる)
        //   新 [10, 27, 51, 103, 247, 501, 1001] (= わざと割り切れない)
        //
        // 後方互換: 実績進捗は `HashMap<AchievementId, Progress>` で管理されており、
        //   - 古いセーブに残った `total_50` などの ID は次回起動で再評価されず放置 (実害なし)
        //   - 新規 ID (`total_27` 等) は空 Progress として `register_default_achievements`
        //     から作られ、現在の `total_acquired` で再計算される
        //   - つまりマイグレーション不要。旧解除済みフラグは無視されて未解放扱いに戻るが、
        //     現在の所持数で再達成される (詳細は docs/spec.md 参照)。
        let total_thresholds = [10, 27, 51, 103, 247, 501, 1001];
        for (idx, &count) in total_thresholds.iter().enumerate() {
            self.register_achievement(
                Achievement::new(
                    format!("total_{count}"),
                    format!("コレクター Lv.{}", idx + 1),
                    format!("{count}個のキュリオンを集める"),
                    AchievementType::TotalCount(count),
                    count as u32 * 10,
                    if count >= 1001 {
                        Some("伝説のコレクター".to_string())
                    } else {
                        None
                    },
                    if count >= 1001 {
                        "👑"
                    } else if count >= 501 {
                        "💎"
                    } else {
                        "📦"
                    }
                    .to_string(),
                )
                .with_en(
                    format!("Collector Lv.{}", idx + 1),
                    format!("Collect {count} curions"),
                    if count >= 1001 {
                        Some("Legendary Collector".to_string())
                    } else {
                        None
                    },
                ),
            );
        }

        // レアリティ別 (Issue #32: 旧 [10,50,100]/[5,25,50]/[1,5,10,50,100] を非線形に)
        for (rarity, counts) in [
            (Rarity::Rare, vec![10, 47, 103]),
            (Rarity::Epic, vec![5, 23, 51]),
            (Rarity::Legendary, vec![1, 7, 23, 47, 101]),
        ] {
            for count in counts {
                let reward_title_en = if count >= 101 && rarity == Rarity::Legendary {
                    Some("Mythical Collector".to_string())
                } else if count == 1 && rarity == Rarity::Legendary {
                    Some("Legendary Hunter".to_string())
                } else {
                    None
                };
                self.register_achievement(
                    Achievement::new(
                        format!("{rarity:?}_{count}").to_lowercase(),
                        format!("{rarity:?} ハンター {count}"),
                        format!("{rarity:?}を{count}個集める"),
                        AchievementType::RarityCount { rarity, count },
                        count as u32
                            * match rarity {
                                Rarity::Rare => 15,
                                Rarity::Epic => 30,
                                Rarity::Legendary => 100,
                                _ => 10,
                            },
                        if count >= 101 && rarity == Rarity::Legendary {
                            Some("神話の収集家".to_string())
                        } else if count == 1 && rarity == Rarity::Legendary {
                            Some("伝説のハンター".to_string())
                        } else {
                            None
                        },
                        match rarity {
                            Rarity::Legendary => "⭐",
                            Rarity::Epic => "💜",
                            Rarity::Rare => "💙",
                            _ => "⚪",
                        }
                        .to_string(),
                    )
                    .with_en(
                        format!("{} Hunter {count}", rarity.display(Language::En)),
                        format!("Collect {count} {} curions", rarity.display(Language::En)),
                        reward_title_en,
                    ),
                );
            }
        }

        // カテゴリ別コンプリート
        for category in [
            Category::Animal,
            Category::Plant,
            Category::Color,
            Category::Object,
            Category::Concept,
            Category::Element,
            Category::Food,
            Category::Phenomenon,
            Category::Abstract,
        ] {
            let cat_en = category.display(Language::En);
            self.register_achievement(
                Achievement::new(
                    format!("complete_{category:?}").to_lowercase(),
                    format!("{}マスター", category.as_str()),
                    format!("{}カテゴリを全種コンプリート", category.as_str()),
                    AchievementType::CategoryComplete(category.clone()),
                    500,
                    Some(format!("{}の覇者", category.as_str())),
                    "🏆".to_string(),
                )
                .with_en(
                    format!("{cat_en} Master"),
                    format!("Complete the entire {cat_en} category"),
                    Some(format!("{cat_en} Overlord")),
                ),
            );
        }

        // 全カテゴリコンプリート
        self.register_achievement(
            Achievement::new(
                "all_categories".to_string(),
                "完璧主義者".to_string(),
                "全カテゴリをコンプリート".to_string(),
                AchievementType::AllCategoriesComplete,
                5000,
                Some("完全なる収集家".to_string()),
                "👑".to_string(),
            )
            .with_en(
                "Perfectionist",
                "Complete all categories",
                Some("Perfect Collector".to_string()),
            ),
        );

        // 連続ログイン (Issue #32: 30 → 33, 100 → 101 で「あと一日」感を残す)
        for days in [3, 7, 14, 33, 101] {
            self.register_achievement(
                Achievement::new(
                    format!("login_{days}"),
                    format!("継続は力なり {days}"),
                    format!("{days}日連続ログイン"),
                    AchievementType::ConsecutiveLogin(days),
                    days as u32 * 50,
                    if days >= 101 {
                        Some("不屈の意志".to_string())
                    } else {
                        None
                    },
                    "🔥".to_string(),
                )
                .with_en(
                    format!("Persistence Is Power — {days} days"),
                    format!("Log in for {days} consecutive days"),
                    if days >= 101 {
                        Some("Unyielding Will".to_string())
                    } else {
                        None
                    },
                ),
            );
        }

        // プレイ時間 (Issue #32: 50 → 47, 100 → 103, 500 → 503)
        for hours in [1, 11, 47, 103, 503] {
            self.register_achievement(
                Achievement::new(
                    format!("playtime_{hours}"),
                    format!("ベテラン {hours}"),
                    format!("{hours}時間プレイ"),
                    AchievementType::PlayTime(hours * 60),
                    hours as u32 * 20,
                    if hours >= 503 {
                        Some("時間の支配者".to_string())
                    } else {
                        None
                    },
                    "⏰".to_string(),
                )
                .with_en(
                    format!("Veteran — {hours}h"),
                    format!("Play for {hours} hours"),
                    if hours >= 503 {
                        Some("Master of Time".to_string())
                    } else {
                        None
                    },
                ),
            );
        }

        // 特殊実績
        self.register_achievement(
            Achievement::new(
                "golden_fever".to_string(),
                "黄金狂".to_string(),
                "金のキュリオンを10個集める".to_string(),
                AchievementType::SpecificNoun {
                    noun: "金".to_string(),
                    count: 10,
                },
                1000,
                Some("錬金術師".to_string()),
                "🥇".to_string(),
            )
            .with_en(
                "Gold Fever",
                "Collect 10 gold curions",
                Some("Alchemist".to_string()),
            ),
        );

        self.register_achievement(
            Achievement::new(
                "dragon_master".to_string(),
                "龍の化身".to_string(),
                "龍のキュリオンを5個集める".to_string(),
                AchievementType::SpecificNoun {
                    noun: "龍".to_string(),
                    count: 5,
                },
                800,
                Some("龍使い".to_string()),
                "🐉".to_string(),
            )
            .with_en(
                "Dragon Incarnate",
                "Collect 5 dragon curions",
                Some("Dragon Master".to_string()),
            ),
        );
    }

    /// 実績を登録
    fn register_achievement(&mut self, achievement: Achievement) {
        let target = self.get_achievement_target(&achievement.achievement_type);
        let progress = AchievementProgress::new(achievement.id.clone(), target);

        self.progress.insert(achievement.id.clone(), progress);
        self.achievements
            .insert(achievement.id.clone(), achievement);
    }

    /// 実績のターゲット値を取得
    fn get_achievement_target(&self, achievement_type: &AchievementType) -> usize {
        match achievement_type {
            AchievementType::TotalCount(n) => *n,
            AchievementType::RarityCount { count, .. } => *count,
            AchievementType::CategoryComplete(_) => 1,
            AchievementType::CategoryCount { count, .. } => *count,
            AchievementType::ConsecutiveLogin(n) => *n,
            AchievementType::PlayTime(n) => *n,
            AchievementType::SpecificNoun { count, .. } => *count,
            AchievementType::AllCategoriesComplete => 1,
            AchievementType::LuckyStreak(n) => *n,
            AchievementType::HourlyAcquisition(n) => *n,
        }
    }

    /// 全実績を取得
    pub fn get_all_achievements(&self) -> Vec<&Achievement> {
        self.achievements.values().collect()
    }

    /// 実績の進捗を取得
    pub fn get_progress(&self, achievement_id: &str) -> Option<&AchievementProgress> {
        self.progress.get(achievement_id)
    }

    /// 実績の進捗を取得（可変）
    pub fn get_progress_mut(&mut self, achievement_id: &str) -> Option<&mut AchievementProgress> {
        self.progress.get_mut(achievement_id)
    }

    /// 達成可能な実績を取得（報酬受け取り可能）
    pub fn get_achievable(&self) -> Vec<(&Achievement, &AchievementProgress)> {
        self.achievements
            .iter()
            .filter_map(|(id, achievement)| {
                self.progress.get(id).and_then(|progress| {
                    if progress.is_achievable() {
                        Some((achievement, progress))
                    } else {
                        None
                    }
                })
            })
            .collect()
    }

    /// 進捗率でソートした実績リストを取得
    pub fn get_sorted_by_progress(&self) -> Vec<(&Achievement, &AchievementProgress)> {
        let mut list: Vec<_> = self
            .achievements
            .iter()
            .filter_map(|(id, achievement)| {
                self.progress
                    .get(id)
                    .map(|progress| (achievement, progress))
            })
            .collect();

        list.sort_by(|a, b| {
            b.1.progress_ratio()
                .partial_cmp(&a.1.progress_ratio())
                .unwrap()
        });

        list
    }

    /// 解除済み実績数を取得
    pub fn get_unlocked_count(&self) -> usize {
        self.progress.values().filter(|p| p.unlocked).count()
    }

    /// 全実績数を取得
    pub fn get_total_count(&self) -> usize {
        self.achievements.len()
    }

    /// 解除率を取得（%）
    pub fn get_unlock_percentage(&self) -> usize {
        if self.achievements.is_empty() {
            return 0;
        }
        (self.get_unlocked_count() * 100) / self.get_total_count()
    }
}

impl Default for AchievementManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Issue #71 Phase 4: `name_for(Ja)` は JA をそのまま返し、`name_for(En)` は
    /// 英訳 (空でなければ) を返す。
    #[test]
    fn test_achievement_name_for_lang() {
        let a = Achievement::new(
            "x".into(),
            "黄金狂".into(),
            "金のキュリオンを10個集める".into(),
            AchievementType::TotalCount(10),
            100,
            Some("錬金術師".into()),
            "🥇".into(),
        )
        .with_en(
            "Gold Fever",
            "Collect 10 gold curions",
            Some("Alchemist".into()),
        );

        assert_eq!(a.name_for(Language::Ja), "黄金狂");
        assert_eq!(a.name_for(Language::En), "Gold Fever");
        assert_eq!(
            a.description_for(Language::Ja),
            "金のキュリオンを10個集める"
        );
        assert_eq!(a.description_for(Language::En), "Collect 10 gold curions");
        assert_eq!(a.reward_title_for(Language::Ja), Some("錬金術師"));
        assert_eq!(a.reward_title_for(Language::En), Some("Alchemist"));
    }

    /// Issue #71 Phase 4: 英訳が空のレガシー実績は JA フォールバックする。
    #[test]
    fn test_achievement_fallbacks_when_en_empty() {
        let a = Achievement::new(
            "legacy".into(),
            "テスト".into(),
            "説明".into(),
            AchievementType::TotalCount(10),
            100,
            Some("称号".into()),
            "🏆".into(),
        );
        assert_eq!(a.name_for(Language::En), "テスト");
        assert_eq!(a.description_for(Language::En), "説明");
        assert_eq!(a.reward_title_for(Language::En), Some("称号"));
    }

    /// Issue #71 Phase 4: デフォルト実績 (全件) について name_en / description_en が
    /// 空でないことを確認する (= register_default_achievements が全件に EN を付けた)。
    #[test]
    fn test_default_achievements_all_have_en_translations() {
        let mgr = AchievementManager::new();
        for a in mgr.get_all_achievements() {
            assert!(!a.name_en.is_empty(), "{}: name_en empty", a.id);
            assert!(
                !a.description_en.is_empty(),
                "{}: description_en empty",
                a.id
            );
            assert_ne!(a.name_en, a.name, "{}: name_en equals JA name", a.id);
            // reward_title_en は reward_title がある実績だけチェック
            if a.reward_title.is_some() {
                assert!(
                    a.reward_title_en.is_some(),
                    "{}: reward_title_en missing while JA exists",
                    a.id
                );
            }
        }
    }
}
