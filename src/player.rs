use chrono::{DateTime, Utc, Duration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::curion::{Curion, Category, Rarity};
use crate::achievement::{AchievementManager, AchievementProgress};
use crate::synthesis::SynthesisManager;

/// プレイヤー状態
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    /// レベル
    pub level: u32,

    /// 現在の経験値
    pub xp: u32,

    /// 総プレイ時間（秒）
    pub total_play_time: u64,

    /// 初回プレイ日時
    pub first_played_at: DateTime<Utc>,

    /// 最終プレイ日時
    pub last_played_at: DateTime<Utc>,

    /// 連続ログイン日数
    pub consecutive_login_days: u32,

    /// 獲得した称号
    pub titles: Vec<String>,

    /// アクティブな称号
    pub active_title: Option<String>,

    /// 今日の獲得数
    pub today_acquired: u32,

    /// 最高日間獲得数
    pub max_daily_acquired: u32,

    /// 最高日間獲得日
    pub max_daily_acquired_date: Option<DateTime<Utc>>,

    /// カテゴリ別統計
    pub category_stats: HashMap<Category, CategoryStats>,

    /// レアリティ別統計
    pub rarity_stats: HashMap<Rarity, RarityStats>,

    /// コレクション（所持キュリオン）
    pub collection: Vec<Curion>,
}

/// カテゴリ別統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStats {
    pub count: usize,
    pub unique_nouns: HashMap<String, usize>,
    pub most_frequent: Option<String>,
}

impl CategoryStats {
    pub fn new() -> Self {
        Self {
            count: 0,
            unique_nouns: HashMap::new(),
            most_frequent: None,
        }
    }

    pub fn add_curion(&mut self, noun: &str) {
        self.count += 1;
        *self.unique_nouns.entry(noun.to_string()).or_insert(0) += 1;
        self.update_most_frequent();
    }

    fn update_most_frequent(&mut self) {
        self.most_frequent = self.unique_nouns
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(noun, _)| noun.clone());
    }

    pub fn unique_count(&self) -> usize {
        self.unique_nouns.len()
    }
}

impl Default for CategoryStats {
    fn default() -> Self {
        Self::new()
    }
}

/// レアリティ別統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RarityStats {
    pub count: usize,
    pub most_recent: Option<String>,
}

impl RarityStats {
    pub fn new() -> Self {
        Self {
            count: 0,
            most_recent: None,
        }
    }

    pub fn add_curion(&mut self, noun: &str) {
        self.count += 1;
        self.most_recent = Some(noun.to_string());
    }
}

impl Default for RarityStats {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            level: 1,
            xp: 0,
            total_play_time: 0,
            first_played_at: now,
            last_played_at: now,
            consecutive_login_days: 1,
            titles: Vec::new(),
            active_title: None,
            today_acquired: 0,
            max_daily_acquired: 0,
            max_daily_acquired_date: None,
            category_stats: HashMap::new(),
            rarity_stats: HashMap::new(),
            collection: Vec::new(),
        }
    }

    /// 経験値を追加
    pub fn add_xp(&mut self, amount: u32) -> Vec<u32> {
        self.xp += amount;
        let mut level_ups = Vec::new();

        while self.xp >= self.xp_for_next_level() {
            self.xp -= self.xp_for_next_level();
            self.level += 1;
            level_ups.push(self.level);
        }

        level_ups
    }

    /// 次のレベルまでに必要な経験値
    pub fn xp_for_next_level(&self) -> u32 {
        self.level * 100
    }

    /// 次のレベルまでの進捗率
    pub fn xp_progress_ratio(&self) -> f64 {
        (self.xp as f64) / (self.xp_for_next_level() as f64)
    }

    /// 次のレベルまでの進捗率（%）
    pub fn xp_progress_percentage(&self) -> usize {
        (self.xp_progress_ratio() * 100.0) as usize
    }

    /// キュリオンを追加
    pub fn add_curion(&mut self, curion: Curion) -> u32 {
        // カテゴリ統計を更新
        let category_stat = self.category_stats
            .entry(curion.category.clone())
            .or_insert_with(CategoryStats::new);
        category_stat.add_curion(&curion.noun);

        // レアリティ統計を更新
        let rarity_stat = self.rarity_stats
            .entry(curion.rarity)
            .or_insert_with(RarityStats::new);
        rarity_stat.add_curion(&curion.noun);

        // 今日の獲得数を更新
        self.today_acquired += 1;
        if self.today_acquired > self.max_daily_acquired {
            self.max_daily_acquired = self.today_acquired;
            self.max_daily_acquired_date = Some(Utc::now());
        }

        // コレクションに追加
        self.collection.push(curion.clone());

        // 経験値を付与
        let xp = match curion.rarity {
            Rarity::Common => 10,
            Rarity::Rare => 25,
            Rarity::Epic => 50,
            Rarity::Legendary => 200,
        };

        self.add_xp(xp);
        xp
    }

    /// 総獲得数
    pub fn total_acquired(&self) -> usize {
        self.collection.len()
    }

    /// レアリティ別の獲得数
    pub fn count_by_rarity(&self, rarity: Rarity) -> usize {
        self.rarity_stats.get(&rarity).map(|s| s.count).unwrap_or(0)
    }

    /// カテゴリ別の獲得数
    pub fn count_by_category(&self, category: &Category) -> usize {
        self.category_stats.get(category).map(|s| s.count).unwrap_or(0)
    }

    /// カテゴリ別のユニーク数
    pub fn unique_count_by_category(&self, category: &Category) -> usize {
        self.category_stats.get(category).map(|s| s.unique_count()).unwrap_or(0)
    }

    /// 特定の名詞の獲得数
    pub fn count_by_noun(&self, noun: &str) -> usize {
        self.category_stats
            .values()
            .filter_map(|stat| stat.unique_nouns.get(noun))
            .sum()
    }

    /// 平均日間獲得数
    pub fn average_daily_acquired(&self) -> f64 {
        let days_played = self.days_played();
        if days_played == 0 {
            return 0.0;
        }
        (self.total_acquired() as f64) / (days_played as f64)
    }

    /// 獲得レート（個/時間）
    pub fn acquisition_rate_per_hour(&self) -> f64 {
        if self.total_play_time == 0 {
            return 0.0;
        }
        let hours = (self.total_play_time as f64) / 3600.0;
        (self.total_acquired() as f64) / hours
    }

    /// プレイ日数
    pub fn days_played(&self) -> u32 {
        let duration = Utc::now().signed_duration_since(self.first_played_at);
        (duration.num_days() + 1).max(1) as u32
    }

    /// ログイン処理
    pub fn update_login(&mut self) {
        let now = Utc::now();
        let last_date = self.last_played_at.date_naive();
        let today = now.date_naive();

        if last_date == today {
            // 同日ログイン、何もしない
        } else if last_date + Duration::days(1) == today {
            // 連続ログイン
            self.consecutive_login_days += 1;
        } else {
            // ログインが途切れた
            self.consecutive_login_days = 1;
        }

        // 日付が変わったら今日の獲得数をリセット
        if last_date != today {
            self.today_acquired = 0;
        }

        self.last_played_at = now;
    }

    /// プレイ時間を更新（秒）
    pub fn add_play_time(&mut self, seconds: u64) {
        self.total_play_time += seconds;
    }

    /// 称号を追加
    pub fn add_title(&mut self, title: String) {
        if !self.titles.contains(&title) {
            self.titles.push(title);
        }
    }

    /// 称号を設定
    pub fn set_active_title(&mut self, title: Option<String>) {
        if let Some(ref t) = title {
            if self.titles.contains(t) {
                self.active_title = title;
            }
        } else {
            self.active_title = None;
        }
    }

    /// 最新のキュリオンを取得
    pub fn latest_curion(&self) -> Option<&Curion> {
        self.collection.last()
    }

    /// レアリティ分布を計算
    pub fn rarity_distribution(&self) -> HashMap<Rarity, f64> {
        let total = self.total_acquired() as f64;
        if total == 0.0 {
            return HashMap::new();
        }

        let mut dist = HashMap::new();
        for rarity in [Rarity::Common, Rarity::Rare, Rarity::Epic, Rarity::Legendary] {
            let count = self.count_by_rarity(rarity) as f64;
            dist.insert(rarity, count / total);
        }
        dist
    }
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

/// ゲーム状態（プレイヤー + 実績 + 合成）
#[derive(Debug)]
pub struct GameState {
    pub player: Player,
    pub achievement_manager: AchievementManager,
    pub synthesis_manager: SynthesisManager,
}

impl GameState {
    pub fn new(synthesis_manager: SynthesisManager) -> Self {
        Self {
            player: Player::new(),
            achievement_manager: AchievementManager::new(),
            synthesis_manager,
        }
    }

    /// キュリオンを追加し、実績を更新
    pub fn add_curion(&mut self, curion: Curion) -> Vec<String> {
        // プレイヤーにキュリオンを追加
        let _xp_gained = self.player.add_curion(curion.clone());

        // 実績の進捗を更新
        let mut newly_unlocked = Vec::new();

        // 全実績をチェック（まず情報を集める）
        let achievement_updates: Vec<_> = self.achievement_manager
            .get_all_achievements()
            .iter()
            .map(|achievement| {
                let current = match &achievement.achievement_type {
                    crate::achievement::AchievementType::TotalCount(_) => {
                        self.player.total_acquired()
                    }
                    crate::achievement::AchievementType::RarityCount { rarity, .. } => {
                        self.player.count_by_rarity(*rarity)
                    }
                    crate::achievement::AchievementType::CategoryCount { category, .. } => {
                        self.player.count_by_category(category)
                    }
                    crate::achievement::AchievementType::SpecificNoun { noun, .. } => {
                        self.player.count_by_noun(noun)
                    }
                    crate::achievement::AchievementType::ConsecutiveLogin(_) => {
                        self.player.consecutive_login_days as usize
                    }
                    crate::achievement::AchievementType::PlayTime(_) => {
                        (self.player.total_play_time / 60) as usize
                    }
                    _ => 0, // その他は手動更新
                };
                (achievement.id.clone(), current)
            })
            .collect();

        // 進捗を更新
        for (achievement_id, current) in achievement_updates {
            if let Some(progress) = self.achievement_manager.get_progress_mut(&achievement_id) {
                let old_unlocked = progress.unlocked;
                progress.update_progress(current);

                // 新規解除された実績があれば記録
                if !old_unlocked && progress.unlocked {
                    newly_unlocked.push(achievement_id);
                }
            }
        }

        newly_unlocked
    }

    /// 実績報酬を受け取る
    pub fn claim_achievement_reward(&mut self, achievement_id: &str) -> Option<u32> {
        if let Some(progress) = self.achievement_manager.get_progress_mut(achievement_id) {
            if progress.claim_reward() {
                if let Some(achievement) = self.achievement_manager.get_all_achievements()
                    .iter()
                    .find(|a| a.id == achievement_id)
                {
                    // 経験値を付与
                    self.player.add_xp(achievement.reward_xp);

                    // 称号を付与
                    if let Some(ref title) = achievement.reward_title {
                        self.player.add_title(title.clone());
                    }

                    return Some(achievement.reward_xp);
                }
            }
        }
        None
    }

    /// 「もうすぐ達成」の実績を取得（進捗率順）
    pub fn get_almost_complete_achievements(&self, limit: usize) -> Vec<(String, AchievementProgress, f64)> {
        let mut list: Vec<_> = self.achievement_manager
            .get_sorted_by_progress()
            .into_iter()
            .filter(|(_, progress)| !progress.unlocked && progress.progress_ratio() >= 0.3)
            .map(|(achievement, progress)| {
                (
                    achievement.name.clone(),
                    progress.clone(),
                    progress.progress_ratio(),
                )
            })
            .collect();

        list.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
        list.truncate(limit);
        list
    }
}
