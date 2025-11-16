use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::curion::{Category, Rarity};

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
    pub achievement_type: AchievementType,
    pub reward_xp: u32,
    pub reward_title: Option<String>,
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
            achievement_type,
            reward_xp,
            reward_title,
            icon,
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
        if self.current >= self.target {
            0
        } else {
            self.target - self.current
        }
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
        // 総数マイルストーン
        for count in [10, 50, 100, 250, 500, 1000] {
            self.register_achievement(Achievement::new(
                format!("total_{}", count),
                format!("コレクター Lv.{}", count / 10),
                format!("{}個のキュリオンを集める", count),
                AchievementType::TotalCount(count),
                count as u32 * 10,
                if count >= 1000 {
                    Some("伝説のコレクター".to_string())
                } else {
                    None
                },
                if count >= 1000 { "👑" } else if count >= 500 { "💎" } else { "📦" }.to_string(),
            ));
        }

        // レアリティ別
        for (rarity, counts) in [
            (Rarity::Rare, vec![10, 50, 100]),
            (Rarity::Epic, vec![5, 25, 50]),
            (Rarity::Legendary, vec![1, 5, 10, 50, 100]),
        ] {
            for count in counts {
                self.register_achievement(Achievement::new(
                    format!("{:?}_{}",rarity, count).to_lowercase(),
                    format!("{:?} ハンター {}", rarity, count),
                    format!("{:?}を{}個集める", rarity, count),
                    AchievementType::RarityCount { rarity, count },
                    count as u32 * match rarity {
                        Rarity::Rare => 15,
                        Rarity::Epic => 30,
                        Rarity::Legendary => 100,
                        _ => 10,
                    },
                    if count >= 100 && rarity == Rarity::Legendary {
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
                    }.to_string(),
                ));
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
        ] {
            self.register_achievement(Achievement::new(
                format!("complete_{:?}", category).to_lowercase(),
                format!("{}マスター", category.as_str()),
                format!("{}カテゴリを全種コンプリート", category.as_str()),
                AchievementType::CategoryComplete(category.clone()),
                500,
                Some(format!("{}の覇者", category.as_str())),
                "🏆".to_string(),
            ));
        }

        // 全カテゴリコンプリート
        self.register_achievement(Achievement::new(
            "all_categories".to_string(),
            "完璧主義者".to_string(),
            "全カテゴリをコンプリート".to_string(),
            AchievementType::AllCategoriesComplete,
            5000,
            Some("完全なる収集家".to_string()),
            "👑".to_string(),
        ));

        // 連続ログイン
        for days in [3, 7, 14, 30, 100] {
            self.register_achievement(Achievement::new(
                format!("login_{}", days),
                format!("継続は力なり {}", days),
                format!("{}日連続ログイン", days),
                AchievementType::ConsecutiveLogin(days),
                days as u32 * 50,
                if days >= 100 {
                    Some("不屈の意志".to_string())
                } else {
                    None
                },
                "🔥".to_string(),
            ));
        }

        // プレイ時間
        for hours in [1, 10, 50, 100, 500] {
            self.register_achievement(Achievement::new(
                format!("playtime_{}", hours),
                format!("ベテラン {}", hours),
                format!("{}時間プレイ", hours),
                AchievementType::PlayTime(hours * 60),
                hours as u32 * 20,
                if hours >= 500 {
                    Some("時間の支配者".to_string())
                } else {
                    None
                },
                "⏰".to_string(),
            ));
        }

        // 特殊実績
        self.register_achievement(Achievement::new(
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
        ));

        self.register_achievement(Achievement::new(
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
        ));
    }

    /// 実績を登録
    fn register_achievement(&mut self, achievement: Achievement) {
        let target = self.get_achievement_target(&achievement.achievement_type);
        let progress = AchievementProgress::new(achievement.id.clone(), target);

        self.progress.insert(achievement.id.clone(), progress);
        self.achievements.insert(achievement.id.clone(), achievement);
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
        let mut list: Vec<_> = self.achievements
            .iter()
            .filter_map(|(id, achievement)| {
                self.progress.get(id).map(|progress| (achievement, progress))
            })
            .collect();

        list.sort_by(|a, b| {
            b.1.progress_ratio().partial_cmp(&a.1.progress_ratio()).unwrap()
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
