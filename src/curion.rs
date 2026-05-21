use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// キュリオンのレアリティ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rarity {
    Common,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    /// レアリティの確率を返す
    pub fn probability(&self) -> f64 {
        match self {
            Rarity::Common => 0.60,    // 60%
            Rarity::Rare => 0.30,      // 30%
            Rarity::Epic => 0.09,      // 9%
            Rarity::Legendary => 0.01, // 1%
        }
    }
}

/// キュリオンのカテゴリ
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    Animal,     // 動物
    Plant,      // 植物
    Color,      // 色
    Object,     // 物体
    Concept,    // 概念
    Element,    // 元素
    Food,       // 食べ物
    Phenomenon, // 現象
    Abstract,   // 抽象概念
}

impl Category {
    /// Japanese-fixed name used as internal ID and legacy save-compat label.
    ///
    /// Issue #63: kept unchanged so that already-persisted save data,
    /// achievement title generation (`achievement.rs`), and other internal
    /// look-ups by string continue to work. New UI code paths should call
    /// [`Category::display`] instead so they respect the player's chosen
    /// language.
    pub fn as_str(&self) -> &str {
        match self {
            Category::Animal => "動物",
            Category::Plant => "植物",
            Category::Color => "色",
            Category::Object => "物体",
            Category::Concept => "概念",
            Category::Element => "元素",
            Category::Food => "食べ物",
            Category::Phenomenon => "現象",
            Category::Abstract => "抽象",
        }
    }

    /// i18n-aware display name (Issue #63 Phase 1).
    ///
    /// Looks up the static `category.{lower}` key in the translation table.
    /// English is canonical; Japanese matches `as_str()`. Callers that need
    /// the legacy fixed Japanese label (achievement titles, internal IDs)
    /// must keep using [`Category::as_str`].
    pub fn display(&self, lang: crate::i18n::Language) -> &'static str {
        let key = match self {
            Category::Animal => "category.animal",
            Category::Plant => "category.plant",
            Category::Color => "category.color",
            Category::Object => "category.object",
            Category::Concept => "category.concept",
            Category::Element => "category.element",
            Category::Food => "category.food",
            Category::Phenomenon => "category.phenomenon",
            Category::Abstract => "category.abstract",
        };
        crate::i18n::t(key, lang)
    }
}

/// レアリティ別の寿命日数 (Issue #30)
///
/// 各キュリオンは入手時にレアリティに応じた寿命日数を持つ。
/// 期限切れになると起動時に自動削除される (合成で消費されたものは
/// 「使ってあげた = 供養」として自然消滅にカウントしない)。
///
/// - Common: 3 日
/// - Rare: 7 日
/// - Epic: 14 日
/// - Legendary: 30 日
pub fn lifespan_for_rarity(rarity: Rarity) -> u32 {
    match rarity {
        Rarity::Common => 3,
        Rarity::Rare => 7,
        Rarity::Epic => 14,
        Rarity::Legendary => 30,
    }
}

/// キュリオン（興味を司る粒子）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Curion {
    /// 一意のID
    pub id: String,

    /// 生成元のGUID
    pub source_guid: Uuid,

    /// 名詞（例: "魚"、"赤色"、"本"）
    pub noun: String,

    /// カテゴリ
    pub category: Category,

    /// レアリティ
    pub rarity: Rarity,

    /// 興味度（0.0〜1.0）
    pub interest: f64,

    /// 美しさ（0.0〜1.0）
    pub beauty: f64,

    /// 取得日時
    pub acquired_at: DateTime<Utc>,

    /// 入手時の通算収集回数 (Issue #27)
    ///
    /// `Player::add_curion` 内で `Player::total_acquisitions` を採番して入れる。
    /// 生成直後 (まだ Player に追加されていない) や、フィールドを持たない旧セーブからの
    /// deserialize 時は `None` になる。`None` の場合 UI 側で「履歴情報なし」と表示する。
    #[serde(default)]
    pub acquisition_index: Option<u32>,

    /// 寿命日数 (Issue #30)
    ///
    /// 入手時に `lifespan_for_rarity(rarity)` で設定される (Common 3 / Rare 7 /
    /// Epic 14 / Legendary 30 日)。`acquired_at + lifespan_days` を過ぎた
    /// キュリオンは起動時に自動削除される。
    ///
    /// `None` は「寿命なし (永遠)」を意味し、フィールドを持たない旧セーブからの
    /// deserialize 時のみ `None` になる。新規生成では必ず `Some(...)` が入る。
    /// 旧セーブのキュリオンは消えずに残り続け、Collection 一覧では残り寿命を
    /// `--` として表示する。
    #[serde(default)]
    pub lifespan_days: Option<u32>,
}

impl Curion {
    /// 新しいキュリオンを作成
    pub fn new(
        source_guid: Uuid,
        noun: String,
        category: Category,
        rarity: Rarity,
        interest: f64,
        beauty: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source_guid,
            noun,
            category,
            rarity,
            interest,
            beauty,
            acquired_at: Utc::now(),
            acquisition_index: None,
            lifespan_days: Some(lifespan_for_rarity(rarity)),
        }
    }

    /// キュリオンの表示用文字列
    pub fn display_name(&self) -> String {
        format!("{} の {}", self.category.as_str(), self.noun)
    }

    /// 寿命の終了予定時刻 (Issue #30)
    ///
    /// `acquired_at + lifespan_days`。`lifespan_days = None` の場合は `None`
    /// (= 永遠、旧セーブ互換)。
    pub fn expires_at(&self) -> Option<DateTime<Utc>> {
        self.lifespan_days
            .map(|d| self.acquired_at + Duration::days(d as i64))
    }

    /// 期限切れかどうか (Issue #30)
    ///
    /// `now > expires_at()` のときに `true`。寿命のないキュリオン
    /// (`lifespan_days = None`) は常に `false`。
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at().map(|e| now > e).unwrap_or(false)
    }

    /// 残り寿命 (日数、Issue #30)
    ///
    /// `expires_at - now` を日数で返す。負の値 (期限切れ) もそのまま返す。
    /// 寿命のないキュリオン (`lifespan_days = None`) は `None`。
    pub fn days_remaining(&self, now: DateTime<Utc>) -> Option<i64> {
        self.expires_at().map(|e| (e - now).num_days())
    }

    /// Collection 詳細ペイン用の入手履歴行 (Issue #27)
    ///
    /// 例:
    /// - `入手: 2026-05-14 23:47  (通算 142回目の収集)`
    /// - `入手: 2026-05-14 23:47  (履歴情報なし)`  // 旧セーブで acquisition_index = None
    ///
    /// `acquired_at` はローカルタイムゾーンに変換して表示する (UTC のままだと
    /// JST ユーザーには分かりにくいため)。
    pub fn format_acquisition_detail(&self) -> String {
        use chrono::Local;
        let local_time = self
            .acquired_at
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M");
        match self.acquisition_index {
            Some(idx) => format!("入手: {local_time}  (通算 {idx}回目の収集)"),
            None => format!("入手: {local_time}  (履歴情報なし)"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Issue #30: レアリティ別寿命の数値 (Common 3 / Rare 7 / Epic 14 / Legendary 30)。
    #[test]
    fn test_lifespan_for_rarity() {
        assert_eq!(lifespan_for_rarity(Rarity::Common), 3);
        assert_eq!(lifespan_for_rarity(Rarity::Rare), 7);
        assert_eq!(lifespan_for_rarity(Rarity::Epic), 14);
        assert_eq!(lifespan_for_rarity(Rarity::Legendary), 30);
    }

    fn make_curion_at(rarity: Rarity, acquired_at: DateTime<Utc>) -> Curion {
        let mut c = Curion::new(
            Uuid::new_v4(),
            "テスト".to_string(),
            Category::Animal,
            rarity,
            0.5,
            0.5,
        );
        c.acquired_at = acquired_at;
        c
    }

    /// Issue #30: `expires_at()` は `acquired_at + lifespan_days` を返す。
    #[test]
    fn test_expires_at_basic() {
        let acquired = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let c = make_curion_at(Rarity::Rare, acquired); // 7 日寿命
        let expected = acquired + Duration::days(7);
        assert_eq!(c.expires_at(), Some(expected));
    }

    /// Issue #30: `expires_at` を過ぎた `now` で `is_expired = true`。
    #[test]
    fn test_is_expired_true_after_lifespan() {
        let acquired = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let c = make_curion_at(Rarity::Common, acquired); // 3 日寿命
        let now = acquired + Duration::days(3) + Duration::seconds(1);
        assert!(c.is_expired(now));
    }

    /// Issue #30: 寿命内では `is_expired = false`。
    #[test]
    fn test_is_expired_false_within_lifespan() {
        let acquired = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let c = make_curion_at(Rarity::Epic, acquired); // 14 日寿命
        let now = acquired + Duration::days(13);
        assert!(!c.is_expired(now));
    }

    /// Issue #30: `days_remaining` は `(expires_at - now).num_days()` を返す。
    #[test]
    fn test_days_remaining_basic() {
        let acquired = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let c = make_curion_at(Rarity::Legendary, acquired); // 30 日寿命
        let now = acquired + Duration::days(5);
        assert_eq!(c.days_remaining(now), Some(25));
    }

    /// Issue #63: `Category::display(En)` returns the English name and
    /// `Category::display(Ja)` matches the legacy `as_str()` Japanese label.
    #[test]
    fn test_category_display_en_and_ja() {
        use crate::i18n::Language;
        assert_eq!(Category::Animal.display(Language::En), "Animal");
        assert_eq!(
            Category::Animal.display(Language::Ja),
            Category::Animal.as_str()
        );
        assert_eq!(Category::Abstract.display(Language::En), "Abstract");
        assert_eq!(
            Category::Abstract.display(Language::Ja),
            Category::Abstract.as_str()
        );
    }

    /// Issue #63 D-1: For all 9 categories, `display(Ja)` matches the legacy
    /// `as_str()` Japanese label (legacy compatibility contract — internal
    /// look-ups by JA string must keep working).
    #[test]
    fn category_display_ja_matches_as_str_for_all_nine() {
        use crate::i18n::Language;
        for cat in [
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
            assert_eq!(
                cat.display(Language::Ja),
                cat.as_str(),
                "{cat:?} Ja display must match as_str()"
            );
        }
    }

    /// Issue #63 D-2: For all 9 categories, `display(En)` is non-empty and
    /// differs from `display(Ja)` (= no English entry is accidentally a JA
    /// fallback).
    #[test]
    fn category_display_en_non_empty_and_distinct_from_ja_for_all_nine() {
        use crate::i18n::Language;
        for cat in [
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
            let en = cat.display(Language::En);
            let ja = cat.display(Language::Ja);
            assert!(!en.is_empty(), "{cat:?} En display empty");
            assert_ne!(en, ja, "{cat:?} En must differ from Ja");
        }
    }

    /// Issue #30: `lifespan_days = None` (旧セーブ等) は永遠扱いで期限切れにならない。
    #[test]
    fn test_is_expired_none_when_no_lifespan() {
        let acquired = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let mut c = make_curion_at(Rarity::Common, acquired);
        c.lifespan_days = None;
        // どれだけ未来でも期限切れにならない
        let far_future = acquired + Duration::days(10_000);
        assert!(!c.is_expired(far_future));
        assert_eq!(c.expires_at(), None);
        assert_eq!(c.days_remaining(far_future), None);
    }
}
