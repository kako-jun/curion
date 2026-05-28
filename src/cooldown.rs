//! Issue #25: レア出現予告クールダウン
//!
//! 収集後 [`FULL_HOURS`] 経過するごとに、レア以上の出現確率が段階的に上昇する。
//! 収集すると `last_collection_at` がリセットされ、進捗 (progress) は 0.0 に戻る。
//!
//! - `progress = 0.0` … 直後 (ボーナス無し、確率テーブルは通常通り)
//! - `progress = 1.0` … クールダウン完了 (最大ボーナス, 「⚡ レア出現確率上昇中」)
//!
//! `last_collection_at = None` (= 初回起動 or 旧セーブ) は「フルボーナス扱い」とする。
//! 初回プレイヤーがダッシュボードを開いた瞬間にバーが満タンで表示され、
//! 「最初の 1 個は当たりが出やすい」という体験を担保する。

use crate::curion::Rarity;
use chrono::{DateTime, Utc};

/// クールダウン満了までの時間 (時間単位)。
pub const FULL_HOURS: f64 = 4.0;

// ── Issue #78: 手動生成クールダウン / 自動ドロー ────────────────────────────

/// 手動生成（Space キー）のクールダウン秒数。3 分。
pub const MANUAL_GENERATE_COOLDOWN_SECS: i64 = 180;

/// 自動ドローのインターバル秒数。1 時間。
pub const AUTO_DRAW_INTERVAL_SECS: i64 = 3600;

/// 手動生成が可能かどうかを返す。
///
/// - `last_at = None`（初回）→ 生成可能
/// - 前回生成から [`MANUAL_GENERATE_COOLDOWN_SECS`] 秒以上経過 → 生成可能
/// - それ以外 → 生成不可
pub fn can_generate_manually(last_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    match last_at {
        None => true,
        Some(t) => (now - t).num_seconds() >= MANUAL_GENERATE_COOLDOWN_SECS,
    }
}

/// 手動生成の残り秒数（0 なら生成可能）。
pub fn generate_remaining_seconds(last_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> i64 {
    match last_at {
        None => 0,
        Some(t) => {
            let elapsed = (now - t).num_seconds().max(0);
            (MANUAL_GENERATE_COOLDOWN_SECS - elapsed).max(0)
        }
    }
}

/// 自動ドローが何回分溜まっているかを返す（上限なし）。
///
/// `last_at` は `None` を取らない（呼び出し元で `if let Some` 済み）。
/// `(now - last_at) / AUTO_DRAW_INTERVAL_SECS` の商を返す。
pub fn pending_auto_draws(last_at: DateTime<Utc>, now: DateTime<Utc>) -> u32 {
    let elapsed = (now - last_at).num_seconds().max(0);
    (elapsed / AUTO_DRAW_INTERVAL_SECS) as u32
}

/// n 番目（0 始まり）の自動ドローが行われた「はずの時刻」を返す。
///
/// `last_at + (n+1) * AUTO_DRAW_INTERVAL_SECS` 秒後の時刻。
/// ログに「14:00 ○○ を入手」のように積まれ、プレイヤーが遡って気づけるようにする。
pub fn auto_draw_timestamp(last_at: DateTime<Utc>, n: u32) -> DateTime<Utc> {
    last_at + chrono::Duration::seconds(AUTO_DRAW_INTERVAL_SECS * (n as i64 + 1))
}

/// クールダウン進捗を計算する。
///
/// - 戻り値は `0.0..=1.0` に clamp 済み
/// - `last_collection_at = None` の場合は `1.0` (フルボーナス) を返す
pub fn cooldown_progress(last_collection_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> f64 {
    let elapsed_hours = match last_collection_at {
        Some(t) => (now - t).num_seconds() as f64 / 3600.0,
        None => FULL_HOURS,
    };
    (elapsed_hours / FULL_HOURS).clamp(0.0, 1.0)
}

/// レア出現倍率 (Rare 以上に対する乗数)。
///
/// `progress 0.0 -> 1.0x`, `progress 1.0 -> 2.0x` の線形補間。
/// 現状は表示用ヒント値であり、実際の確率反映は
/// [`crate::generator::CurionGenerator::generate_with_bonus`] が roll 値を直接補正する。
/// 将来 UI でツールチップ表示する想定。
#[allow(dead_code)]
pub fn rare_probability_multiplier(progress: f64) -> f64 {
    1.0 + progress.clamp(0.0, 1.0)
}

/// クールダウン残時間 (秒)。表示用。
/// `progress >= 1.0` のときは 0 を返す。
pub fn remaining_seconds(last_collection_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> i64 {
    let full_secs = (FULL_HOURS * 3600.0) as i64;
    match last_collection_at {
        Some(t) => {
            let elapsed = (now - t).num_seconds().max(0);
            (full_secs - elapsed).max(0)
        }
        None => 0,
    }
}

/// Issue #28: 現在のレアリティ別出現確率 (cooldown progress 反映済み)。
///
/// `crate::generator::CurionGenerator::generate_with_bonus` の roll-shift モデルに整合する。
/// roll は `[0.0, 1.0)` の一様分布で、`shifted = (roll - 0.3 * progress).max(0.0)`。
/// 累積確率は Legendary -> Epic -> Rare -> Common の順 (それぞれの基礎確率は 0.01 / 0.09 / 0.30 / 0.60)。
///
/// この関数は実機のサンプリングを介さず、確率密度を直接積分して返す。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RarityProbabilities {
    pub common: f64,
    pub rare: f64,
    pub epic: f64,
    pub legendary: f64,
}

impl RarityProbabilities {
    /// レア以上 (Rare + Epic + Legendary) の合算確率
    pub fn rare_or_higher(&self) -> f64 {
        self.rare + self.epic + self.legendary
    }

    /// レアリティ別確率を順に取り出すイテレータ (UI 用)
    #[allow(dead_code)]
    pub fn for_rarity(&self, rarity: Rarity) -> f64 {
        match rarity {
            Rarity::Common => self.common,
            Rarity::Rare => self.rare,
            Rarity::Epic => self.epic,
            Rarity::Legendary => self.legendary,
        }
    }
}

/// 現在の cooldown progress を反映したレアリティ別出現確率を返す。
///
/// `progress = 0.0` → 基礎確率 (Common 0.60 / Rare 0.30 / Epic 0.09 / Legendary 0.01)。
/// `progress = 1.0` → 最大シフト (-0.3)。Common 帯が削れ、Legendary 帯が +0.3 シフトの分だけ太る。
///
/// 計算は generator.rs の roll-shift モデルから積分で導出:
/// - P(Legendary) = clamp(0.01 + 0.3 * p, 0, 1)
/// - P(Epic)      = clamp(0.10 + 0.3 * p, 0, 1) - P(Legendary)
/// - P(Rare)      = clamp(0.40 + 0.3 * p, 0, 1) - P(Legendary) - P(Epic)
/// - P(Common)    = 1.0 - その他
///
/// 4 値はそれぞれ [0, 1] にクランプされ、総和は 1.0 になる。
pub fn current_rarity_probabilities(progress: f64) -> RarityProbabilities {
    let p = progress.clamp(0.0, 1.0);
    let shift = 0.3 * p;

    // 累積確率の境界 (shift 込み, clamp [0, 1])
    let cum_legendary = (0.01 + shift).clamp(0.0, 1.0);
    let cum_epic = (0.10 + shift).clamp(0.0, 1.0);
    let cum_rare = (0.40 + shift).clamp(0.0, 1.0);

    let legendary = cum_legendary;
    let epic = (cum_epic - cum_legendary).max(0.0);
    let rare = (cum_rare - cum_epic).max(0.0);
    let common = (1.0 - cum_rare).max(0.0);

    RarityProbabilities {
        common,
        rare,
        epic,
        legendary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn dt(h: i64, m: i64) -> DateTime<Utc> {
        // 2026-05-16 12:00:00 UTC を起点に (時, 分) のオフセットを足す
        let base = Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap();
        base + chrono::Duration::hours(h) + chrono::Duration::minutes(m)
    }

    /// 秒単位のオフセットで DateTime を作るヘルパー
    fn dt_secs(secs: i64) -> DateTime<Utc> {
        let base = Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap();
        base + chrono::Duration::seconds(secs)
    }

    // ── Issue #78: can_generate_manually ──────────────────────────────

    /// B-1: elapsed=179 秒 → クールダウン中のため生成不可
    #[test]
    fn can_generate_manually_false_when_elapsed_is_179_secs() {
        let last = dt_secs(0);
        let now = dt_secs(179);
        assert!(!can_generate_manually(Some(last), now));
    }

    /// B-2: elapsed=180 秒（境界値）→ クールダウン満了のため生成可能
    #[test]
    fn can_generate_manually_true_when_elapsed_is_exactly_cooldown() {
        let last = dt_secs(0);
        let now = dt_secs(180);
        assert!(can_generate_manually(Some(last), now));
    }

    /// N-1: last_at=None → 初回起動は無条件で生成可能
    #[test]
    fn can_generate_manually_true_when_last_at_is_none() {
        assert!(can_generate_manually(None, dt_secs(0)));
    }

    /// N-2: elapsed=180 → 生成可能（境界の再確認）
    #[test]
    fn can_generate_manually_true_when_elapsed_equals_cooldown_boundary() {
        let last = dt_secs(0);
        let now = dt_secs(MANUAL_GENERATE_COOLDOWN_SECS);
        assert!(can_generate_manually(Some(last), now));
    }

    /// N-3: elapsed=181 → 境界を超えても生成可能
    #[test]
    fn can_generate_manually_true_when_elapsed_exceeds_cooldown() {
        let last = dt_secs(0);
        let now = dt_secs(MANUAL_GENERATE_COOLDOWN_SECS + 1);
        assert!(can_generate_manually(Some(last), now));
    }

    // ── Issue #78: generate_remaining_seconds ────────────────────────

    /// B-3: elapsed=179 → 残り 1 秒
    #[test]
    fn generate_remaining_seconds_is_1_when_elapsed_is_179() {
        let last = dt_secs(0);
        let now = dt_secs(179);
        assert_eq!(generate_remaining_seconds(Some(last), now), 1);
    }

    /// B-4: elapsed=180 → 残り 0 秒（生成可能）
    #[test]
    fn generate_remaining_seconds_is_0_when_elapsed_equals_cooldown() {
        let last = dt_secs(0);
        let now = dt_secs(180);
        assert_eq!(generate_remaining_seconds(Some(last), now), 0);
    }

    /// N-4: last_at=None → 残り 0 秒
    #[test]
    fn generate_remaining_seconds_is_0_when_last_at_is_none() {
        assert_eq!(generate_remaining_seconds(None, dt_secs(0)), 0);
    }

    /// N-5: elapsed=60 → 残り 120 秒
    #[test]
    fn generate_remaining_seconds_is_120_when_elapsed_is_60() {
        let last = dt_secs(0);
        let now = dt_secs(60);
        assert_eq!(generate_remaining_seconds(Some(last), now), 120);
    }

    /// N-6: elapsed=180 → 残り 0 秒（境界値）
    #[test]
    fn generate_remaining_seconds_is_0_when_elapsed_is_180() {
        let last = dt_secs(0);
        let now = dt_secs(180);
        assert_eq!(generate_remaining_seconds(Some(last), now), 0);
    }

    /// K-1: now < last_at（クロックスキュー）→ MANUAL_GENERATE_COOLDOWN_SECS を返す
    #[test]
    fn generate_remaining_seconds_returns_full_cooldown_on_clock_skew() {
        let last = dt_secs(100); // last_at が now より未来
        let now = dt_secs(0);
        assert_eq!(
            generate_remaining_seconds(Some(last), now),
            MANUAL_GENERATE_COOLDOWN_SECS
        );
    }

    // ── Issue #78: pending_auto_draws ────────────────────────────────

    /// B-5: elapsed=3599 → pending=0（インターバル未満）
    #[test]
    fn pending_auto_draws_is_0_when_elapsed_is_3599() {
        let last = dt_secs(0);
        let now = dt_secs(3599);
        assert_eq!(pending_auto_draws(last, now), 0);
    }

    /// B-6: elapsed=3600 → pending=1（インターバルちょうど）
    #[test]
    fn pending_auto_draws_is_1_when_elapsed_is_exactly_one_interval() {
        let last = dt_secs(0);
        let now = dt_secs(3600);
        assert_eq!(pending_auto_draws(last, now), 1);
    }

    /// B-7: elapsed=7199 → pending=1（2 インターバル未満）
    #[test]
    fn pending_auto_draws_is_1_when_elapsed_is_7199() {
        let last = dt_secs(0);
        let now = dt_secs(7199);
        assert_eq!(pending_auto_draws(last, now), 1);
    }

    /// N-8: elapsed=3600 → pending=1
    #[test]
    fn pending_auto_draws_is_1_when_elapsed_is_one_interval() {
        let last = dt_secs(0);
        let now = dt_secs(AUTO_DRAW_INTERVAL_SECS);
        assert_eq!(pending_auto_draws(last, now), 1);
    }

    /// N-9: elapsed=7200 → pending=2
    #[test]
    fn pending_auto_draws_is_2_when_elapsed_is_two_intervals() {
        let last = dt_secs(0);
        let now = dt_secs(AUTO_DRAW_INTERVAL_SECS * 2);
        assert_eq!(pending_auto_draws(last, now), 2);
    }

    /// K-2: now < last_at（クロックスキュー）→ pending=0（負の elapsed は 0 扱い）
    #[test]
    fn pending_auto_draws_is_0_on_clock_skew() {
        let last = dt_secs(1000); // last_at が now より未来
        let now = dt_secs(0);
        assert_eq!(pending_auto_draws(last, now), 0);
    }

    // ── Issue #78: auto_draw_timestamp ───────────────────────────────

    /// B-8 / N-10: auto_draw_timestamp(last_at, n=0) = last_at + 3600秒
    #[test]
    fn auto_draw_timestamp_n0_is_last_at_plus_one_interval() {
        let last = dt_secs(0);
        let expected = dt_secs(AUTO_DRAW_INTERVAL_SECS);
        assert_eq!(auto_draw_timestamp(last, 0), expected);
    }

    /// B-8 / N-11: auto_draw_timestamp(last_at, n=2) = last_at + 10800秒
    #[test]
    fn auto_draw_timestamp_n2_is_last_at_plus_three_intervals() {
        let last = dt_secs(0);
        let expected = dt_secs(AUTO_DRAW_INTERVAL_SECS * 3);
        assert_eq!(auto_draw_timestamp(last, 2), expected);
    }

    #[test]
    fn test_cooldown_progress_zero_immediately_after_collection() {
        let t = dt(0, 0);
        assert_eq!(cooldown_progress(Some(t), t), 0.0);
    }

    #[test]
    fn test_cooldown_progress_full_after_4_hours() {
        let t = dt(0, 0);
        let now = dt(4, 0);
        assert!((cooldown_progress(Some(t), now) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_cooldown_progress_half_after_2_hours() {
        let t = dt(0, 0);
        let now = dt(2, 0);
        assert!((cooldown_progress(Some(t), now) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_cooldown_progress_none_treated_as_full() {
        // 初回起動時 (last_collection_at=None) は最初からフルボーナス扱い
        let now = dt(0, 0);
        assert_eq!(cooldown_progress(None, now), 1.0);
    }

    #[test]
    fn test_cooldown_progress_clamps_at_1() {
        let t = dt(0, 0);
        let now_24h_later = dt(24, 0);
        assert_eq!(cooldown_progress(Some(t), now_24h_later), 1.0);
    }

    #[test]
    fn test_cooldown_progress_clamps_at_0_on_clock_skew() {
        // 何らかの理由で last_collection_at が未来 (=clock skew) でも 0 を下回らない
        let t = dt(2, 0);
        let now = dt(0, 0);
        assert_eq!(cooldown_progress(Some(t), now), 0.0);
    }

    #[test]
    fn test_rare_multiplier_at_zero_is_1() {
        assert!((rare_probability_multiplier(0.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn test_rare_multiplier_at_full_is_2() {
        assert!((rare_probability_multiplier(1.0) - 2.0).abs() < 1e-9);
    }

    // ── Issue #28: current_rarity_probabilities ──────────────────────

    fn sum(p: &RarityProbabilities) -> f64 {
        p.common + p.rare + p.epic + p.legendary
    }

    /// progress 0 / 0.5 / 1.0 のいずれでも 4 値の総和は 1.0
    #[test]
    fn test_rarity_probabilities_sum_to_one() {
        for &p in &[0.0_f64, 0.5, 1.0] {
            let probs = current_rarity_probabilities(p);
            assert!(
                (sum(&probs) - 1.0).abs() < 1e-9,
                "progress={p}: sum={} probs={probs:?}",
                sum(&probs)
            );
        }
    }

    /// progress=0.0 のとき基礎確率 (60/30/9/1) と一致
    #[test]
    fn test_rarity_probabilities_at_zero_matches_base() {
        let p = current_rarity_probabilities(0.0);
        assert!((p.common - 0.60).abs() < 1e-9, "common={}", p.common);
        assert!((p.rare - 0.30).abs() < 1e-9, "rare={}", p.rare);
        assert!((p.epic - 0.09).abs() < 1e-9, "epic={}", p.epic);
        assert!(
            (p.legendary - 0.01).abs() < 1e-9,
            "legendary={}",
            p.legendary
        );
    }

    /// progress が増えるとレア以上の合算確率は単調増加 (狭義)
    #[test]
    fn test_rare_or_higher_increases_with_progress() {
        let p0 = current_rarity_probabilities(0.0).rare_or_higher();
        let p_half = current_rarity_probabilities(0.5).rare_or_higher();
        let p_full = current_rarity_probabilities(1.0).rare_or_higher();

        assert!(p_half > p0, "p_half={p_half} should be > p0={p0}");
        assert!(
            p_full > p_half,
            "p_full={p_full} should be > p_half={p_half}"
        );
        // 範囲確認
        assert!((p0 - 0.40).abs() < 1e-9);
        assert!(p_full <= 1.0);
    }

    /// 各値は [0, 1] にクランプされる (progress=1.0 でも負値や 1.0 超は出ない)
    #[test]
    fn test_rarity_probabilities_clamp_at_full_progress() {
        let probs = current_rarity_probabilities(1.0);
        for v in [probs.common, probs.rare, probs.epic, probs.legendary] {
            assert!((0.0..=1.0).contains(&v), "value out of range: {v}");
        }
        // Legendary 帯は 0.01 + 0.3 = 0.31
        assert!((probs.legendary - 0.31).abs() < 1e-9);
        // Common 帯は 0.60 - 0.30 = 0.30
        assert!((probs.common - 0.30).abs() < 1e-9);
    }

    /// 範囲外 (負・1超) の progress でも例外なくクランプ動作する
    #[test]
    fn test_rarity_probabilities_clamps_out_of_range_progress() {
        let neg = current_rarity_probabilities(-0.5);
        let over = current_rarity_probabilities(1.5);
        let zero = current_rarity_probabilities(0.0);
        let full = current_rarity_probabilities(1.0);
        assert_eq!(neg, zero);
        assert_eq!(over, full);
    }

    #[test]
    fn test_remaining_seconds_basic() {
        let t = dt(0, 0);
        // 1 時間経過 → 3 時間残り = 10800 秒
        assert_eq!(remaining_seconds(Some(t), dt(1, 0)), 3 * 3600);
        // 4 時間以上 → 0
        assert_eq!(remaining_seconds(Some(t), dt(4, 30)), 0);
        // None → 0 (フルボーナス済みなので残時間は 0)
        assert_eq!(remaining_seconds(None, dt(0, 0)), 0);
    }
}
