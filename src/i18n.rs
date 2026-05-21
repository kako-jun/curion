//! Phase 1 internationalisation scaffold for curion (Issue #63).
//!
//! This module provides the runtime language switch and the static translation
//! dictionary that the rest of the UI looks up by key. The English string is
//! treated as the canonical text and is used as a fallback whenever a key has
//! no Japanese rendition (the inverse situation is what triggers Phase 2 work).
//!
//! Conventions
//! -----------
//! * Keys use dotted namespaces: `tab.{name}`, `section.{name}`, `block.{name}`,
//!   `help.{tab}.{action}`, `category.{lower}`, `settings.{field}`, `msg.{event}`.
//! * The English entry sits at index `0` and the Japanese entry at index `1` so
//!   that `Language::index()` directly addresses the table.
//! * Out-of-scope strings for Phase 1 (achievement titles, evolution display
//!   names, flavor text, daily mission descriptions, interactive/plain sub
//!   modes) are intentionally not registered here. They will be addressed in
//!   Phase 2 / Issue #65.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

/// UI language (Phase 1: English canonical + Japanese translation).
///
/// The default is [`Language::En`] because Issue #63 promotes English to the
/// reference locale. Existing save files without a `language` field will
/// deserialize into [`Language::En`] via `#[serde(default)]`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    /// English (canonical).
    #[default]
    En,
    /// Japanese.
    Ja,
}

impl Language {
    /// Stable column index inside the translation table.
    ///
    /// `En` => 0, `Ja` => 1.
    #[allow(dead_code)] // Used by t() and Settings tab (later commit in this PR).
    pub fn index(self) -> usize {
        match self {
            Language::En => 0,
            Language::Ja => 1,
        }
    }

    /// Short code for UI rendering ("En" / "Ja").
    #[allow(dead_code)] // Used by Settings tab (later commit in this PR).
    pub fn short_label(self) -> &'static str {
        match self {
            Language::En => "En",
            Language::Ja => "Ja",
        }
    }

    /// Cycle through available languages. Used by the Settings tab's left/right
    /// keys to toggle between the (currently two) supported locales.
    #[allow(dead_code)] // Used by Settings tab (later commit in this PR).
    pub fn next(self) -> Self {
        match self {
            Language::En => Language::Ja,
            Language::Ja => Language::En,
        }
    }
}

/// Static translation table.
///
/// Indexed by key, each value is `[english, japanese]`. The table is built
/// once on first access via [`OnceLock`] (`LazyLock` would require MSRV >=
/// 1.80, but this crate pins MSRV to 1.78).
#[allow(dead_code)] // Used by t() (which itself is used in later commits of this PR).
fn dict() -> &'static HashMap<&'static str, [&'static str; 2]> {
    static DICT: OnceLock<HashMap<&'static str, [&'static str; 2]>> = OnceLock::new();
    DICT.get_or_init(build_dict)
}

#[allow(dead_code)] // Used by dict() (which itself is used in later commits of this PR).
fn build_dict() -> HashMap<&'static str, [&'static str; 2]> {
    let mut m: HashMap<&'static str, [&'static str; 2]> = HashMap::new();

    // ── Tabs ─────────────────────────────────────────────────────
    m.insert("tab.dashboard", ["Dashboard", "ダッシュボード"]);
    m.insert("tab.collection", ["Collection", "コレクション"]);
    m.insert("tab.achievements", ["Achievements", "実績"]);
    m.insert("tab.stats", ["Stats", "統計"]);
    m.insert("tab.synthesis", ["Synthesis", "合成"]);
    m.insert("tab.settings", ["Settings", "設定"]);

    // ── Sections (per tab) ───────────────────────────────────────
    m.insert("section.overview", ["Overview", "概要"]);
    m.insert("section.login_bonus", ["Login Bonus", "ログインボーナス"]);
    m.insert(
        "section.daily_missions",
        ["Daily Missions", "デイリーミッション"],
    );
    m.insert("section.collection", ["Collection", "所持一覧"]);
    m.insert("section.dictionary", ["Dictionary", "図鑑"]);
    m.insert("section.achievable", ["Achievable", "達成可能"]);
    m.insert("section.in_progress", ["In Progress", "進行中"]);
    m.insert("section.unlocked", ["Unlocked", "達成済み"]);
    m.insert("section.rarity", ["Rarity", "レアリティ"]);
    m.insert("section.category", ["Category", "カテゴリ"]);
    m.insert("section.timeline", ["Timeline", "時系列"]);
    m.insert("section.recipes", ["Recipes", "レシピ一覧"]);
    m.insert("section.synthesize", ["Synthesize", "合成実行"]);
    m.insert("section.language", ["Language", "言語"]);

    // ── Block titles ─────────────────────────────────────────────
    m.insert("block.overview", ["Overview", "概要"]);
    m.insert("block.login_bonus", ["Login Bonus", "ログインボーナス"]);
    m.insert(
        "block.next_curion",
        ["Next Curion", "次のキュリオン生成まで"],
    );
    m.insert("block.xp", ["XP", "XP"]);
    m.insert("block.stats", ["Stats", "統計"]);
    m.insert("block.latest_curion", ["Latest Curion", "最新キュリオン"]);
    m.insert(
        "block.rarity_breakdown",
        ["Rarity Breakdown", "レアリティ分布"],
    );
    m.insert(
        "block.category_breakdown",
        ["Category Breakdown", "カテゴリ分布"],
    );
    m.insert(
        "block.upcoming_goals",
        ["Upcoming Goals", "もうすぐ達成できる目標"],
    );
    m.insert("block.collection", ["Collection", "コレクション"]);
    m.insert("block.dictionary", ["Dictionary", "図鑑"]);
    m.insert("block.categories", ["Categories", "Categories"]);
    m.insert("block.achievable", ["Achievable", "達成可能"]);
    m.insert("block.in_progress", ["In Progress", "進行中"]);
    m.insert("block.unlocked", ["Unlocked", "達成済み"]);
    m.insert("block.rarity", ["Rarity", "レアリティ"]);
    m.insert("block.category", ["Category", "カテゴリ"]);
    m.insert("block.timeline", ["Timeline", "時系列"]);
    m.insert("block.player", ["PLAYER", "PLAYER"]);
    m.insert(
        "block.recent_acquisitions",
        ["RECENT ACQUISITIONS", "RECENT ACQUISITIONS"],
    );
    m.insert(
        "block.rarity_breakdown_caps",
        ["RARITY BREAKDOWN", "RARITY BREAKDOWN"],
    );
    m.insert(
        "block.category_breakdown_caps",
        ["CATEGORY BREAKDOWN", "CATEGORY BREAKDOWN"],
    );
    m.insert(
        "block.category_detail",
        ["CATEGORY DETAIL", "CATEGORY DETAIL"],
    );
    m.insert("block.session", ["SESSION", "SESSION"]);
    m.insert("block.login_streak", ["LOGIN STREAK", "LOGIN STREAK"]);
    m.insert("block.today_vs_best", ["TODAY VS BEST", "TODAY VS BEST"]);
    m.insert(
        "block.daily_30",
        ["DAILY (last 30 days)", "DAILY (last 30 days)"],
    );
    m.insert("block.recipes", ["Recipes", "レシピ一覧"]);
    m.insert("block.synthesize", ["Synthesize", "合成実行"]);
    m.insert("block.help", ["Help", "Help"]);
    m.insert("block.ingredient1", ["Ingredient 1", "Ingredient 1"]);
    m.insert("block.ingredient2", ["Ingredient 2", "Ingredient 2"]);
    m.insert(
        "block.select_ingredient1",
        ["Select Ingredient 1", "Select Ingredient 1"],
    );
    m.insert(
        "block.select_ingredient2",
        ["Select Ingredient 2", "Select Ingredient 2"],
    );
    m.insert("block.selected", ["Selected", "Selected"]);
    m.insert("block.settings", ["Settings", "設定"]);

    // ── Categories ───────────────────────────────────────────────
    m.insert("category.animal", ["Animal", "動物"]);
    m.insert("category.plant", ["Plant", "植物"]);
    m.insert("category.color", ["Color", "色"]);
    m.insert("category.object", ["Object", "物体"]);
    m.insert("category.concept", ["Concept", "概念"]);
    m.insert("category.element", ["Element", "元素"]);
    m.insert("category.food", ["Food", "食べ物"]);
    m.insert("category.phenomenon", ["Phenomenon", "現象"]);
    m.insert("category.abstract", ["Abstract", "抽象"]);

    // ── Settings labels ──────────────────────────────────────────
    m.insert("settings.language", ["Language", "言語"]);
    m.insert(
        "settings.language_help",
        [
            "Use ←/→ to switch language. The change is saved immediately.",
            "←/→ で言語を切り替えます。変更は即座に保存されます。",
        ],
    );

    // ── Help-line phrases (shared) ───────────────────────────────
    m.insert("help.left_pane", ["left pane", "左ペイン"]);
    m.insert("help.detail_scroll", ["detail scroll", "詳細スクロール"]);
    m.insert("help.scroll", ["scroll", "スクロール"]);
    m.insert("help.category_move", ["category move", "カテゴリ移動"]);
    m.insert("help.noun_scroll", ["noun scroll", "名詞スクロール"]);
    m.insert("help.filter", ["filter", "絞り込み"]);
    m.insert("help.generate", ["generate", "生成"]);
    m.insert("help.tab", ["tab", "タブ"]);
    m.insert("help.save", ["save", "保存"]);
    m.insert("help.quit", ["quit", "終了"]);
    m.insert("help.achievement_select", ["select", "実績選択"]);
    m.insert("help.claim_reward", ["claim", "報酬受取"]);
    m.insert("help.candidate_select", ["candidate", "候補選択"]);
    m.insert("help.synthesize_now", ["synthesize", "合成"]);
    m.insert("help.back", ["back", "戻る"]);
    m.insert("help.lang_switch", ["language", "言語切替"]);

    // ── Filter mode ──────────────────────────────────────────────
    m.insert("help.filter_mode", ["filter", "filter"]);
    m.insert("help.filter_typing", ["typing regex", "正規表現入力中"]);
    m.insert(
        "help.filter_confirm",
        ["confirm (keep filter)", "入力確定 (フィルタ維持)"],
    );
    m.insert("help.filter_clear", ["clear", "解除"]);
    m.insert("help.filter_backspace", ["delete 1 char", "1 文字削除"]);

    m
}

/// Look up `key` in the configured `lang`, falling back to English when the
/// key is missing entirely, and finally to the key string itself.
///
/// Returning a `&'static str` keeps callers allocation-free for the common
/// case where the dictionary holds every key used by the UI.
#[allow(dead_code)] // Wired into ui.rs in later commits of this PR.
pub fn t(key: &str, lang: Language) -> &'static str {
    if let Some(entry) = dict().get(key) {
        return entry[lang.index()];
    }
    // Unknown key: surface the key so missing dictionary entries are obvious
    // during development without panicking at runtime.
    Box::leak(key.to_string().into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// English is the default per Issue #63.
    #[test]
    fn language_default_is_english() {
        assert_eq!(Language::default(), Language::En);
    }

    /// `Language::next` cycles between the two supported locales.
    #[test]
    fn language_next_cycles() {
        assert_eq!(Language::En.next(), Language::Ja);
        assert_eq!(Language::Ja.next(), Language::En);
    }

    /// `t` returns the column matching the configured language.
    #[test]
    fn t_returns_per_language_strings() {
        assert_eq!(t("tab.dashboard", Language::En), "Dashboard");
        assert_eq!(t("tab.dashboard", Language::Ja), "ダッシュボード");
    }

    /// Unknown keys do not panic and surface the key itself.
    #[test]
    fn t_falls_back_to_key_for_unknown() {
        let out = t("definitely.missing.key", Language::En);
        assert_eq!(out, "definitely.missing.key");
    }

    /// Spot check that every Phase 1 category key is present in both columns.
    #[test]
    fn all_categories_are_translated() {
        for key in [
            "category.animal",
            "category.plant",
            "category.color",
            "category.object",
            "category.concept",
            "category.element",
            "category.food",
            "category.phenomenon",
            "category.abstract",
        ] {
            assert!(!t(key, Language::En).is_empty(), "{key} En empty");
            assert!(!t(key, Language::Ja).is_empty(), "{key} Ja empty");
        }
    }

    // ── Issue #63 Phase 1: i18n core coverage ────────────────────────

    /// A-2: Calling `next()` twice cycles `En → Ja → En` (round trip).
    #[test]
    fn language_next_round_trip_en_ja_en() {
        assert_eq!(Language::En.next().next(), Language::En);
    }

    /// A-3: `short_label()` returns the canonical "En" / "Ja" pair used by
    /// the Settings tab.
    #[test]
    fn language_short_label_pair() {
        assert_eq!(Language::En.short_label(), "En");
        assert_eq!(Language::Ja.short_label(), "Ja");
    }

    /// A-4: dict column contract — `En` is index 0 and `Ja` is index 1.
    /// Drifting this would silently swap the entire translation table.
    #[test]
    fn language_index_is_dict_column() {
        assert_eq!(Language::En.index(), 0);
        assert_eq!(Language::Ja.index(), 1);
    }

    /// A-8: No dictionary entry may carry an empty string in either column.
    /// Empty entries would render blank labels in the UI.
    #[test]
    fn dict_has_no_empty_string_in_either_column() {
        for (key, columns) in dict().iter() {
            assert!(!columns[0].is_empty(), "{key} En column is empty");
            assert!(!columns[1].is_empty(), "{key} Ja column is empty");
        }
    }

    /// B-3: Every Category's `display()` is non-empty in both languages and
    /// the En/Ja renditions are distinct (= no accidental Ja-as-En fallback).
    #[test]
    fn category_keys_all_resolved() {
        use crate::curion::Category;
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
            assert!(!en.is_empty(), "{cat:?} En display is empty");
            assert!(!ja.is_empty(), "{cat:?} Ja display is empty");
            assert_ne!(en, ja, "{cat:?} En and Ja display happen to be identical");
        }
    }
}
