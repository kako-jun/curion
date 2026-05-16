//! Issue #38: 装備システム (latent → SemanticProfile → 効果)
//!
//! プレイヤーは Curion を 1 体だけ「装備」できる。装備中の Curion の
//! `SemanticProfile` から派生する [`EquipmentEffect`] が常時適用される。
//!
//! Phase 1 (本 Issue) で実際に効果がかかるのは以下の 2 つ:
//!
//! - `xp_multiplier` — `Player::add_curion` の XP 計算に乗算される (1.0 で no-op)
//! - `san_decay_modifier` — `Player::add_play_time` の SAN 減衰に乗算される
//!   (1.0 で no-op、0.5 で半減)
//!
//! `rare_probability_bonus` と `synthesis_success_bonus` は計算と UI 表示まで
//! 用意するが、ゲームロジックへの適用は将来の Issue に回す
//! (個別チューニング/レビューが必要なため、本 Issue ではスコープ外と明示)。

use crate::curion::Curion;
use crate::semantic::SemanticProfile;
use serde::{Deserialize, Serialize};

/// 装備スロット (現状 1 枠だけ)。
///
/// `curion_id = None` で「未装備」。装備中の curion_id は `Player::collection`
/// に含まれていない可能性がある (合成消費・寿命切れで消えた場合)。その場合の
/// 効果は [`EquipmentEffect::none`] として扱う。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct EquipmentSlot {
    /// 装備中 Curion の id (`Curion::id`)。
    pub curion_id: Option<String>,
}

impl EquipmentSlot {
    /// `curion_id` が設定されているか。テストでの slot 状態確認に使う想定で公開している
    /// (UI 側は `Player::equipped_curion()` の Option で判定するためここは未使用)。
    #[allow(dead_code)]
    pub fn is_equipped(&self) -> bool {
        self.curion_id.is_some()
    }
}

/// 装備中 curion から導出される効果値。
///
/// 全フィールド「baseline = 何もしない値」。装備していない (or 装備 curion が
/// 行方不明) のときは [`EquipmentEffect::none`] が返り、ロジック側は乗算/加算しても
/// 振る舞いが変わらない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EquipmentEffect {
    /// XP 取得倍率。baseline 1.0、上限 2.0。
    /// `1.0 + (heat + speed) * 0.5` で計算 (heat と speed が共に 1.0 なら 2.0 = +100%)。
    pub xp_multiplier: f64,

    /// レアリティ判定の bonus_progress 加算量 (0.0..=0.3、baseline 0.0)。
    /// `luck * 0.3` で計算。
    /// 現状 UI 表示のみ (将来 Issue で `generate_with_bonus` の bonus に足す予定)。
    pub rare_probability_bonus: f64,

    /// SAN 減衰の倍率 (baseline 1.0、下限 0.5)。
    /// `1.0 - purity * 0.5` で計算 (purity 1.0 なら 0.5 = 半減)。
    pub san_decay_modifier: f64,

    /// 合成成功率の加算量 (0.0..=0.2、baseline 0.0)。
    /// `order * 0.2` で計算。
    /// 現状 UI 表示のみ (将来 Issue で `SynthesisRecipe::success_probability` に足す予定)。
    pub synthesis_success_bonus: f64,
}

impl EquipmentEffect {
    /// SemanticProfile から効果を導出する。
    ///
    /// 各タグの値域 `[0, 1]` を係数とした線形マップ。装備なしの場合は
    /// [`EquipmentEffect::none`] を使う (こちらは profile を必要としない)。
    pub fn from_profile(profile: &SemanticProfile) -> Self {
        let xp_multiplier = (1.0 + (profile.heat + profile.speed) * 0.5).clamp(1.0, 2.0);
        let rare_probability_bonus = (profile.luck * 0.3).clamp(0.0, 0.3);
        let san_decay_modifier = (1.0 - profile.purity * 0.5).clamp(0.5, 1.0);
        let synthesis_success_bonus = (profile.order * 0.2).clamp(0.0, 0.2);
        Self {
            xp_multiplier,
            rare_probability_bonus,
            san_decay_modifier,
            synthesis_success_bonus,
        }
    }

    /// Curion から効果を導出する (内部で profile を作る)。
    pub fn from_curion(curion: &Curion) -> Self {
        Self::from_profile(&SemanticProfile::from_curion(curion))
    }

    /// 装備なし時の neutral 値 (= 既存ロジックに掛けても変化なし)。
    pub fn none() -> Self {
        Self {
            xp_multiplier: 1.0,
            rare_probability_bonus: 0.0,
            san_decay_modifier: 1.0,
            synthesis_success_bonus: 0.0,
        }
    }

    /// Dashboard 表示用の 1 行サマリ。
    ///
    /// 例: `XP +20% / SAN 減衰 -30% / レア +5% / 合成 +3%`。
    /// 全てが baseline なら `"効果なし"` を返す。
    pub fn summary_line(&self) -> String {
        let xp_pct = ((self.xp_multiplier - 1.0) * 100.0).round() as i64;
        let san_pct = (-(1.0 - self.san_decay_modifier) * 100.0).round() as i64;
        let rare_pct = (self.rare_probability_bonus * 100.0).round() as i64;
        let syn_pct = (self.synthesis_success_bonus * 100.0).round() as i64;
        if xp_pct == 0 && san_pct == 0 && rare_pct == 0 && syn_pct == 0 {
            return "効果なし".to_string();
        }
        format!("XP +{xp_pct}% / SAN 減衰 {san_pct:+}% / レア +{rare_pct}% / 合成 +{syn_pct}%")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 全タグ 0 の profile (= 何もしない) から作った効果は baseline と等しい。
    #[test]
    fn test_effect_from_zero_profile_is_baseline() {
        let profile = SemanticProfile {
            heat: 0.0,
            speed: 0.0,
            order: 0.0,
            chaos: 0.0,
            life: 0.0,
            machine: 0.0,
            dream: 0.0,
            violence: 0.0,
            luck: 0.0,
            purity: 0.0,
        };
        let effect = EquipmentEffect::from_profile(&profile);
        assert!((effect.xp_multiplier - 1.0).abs() < 1e-9);
        assert!((effect.rare_probability_bonus - 0.0).abs() < 1e-9);
        assert!((effect.san_decay_modifier - 1.0).abs() < 1e-9);
        assert!((effect.synthesis_success_bonus - 0.0).abs() < 1e-9);
    }

    /// 全タグ 1.0 (= 最大) でも上限内にクランプされる。
    #[test]
    fn test_effect_from_max_profile_caps_at_expected() {
        let profile = SemanticProfile {
            heat: 1.0,
            speed: 1.0,
            order: 1.0,
            chaos: 1.0,
            life: 1.0,
            machine: 1.0,
            dream: 1.0,
            violence: 1.0,
            luck: 1.0,
            purity: 1.0,
        };
        let effect = EquipmentEffect::from_profile(&profile);
        // xp_multiplier: 1.0 + (1.0 + 1.0) * 0.5 = 2.0
        assert!((effect.xp_multiplier - 2.0).abs() < 1e-9);
        // rare_probability_bonus: 1.0 * 0.3 = 0.3
        assert!((effect.rare_probability_bonus - 0.3).abs() < 1e-9);
        // san_decay_modifier: 1.0 - 1.0 * 0.5 = 0.5
        assert!((effect.san_decay_modifier - 0.5).abs() < 1e-9);
        // synthesis_success_bonus: 1.0 * 0.2 = 0.2
        assert!((effect.synthesis_success_bonus - 0.2).abs() < 1e-9);
    }

    /// none() は baseline。
    #[test]
    fn test_effect_none_is_neutral() {
        let e = EquipmentEffect::none();
        assert_eq!(e.xp_multiplier, 1.0);
        assert_eq!(e.rare_probability_bonus, 0.0);
        assert_eq!(e.san_decay_modifier, 1.0);
        assert_eq!(e.synthesis_success_bonus, 0.0);
    }

    /// summary_line: baseline は「効果なし」、変化があれば一覧表示。
    #[test]
    fn test_summary_line_formats() {
        assert_eq!(EquipmentEffect::none().summary_line(), "効果なし");

        let e = EquipmentEffect {
            xp_multiplier: 1.5,            // +50%
            rare_probability_bonus: 0.10,  // +10%
            san_decay_modifier: 0.7,       // -30%
            synthesis_success_bonus: 0.05, // +5%
        };
        let s = e.summary_line();
        assert!(s.contains("XP +50%"), "got: {s}");
        assert!(s.contains("レア +10%"), "got: {s}");
        assert!(s.contains("SAN 減衰 -30%"), "got: {s}");
        assert!(s.contains("合成 +5%"), "got: {s}");
    }
}
