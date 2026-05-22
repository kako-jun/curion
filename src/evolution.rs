//! Issue #36: 段階進化ガチャ (Stage Evolution)
//!
//! 一部のキュリオン (`noun`) を「進化系列」の構成員と見なし、所持数が閾値に達するごとに
//! 次の段階を解放する。図鑑的なメタ進捗としてプレイヤーに「あと N 個で次の進化」という
//! 期待感を提示するためのロジック層。UI 表示は `ui.rs` に閉じている。
//!
//! ## 設計方針 (シンプル化)
//!
//! - 「合成成功で進化」「時間経過で進化」は本 Issue ではスコープ外。
//!   `Player::collection` の所持数だけを見て段階を計算する純粋関数。将来 Issue で
//!   `recipe id` フックや `acquired_at` を見る経路を追加する余地を残す。
//! - 進化系列は `data/evolutions/lines.json` を `include_str!` で埋め込み、起動時に
//!   `EvolutionDatabase::load_embedded()` で読み込む。データ駆動。
//! - `EvolutionProgress` はデータベースへの参照を持つ。テストでは単一のデータベースを
//!   構築して借用するスタイルで書く。
//!
//! ## データモデル
//!
//! ```text
//! EvolutionLine
//!   id: "fish_dragon"
//!   display_name: "魚 → 蛇 → 龍"
//!   stages:
//!     - { stage: 1, noun: "魚", required_count: 10 }   ← 10 体集めると stage 2 解放
//!     - { stage: 2, noun: "蛇", required_count: 3 }    ← さらに 3 体集めると stage 3 解放
//!     - { stage: 3, noun: "龍", required_count: 1 }    ← 1 体所持で系列完成
//! ```
//!
//! プレイヤー視点では `EvolutionProgress.current_stage` が「到達済みの最大 stage」、
//! `remaining_to_next` が次段階解放までの不足数になる。

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::curion::Curion;
use crate::i18n::Language;

/// 埋め込みデータ。`data/evolutions/lines.json` をビルド時に取り込む。
const EVOLUTIONS_JSON: &str = include_str!("../data/evolutions/lines.json");

/// 進化系列の 1 段階。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvolutionStage {
    /// 段階番号 (1..=3 想定)。1 が始点、最大値が完成形。
    pub stage: u8,
    /// この段階の構成名詞 (例: "魚")。`Curion::noun` と完全一致で判定する。
    pub noun: String,
    /// この段階の解放条件となる所持数 (累積ではなく「この noun の所持数」)。
    ///
    /// 例: stage 1 の required_count = 10 は「`noun` を 10 体集めると stage 2 が解放される」を意味する。
    /// 最終 stage の required_count は「系列完成 (stage N 到達) に必要な最終 noun 数」を表す。
    pub required_count: u32,
}

/// 進化系列 (魚 → 蛇 → 龍 など)。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvolutionLine {
    /// 系列を一意に識別する ID (例: "fish_dragon")。
    pub id: String,
    /// UI 表示用ラベル (例: "魚 → 蛇 → 龍")。
    pub display_name: String,
    /// English UI 表示用ラベル (例: "fish → snake → dragon")。Issue #71 Phase 4。
    #[serde(default)]
    pub display_name_en: String,
    /// 段階リスト。`stage` 昇順で並んでいる前提。
    pub stages: Vec<EvolutionStage>,
}

impl EvolutionLine {
    /// 言語別の表示名 (Issue #71 Phase 4)。
    ///
    /// `Language::En` のときに `display_name_en` が空文字なら (旧データ互換)
    /// JA の `display_name` を fallback として返す。
    pub fn display_name_for(&self, lang: Language) -> &str {
        match lang {
            Language::Ja => &self.display_name,
            Language::En => {
                if self.display_name_en.is_empty() {
                    &self.display_name
                } else {
                    &self.display_name_en
                }
            }
        }
    }

    /// 最大 stage 番号。空配列に対しては 0 を返す (異常データ防御)。
    pub fn max_stage(&self) -> u8 {
        self.stages.iter().map(|s| s.stage).max().unwrap_or(0)
    }

    /// この系列に登場するすべての noun。
    /// 将来 Issue (例: Collection タブの「進化系列タグ」表示) 用に公開しておく。
    #[allow(dead_code)]
    pub fn nouns(&self) -> impl Iterator<Item = &str> {
        self.stages.iter().map(|s| s.noun.as_str())
    }

    /// 与えた noun がこの系列のどの stage に該当するか。
    fn stage_for_noun(&self, noun: &str) -> Option<&EvolutionStage> {
        self.stages.iter().find(|s| s.noun == noun)
    }
}

/// 進化系列の埋め込みデータベース。
#[derive(Debug, Clone)]
pub struct EvolutionDatabase {
    lines: Vec<EvolutionLine>,
}

impl EvolutionDatabase {
    /// `data/evolutions/lines.json` を読み込む。
    pub fn load_embedded() -> Result<Self> {
        let lines: Vec<EvolutionLine> = serde_json::from_str(EVOLUTIONS_JSON)
            .context("failed to parse data/evolutions/lines.json")?;
        Ok(Self { lines })
    }

    /// 登録された進化系列をすべて返す。
    #[allow(dead_code)]
    pub fn all_lines(&self) -> &[EvolutionLine] {
        &self.lines
    }

    /// `noun` を含む進化系列を返す (なければ `None`)。
    /// 同じ noun が複数系列に登場するケースは想定しないが、念のため最初の一致を返す。
    /// 将来 Issue (Collection 詳細ペインで「この curion は ○ 系列の Stage N」表示) 用に公開しておく。
    #[allow(dead_code)]
    pub fn line_for_noun(&self, noun: &str) -> Option<&EvolutionLine> {
        self.lines
            .iter()
            .find(|line| line.stage_for_noun(noun).is_some())
    }

    /// プレイヤーの所持コレクションから、全系列の進捗を計算する。
    ///
    /// 計算ルール:
    /// - 各 stage について `collection` 内の `noun` 所持数を数える。
    /// - stage k に到達するには、stage 1..k-1 のすべての `required_count` を満たし、
    ///   かつ stage k の noun を最低 1 体所持している必要がある。
    /// - 最終 stage に到達 (=完成) した場合、`remaining_to_next = 0` / `next_stage_required = None`。
    pub fn calculate_progress<'a>(&'a self, collection: &[Curion]) -> Vec<EvolutionProgress<'a>> {
        self.lines
            .iter()
            .map(|line| EvolutionProgress::from_collection(line, collection))
            .collect()
    }
}

/// プレイヤー視点での進化系列進捗。
///
/// データベース (`EvolutionLine`) への参照を保持するため、`EvolutionDatabase` よりも
/// 長く生きてはならない。UI レンダリングや 1 ティック内の表示計算用。
#[derive(Debug, Clone)]
pub struct EvolutionProgress<'a> {
    /// 元の進化系列定義。
    pub line: &'a EvolutionLine,
    /// 到達済みの最大 stage (1..=max_stage)。1 体も所持していない場合は 0。
    pub current_stage: u8,
    /// 次段階に到達するために必要な「次 noun」の所持数。完成済みなら `None`。
    pub next_stage_required: Option<u32>,
    /// 次段階に到達するための不足数 (= required - 現在所持)。完成済みなら 0。
    pub remaining_to_next: u32,
    /// 次段階の noun (例: "海蛇")。完成済みなら `None`。
    pub next_stage_noun: Option<&'a str>,
    /// 現所持数 / required の比率 (0.0..=1.0)。完成済みなら 1.0。
    /// 「あと少し」感のソート用に使う。
    pub progress_ratio: f64,
}

impl<'a> EvolutionProgress<'a> {
    /// 完成済みかどうか (= 最終 stage に到達)。
    pub fn is_complete(&self) -> bool {
        self.next_stage_required.is_none()
    }

    /// 「あと 1 個」状態かどうか (未完成 かつ remaining_to_next == 1)。
    pub fn is_almost_complete(&self) -> bool {
        !self.is_complete() && self.remaining_to_next == 1
    }

    fn from_collection(line: &'a EvolutionLine, collection: &[Curion]) -> Self {
        // 各 stage の noun を所持カウント。
        let counts: Vec<u32> = line
            .stages
            .iter()
            .map(|s| collection.iter().filter(|c| c.noun == s.noun).count() as u32)
            .collect();

        // 到達済み最大 stage を決定。
        // - stage 1 は 1 体所持で「到達」。required_count はあくまで「次段階解放条件」。
        // - stage k (k>=2) に到達するには stage (k-1) の所持 >= required_count か、
        //   かつ stage k の noun を 1 体以上所持。
        let mut current_stage: u8 = 0;
        for (i, stage) in line.stages.iter().enumerate() {
            let owned = counts[i];
            if i == 0 {
                if owned >= 1 {
                    current_stage = stage.stage;
                }
            } else {
                // 前段階の閾値を満たしているか
                let prev = &line.stages[i - 1];
                let prev_owned = counts[i - 1];
                if prev_owned >= prev.required_count && owned >= 1 {
                    current_stage = stage.stage;
                }
            }
        }

        // 次段階の決定: current_stage の次に解放されるべき stage を探す。
        // current_stage == 0 (1 体も持っていない) なら 次は stage 1。
        // current_stage == max なら完成。
        let max_stage = line.max_stage();
        if current_stage >= max_stage && max_stage > 0 {
            return Self {
                line,
                current_stage,
                next_stage_required: None,
                remaining_to_next: 0,
                next_stage_noun: None,
                progress_ratio: 1.0,
            };
        }

        // 次段階に必要な行動を計算する。
        // - current_stage == 0: stage 1 noun を 1 体入手する (= required 1, 不足 1)
        // - current_stage >= 1: stage current の required_count を満たすこと
        if current_stage == 0 {
            let stage1 = &line.stages[0];
            return Self {
                line,
                current_stage: 0,
                next_stage_required: Some(1),
                remaining_to_next: 1,
                next_stage_noun: Some(stage1.noun.as_str()),
                progress_ratio: 0.0,
            };
        }

        // current_stage は line.stages の (current_stage - 1) インデックスに対応する想定。
        // (stage 番号が 1 から連番である前提。)
        let cur_idx = (current_stage as usize).saturating_sub(1);
        let cur_stage = &line.stages[cur_idx];
        let cur_owned = counts[cur_idx];
        let required = cur_stage.required_count;
        let remaining = required.saturating_sub(cur_owned);
        let next_idx = cur_idx + 1;
        let next_noun = line.stages.get(next_idx).map(|s| s.noun.as_str());
        let ratio = if required == 0 {
            1.0
        } else {
            (cur_owned as f64 / required as f64).min(1.0)
        };

        Self {
            line,
            current_stage,
            next_stage_required: Some(required),
            remaining_to_next: remaining,
            next_stage_noun: next_noun,
            progress_ratio: ratio,
        }
    }
}

/// 進捗を「あと少し感」順に並べる (Dashboard の「進化進捗トップ N」用)。
///
/// ソートキー:
/// 1. 未完成を先に、完成を後ろに
/// 2. `progress_ratio` が大きい方 (= 次段階に近い方) を先に
/// 3. 同率なら remaining_to_next が小さい順
/// 4. 最後に line.id で安定化
pub fn sort_progress_by_urgency(progresses: &mut [EvolutionProgress<'_>]) {
    progresses.sort_by(|a, b| match (a.is_complete(), b.is_complete()) {
        (true, false) => std::cmp::Ordering::Greater,
        (false, true) => std::cmp::Ordering::Less,
        _ => b
            .progress_ratio
            .partial_cmp(&a.progress_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.remaining_to_next.cmp(&b.remaining_to_next))
            .then_with(|| a.line.id.cmp(&b.line.id)),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curion::{Category, Rarity};
    use uuid::Uuid;

    fn db() -> EvolutionDatabase {
        EvolutionDatabase::load_embedded().expect("embedded evolutions should load")
    }

    /// 同じ noun のキュリオンを `count` 体分作って返す。
    fn curions(noun: &str, count: usize) -> Vec<Curion> {
        (0..count)
            .map(|_| {
                Curion::new(
                    Uuid::new_v4(),
                    noun.to_string(),
                    Category::Animal,
                    Rarity::Common,
                    0.5,
                    0.5,
                )
            })
            .collect()
    }

    /// 1. 埋め込み JSON が読めて、3 系列以上ある。
    #[test]
    fn test_load_embedded_evolutions() {
        let db = db();
        assert!(
            db.all_lines().len() >= 3,
            "expected at least 3 evolution lines, got {}",
            db.all_lines().len()
        );
        // 各系列に stage 1..N が並んでいること
        for line in db.all_lines() {
            assert!(!line.stages.is_empty(), "line {} has no stages", line.id);
            for (i, stage) in line.stages.iter().enumerate() {
                assert_eq!(
                    stage.stage as usize,
                    i + 1,
                    "line {}: stages must be 1..=N in order",
                    line.id
                );
                assert!(stage.required_count >= 1, "required_count must be >= 1");
                assert!(!stage.noun.is_empty(), "noun must not be empty");
            }
        }
    }

    /// 2. `line_for_noun` が正しい系列を返す。「魚」→ fish_dragon。
    #[test]
    fn test_line_for_noun_returns_correct_line() {
        let db = db();
        let line = db.line_for_noun("魚").expect("魚 should map to a line");
        assert_eq!(line.id, "fish_dragon");
        // 該当 noun が無い場合は None
        assert!(db.line_for_noun("__no_such_noun__").is_none());
    }

    /// 3. 何も所持していない場合、全系列の current_stage = 0、次は stage 1 noun ×1。
    #[test]
    fn test_calculate_progress_no_collection() {
        let db = db();
        let progress = db.calculate_progress(&[]);
        assert_eq!(progress.len(), db.all_lines().len());
        for p in progress {
            assert_eq!(p.current_stage, 0);
            assert_eq!(p.remaining_to_next, 1);
            assert_eq!(p.next_stage_required, Some(1));
            assert!(p.next_stage_noun.is_some());
            assert!(!p.is_complete());
        }
    }

    /// 4. 「魚」×5 で stage 1 到達、stage 2 解放まで残り 5 (10-5)、ratio = 0.5。
    #[test]
    fn test_calculate_progress_partial_stage_1() {
        let db = db();
        let collection = curions("魚", 5);
        let progresses = db.calculate_progress(&collection);
        let fish = progresses
            .iter()
            .find(|p| p.line.id == "fish_dragon")
            .expect("fish_dragon line missing");
        assert_eq!(fish.current_stage, 1);
        assert_eq!(fish.next_stage_required, Some(10));
        assert_eq!(fish.remaining_to_next, 5);
        assert_eq!(fish.next_stage_noun, Some("蛇"));
        assert!((fish.progress_ratio - 0.5).abs() < 1e-9);
        assert!(!fish.is_complete());
    }

    /// 5. 「魚」×10 + 「蛇」×1 で stage 2 解放、次段階 (蛇 ×3) の進捗が表示される。
    #[test]
    fn test_calculate_progress_advances_to_stage_2() {
        let db = db();
        let mut collection = curions("魚", 10);
        collection.extend(curions("蛇", 1));
        let progresses = db.calculate_progress(&collection);
        let fish = progresses
            .iter()
            .find(|p| p.line.id == "fish_dragon")
            .unwrap();
        assert_eq!(fish.current_stage, 2);
        assert_eq!(fish.next_stage_required, Some(3));
        assert_eq!(fish.remaining_to_next, 2); // 蛇 1/3 → あと 2
        assert_eq!(fish.next_stage_noun, Some("龍"));
        assert!(!fish.is_complete());
    }

    /// 6. 全 stage 完成: 魚×10 + 蛇×3 + 龍×1 で current=3, next=None, ratio=1.0。
    #[test]
    fn test_calculate_progress_complete() {
        let db = db();
        let mut collection = curions("魚", 10);
        collection.extend(curions("蛇", 3));
        collection.extend(curions("龍", 1));
        let progresses = db.calculate_progress(&collection);
        let fish = progresses
            .iter()
            .find(|p| p.line.id == "fish_dragon")
            .unwrap();
        assert_eq!(fish.current_stage, 3);
        assert_eq!(fish.next_stage_required, None);
        assert_eq!(fish.remaining_to_next, 0);
        assert_eq!(fish.next_stage_noun, None);
        assert!(fish.is_complete());
        assert!((fish.progress_ratio - 1.0).abs() < 1e-9);
    }

    /// 7. ソート: 完成済みは最後、未完成は ratio が高い順、次に remaining_to_next が少ない順。
    #[test]
    fn test_progress_sort_order() {
        let db = db();
        // fish_dragon を完成 (ratio = 1.0, complete)
        let mut col = curions("魚", 10);
        col.extend(curions("蛇", 3));
        col.extend(curions("龍", 1));
        // bamboo_pine_forest: 竹 ×4/8 → ratio 0.5, remaining 4
        col.extend(curions("竹", 4));
        // fire_flame_phoenix: 火 ×6/7 → ratio ~0.857, remaining 1 (あと 1)
        col.extend(curions("火", 6));
        // water_ice_whale / light_star_sun: 0 体 → ratio 0.0, remaining 1 (stage 0)
        let mut progresses = db.calculate_progress(&col);
        sort_progress_by_urgency(&mut progresses);

        // 1 番目は「あと 1 個」状態の fire_flame_phoenix
        assert_eq!(progresses[0].line.id, "fire_flame_phoenix");
        assert!(progresses[0].is_almost_complete());
        // 2 番目は bamboo_pine_forest (ratio 0.5)
        assert_eq!(progresses[1].line.id, "bamboo_pine_forest");
        // 最後は完成済み fish_dragon
        let last = progresses.last().unwrap();
        assert_eq!(last.line.id, "fish_dragon");
        assert!(last.is_complete());
    }

    /// 8. 同 noun の複数キュリオンを正しくカウントする (id ではなく noun でカウント)。
    #[test]
    fn test_count_by_noun_not_id() {
        let db = db();
        // 「光」×11 = stage 1 到達 (light_star_sun は stage 1 required=12, あと 1)
        let collection = curions("光", 11);
        let progresses = db.calculate_progress(&collection);
        let light = progresses
            .iter()
            .find(|p| p.line.id == "light_star_sun")
            .unwrap();
        assert_eq!(light.current_stage, 1);
        assert_eq!(light.remaining_to_next, 1);
        assert!(light.is_almost_complete());
    }

    /// Issue #71 Phase 4: `display_name_for(Ja)` は既存 JA 文字列を返す。
    #[test]
    fn test_display_name_for_ja_matches_legacy() {
        let db = db();
        for line in db.all_lines() {
            assert_eq!(line.display_name_for(Language::Ja), line.display_name);
        }
    }

    /// Issue #71 Phase 4: `display_name_for(En)` は英訳を返す (5 系列すべて非空)。
    #[test]
    fn test_display_name_for_en_is_translated() {
        let db = db();
        for line in db.all_lines() {
            let en = line.display_name_for(Language::En);
            assert!(!en.is_empty(), "{}: En display_name is empty", line.id);
            assert_ne!(en, line.display_name, "{}: En must differ from JA", line.id);
        }
    }

    /// Issue #71 Phase 4: `display_name_en` が空のとき (legacy データ互換) は
    /// JA にフォールバックする。
    #[test]
    fn test_display_name_for_en_fallbacks_to_ja_when_empty() {
        let mut line = EvolutionLine {
            id: "legacy".to_string(),
            display_name: "魚 → 蛇 → 龍".to_string(),
            display_name_en: String::new(),
            stages: vec![],
        };
        assert_eq!(line.display_name_for(Language::En), "魚 → 蛇 → 龍");
        line.display_name_en = "fish → snake → dragon".to_string();
        assert_eq!(line.display_name_for(Language::En), "fish → snake → dragon");
    }

    /// 9. 関係のない noun を持っていても進化進捗には影響しない。
    #[test]
    fn test_unrelated_nouns_do_not_affect_progress() {
        let db = db();
        let collection = curions("猫", 100); // 猫 はどの evolution line にも属さない
        let progresses = db.calculate_progress(&collection);
        for p in &progresses {
            assert_eq!(p.current_stage, 0, "line {} should be untouched", p.line.id);
        }
    }
}
