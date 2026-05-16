use crate::achievement::{AchievementManager, AchievementProgress};
use crate::curion::{Category, Curion, Rarity};
use crate::daily_mission::{DailyMission, DailyMissionManager};
use crate::synthesis::SynthesisManager;
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 確定チケットの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuaranteedTicket {
    Common,
    Rare,
    Epic,
}

impl GuaranteedTicket {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Common => "コモン確定チケット",
            Self::Rare => "レア確定チケット",
            Self::Epic => "エピック確定チケット",
        }
    }
}

/// ログインボーナスの報酬
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginBonusReward {
    pub day: u32,
    pub xp: u32,
    pub ticket: Option<GuaranteedTicket>,
    pub title: Option<String>,
}

impl LoginBonusReward {
    pub fn summary_lines(&self) -> Vec<String> {
        let mut lines = vec![format!("Day {}: {} XP", self.day, self.xp)];

        if let Some(ticket) = self.ticket {
            lines.push(ticket.label().to_string());
        }

        if let Some(title) = &self.title {
            lines.push(format!("称号: {title}"));
        }

        lines
    }
}

/// 確定チケットの所持数
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TicketInventory {
    pub common: u32,
    pub rare: u32,
    pub epic: u32,
}

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

    /// ログインボーナスを最後に受け取った日
    #[serde(default)]
    pub login_bonus_last_claim_date: Option<NaiveDate>,

    /// 今日のログインボーナスを受け取ったか
    #[serde(default)]
    pub login_bonus_claimed_today: bool,

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

    /// 確定チケット
    #[serde(default)]
    pub guaranteed_tickets: TicketInventory,

    /// カテゴリ別統計
    pub category_stats: HashMap<Category, CategoryStats>,

    /// レアリティ別統計
    pub rarity_stats: HashMap<Rarity, RarityStats>,

    /// コレクション（所持キュリオン）
    pub collection: Vec<Curion>,

    /// デイリーミッション管理（旧セーブには無いため `#[serde(default)]`）
    #[serde(default)]
    pub daily_mission_manager: DailyMissionManager,

    /// 現在のコンボカウント (Rare 以上を連続獲得した数)
    /// Common 獲得で 0 にリセット。旧セーブには無いため `#[serde(default)]`。
    #[serde(default)]
    pub combo_count: u32,

    /// 過去最高コンボカウント。Stats 等で将来表示する用途。
    /// 旧セーブには無いため `#[serde(default)]`。
    #[serde(default)]
    pub max_combo: u32,

    /// 通算入手回数 (Issue #27)
    ///
    /// `collection.len()` は合成で消費されると減ってしまうため、
    /// 別途「過去に何回入手したか」を保持するカウンタを持つ。
    /// `add_curion` が呼ばれるたびに +1 され、その値が
    /// `Curion::acquisition_index` に採番される。
    /// 旧セーブには無いため `#[serde(default)]` (= 0 で復元)。
    #[serde(default)]
    pub total_acquisitions: u32,
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
        self.most_frequent = self
            .unique_nouns
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(noun, _)| noun.clone());
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
            login_bonus_last_claim_date: None,
            login_bonus_claimed_today: false,
            titles: Vec::new(),
            active_title: None,
            today_acquired: 0,
            max_daily_acquired: 0,
            max_daily_acquired_date: None,
            guaranteed_tickets: TicketInventory::default(),
            category_stats: HashMap::new(),
            rarity_stats: HashMap::new(),
            collection: Vec::new(),
            daily_mission_manager: DailyMissionManager::new(),
            combo_count: 0,
            max_combo: 0,
            total_acquisitions: 0,
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
    pub fn add_curion(&mut self, mut curion: Curion) -> u32 {
        // カテゴリ統計を更新
        let category_stat = self
            .category_stats
            .entry(curion.category.clone())
            .or_default();
        category_stat.add_curion(&curion.noun);

        // レアリティ統計を更新
        let rarity_stat = self.rarity_stats.entry(curion.rarity).or_default();
        rarity_stat.add_curion(&curion.noun);

        // 今日の獲得数を更新
        self.today_acquired += 1;
        if self.today_acquired > self.max_daily_acquired {
            self.max_daily_acquired = self.today_acquired;
            self.max_daily_acquired_date = Some(Utc::now());
        }

        // Issue #27: 通算入手回数を採番
        // 合成消費で collection.len() は減ることがあるため、別カウンタで管理する。
        self.total_acquisitions += 1;
        curion.acquisition_index = Some(self.total_acquisitions);

        // コレクションに追加
        self.collection.push(curion.clone());

        // コンボ更新 (Common でリセット、Rare 以上で +1)
        let prev_combo = self.combo_count;
        match curion.rarity {
            Rarity::Common => self.combo_count = 0,
            _ => self.combo_count += 1,
        }
        if self.combo_count > self.max_combo {
            self.max_combo = self.combo_count;
        }

        // ベース経験値
        let base_xp = match curion.rarity {
            Rarity::Common => 10,
            Rarity::Rare => 25,
            Rarity::Epic => 50,
            Rarity::Legendary => 200,
        };

        // コンボ倍率
        let multiplier = match self.combo_count {
            0 | 1 => 1.0,
            2 => 1.5,
            3 | 4 => 2.0,
            _ => 3.0,
        };
        let xp = (base_xp as f64 * multiplier) as u32;

        if self.combo_count == 5 && prev_combo < 5 {
            self.add_title("コンボマスター".to_string());
        }

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
        self.category_stats
            .get(category)
            .map(|s| s.count)
            .unwrap_or(0)
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
        let duration =
            Local::now().signed_duration_since(self.first_played_at.with_timezone(&Local));
        (duration.num_days() + 1).max(1) as u32
    }

    /// 今日のログインボーナス報酬
    pub fn current_login_bonus_reward(&self) -> LoginBonusReward {
        Self::login_bonus_reward_for_day(self.consecutive_login_days.max(1))
    }

    /// 次のログインボーナス報酬
    pub fn next_login_bonus_reward(&self) -> LoginBonusReward {
        Self::login_bonus_reward_for_day(self.consecutive_login_days.saturating_add(1).max(1))
    }

    fn login_bonus_reward_for_day(day: u32) -> LoginBonusReward {
        match day {
            1 => LoginBonusReward {
                day,
                xp: 50,
                ticket: None,
                title: None,
            },
            2 => LoginBonusReward {
                day,
                xp: 100,
                ticket: None,
                title: None,
            },
            3 => LoginBonusReward {
                day,
                xp: 200,
                ticket: Some(GuaranteedTicket::Common),
                title: None,
            },
            4 => LoginBonusReward {
                day,
                xp: 300,
                ticket: None,
                title: None,
            },
            5 => LoginBonusReward {
                day,
                xp: 500,
                ticket: Some(GuaranteedTicket::Rare),
                title: None,
            },
            6 => LoginBonusReward {
                day,
                xp: 800,
                ticket: None,
                title: None,
            },
            7 => LoginBonusReward {
                day,
                xp: 1500,
                ticket: Some(GuaranteedTicket::Epic),
                title: Some("連続加速".to_string()),
            },
            _ => LoginBonusReward {
                day,
                // Day 8+ は仮のエスカレーション。Issue 側で詳細化されるまで差し替えやすく保つ。
                xp: 1500 + (day - 7) * 150,
                ticket: if day % 7 == 0 {
                    Some(GuaranteedTicket::Epic)
                } else if day % 5 == 0 {
                    Some(GuaranteedTicket::Rare)
                } else if day % 3 == 0 {
                    Some(GuaranteedTicket::Common)
                } else {
                    None
                },
                title: match day {
                    14 => Some("二週間の執着".to_string()),
                    30 => Some("習慣の支配者".to_string()),
                    100 => Some("時の蒐集王".to_string()),
                    _ => None,
                },
            },
        }
    }

    fn add_guaranteed_ticket(&mut self, ticket: GuaranteedTicket) {
        match ticket {
            GuaranteedTicket::Common => self.guaranteed_tickets.common += 1,
            GuaranteedTicket::Rare => self.guaranteed_tickets.rare += 1,
            GuaranteedTicket::Epic => self.guaranteed_tickets.epic += 1,
        }
    }

    fn apply_login_bonus_reward(&mut self, reward: &LoginBonusReward) {
        self.add_xp(reward.xp);

        if let Some(ticket) = reward.ticket {
            self.add_guaranteed_ticket(ticket);
        }

        if let Some(title) = &reward.title {
            self.add_title(title.clone());
        }
    }

    /// ログイン処理とログインボーナス自動付与
    pub fn update_login(&mut self) -> Option<LoginBonusReward> {
        let now = Utc::now();
        let today_local = now.with_timezone(&Local).date_naive();
        self.update_login_at(now, today_local)
    }

    fn update_login_at(
        &mut self,
        now: DateTime<Utc>,
        today_local: NaiveDate,
    ) -> Option<LoginBonusReward> {
        let last_date = self.last_played_at.with_timezone(&Local).date_naive();
        let today = today_local;

        if self.login_bonus_last_claim_date == Some(today) {
            self.login_bonus_claimed_today = true;
        }

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
            self.login_bonus_claimed_today = false;
        }

        let reward = if !self.login_bonus_claimed_today {
            let reward = self.current_login_bonus_reward();
            self.apply_login_bonus_reward(&reward);
            self.login_bonus_last_claim_date = Some(today);
            self.login_bonus_claimed_today = true;
            Some(reward)
        } else {
            None
        };

        self.last_played_at = now;
        reward
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

    /// 最新のキュリオンを取得
    pub fn latest_curion(&self) -> Option<&Curion> {
        self.collection.last()
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

    /// 起動時ログイン処理
    ///
    /// 順序が重要:
    ///   1. `update_login()` でログインボーナス処理を行う
    ///   2. **`ensure_today_missions` の前に** `auto_claim_daily_missions()` を呼んで、
    ///      昨夜達成・未受取のまま終了したミッションの XP を救済する。
    ///      ここで claim しないと、直後の `ensure_today_missions(today)` で
    ///      日付ロールオーバー判定→ミッション再生成→ current=0 にリセットされ、
    ///      昨日達成済みだったミッションの XP を取りこぼす。
    ///   3. `ensure_today_missions` で今日のミッション 3 本を生成 (同日なら no-op)
    ///   4. 実績進捗の再計算
    pub fn process_login(&mut self) -> Option<LoginBonusReward> {
        let reward = self.player.update_login();
        // 前日に達成して未 claim のまま終了したミッションを先に回収する。
        // `auto_claim_daily_missions` は現在保持中の missions を見るだけで
        // 日付ベースの分岐は行わないため、再生成前に呼ぶことで取りこぼし救済になる。
        self.auto_claim_daily_missions();
        let today = Local::now().date_naive();
        self.player
            .daily_mission_manager
            .ensure_today_missions(today);
        self.refresh_achievement_progress();
        reward
    }

    /// キュリオンを追加し、実績とデイリーミッションを更新する。
    /// 戻り値は新規解除された実績 ID のリスト（既存仕様を踏襲）。
    pub fn add_curion(&mut self, curion: Curion) -> Vec<String> {
        // プレイヤーにキュリオンを追加
        let _xp_gained = self.player.add_curion(curion.clone());

        // デイリーミッション進捗を更新（日付が変わっていれば先に再生成）
        let today = Local::now().date_naive();
        self.player
            .daily_mission_manager
            .ensure_today_missions(today);
        self.player
            .daily_mission_manager
            .record_curion_acquired(&curion);

        // 実績の進捗を更新
        self.refresh_achievement_progress()
    }

    /// 合成成功を記録し、デイリーミッションの進捗を更新する
    pub fn record_synthesis_success(&mut self) {
        let today = Local::now().date_naive();
        self.player
            .daily_mission_manager
            .ensure_today_missions(today);
        self.player.daily_mission_manager.record_synthesis_success();
    }

    /// 達成済みのデイリーミッションに自動で報酬 (XP) を付与し、
    /// 通知用にミッション情報を返す。
    pub fn auto_claim_daily_missions(&mut self) -> Vec<DailyMission> {
        let claimed = self.player.daily_mission_manager.claim_completed();
        for mission in &claimed {
            self.player.add_xp(mission.reward_xp);
        }
        claimed
    }

    fn refresh_achievement_progress(&mut self) -> Vec<String> {
        let mut newly_unlocked = Vec::new();

        // 全実績をチェック（まず情報を集める）
        let achievement_updates: Vec<_> = self
            .achievement_manager
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
                if let Some(achievement) = self
                    .achievement_manager
                    .get_all_achievements()
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
    pub fn get_almost_complete_achievements(
        &self,
        limit: usize,
    ) -> Vec<(String, AchievementProgress, f64)> {
        let mut list: Vec<_> = self
            .achievement_manager
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    fn dt(y: i32, m: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, 12, 0, 0).unwrap()
    }

    fn dt_hms(y: i32, m: u32, d: u32, h: u32, min: u32, sec: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, sec).unwrap()
    }

    #[test]
    fn login_bonus_is_claimed_only_once_per_day() {
        let mut player = Player::new();
        player.last_played_at = dt(2026, 5, 14);

        let reward = player
            .update_login_at(
                dt(2026, 5, 15),
                NaiveDate::from_ymd_opt(2026, 5, 15).unwrap(),
            )
            .unwrap();
        assert_eq!(reward.day, 2);
        assert_eq!(reward.xp, 100);
        assert!(player.login_bonus_claimed_today);
        assert_eq!(
            player.login_bonus_last_claim_date,
            Some(dt(2026, 5, 15).date_naive())
        );

        let second = player.update_login_at(
            dt(2026, 5, 15),
            NaiveDate::from_ymd_opt(2026, 5, 15).unwrap(),
        );
        assert!(second.is_none());
    }

    #[test]
    fn login_bonus_resets_streak_after_gap() {
        let mut player = Player::new();
        player.consecutive_login_days = 5;
        player.last_played_at = dt(2026, 5, 10);

        let reward = player
            .update_login_at(
                dt(2026, 5, 15),
                NaiveDate::from_ymd_opt(2026, 5, 15).unwrap(),
            )
            .unwrap();
        assert_eq!(player.consecutive_login_days, 1);
        assert_eq!(reward.day, 1);
        assert_eq!(reward.xp, 50);
    }

    #[test]
    fn login_bonus_grants_tickets_and_titles() {
        let mut player = Player::new();
        player.consecutive_login_days = 6;
        player.last_played_at = dt(2026, 5, 14);

        let reward = player
            .update_login_at(
                dt(2026, 5, 15),
                NaiveDate::from_ymd_opt(2026, 5, 15).unwrap(),
            )
            .unwrap();
        assert_eq!(reward.day, 7);
        assert_eq!(player.guaranteed_tickets.epic, 1);
        assert!(player.titles.iter().any(|title| title == "連続加速"));
    }

    #[test]
    fn login_bonus_uses_local_date_boundary_instead_of_utc_date() {
        let mut player = Player::new();
        player.last_played_at = dt_hms(2026, 5, 14, 12, 0, 0);
        player.consecutive_login_days = 4;

        let reward = player
            .update_login_at(
                dt_hms(2026, 5, 14, 23, 30, 0),
                NaiveDate::from_ymd_opt(2026, 5, 15).unwrap(),
            )
            .unwrap();

        assert_eq!(reward.day, 5);
        assert_eq!(player.consecutive_login_days, 5);
    }

    #[test]
    fn legacy_save_fields_default_cleanly() {
        let legacy = json!({
            "level": 1,
            "xp": 0,
            "total_play_time": 0,
            "first_played_at": "2026-05-14T12:00:00Z",
            "last_played_at": "2026-05-14T12:00:00Z",
            "consecutive_login_days": 1,
            "titles": [],
            "active_title": null,
            "today_acquired": 0,
            "max_daily_acquired": 0,
            "max_daily_acquired_date": null,
            "category_stats": {},
            "rarity_stats": {},
            "collection": []
        });

        let player: Player = serde_json::from_value(legacy).unwrap();
        assert_eq!(player.login_bonus_last_claim_date, None);
        assert!(!player.login_bonus_claimed_today);
        assert_eq!(player.guaranteed_tickets.common, 0);
        assert_eq!(player.guaranteed_tickets.rare, 0);
        assert_eq!(player.guaranteed_tickets.epic, 0);
    }

    // -----------------------------------------------------------------
    // Issue #20 デイリーミッション関連のテスト
    // -----------------------------------------------------------------

    #[test]
    fn test_auto_claim_grants_xp() {
        use crate::daily_mission::{DailyMission, DailyMissionKind};
        use crate::synthesis::{RecipeDatabase, SynthesisManager};

        let recipe_db = RecipeDatabase::load_embedded().expect("load recipes");
        let mut state = GameState::new(SynthesisManager::new(recipe_db));
        let xp_before = state.player.xp;

        // ミッションを 1 本だけ手動で達成状態に
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        state.player.daily_mission_manager.missions = vec![DailyMission {
            id: "collect_any_10".to_string(),
            description: "10 個".to_string(),
            kind: DailyMissionKind::CollectAny(10),
            target: 10,
            current: 10,
            reward_xp: 100,
            expires_at: date,
            claimed: false,
        }];
        state.player.daily_mission_manager.generated_date = Some(date);

        let claimed = state.auto_claim_daily_missions();
        assert_eq!(claimed.len(), 1, "達成ミッション 1 本を回収する");
        assert_eq!(claimed[0].reward_xp, 100);

        // XP は add_xp 経由なのでレベルアップで繰り上がる可能性がある。
        // ここでは「+100 XP 相当進んでいる」ことを総量で確認する。
        let xp_now = state.player.xp;
        let total_xp_gained = if state.player.level > 1 {
            // レベルが上がっている場合、消費した XP + 残りの XP で 100 になる想定。
            // level=1, xp_for_next_level=100 から 100 XP 入れると level=2, xp=0。
            xp_now + (1..state.player.level).map(|lvl| lvl * 100).sum::<u32>()
        } else {
            xp_now - xp_before
        };
        assert_eq!(total_xp_gained, 100);

        // 2 度目は重複付与されない
        let claimed_again = state.auto_claim_daily_missions();
        assert!(claimed_again.is_empty());
    }

    #[test]
    fn test_legacy_save_compatibility_with_daily_mission() {
        // 旧セーブ JSON には `daily_mission_manager` フィールドが存在しない。
        // それでも Player::deserialize が default で埋めて読めることを確認する。
        let legacy = json!({
            "level": 1,
            "xp": 0,
            "total_play_time": 0,
            "first_played_at": "2026-05-14T12:00:00Z",
            "last_played_at": "2026-05-14T12:00:00Z",
            "consecutive_login_days": 1,
            "titles": [],
            "active_title": null,
            "today_acquired": 0,
            "max_daily_acquired": 0,
            "max_daily_acquired_date": null,
            "category_stats": {},
            "rarity_stats": {},
            "collection": []
        });

        let player: Player = serde_json::from_value(legacy).expect("旧セーブでも読める");
        // 各サブ構造体は Default で埋まっているはず
        assert!(player.daily_mission_manager.missions.is_empty());
        assert!(player.daily_mission_manager.generated_date.is_none());
        assert!(player
            .daily_mission_manager
            .unique_categories_today
            .is_empty());
    }

    /// レビュー指摘 M1: 日付跨ぎでの XP 取りこぼし回帰テスト
    ///
    /// シナリオ:
    ///   Day1 にミッションを達成 (`claimed=false` のまま終了)
    ///   → Day2 でゲームを起動すると、旧コードでは `ensure_today_missions(day2)` が
    ///      先に走って `current=0` にリセットされ XP が消えていた。
    ///   → 修正後は `auto_claim_daily_missions()` → `ensure_today_missions` の順なので
    ///      Day1 分の XP が確実に加算される。
    ///
    /// `process_login` を直接呼ぶと `Local::now()` 依存でテストが脆くなるため、
    /// `process_login` 内部のステップを順序通りに呼び出して同じ性質を検証する。
    #[test]
    fn test_xp_not_lost_when_date_rolls_over() {
        use crate::daily_mission::{DailyMission, DailyMissionKind};
        use crate::synthesis::{RecipeDatabase, SynthesisManager};

        let recipe_db = RecipeDatabase::load_embedded().expect("load recipes");
        let mut state = GameState::new(SynthesisManager::new(recipe_db));
        let xp_before = state.player.xp;
        let level_before = state.player.level;

        let day1 = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let day2 = NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();

        // Day1: ミッション 1 本を達成済み (未受取) で保存して終了したシナリオ
        state.player.daily_mission_manager.missions = vec![DailyMission {
            id: "collect_any_10".to_string(),
            description: "10 個収集".to_string(),
            kind: DailyMissionKind::CollectAny(10),
            target: 10,
            current: 10,
            reward_xp: 100,
            expires_at: day1,
            claimed: false,
        }];
        state.player.daily_mission_manager.generated_date = Some(day1);

        // Day2 起動時: process_login が辿る順序を再現
        //   1) auto_claim_daily_missions() で前日分の XP を救う
        //   2) ensure_today_missions(day2) でミッションを再生成
        let claimed = state.auto_claim_daily_missions();
        assert_eq!(claimed.len(), 1, "Day1 分が claim される");
        assert!(
            state.player.daily_mission_manager.missions[0].claimed,
            "claim フラグが立つ"
        );

        state
            .player
            .daily_mission_manager
            .ensure_today_missions(day2);

        // Day2 のミッションが新たに生成されている
        assert_eq!(
            state.player.daily_mission_manager.generated_date,
            Some(day2)
        );
        assert!(
            !state.player.daily_mission_manager.missions.is_empty(),
            "Day2 のミッションが生成されている"
        );
        assert!(
            state
                .player
                .daily_mission_manager
                .missions
                .iter()
                .all(|m| m.current == 0 && !m.claimed),
            "Day2 のミッションは current=0, claimed=false で開始"
        );

        // 100 XP が確かに加算されている (レベルアップで level=2, xp=0 になる想定)
        let xp_now = state.player.xp;
        let total_xp_gained = if state.player.level > level_before {
            xp_now
                + (level_before..state.player.level)
                    .map(|lvl| lvl * 100)
                    .sum::<u32>()
        } else {
            xp_now - xp_before
        };
        assert_eq!(total_xp_gained, 100, "前日分の +100 XP が消えていないこと");
    }

    /// レビュー指摘 M1: 順序が逆だった場合の挙動を明示するための回帰確認。
    /// 「`ensure_today_missions` を先に呼ぶ」と claim 対象が失われることを直接検証し、
    /// 修正後のステップ順 (claim → ensure) を維持する根拠を残す。
    #[test]
    fn test_xp_is_lost_if_ensure_runs_before_claim() {
        use crate::daily_mission::{DailyMission, DailyMissionKind};
        use crate::synthesis::{RecipeDatabase, SynthesisManager};

        let recipe_db = RecipeDatabase::load_embedded().expect("load recipes");
        let mut state = GameState::new(SynthesisManager::new(recipe_db));
        let xp_before = state.player.xp;

        let day1 = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let day2 = NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();

        state.player.daily_mission_manager.missions = vec![DailyMission {
            id: "collect_any_10".to_string(),
            description: "10 個収集".to_string(),
            kind: DailyMissionKind::CollectAny(10),
            target: 10,
            current: 10,
            reward_xp: 100,
            expires_at: day1,
            claimed: false,
        }];
        state.player.daily_mission_manager.generated_date = Some(day1);

        // 順序を逆にした場合 (旧バグ挙動の再現)
        state
            .player
            .daily_mission_manager
            .ensure_today_missions(day2);
        let claimed = state.auto_claim_daily_missions();

        // 再生成済みなので claim 対象は無く、XP は増えない
        assert!(claimed.is_empty(), "順序が逆だと claim 対象が消える");
        assert_eq!(
            state.player.xp, xp_before,
            "順序が逆だと XP が増えない (= バグ再現)"
        );
    }

    // -----------------------------------------------------------------
    // Issue #21 コンボシステム関連のテスト
    // -----------------------------------------------------------------

    /// テスト用 Curion を生成 (rarity だけ可変)
    fn combo_test_curion(rarity: Rarity) -> Curion {
        Curion::new(
            uuid::Uuid::new_v4(),
            "テスト".to_string(),
            Category::Concept,
            rarity,
            50.0,
            50.0,
        )
    }

    #[test]
    fn test_combo_increments_on_rare() {
        let mut player = Player::new();
        player.add_curion(combo_test_curion(Rarity::Rare));
        assert_eq!(player.combo_count, 1);
        player.add_curion(combo_test_curion(Rarity::Rare));
        assert_eq!(player.combo_count, 2);
    }

    #[test]
    fn test_combo_resets_on_common() {
        let mut player = Player::new();
        player.add_curion(combo_test_curion(Rarity::Rare));
        player.add_curion(combo_test_curion(Rarity::Rare));
        player.add_curion(combo_test_curion(Rarity::Rare));
        assert_eq!(player.combo_count, 3);

        player.add_curion(combo_test_curion(Rarity::Common));
        assert_eq!(player.combo_count, 0, "Common でコンボがリセットされる");
    }

    #[test]
    fn test_combo_count_caps_growth_via_max_combo() {
        let mut player = Player::new();
        // combo を 5 まで上げる
        for _ in 0..5 {
            player.add_curion(combo_test_curion(Rarity::Rare));
        }
        assert_eq!(player.combo_count, 5);
        assert_eq!(player.max_combo, 5);

        // Common でリセット
        player.add_curion(combo_test_curion(Rarity::Common));
        assert_eq!(player.combo_count, 0);
        assert_eq!(player.max_combo, 5, "max_combo は最高値を維持する");

        // 再度 Rare を 1 回
        player.add_curion(combo_test_curion(Rarity::Rare));
        assert_eq!(player.combo_count, 1);
        assert_eq!(player.max_combo, 5);
    }

    #[test]
    fn test_combo_xp_multipliers() {
        // combo=1, base 25 → 25
        let mut player = Player::new();
        let xp1 = player.add_curion(combo_test_curion(Rarity::Rare));
        assert_eq!(player.combo_count, 1);
        assert_eq!(xp1, 25, "combo 1: 1.0x");

        // combo=2, base 25 → 25 * 1.5 = 37.5 → 37 (f64→u32 切り捨て)
        let xp2 = player.add_curion(combo_test_curion(Rarity::Rare));
        assert_eq!(player.combo_count, 2);
        assert_eq!(xp2, 37, "combo 2: 1.5x で 37.5 切り捨て → 37");

        // combo=3, base 25 → 25 * 2.0 = 50
        let xp3 = player.add_curion(combo_test_curion(Rarity::Rare));
        assert_eq!(player.combo_count, 3);
        assert_eq!(xp3, 50, "combo 3: 2.0x");

        // combo=4 (still 2.0x), then combo=5 (3.0x → 75)
        let _ = player.add_curion(combo_test_curion(Rarity::Rare));
        let xp5 = player.add_curion(combo_test_curion(Rarity::Rare));
        assert_eq!(player.combo_count, 5);
        assert_eq!(xp5, 75, "combo 5: 3.0x");

        // Common はコンボを 0 にリセットし、base 10 のまま付与
        let xp_common = player.add_curion(combo_test_curion(Rarity::Common));
        assert_eq!(player.combo_count, 0);
        assert_eq!(xp_common, 10, "Common: combo 0 + 1.0x で base 10");
    }

    #[test]
    fn test_combo_master_title_awarded_at_5() {
        let mut player = Player::new();
        for _ in 0..4 {
            player.add_curion(combo_test_curion(Rarity::Rare));
        }
        assert!(
            !player.titles.iter().any(|t| t == "コンボマスター"),
            "combo 4 ではまだ称号は付与されない"
        );
        player.add_curion(combo_test_curion(Rarity::Rare));
        assert_eq!(player.combo_count, 5);
        assert!(
            player.titles.iter().any(|t| t == "コンボマスター"),
            "combo 5 で「コンボマスター」が付与される"
        );
    }

    #[test]
    fn test_combo_master_title_not_duplicated() {
        let mut player = Player::new();
        // 1 回目: combo 5 まで
        for _ in 0..5 {
            player.add_curion(combo_test_curion(Rarity::Rare));
        }
        // リセットして再度 5 まで
        player.add_curion(combo_test_curion(Rarity::Common));
        for _ in 0..5 {
            player.add_curion(combo_test_curion(Rarity::Rare));
        }
        let master_count = player
            .titles
            .iter()
            .filter(|t| *t == "コンボマスター")
            .count();
        assert_eq!(master_count, 1, "称号は重複付与されない");
    }

    #[test]
    fn test_combo_serde_default_for_legacy_save() {
        // combo_count / max_combo フィールドの無い旧 JSON でロードできる
        let legacy = json!({
            "level": 1,
            "xp": 0,
            "total_play_time": 0,
            "first_played_at": "2026-05-14T12:00:00Z",
            "last_played_at": "2026-05-14T12:00:00Z",
            "consecutive_login_days": 1,
            "titles": [],
            "active_title": null,
            "today_acquired": 0,
            "max_daily_acquired": 0,
            "max_daily_acquired_date": null,
            "category_stats": {},
            "rarity_stats": {},
            "collection": []
        });

        let player: Player = serde_json::from_value(legacy).expect("旧セーブでも読める");
        assert_eq!(player.combo_count, 0);
        assert_eq!(player.max_combo, 0);
    }

    #[test]
    fn test_combo_legendary_counts_toward_combo() {
        let mut player = Player::new();
        player.add_curion(combo_test_curion(Rarity::Legendary));
        assert_eq!(player.combo_count, 1);
        player.add_curion(combo_test_curion(Rarity::Epic));
        assert_eq!(player.combo_count, 2);
        player.add_curion(combo_test_curion(Rarity::Legendary));
        assert_eq!(player.combo_count, 3);
    }

    // -----------------------------------------------------------------
    // Issue #27 入手履歴 (acquisition_index / total_acquisitions)
    // -----------------------------------------------------------------

    fn history_test_curion(rarity: Rarity) -> Curion {
        Curion::new(
            uuid::Uuid::new_v4(),
            "履歴テスト".to_string(),
            Category::Concept,
            rarity,
            50.0,
            50.0,
        )
    }

    #[test]
    fn test_total_acquisitions_increments() {
        let mut player = Player::new();
        assert_eq!(player.total_acquisitions, 0);
        for _ in 0..3 {
            player.add_curion(history_test_curion(Rarity::Rare));
        }
        assert_eq!(
            player.total_acquisitions, 3,
            "add_curion 3 回で total_acquisitions=3"
        );
    }

    #[test]
    fn test_acquisition_index_assigned_in_order() {
        let mut player = Player::new();
        for _ in 0..3 {
            player.add_curion(history_test_curion(Rarity::Rare));
        }
        let indices: Vec<_> = player
            .collection
            .iter()
            .map(|c| c.acquisition_index)
            .collect();
        assert_eq!(
            indices,
            vec![Some(1), Some(2), Some(3)],
            "追加順に 1, 2, 3 が採番される"
        );
    }

    #[test]
    fn test_acquisition_index_persists_through_synthesis() {
        // 合成で collection から消えても、total_acquisitions は減らないので
        // 次の入手は前回の続きの番号で採番される。
        let mut player = Player::new();
        for _ in 0..3 {
            player.add_curion(history_test_curion(Rarity::Rare));
        }
        assert_eq!(player.total_acquisitions, 3);
        assert_eq!(player.collection.len(), 3);

        // 合成で 2 個消費したシミュレーション (collection を 2 個削る)。
        // total_acquisitions は触らない (= 入手履歴は減らない) のが本機能の肝。
        player.collection.drain(0..2);
        assert_eq!(player.collection.len(), 1);
        assert_eq!(
            player.total_acquisitions, 3,
            "合成消費で total_acquisitions は減らない"
        );

        // 次に入手したものは 4 番目として採番される
        player.add_curion(history_test_curion(Rarity::Rare));
        assert_eq!(player.total_acquisitions, 4);
        let last = player.collection.last().expect("collection 非空");
        assert_eq!(
            last.acquisition_index,
            Some(4),
            "合成後の新規入手は 4 番目になる (連番が維持される)"
        );
    }

    #[test]
    fn test_legacy_save_acquisition_index_is_none() {
        // 旧セーブの Curion JSON には acquisition_index フィールドが無い。
        // それでも Curion::deserialize は default で None を埋めて成功する。
        let legacy_curion = json!({
            "id": "abc",
            "source_guid": "00000000-0000-0000-0000-000000000000",
            "noun": "魚",
            "category": "Animal",
            "rarity": "Common",
            "interest": 0.5,
            "beauty": 0.5,
            "acquired_at": "2026-05-14T12:00:00Z"
        });
        let curion: Curion =
            serde_json::from_value(legacy_curion).expect("旧 Curion JSON が読める");
        assert_eq!(curion.acquisition_index, None);
    }

    #[test]
    fn test_legacy_save_total_acquisitions_default_zero() {
        // 旧 Player JSON には total_acquisitions フィールドが無い。
        // それでも default で 0 が入る。
        let legacy = json!({
            "level": 1,
            "xp": 0,
            "total_play_time": 0,
            "first_played_at": "2026-05-14T12:00:00Z",
            "last_played_at": "2026-05-14T12:00:00Z",
            "consecutive_login_days": 1,
            "titles": [],
            "active_title": null,
            "today_acquired": 0,
            "max_daily_acquired": 0,
            "max_daily_acquired_date": null,
            "category_stats": {},
            "rarity_stats": {},
            "collection": []
        });
        let player: Player = serde_json::from_value(legacy).expect("旧 Player JSON が読める");
        assert_eq!(player.total_acquisitions, 0);
    }

    #[test]
    fn test_format_acquisition_detail_with_index() {
        // acquisition_index = Some(N) の場合の表示フォーマット
        let mut curion = history_test_curion(Rarity::Rare);
        curion.acquired_at = Utc.with_ymd_and_hms(2026, 5, 14, 14, 47, 0).unwrap();
        curion.acquisition_index = Some(142);

        let detail = curion.format_acquisition_detail();
        // 「(通算 142回目の収集)」が含まれる
        assert!(
            detail.contains("(通算 142回目の収集)"),
            "通算回数の表記が含まれる: {detail}"
        );
        // 「入手: 」プレフィックスが付く
        assert!(detail.starts_with("入手: "), "プレフィックス: {detail}");
        // Local TZ への変換が行われた YYYY-MM-DD HH:MM が含まれる
        // (TZ 依存だが日付の桁数フォーマット自体は不変)
        // 例: "入手: 2026-05-14 23:47  (通算 142回目の収集)"
        assert!(
            detail.contains("2026-05-1"),
            "日付らしき文字列を含む: {detail}"
        );
    }

    #[test]
    fn test_format_acquisition_detail_without_index() {
        // acquisition_index = None (旧セーブ) の場合は「履歴情報なし」になる
        let mut curion = history_test_curion(Rarity::Rare);
        curion.acquired_at = Utc.with_ymd_and_hms(2026, 5, 14, 14, 47, 0).unwrap();
        curion.acquisition_index = None;

        let detail = curion.format_acquisition_detail();
        assert!(
            detail.contains("(履歴情報なし)"),
            "「履歴情報なし」が含まれる: {detail}"
        );
        assert!(
            !detail.contains("通算"),
            "通算回数の表記は含まれない: {detail}"
        );
    }
}
