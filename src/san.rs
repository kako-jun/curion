//! SAN 値 (正気度) ロジック (Issue #29)
//!
//! プレイヤーの「正気度」を 0.0〜100.0 の `f64` で表現する。
//! キュリオン収集や合成成功で回復し、時間経過で減少する。
//!
//! UI 非依存の純粋ロジックに閉じ、ratatui や Player への参照を持たない。
//! `Player` 側はこの関数群を呼び出して `Player::san` を更新するだけにする。

use crate::curion::Rarity;

/// SAN 値の最大値。
pub const SAN_MAX: f64 = 100.0;
/// SAN 値の最小値。
pub const SAN_MIN: f64 = 0.0;
/// 合成成功 1 回あたりの SAN 回復量。
pub const SAN_GAIN_SYNTHESIS: f64 = 3.0;
/// 時間経過による 1 分あたりの SAN 減少量 (放置で減る)。
pub const SAN_DECAY_PER_MINUTE: f64 = 0.1;

/// SAN の状態区分 (Dashboard のバー色と警告表示の切り替えに使う)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SanState {
    /// >= 80
    Healthy,
    /// 50..80
    Slight,
    /// 30..50
    Warning,
    /// < 30 (「異常状態」表示)
    Critical,
}

/// レアリティに応じたキュリオン収集時の SAN 回復量。
///
/// - Common: +0.5
/// - Rare: +2.0
/// - Epic: +5.0
/// - Legendary: +15.0
pub fn san_gain_for_acquisition(rarity: Rarity) -> f64 {
    match rarity {
        Rarity::Common => 0.5,
        Rarity::Rare => 2.0,
        Rarity::Epic => 5.0,
        Rarity::Legendary => 15.0,
    }
}

/// 時間経過による SAN 減少を適用する。
///
/// `minutes_elapsed` 分だけ `SAN_DECAY_PER_MINUTE` を減らし、
/// `SAN_MIN` (0.0) でクランプする。
/// 負の `minutes_elapsed` は時間が巻き戻ったケース (clock skew 等)
/// なので無視 (= 現在値を返す)。
pub fn apply_decay(current: f64, minutes_elapsed: f64) -> f64 {
    if minutes_elapsed <= 0.0 {
        return current.clamp(SAN_MIN, SAN_MAX);
    }
    (current - minutes_elapsed * SAN_DECAY_PER_MINUTE).clamp(SAN_MIN, SAN_MAX)
}

/// SAN 回復を適用する。
///
/// `gain` を加算し、`SAN_MAX` (100.0) でクランプする。
/// 負の `gain` も受け付けるが、その場合は事実上の減少となる
/// (両端で `[SAN_MIN, SAN_MAX]` にクランプ)。
pub fn apply_gain(current: f64, gain: f64) -> f64 {
    (current + gain).clamp(SAN_MIN, SAN_MAX)
}

/// 現在の SAN 値から状態区分を返す。
///
/// 境界値は閾値「以上」で上の区分に入る:
/// - 100, 80 -> `Healthy`
/// - 79.99, 50 -> `Slight`
/// - 49.99, 30 -> `Warning`
/// - 29.99, 0 -> `Critical`
pub fn san_state(san: f64) -> SanState {
    if san >= 80.0 {
        SanState::Healthy
    } else if san >= 50.0 {
        SanState::Slight
    } else if san >= 30.0 {
        SanState::Warning
    } else {
        SanState::Critical
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    #[test]
    fn test_san_gain_for_acquisition_by_rarity() {
        assert!((san_gain_for_acquisition(Rarity::Common) - 0.5).abs() < EPS);
        assert!((san_gain_for_acquisition(Rarity::Rare) - 2.0).abs() < EPS);
        assert!((san_gain_for_acquisition(Rarity::Epic) - 5.0).abs() < EPS);
        assert!((san_gain_for_acquisition(Rarity::Legendary) - 15.0).abs() < EPS);
    }

    #[test]
    fn test_apply_decay_reduces_san() {
        // 60 分経過 -> 0.1 * 60 = 6.0 減少
        let after = apply_decay(100.0, 60.0);
        assert!((after - 94.0).abs() < EPS, "got {after}");

        // 10 分経過 -> 1.0 減少
        let after2 = apply_decay(50.0, 10.0);
        assert!((after2 - 49.0).abs() < EPS, "got {after2}");

        // 0 分または負: 値そのまま (クランプ済み)
        assert!((apply_decay(50.0, 0.0) - 50.0).abs() < EPS);
        assert!((apply_decay(50.0, -10.0) - 50.0).abs() < EPS);
    }

    #[test]
    fn test_apply_decay_clamps_at_zero() {
        // 2.0 から 1000 分減少 -> 100 減るが SAN_MIN でクランプ
        let after = apply_decay(2.0, 1000.0);
        assert!((after - 0.0).abs() < EPS, "got {after}");

        // すでに 0 でさらに減らしても 0
        let after2 = apply_decay(0.0, 100.0);
        assert!((after2 - 0.0).abs() < EPS, "got {after2}");
    }

    #[test]
    fn test_apply_gain_clamps_at_max() {
        // 99.0 + 5.0 = 104.0 -> 100.0 にクランプ
        let after = apply_gain(99.0, 5.0);
        assert!((after - SAN_MAX).abs() < EPS, "got {after}");

        // 50.0 + 0.5 = 50.5 (クランプ無し)
        let after2 = apply_gain(50.0, 0.5);
        assert!((after2 - 50.5).abs() < EPS, "got {after2}");

        // 100.0 + 15.0 -> 100.0
        let after3 = apply_gain(100.0, 15.0);
        assert!((after3 - SAN_MAX).abs() < EPS, "got {after3}");
    }

    #[test]
    fn test_san_state_thresholds() {
        // 100 / 80 は Healthy
        assert_eq!(san_state(100.0), SanState::Healthy);
        assert_eq!(san_state(80.0), SanState::Healthy);
        // 80 直下〜50 は Slight
        assert_eq!(san_state(79.9999), SanState::Slight);
        assert_eq!(san_state(50.0), SanState::Slight);
        // 50 直下〜30 は Warning
        assert_eq!(san_state(49.9999), SanState::Warning);
        assert_eq!(san_state(30.0), SanState::Warning);
        // 30 直下〜0 は Critical
        assert_eq!(san_state(29.9999), SanState::Critical);
        assert_eq!(san_state(0.0), SanState::Critical);
    }
}
