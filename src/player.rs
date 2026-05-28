use crate::achievement::{AchievementManager, AchievementProgress};
use crate::curion::{Category, Curion, Rarity};
use crate::daily_mission::{DailyMission, DailyMissionManager};
use crate::equipment::{EquipmentEffect, EquipmentSlot};
use crate::san::{apply_decay, apply_gain, san_gain_for_acquisition, SAN_GAIN_SYNTHESIS, SAN_MAX};
use crate::synthesis::SynthesisManager;
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Issue #32 きりの悪い数字設計: Lv.N -> Lv.N+1 に必要な XP テーブル。
///
/// 等差 (`level * 100`) は「あと 50 でレベルアップ…切りがいい所まで」みたいに
/// 区切られて飽きやすいので、わざと割り切れない数列にしている。
/// 末尾 (Lv.20 以降) は `extrapolate_xp_for_next_level()` で外挿する。
const XP_THRESHOLDS: &[u32] = &[
    100, 270, 510, 870, 1280, 1820, 2450, 3210, 4080, 5060, 6170, 7400, 8770, 10260, 11900, 13680,
    15600, 17680, 19920, 22320,
];

/// 表の範囲外 (Lv.21 以降) の XP 閾値を外挿する。
///
/// 表の最後 (Lv.20 の閾値 22320) を起点に、`last + (last / 10) * 1.18` 風の
/// 漸近指数で伸ばす。Lv.30〜100 でも 0 を返さず、十分大きな数字になる。
fn extrapolate_xp_for_next_level(level: u32) -> u32 {
    let table_len = XP_THRESHOLDS.len() as u32;
    if level <= table_len {
        return XP_THRESHOLDS[(level - 1) as usize];
    }
    let mut current = *XP_THRESHOLDS.last().expect("XP_THRESHOLDS is non-empty") as f64;
    // table_len から level までの分だけ +1.18%×(current/10) 風に伸ばす。
    // u32 直前で頭打ちにして単調増加を保つ (各レベル +1 で 1 ずつ増える)。
    const CAP: f64 = u32::MAX as f64 - 1.0;
    let mut steps_at_cap: f64 = 0.0;
    for _ in table_len..level {
        if current >= CAP {
            // u32::MAX を超える前にこれ以上の指数成長を止め、毎レベル +1 だけ加算。
            steps_at_cap += 1.0;
            continue;
        }
        let increment = (current / 10.0) * 1.18;
        current = (current + increment).min(CAP);
    }
    ((current + steps_at_cap).min(u32::MAX as f64)) as u32
}

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

    /// 最終収集時刻 (Issue #25 レア出現予告クールダウン)
    ///
    /// `add_curion` のたびに `Some(Utc::now())` に更新される。
    /// `None` の間 (= 初回起動 or 旧セーブ) はフルボーナス扱い。
    /// 旧セーブには無いため `#[serde(default)]` (= None で復元)。
    #[serde(default)]
    pub last_collection_at: Option<DateTime<Utc>>,

    /// Issue #78: 手動生成（Space キー）の最終実行時刻。
    ///
    /// `generate_curion` 成功時に更新される。
    /// `None`（初回・旧セーブ）はクールダウンなし（即生成可能）扱い。
    /// 旧セーブには無いため `#[serde(default)]` (= None で復元)。
    #[serde(default)]
    pub last_manual_generate_at: Option<DateTime<Utc>>,

    /// Issue #78: 自動ドロー（1 時間インターバル）の最終ドロー時刻。
    ///
    /// 起動時に `pending_auto_draws` を計算し、溜まった分を一括適用した後に更新。
    /// `None`（初回・旧セーブ）は自動ドローを発生させない（初回大量ドロー防止）。
    /// 旧セーブには無いため `#[serde(default)]` (= None で復元)。
    #[serde(default)]
    pub last_auto_draw_at: Option<DateTime<Utc>>,

    /// SAN 値 (正気度) (Issue #29)
    ///
    /// 0.0〜100.0 の `f64`。初期値 100.0。
    /// - キュリオン収集 / 合成成功で回復 (`san_gain_for_acquisition`, `SAN_GAIN_SYNTHESIS`)
    /// - 時間経過で減少 (`SAN_DECAY_PER_MINUTE` per minute)
    ///
    /// 旧セーブには無いため `#[serde(default = "default_san")]` (= 100.0 で復元)。
    #[serde(default = "default_san")]
    pub san: f64,

    /// 装備スロット (Issue #38)
    ///
    /// 装備中 Curion の id を 1 つだけ保持する。装備中の Curion から導出される
    /// [`EquipmentEffect`] が `add_curion` の XP 計算と `add_play_time` の SAN 減衰に
    /// 常時適用される。装備なし、または装備対象が `collection` から消えていた場合
    /// (合成消費や寿命切れ) は `EquipmentEffect::none()` 扱いになり、振る舞いは変化しない。
    ///
    /// 旧セーブには無いため `#[serde(default)]` (= 空スロットで復元)。
    #[serde(default)]
    pub equipment: EquipmentSlot,
}

/// 旧セーブに `san` フィールドが無い場合のデフォルト値 (Issue #29)。
/// SAN は初回プレイ時に最大値で始まる仕様。
fn default_san() -> f64 {
    SAN_MAX
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
            last_collection_at: None,
            last_manual_generate_at: None,
            last_auto_draw_at: None,
            san: SAN_MAX,
            equipment: EquipmentSlot::default(),
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

    /// 次のレベルまでに必要な経験値 (Issue #32: きりの悪い非線形テーブル)
    pub fn xp_for_next_level(&self) -> u32 {
        if self.level == 0 {
            // 想定外だが万全のため Lv.1 として扱う
            return XP_THRESHOLDS[0];
        }
        let idx = (self.level - 1) as usize;
        XP_THRESHOLDS
            .get(idx)
            .copied()
            .unwrap_or_else(|| extrapolate_xp_for_next_level(self.level))
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
        self.total_acquisitions = self.total_acquisitions.saturating_add(1);
        curion.acquisition_index = Some(self.total_acquisitions);

        // Issue #25: 最終収集時刻を更新 (レア出現予告クールダウンをリセット)
        self.last_collection_at = Some(Utc::now());

        // Issue #29: SAN 値をレアリティに応じて回復 (Common +0.5 〜 Legendary +15.0)
        self.san = apply_gain(self.san, san_gain_for_acquisition(curion.rarity));

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

        // Issue #38: 装備中 Curion から導出される XP 倍率を乗算する
        // (未装備 or 装備対象が collection から消えていれば 1.0 = 影響なし)。
        let equipment_xp_multiplier = self.current_equipment_effect().xp_multiplier;
        let xp = (base_xp as f64 * multiplier * equipment_xp_multiplier) as u32;

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

    /// 直近 `days` 日分の日別獲得数を返す（昇順、長さは `days`）。
    ///
    /// `now` を引数で受け取り、テスト時には固定値で呼び出せるようにしている。
    /// Issue #26: Stats タブ「時系列」の Sparkline 用ピュア関数。
    ///
    /// - 返り値 `v[i]` は「now の Local 日付から `(days - 1 - i)` 日前」の獲得数
    /// - つまり `v[days - 1]` は今日（now の Local 日付）の獲得数
    /// - `days == 0` のときは空の `Vec` を返す
    /// - 集計対象は `now` の Local 日付を起点に過去 `days` 日のウィンドウ内のみ
    pub fn daily_acquisition_counts(&self, days: usize, now: DateTime<Utc>) -> Vec<u64> {
        if days == 0 {
            return Vec::new();
        }

        let today = now.with_timezone(&Local).date_naive();
        // window_start = today - (days - 1) 日。今日も含めて days 日分。
        let offset = (days as i64).saturating_sub(1);
        let window_start = today - Duration::days(offset);

        let mut buckets = vec![0_u64; days];
        for curion in &self.collection {
            let acquired_date = curion.acquired_at.with_timezone(&Local).date_naive();
            if acquired_date < window_start || acquired_date > today {
                continue;
            }
            let diff = (acquired_date - window_start).num_days();
            if diff < 0 {
                continue;
            }
            let idx = diff as usize;
            if idx < days {
                buckets[idx] += 1;
            }
        }
        buckets
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
    ///
    /// Issue #29: 経過時間に応じて SAN 値も減少させる。
    /// 1 分あたり `SAN_DECAY_PER_MINUTE` (= 0.1) ずつ減らし、0 でクランプ。
    /// Issue #38: 装備中 Curion の `san_decay_modifier` を経過分に乗算する
    /// (= purity 高い curion を装備すると放置による減りが緩む)。
    pub fn add_play_time(&mut self, seconds: u64) {
        self.total_play_time += seconds;

        // Issue #29 + #38: 時間経過による SAN 減少 (放置で減る)。
        // 装備中の curion から得た san_decay_modifier (1.0 = 変化なし、0.5 = 半減) を
        // 経過分にそのまま乗算する。未装備なら 1.0 で振る舞いは旧来と同じ。
        let minutes_elapsed = (seconds as f64) / 60.0;
        let san_decay_modifier = self.current_equipment_effect().san_decay_modifier;
        self.san = apply_decay(self.san, minutes_elapsed * san_decay_modifier);
    }

    /// 称号を追加
    pub fn add_title(&mut self, title: String) {
        if !self.titles.contains(&title) {
            self.titles.push(title);
        }
    }

    /// 期限切れキュリオンを collection から取り除き、削除した一覧を返す (Issue #30)。
    ///
    /// `Curion::is_expired(now)` が true のものを除外する。
    /// `lifespan_days = None` (旧セーブ等の永遠キュリオン) は対象外。
    /// 統計 (`rarity_stats` / `category_stats`) は触らない — これらは
    /// 「過去に何個入手したか」を表す累積カウンタとして扱い、自然消滅で
    /// 履歴を消さない方針。
    pub fn prune_expired(&mut self, now: DateTime<Utc>) -> Vec<Curion> {
        let mut removed = Vec::new();
        let mut kept = Vec::with_capacity(self.collection.len());
        for c in self.collection.drain(..) {
            if c.is_expired(now) {
                removed.push(c);
            } else {
                kept.push(c);
            }
        }
        self.collection = kept;
        removed
    }

    /// 最新のキュリオンを取得
    pub fn latest_curion(&self) -> Option<&Curion> {
        self.collection.last()
    }

    // -----------------------------------------------------------------
    // Issue #38: 装備システム
    // -----------------------------------------------------------------

    /// 装備中の Curion を取得する (`equipment.curion_id` に対応する collection 要素)。
    ///
    /// 装備されていない、または装備対象の id が collection に無い (合成消費・
    /// 寿命切れで消えた) 場合は `None`。
    pub fn equipped_curion(&self) -> Option<&Curion> {
        let id = self.equipment.curion_id.as_deref()?;
        self.collection.iter().find(|c| c.id == id)
    }

    /// 現在装備中の Curion から導出される効果を返す。
    ///
    /// 装備なし or 装備対象不在のときは [`EquipmentEffect::none`] (= baseline)。
    /// 呼び出し側はこの値を「ロジックに乗算しても変化しない値」として扱える。
    pub fn current_equipment_effect(&self) -> EquipmentEffect {
        match self.equipped_curion() {
            Some(c) => EquipmentEffect::from_curion(c),
            None => EquipmentEffect::none(),
        }
    }

    /// 指定 id の Curion を装備する。
    ///
    /// - `curion_id` が collection に無い場合は何もしない (= 装備状態は変わらない)。
    /// - 既に同じ id を装備しているなら no-op。
    /// - 既に別の curion を装備していたなら自動で取り替え。
    pub fn equip(&mut self, curion_id: &str) {
        if !self.collection.iter().any(|c| c.id == curion_id) {
            return;
        }
        self.equipment.curion_id = Some(curion_id.to_string());
    }

    /// 装備を解除する (slot を空にする)。
    pub fn unequip(&mut self) {
        self.equipment.curion_id = None;
    }

    /// 同じ id なら解除、違う id なら装備し直すトグル。
    /// `curion_id` が collection に無い場合は無視 (`equip` と同じ)。
    pub fn toggle_equip(&mut self, curion_id: &str) {
        if self.equipment.curion_id.as_deref() == Some(curion_id) {
            self.unequip();
        } else {
            self.equip(curion_id);
        }
    }

    /// 次のレベルまでの XP マイルストーンを返す (Issue #32)。
    ///
    /// 「次のレベルまであと X XP」を `MilestoneHint` で返す。
    /// 実績側のマイルストーンと一緒に `MilestoneHint::pick_smallest` で
    /// 残量最小のものを選ぶ用途を想定している。
    pub fn next_level_milestone(&self) -> MilestoneHint {
        let target = self.xp_for_next_level();
        let remaining = target.saturating_sub(self.xp);
        MilestoneHint {
            label: format!("Lv.{} → Lv.{}", self.level, self.level + 1),
            remaining,
        }
    }
}

/// Issue #32 きりの悪い数字設計: マイルストーンヒント。
///
/// Dashboard の "next milestone: ⭐ コレクター Lv.3 (あと 4 個)" に使う。
/// `remaining` は「あとどれだけで達成か」の数値で、複数候補から最も小さい
/// (= もうすぐ達成できる) ものを選ぶための比較キーになる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MilestoneHint {
    /// マイルストーンの表示名 (例: `"⭐ コレクター Lv.3"` や `"Lv.3 → Lv.4"`)
    pub label: String,
    /// 達成までの残り個数 / 残り XP。0 はもう達成済み (普通は返らない想定)。
    pub remaining: u32,
}

impl MilestoneHint {
    /// 候補の中から `remaining` が最小のものを返す (= 「あと少し感」が最大)。
    /// 残量 0 は除外する (達成済みのものは候補にしない)。
    pub fn pick_smallest(candidates: impl IntoIterator<Item = MilestoneHint>) -> Option<Self> {
        candidates
            .into_iter()
            .filter(|c| c.remaining > 0)
            .min_by_key(|c| c.remaining)
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
    /// UI 言語 (Issue #63 Phase 1)。
    ///
    /// 既定値は [`Language::En`] (英語正本化)。`SerializableGameState` 経由で
    /// 永続化され、旧セーブ (フィールド無し) は `#[serde(default)]` により
    /// `Language::En` で復元される。
    pub language: crate::i18n::Language,
}

impl GameState {
    pub fn new(synthesis_manager: SynthesisManager) -> Self {
        Self {
            player: Player::new(),
            achievement_manager: AchievementManager::new(),
            synthesis_manager,
            language: crate::i18n::Language::default(),
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

    /// 期限切れキュリオンを起動時に削除する (Issue #30)。
    ///
    /// `process_login` の後に呼び出して、UI 側で削除されたキュリオンの
    /// 通知 (トースト等) に使うことを想定している。実績/ミッション進捗には
    /// 影響を与えない (削除は履歴 = `rarity_stats` 等に残す)。
    pub fn prune_expired_curions(&mut self, now: DateTime<Utc>) -> Vec<Curion> {
        self.player.prune_expired(now)
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
    ///
    /// Issue #29: SAN 値も `SAN_GAIN_SYNTHESIS` (+3.0) 回復する。
    pub fn record_synthesis_success(&mut self) {
        let today = Local::now().date_naive();
        self.player
            .daily_mission_manager
            .ensure_today_missions(today);
        self.player.daily_mission_manager.record_synthesis_success();

        // Issue #29: SAN 値回復 (合成成功 +3.0)
        self.player.san = apply_gain(self.player.san, SAN_GAIN_SYNTHESIS);
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

    /// 次のマイルストーンを返す (Issue #32)。
    ///
    /// 「あと少し感」を演出するため、以下の候補から `remaining` が最小のものを選ぶ:
    ///
    /// 1. 次レベルまでの XP (`Player::next_level_milestone`)
    /// 2. 未解除実績のうち残量が最小のもの
    ///    (RarityCount / TotalCount / CategoryCount / SpecificNoun / ConsecutiveLogin /
    ///    PlayTime を網羅。残量は `target - current` を実績名ベースで生成)
    ///
    /// 全部達成済み、または開始 0% から始まる長期目標しか残っていない場合は `None`。
    pub fn next_milestone(&self, lang: crate::i18n::Language) -> Option<MilestoneHint> {
        let mut candidates: Vec<MilestoneHint> = Vec::new();
        candidates.push(self.player.next_level_milestone());

        for (achievement, progress) in self.achievement_manager.get_sorted_by_progress() {
            if progress.unlocked {
                continue;
            }
            let remaining = progress.remaining();
            if remaining == 0 {
                continue;
            }
            // 残量が大きすぎる (進捗率 < 30% かつ remaining > 50) ものは
            // 「あと少し感」が出ないので候補から外す。残量数字の小さい実績
            // (Legendary 1 個など) は進捗率が低くても拾いたいので残量で足切り。
            if progress.progress_ratio() < 0.3 && remaining > 50 {
                continue;
            }
            candidates.push(MilestoneHint {
                label: format!("{} {}", achievement.icon, achievement.name_for(lang)),
                remaining: remaining as u32,
            });
        }

        MilestoneHint::pick_smallest(candidates)
    }

    /// 「もうすぐ達成」の実績を取得（進捗率順）
    pub fn get_almost_complete_achievements(
        &self,
        limit: usize,
    ) -> Vec<(String, AchievementProgress, f64)> {
        self.get_almost_complete_achievements_lang(limit, crate::i18n::Language::Ja)
    }

    /// Issue #71 Phase 4: 言語別の名前を返すバリアント。
    pub fn get_almost_complete_achievements_lang(
        &self,
        limit: usize,
        lang: crate::i18n::Language,
    ) -> Vec<(String, AchievementProgress, f64)> {
        let mut list: Vec<_> = self
            .achievement_manager
            .get_sorted_by_progress()
            .into_iter()
            .filter(|(_, progress)| !progress.unlocked && progress.progress_ratio() >= 0.3)
            .map(|(achievement, progress)| {
                (
                    achievement.name_for(lang).to_string(),
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
            description_en: String::new(),
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
        // Issue #32 で閾値が非線形化したので、各レベルの実消費分を `XP_THRESHOLDS` から拾う。
        let xp_now = state.player.xp;
        let total_xp_gained = if state.player.level > 1 {
            let consumed: u32 = (1..state.player.level)
                .map(|lvl| {
                    let mut tmp = Player::new();
                    tmp.level = lvl;
                    tmp.xp_for_next_level()
                })
                .sum();
            xp_now + consumed
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
            description_en: String::new(),
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
        // Issue #32: 非線形閾値なので各レベルの実消費分は `xp_for_next_level` で拾う。
        let xp_now = state.player.xp;
        let total_xp_gained = if state.player.level > level_before {
            let consumed: u32 = (level_before..state.player.level)
                .map(|lvl| {
                    let mut tmp = Player::new();
                    tmp.level = lvl;
                    tmp.xp_for_next_level()
                })
                .sum();
            xp_now + consumed
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
            description_en: String::new(),
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

    // -----------------------------------------------------------------
    // Issue #25 レア出現予告クールダウン
    // -----------------------------------------------------------------

    #[test]
    fn test_add_curion_sets_last_collection_at() {
        let mut player = Player::new();
        assert!(
            player.last_collection_at.is_none(),
            "新規 Player は last_collection_at=None"
        );
        let before = Utc::now();
        player.add_curion(history_test_curion(Rarity::Rare));
        let after = Utc::now();
        let stamp = player
            .last_collection_at
            .expect("add_curion 後は Some が設定される");
        assert!(
            stamp >= before && stamp <= after,
            "stamp は呼び出し前後に挟まれる"
        );
    }

    #[test]
    fn test_legacy_save_last_collection_at_default_none() {
        // 旧 Player JSON には last_collection_at フィールドが無い。
        // それでも default で None が入る。
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
        assert_eq!(player.last_collection_at, None);
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

    // -----------------------------------------------------------------
    // Issue #32 きりの悪い数字設計
    // -----------------------------------------------------------------

    /// 非線形閾値: Lv.1〜Lv.20 で単調増加かつ等差ではないことを確認する。
    #[test]
    fn test_xp_thresholds_are_nonlinear() {
        let mut prev = 0u32;
        let mut diffs: Vec<i64> = Vec::new();
        for level in 1..=20u32 {
            let mut player = Player::new();
            player.level = level;
            let threshold = player.xp_for_next_level();
            assert!(
                threshold > prev,
                "Lv.{level} の閾値 {threshold} が直前 {prev} より大きくない (単調増加が崩れている)"
            );
            if level > 1 {
                diffs.push(threshold as i64 - prev as i64);
            }
            prev = threshold;
        }
        // 差分が一定でないことを確認する (= 等差ではない)
        let first_diff = diffs[0];
        let all_same = diffs.iter().all(|&d| d == first_diff);
        assert!(
            !all_same,
            "差分が全部 {first_diff} だと等差になっている: diffs={diffs:?}"
        );
    }

    /// 「きりの悪い」値が混じっていることを確認する。
    /// `XP_THRESHOLDS` の過半数 (Lv.1〜Lv.20 中 11 個以上) が `% 100 != 0` になっている。
    #[test]
    fn test_xp_thresholds_have_cliffhanger_values() {
        let mut not_round = 0;
        for level in 1..=20u32 {
            let mut player = Player::new();
            player.level = level;
            let threshold = player.xp_for_next_level();
            if threshold % 100 != 0 {
                not_round += 1;
            }
        }
        assert!(
            not_round >= 11,
            "Lv.1-20 で 100 倍数でない閾値が {not_round} 個しかない (11 個以上を期待: \
             きりの悪い値を採用しているか確認)"
        );
        // 特に Lv.2 の閾値 270 はテーブルの代表的な「きりの悪い数」
        let mut player = Player::new();
        player.level = 2;
        assert_eq!(player.xp_for_next_level(), 270, "Lv.2 の閾値は 270");
    }

    /// Lv.20 のテーブル外でも 0 でない正の値を返す (外挿が機能している)。
    #[test]
    fn test_xp_for_next_level_extrapolates_beyond_table() {
        // 各レベルの閾値が前レベル以上であり、毎レベル strict に増加する。
        // u32 飽和域でも +1 ずつ増えて単調性を維持する。
        let mut prev = {
            let mut p = Player::new();
            p.level = 20;
            p.xp_for_next_level()
        };

        // Lv.100 までは strict 単調増加（実プレイで到達しうる範囲）。
        for level in 21..=100u32 {
            let mut player = Player::new();
            player.level = level;
            let threshold = player.xp_for_next_level();
            assert!(threshold > 0, "Lv.{level} の閾値が 0 (外挿失敗)");
            assert!(
                threshold > prev,
                "Lv.{level} の閾値 {threshold} が Lv.{} の {prev} 以下 (単調増加が崩れている)",
                level - 1
            );
            prev = threshold;
        }

        // Lv.100 を超えても u32::MAX に張り付きはするが、0 にも下落にもならない。
        for level in 101..=200u32 {
            let mut player = Player::new();
            player.level = level;
            let threshold = player.xp_for_next_level();
            assert!(
                threshold >= prev,
                "Lv.{level} の閾値 {threshold} が前レベル {prev} を下回った"
            );
            prev = threshold;
        }
    }

    /// 新しい閾値で `add_xp` がレベルアップを起こす。
    /// Lv.1 → Lv.2 に 100 XP、Lv.2 → Lv.3 にさらに 270 XP 必要 (合計 370)。
    #[test]
    fn test_add_xp_levels_up_with_new_thresholds() {
        let mut player = Player::new();
        assert_eq!(player.level, 1);

        // ちょうど Lv.2 の閾値ぴったり = レベルアップして xp=0
        let ups = player.add_xp(100);
        assert_eq!(ups, vec![2], "100 XP で Lv.2");
        assert_eq!(player.level, 2);
        assert_eq!(player.xp, 0);
        assert_eq!(
            player.xp_for_next_level(),
            270,
            "Lv.2 の次の閾値は 270 (Lv.3 に必要)"
        );

        // さらに 270 XP で Lv.3 へ
        let ups2 = player.add_xp(270);
        assert_eq!(ups2, vec![3], "あと 270 XP で Lv.3");
        assert_eq!(player.level, 3);
        assert_eq!(player.xp, 0);
    }

    /// 旧セーブ (level=5, xp=200) を deserialize しても新閾値で正しく
    /// `xp_for_next_level` が引ける (= 旧式 `level * 100` に依存しない)。
    #[test]
    fn test_existing_legacy_save_loads_with_new_thresholds() {
        let legacy = json!({
            "level": 5,
            "xp": 200,
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
        let player: Player = serde_json::from_value(legacy).expect("旧セーブが読める");
        assert_eq!(player.level, 5);
        assert_eq!(player.xp, 200);
        // 旧コードでは Lv.5 は 500、新コードでは XP_THRESHOLDS[4] = 1280
        assert_eq!(
            player.xp_for_next_level(),
            1280,
            "旧セーブの level=5 でも新閾値 1280 が返る"
        );
    }

    /// 複数のマイルストーン候補から `remaining` 最小のものを選ぶ。
    #[test]
    fn test_next_milestone_returns_smallest_remaining() {
        let candidates = vec![
            MilestoneHint {
                label: "Lv.3 → Lv.4".to_string(),
                remaining: 50,
            },
            MilestoneHint {
                label: "⭐ コレクター Lv.3".to_string(),
                remaining: 4,
            },
            MilestoneHint {
                label: "💜 Epic ハンター 23".to_string(),
                remaining: 17,
            },
        ];
        let picked = MilestoneHint::pick_smallest(candidates).expect("候補がある");
        assert_eq!(picked.remaining, 4);
        assert_eq!(picked.label, "⭐ コレクター Lv.3");

        // remaining=0 は除外
        let zero_only = vec![
            MilestoneHint {
                label: "達成済み".to_string(),
                remaining: 0,
            },
            MilestoneHint {
                label: "残あり".to_string(),
                remaining: 9,
            },
        ];
        let picked2 = MilestoneHint::pick_smallest(zero_only).expect("残ありが選ばれる");
        assert_eq!(picked2.remaining, 9);

        // 全部 0 なら None
        let all_zero: Vec<MilestoneHint> = vec![MilestoneHint {
            label: "達成済み".to_string(),
            remaining: 0,
        }];
        assert!(MilestoneHint::pick_smallest(all_zero).is_none());
    }

    /// 実績側の閾値が「きりの悪い数」になっている (= 旧 25/50 等ではない)。
    #[test]
    fn test_achievement_thresholds_use_cliffhanger_numbers() {
        use crate::achievement::{AchievementManager, AchievementType};

        let mgr = AchievementManager::new();
        let total_counts: Vec<usize> = mgr
            .get_all_achievements()
            .iter()
            .filter_map(|a| match a.achievement_type {
                AchievementType::TotalCount(n) => Some(n),
                _ => None,
            })
            .collect();

        // 旧来のキリのいい値 (25, 50, 100, 250, 500, 1000) は採用していない
        for legacy in [25usize, 50, 100, 250, 500, 1000] {
            assert!(
                !total_counts.contains(&legacy),
                "TotalCount に旧来のキリのいい値 {legacy} が残っている: {total_counts:?}"
            );
        }

        // 新仕様のきりの悪い値が少なくとも 1 件入っている
        let has_cliffhanger = total_counts
            .iter()
            .any(|&c| c == 27 || c == 51 || c == 103 || c == 247 || c == 501 || c == 1001);
        assert!(
            has_cliffhanger,
            "TotalCount に新仕様のきりの悪い値 (27/51/103/247/501/1001) が無い: {total_counts:?}"
        );
    }

    // -----------------------------------------------------------------
    // Issue #26: Stats タブ 時系列 / 直近 30 日 Sparkline 用 daily_acquisition_counts
    // -----------------------------------------------------------------

    /// テスト用 Curion を `acquired_at` 指定で生成。
    fn dated_curion(acquired_at: DateTime<Utc>) -> Curion {
        let mut c = Curion::new(
            uuid::Uuid::new_v4(),
            "日次テスト".to_string(),
            Category::Concept,
            Rarity::Common,
            50.0,
            50.0,
        );
        c.acquired_at = acquired_at;
        c
    }

    /// `now` の Local 日付から `days_ago` 日前の 12:00 (UTC) を返す。
    fn utc_days_ago(now: DateTime<Utc>, days_ago: i64) -> DateTime<Utc> {
        let local_today = now.with_timezone(&Local).date_naive();
        let target = local_today - Duration::days(days_ago);
        target.and_hms_opt(12, 0, 0).unwrap().and_utc()
    }

    #[test]
    fn test_daily_acquisition_counts_basic() {
        let now = dt_hms(2026, 5, 15, 18, 0, 0);
        let mut player = Player::new();
        // 3 日前 / 1 日前 / 今日 にそれぞれ 1 件ずつ
        player.collection.push(dated_curion(utc_days_ago(now, 3)));
        player.collection.push(dated_curion(utc_days_ago(now, 1)));
        player.collection.push(dated_curion(utc_days_ago(now, 0)));

        let buckets = player.daily_acquisition_counts(5, now);
        assert_eq!(buckets.len(), 5, "5 日分の長さで返る");

        // index 順序: [4 日前, 3 日前, 2 日前, 1 日前, 今日]
        assert_eq!(buckets[0], 0, "4 日前: 0");
        assert_eq!(buckets[1], 1, "3 日前: 1");
        assert_eq!(buckets[2], 0, "2 日前: 0");
        assert_eq!(buckets[3], 1, "1 日前: 1");
        assert_eq!(buckets[4], 1, "今日: 1");
    }

    #[test]
    fn test_daily_acquisition_counts_empty_collection() {
        let now = dt_hms(2026, 5, 15, 18, 0, 0);
        let player = Player::new();
        // Player::new() 直後でも default の curion を持っていないか確認しつつ
        // collection が空のときは全ゼロ
        let buckets = player.daily_acquisition_counts(30, now);
        assert_eq!(buckets.len(), 30);
        assert!(
            buckets.iter().all(|&v| v == 0),
            "collection 空 → 全 0 が返る: {buckets:?}"
        );
    }

    #[test]
    fn test_daily_acquisition_counts_ignores_outside_window() {
        let now = dt_hms(2026, 5, 15, 18, 0, 0);
        let mut player = Player::new();
        // ウィンドウ外 (31 日前 / 100 日前) と ウィンドウ内 (10 日前) を混在
        player.collection.push(dated_curion(utc_days_ago(now, 31)));
        player.collection.push(dated_curion(utc_days_ago(now, 100)));
        player.collection.push(dated_curion(utc_days_ago(now, 10)));

        let buckets = player.daily_acquisition_counts(30, now);
        let total: u64 = buckets.iter().sum();
        assert_eq!(
            total, 1,
            "30 日ウィンドウ内に 1 件だけ集計される: {buckets:?}"
        );
        // 10 日前 = index 30 - 1 - 10 = 19
        assert_eq!(buckets[19], 1);
    }

    #[test]
    fn test_daily_acquisition_counts_days_zero() {
        let now = dt_hms(2026, 5, 15, 18, 0, 0);
        let mut player = Player::new();
        player.collection.push(dated_curion(utc_days_ago(now, 0)));

        let buckets = player.daily_acquisition_counts(0, now);
        assert!(
            buckets.is_empty(),
            "days = 0 のときは空 Vec を返す: {buckets:?}"
        );
    }

    // -----------------------------------------------------------------
    // Issue #29 SAN 値パラメータ関連のテスト
    // -----------------------------------------------------------------

    /// テスト用 Curion (Category::Concept 固定、レアリティ可変)。
    fn san_test_curion(rarity: Rarity) -> Curion {
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
    fn test_san_starts_at_100() {
        let player = Player::new();
        assert!(
            (player.san - 100.0).abs() < 1e-9,
            "Player::new() の SAN は 100.0 で始まる: {}",
            player.san
        );
    }

    #[test]
    fn test_san_increases_on_acquisition() {
        let mut player = Player::new();
        // SAN を 50.0 に下げてから Common を 1 個拾うと +0.5
        player.san = 50.0;
        player.add_curion(san_test_curion(Rarity::Common));
        assert!(
            (player.san - 50.5).abs() < 1e-9,
            "Common +0.5: {}",
            player.san
        );

        // 続けて Rare を拾うと +2.0 で 52.5
        player.add_curion(san_test_curion(Rarity::Rare));
        assert!(
            (player.san - 52.5).abs() < 1e-9,
            "Rare +2.0: {}",
            player.san
        );

        // Epic +5.0 で 57.5
        player.add_curion(san_test_curion(Rarity::Epic));
        assert!(
            (player.san - 57.5).abs() < 1e-9,
            "Epic +5.0: {}",
            player.san
        );
    }

    #[test]
    fn test_san_increases_more_on_legendary() {
        let mut player = Player::new();
        player.san = 50.0;
        player.add_curion(san_test_curion(Rarity::Common));
        let after_common = player.san;

        player.san = 50.0;
        player.add_curion(san_test_curion(Rarity::Legendary));
        let after_legendary = player.san;

        assert!(
            after_legendary > after_common,
            "Legendary 回復量 ({after_legendary}) > Common 回復量 ({after_common})"
        );
        // Legendary は +15.0 ぴったり
        assert!(
            (after_legendary - 65.0).abs() < 1e-9,
            "Legendary +15.0: {after_legendary}"
        );
    }

    #[test]
    fn test_san_clamps_at_max_on_acquisition() {
        // 99.0 + Legendary(+15.0) = 114.0 -> 100.0 にクランプ
        let mut player = Player::new();
        player.san = 99.0;
        player.add_curion(san_test_curion(Rarity::Legendary));
        assert!(
            (player.san - 100.0).abs() < 1e-9,
            "SAN は 100 でクランプ: {}",
            player.san
        );
    }

    #[test]
    fn test_san_legacy_save_defaults_to_100() {
        // 旧セーブ JSON (san フィールド無し) を復元したとき
        // `#[serde(default = "default_san")]` で 100.0 が入る
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
        assert!(
            (player.san - 100.0).abs() < 1e-9,
            "legacy save の SAN は 100.0 にデフォルト復元: {}",
            player.san
        );
    }

    #[test]
    fn test_san_decay_via_add_play_time() {
        let mut player = Player::new();
        assert!((player.san - 100.0).abs() < 1e-9);

        // 60 秒 = 1 分 経過 → -0.1
        player.add_play_time(60);
        assert!(
            (player.san - 99.9).abs() < 1e-9,
            "1 分 → -0.1: {}",
            player.san
        );

        // 600 秒 = 10 分経過 → -1.0
        player.add_play_time(600);
        assert!(
            (player.san - 98.9).abs() < 1e-9,
            "+10 分 → -1.0: {}",
            player.san
        );

        // 巨大時間経過でも 0 でクランプ
        player.add_play_time(60_000_000);
        assert!(
            (player.san - 0.0).abs() < 1e-9,
            "巨大時間経過でも 0 でクランプ: {}",
            player.san
        );
    }

    #[test]
    fn test_san_increases_on_synthesis_success() {
        use crate::synthesis::{RecipeDatabase, SynthesisManager};

        let recipe_db = RecipeDatabase::load_embedded().expect("load recipes");
        let mut state = GameState::new(SynthesisManager::new(recipe_db));
        state.player.san = 50.0;
        state.record_synthesis_success();
        assert!(
            (state.player.san - 53.0).abs() < 1e-9,
            "合成成功 +3.0: {}",
            state.player.san
        );
    }

    // -------------------------------------------------------------------
    // Issue #30 寿命システム (Player::prune_expired)
    // -------------------------------------------------------------------

    fn make_curion_with_lifespan(
        rarity: Rarity,
        acquired_at: DateTime<Utc>,
        lifespan_days: Option<u32>,
    ) -> Curion {
        let mut c = Curion::new(
            uuid::Uuid::new_v4(),
            "テスト".to_string(),
            Category::Animal,
            rarity,
            0.5,
            0.5,
        );
        c.acquired_at = acquired_at;
        c.lifespan_days = lifespan_days;
        c
    }

    /// Issue #30: prune_expired は期限切れだけを除外し、寿命内のキュリオンは残す。
    #[test]
    fn test_prune_expired_removes_expired_only() {
        let mut player = Player::new();
        let acquired = dt(2026, 5, 1);
        // 期限切れ (Common 3 日寿命、5 日経過)
        let expired = make_curion_with_lifespan(Rarity::Common, acquired, Some(3));
        // 寿命内 (Legendary 30 日寿命)
        let alive = make_curion_with_lifespan(Rarity::Legendary, acquired, Some(30));
        player.collection.push(expired);
        player.collection.push(alive);

        let now = acquired + Duration::days(5);
        let removed = player.prune_expired(now);
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].rarity, Rarity::Common);
        assert_eq!(player.collection.len(), 1);
        assert_eq!(player.collection[0].rarity, Rarity::Legendary);
    }

    /// Issue #30: prune_expired は削除したキュリオンを Vec で返す (順序は元の collection 順)。
    #[test]
    fn test_prune_expired_returns_removed_curions() {
        let mut player = Player::new();
        let acquired = dt(2026, 5, 1);
        let c1 = make_curion_with_lifespan(Rarity::Common, acquired, Some(3));
        let c2 = make_curion_with_lifespan(Rarity::Rare, acquired, Some(7));
        let c3 = make_curion_with_lifespan(Rarity::Epic, acquired, Some(14));
        player.collection.push(c1);
        player.collection.push(c2);
        player.collection.push(c3);

        // 8 日経過: Common (3 日) と Rare (7 日) が期限切れ、Epic は残る
        let now = acquired + Duration::days(8);
        let removed = player.prune_expired(now);
        assert_eq!(removed.len(), 2);
        assert_eq!(removed[0].rarity, Rarity::Common);
        assert_eq!(removed[1].rarity, Rarity::Rare);
        assert_eq!(player.collection.len(), 1);
        assert_eq!(player.collection[0].rarity, Rarity::Epic);
    }

    /// Issue #30: lifespan_days = None の旧セーブ由来キュリオンは絶対に削除しない。
    #[test]
    fn test_prune_expired_keeps_curions_without_lifespan() {
        let mut player = Player::new();
        let acquired = dt(2020, 1, 1);
        // 寿命なし (旧セーブ互換)
        let eternal = make_curion_with_lifespan(Rarity::Common, acquired, None);
        player.collection.push(eternal);

        let now = dt(2026, 5, 1); // 6 年経過
        let removed = player.prune_expired(now);
        assert!(removed.is_empty());
        assert_eq!(player.collection.len(), 1);
        assert!(player.collection[0].lifespan_days.is_none());
    }

    // -----------------------------------------------------------------
    // Issue #38 装備システム
    // -----------------------------------------------------------------

    /// 装備テスト用 Curion: source_guid を指定して latent / SemanticProfile を制御。
    fn equip_test_curion(rarity: Rarity, source_guid: uuid::Uuid) -> Curion {
        let mut c = Curion::new(
            source_guid,
            "装備テスト".to_string(),
            Category::Concept,
            rarity,
            0.5,
            0.5,
        );
        c.id = format!("equip-id-{source_guid}");
        c
    }

    /// Issue #38: equip(id) は curion_id を slot に立てる。
    #[test]
    fn test_equip_sets_curion_id() {
        let mut player = Player::new();
        let c = equip_test_curion(Rarity::Rare, uuid::Uuid::new_v4());
        let id = c.id.clone();
        player.collection.push(c);

        player.equip(&id);
        assert_eq!(player.equipment.curion_id.as_deref(), Some(id.as_str()));
        assert!(player.equipment.is_equipped());
        // equipped_curion() でも取れる
        assert_eq!(
            player.equipped_curion().map(|c| c.id.clone()),
            Some(id.clone())
        );
    }

    /// Issue #38: unequip() で slot がクリアされる。
    #[test]
    fn test_unequip_clears() {
        let mut player = Player::new();
        let c = equip_test_curion(Rarity::Rare, uuid::Uuid::new_v4());
        let id = c.id.clone();
        player.collection.push(c);
        player.equip(&id);

        player.unequip();
        assert!(player.equipment.curion_id.is_none());
        assert!(player.equipped_curion().is_none());
        // 効果は baseline に戻る
        let e = player.current_equipment_effect();
        assert_eq!(e.xp_multiplier, 1.0);
        assert_eq!(e.san_decay_modifier, 1.0);
    }

    /// Issue #38: 存在しない id を渡しても crash しない (no-op)。
    #[test]
    fn test_equip_invalid_id_does_nothing() {
        let mut player = Player::new();
        // collection が空でも引数 id が無くてもパニックしない
        player.equip("nonexistent-curion-id");
        assert!(player.equipment.curion_id.is_none());

        // すでに別の curion を装備していたら、無効 id では装備は変更されない
        let c = equip_test_curion(Rarity::Rare, uuid::Uuid::new_v4());
        let id = c.id.clone();
        player.collection.push(c);
        player.equip(&id);
        assert_eq!(player.equipment.curion_id.as_deref(), Some(id.as_str()));
        player.equip("still-nonexistent");
        assert_eq!(
            player.equipment.curion_id.as_deref(),
            Some(id.as_str()),
            "無効な id を渡しても元の装備は維持される"
        );
    }

    /// Issue #38: 装備中の curion から導出される xp_multiplier が add_curion に反映される。
    ///
    /// 装備対象は profile.heat + profile.speed が高い source_guid を探して使う。
    /// 装備時の XP が未装備時より多いことを検証する (具体値は latent 依存)。
    #[test]
    fn test_xp_multiplier_applies_on_acquisition() {
        use crate::equipment::EquipmentEffect;
        use crate::semantic::SemanticProfile;

        // heat + speed が高い (= xp_multiplier > 1.0) source_guid を探索する。
        // ランダムに見える uuid を試して xp_multiplier が一定以上のものを採用。
        let mut equip_guid = None;
        for _ in 0..200 {
            let g = uuid::Uuid::new_v4();
            let p = SemanticProfile::from_curion(&equip_test_curion(Rarity::Common, g));
            let e = EquipmentEffect::from_profile(&p);
            if e.xp_multiplier > 1.3 {
                equip_guid = Some((g, e.xp_multiplier));
                break;
            }
        }
        let (equip_guid, multiplier) =
            equip_guid.expect("200 回試して heat+speed の高い source_guid が見つからない");

        // 装備候補を作って collection に追加
        let equip_curion = equip_test_curion(Rarity::Common, equip_guid);
        let equip_id = equip_curion.id.clone();

        // ベースケース: 未装備で Rare を取得 → 25 XP (combo=1 倍率 1.0)
        let mut base = Player::new();
        base.collection.push(equip_curion.clone());
        let base_xp_gained = base.add_curion(equip_test_curion(Rarity::Rare, uuid::Uuid::new_v4()));

        // 装備ケース: 同じ装備状態で同じ Rare を取得
        let mut equipped = Player::new();
        equipped.collection.push(equip_curion.clone());
        equipped.equip(&equip_id);
        let equipped_xp_gained =
            equipped.add_curion(equip_test_curion(Rarity::Rare, uuid::Uuid::new_v4()));

        // 期待値: 25 * multiplier を切り捨て
        let expected = (25.0 * multiplier) as u32;
        assert_eq!(
            equipped_xp_gained, expected,
            "装備時 XP = 25 * {multiplier}"
        );
        assert!(
            equipped_xp_gained > base_xp_gained,
            "装備で XP が増える (base {base_xp_gained}, equipped {equipped_xp_gained})"
        );
    }

    /// Issue #38: 装備中の curion から導出される san_decay_modifier が add_play_time に反映される。
    ///
    /// purity が高い source_guid を探して装備し、未装備ケースより SAN 減衰が小さいことを検証する。
    #[test]
    fn test_san_decay_modifier_applies() {
        use crate::equipment::EquipmentEffect;
        use crate::semantic::SemanticProfile;

        // purity が 0.5 以上 (= san_decay_modifier < 0.75) になる source_guid を探索。
        let mut equip_guid = None;
        for _ in 0..200 {
            let g = uuid::Uuid::new_v4();
            let p = SemanticProfile::from_curion(&equip_test_curion(Rarity::Common, g));
            let e = EquipmentEffect::from_profile(&p);
            if e.san_decay_modifier < 0.75 {
                equip_guid = Some((g, e.san_decay_modifier));
                break;
            }
        }
        let (equip_guid, modifier) =
            equip_guid.expect("200 回試して purity の高い source_guid が見つからない");

        let equip_curion = equip_test_curion(Rarity::Common, equip_guid);
        let equip_id = equip_curion.id.clone();

        // 未装備: 600 秒 = 10 分 → -1.0
        let mut base = Player::new();
        base.collection.push(equip_curion.clone());
        let san_before = base.san;
        base.add_play_time(600);
        let base_decay = san_before - base.san;
        assert!((base_decay - 1.0).abs() < 1e-9, "未装備: 10 分で -1.0");

        // 装備: 600 秒 → -1.0 * modifier
        let mut equipped = Player::new();
        equipped.collection.push(equip_curion.clone());
        equipped.equip(&equip_id);
        let san_before_eq = equipped.san;
        equipped.add_play_time(600);
        let eq_decay = san_before_eq - equipped.san;
        let expected = 1.0 * modifier;
        assert!(
            (eq_decay - expected).abs() < 1e-6,
            "装備: 10 分で -{expected} (modifier={modifier}) got -{eq_decay}"
        );
        assert!(eq_decay < base_decay, "装備時の方が減衰が少ない");
    }

    /// Issue #38: 旧セーブ (equipment フィールド無し) でも空 slot で復元される。
    #[test]
    fn test_legacy_save_equipment_defaults_empty() {
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
        assert!(player.equipment.curion_id.is_none());
        assert!(!player.equipment.is_equipped());
        // current_equipment_effect は baseline
        let e = player.current_equipment_effect();
        assert_eq!(e.xp_multiplier, 1.0);
        assert_eq!(e.san_decay_modifier, 1.0);
    }

    /// Issue #38: 装備中 Curion が collection から消えた場合 (合成消費・寿命切れ等) は
    /// `equipped_curion = None` になり、効果は baseline に戻る。
    #[test]
    fn test_equipped_curion_disappears_after_removal() {
        let mut player = Player::new();
        let c = equip_test_curion(Rarity::Rare, uuid::Uuid::new_v4());
        let id = c.id.clone();
        player.collection.push(c);
        player.equip(&id);
        assert!(player.equipped_curion().is_some());

        // 合成消費等で collection から削除
        player.collection.retain(|c| c.id != id);
        assert!(player.equipped_curion().is_none());
        // slot 自体は残るが、効果は baseline 扱い
        assert!(player.equipment.curion_id.is_some());
        let e = player.current_equipment_effect();
        assert_eq!(e.xp_multiplier, 1.0);
    }

    /// Issue #38: toggle_equip は同じ id で解除、違う id で装備し直す。
    #[test]
    fn test_toggle_equip() {
        let mut player = Player::new();
        let c1 = equip_test_curion(Rarity::Rare, uuid::Uuid::new_v4());
        let id1 = c1.id.clone();
        let c2 = equip_test_curion(Rarity::Epic, uuid::Uuid::new_v4());
        let id2 = c2.id.clone();
        player.collection.push(c1);
        player.collection.push(c2);

        // 初回トグル: 装備
        player.toggle_equip(&id1);
        assert_eq!(player.equipment.curion_id.as_deref(), Some(id1.as_str()));
        // 同じ id で再トグル: 解除
        player.toggle_equip(&id1);
        assert!(player.equipment.curion_id.is_none());
        // 別 id をトグル: 装備
        player.toggle_equip(&id2);
        assert_eq!(player.equipment.curion_id.as_deref(), Some(id2.as_str()));
        // 違う id のトグルで取り替え
        player.toggle_equip(&id1);
        assert_eq!(player.equipment.curion_id.as_deref(), Some(id1.as_str()));
    }

    // -----------------------------------------------------------------
    // (元の Issue #30 テストはここから続く)
    // -----------------------------------------------------------------

    /// Issue #30: 旧セーブ JSON (lifespan_days フィールド無し) からの deserialize で
    /// `lifespan_days = None` に復元され、期限切れ扱いされない。
    #[test]
    fn test_legacy_save_curion_without_lifespan_loads() {
        let legacy_json = json!({
            "id": "11111111-1111-4111-9111-111111111111",
            "source_guid": "22222222-2222-4222-9222-222222222222",
            "noun": "テスト",
            "category": "Animal",
            "rarity": "Common",
            "interest": 0.5,
            "beauty": 0.5,
            "acquired_at": "2020-01-01T00:00:00Z"
        });
        let curion: Curion =
            serde_json::from_value(legacy_json).expect("legacy Curion should deserialize");
        assert!(
            curion.lifespan_days.is_none(),
            "旧セーブの lifespan_days は None でロードされる"
        );
        assert!(curion.expires_at().is_none());
        assert!(!curion.is_expired(dt(2026, 5, 1)));
    }
}
