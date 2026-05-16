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

use chrono::{DateTime, Utc};

/// クールダウン満了までの時間 (時間単位)。
pub const FULL_HOURS: f64 = 4.0;

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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn dt(h: i64, m: i64) -> DateTime<Utc> {
        // 2026-05-16 12:00:00 UTC を起点に (時, 分) のオフセットを足す
        let base = Utc.with_ymd_and_hms(2026, 5, 16, 12, 0, 0).unwrap();
        base + chrono::Duration::hours(h) + chrono::Duration::minutes(m)
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
