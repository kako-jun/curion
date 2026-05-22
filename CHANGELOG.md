# Changelog

## Unreleased

### Changed
- `docs/design.md` をルート直下の `DESIGN.md` に戻した。他リポ (selona / open-sen / osaka-kenpo / xmj 等) の慣習に合わせ、UI デザインシステム文書はリポルート直下 `DESIGN.md` に置く方針で統一。v0.2.0 で `docs/` 配下に移したが、ルートに集約する形に揃え直した。内容変更なし。

### Added
- i18n Phase 4 (#71): Achievement / Daily Mission / Evolution / Synthesis recipe のゲーム内本文を lang gate に通した。
  - `data/evolutions/lines.json` の 5 系列に `display_name_en` を追加。`EvolutionLine::display_name_for(lang)` で取得。
  - `data/recipes/basic_recipes.json` の 17 レシピに `name_en` と `description_en` を追加。`SynthesisRecipe::name_for(lang)` / `description_for(lang)` を提供。Unknown レシピのラベル "未確認レシピ #NN" は EN 時 "Unrecorded recipe #NN" に切り替わる。
  - `Achievement` に `name_en` / `description_en` / `reward_title_en` を追加し、`with_en(...)` builder で register 時に英訳を流し込む。`name_for(lang)` / `description_for(lang)` / `reward_title_for(lang)` を提供。
  - `DailyMission` テンプレートに `description_en` を追加し、`DailyMission::description_for(lang)` を提供。
  - `SynthesisManager::try_synthesize_lang` / `try_synthesize_with_rolls_lang` を追加し、合成成功・発見・高リスク失敗・No recipe のトースト文を `--lang en` 起動時に英語化。Dashboard の進化進捗、Achievements タブ、デイリーミッション、レシピ一覧の各タブ本文を lang 経由で表示。

### Removed
- Interactive モードの `save` コマンドを削除 (#73, #62 follow-up)。永続化は startup / exit / tui / Ctrl-D の自動セーブに一本化。
- 矢印キー (Up/Down/Left/Right) / PageUp / PageDown / Tab キーの TUI 入力ハンドラを廃止 (#72)。

### Changed
- TUI キーマップを vim 風に統一 (#74): `h/l` でタブ移動、`j/k` でスクロール、`J/K` でセクション切替（複数セクションを持つ全タブ）、Settings タブの言語切替を `←/→` から `Enter` に変更、`gg` で先頭・`G` で末尾へジャンプ（隠しモーション）。

## v0.3.0 - 2026-05-22

The "event-driven + bilingual" release: save is no longer a 60-second poll, and the UI can switch between English (default) and Japanese at runtime.

### Added
- i18n foundation (#63 Phase 1): English is now the canonical UI locale with a runtime switch to Japanese via a new Settings tab. `←/→` toggles the language and persists it immediately. Tab names, sections, block titles, help-line labels, and category names are routed through a static `t(key, lang)` translation table. `Language { En, Ja }` (default `En`) lives on `SerializableGameState` with `#[serde(default)]` so older save files load as English.
- i18n flavor data (#65 Phase 2): every one of the 268 nouns now carries a `flavor_en` field alongside the existing Japanese `flavor`. Translations preserve the curion world vocabulary ("particle of curiosity", "observer", "crystallization") with a per-category tone (abstracts most philosophical, phenomena most poetic, objects/foods symbolic but grounded). Flavor display routing through the language gate is tracked under #68 (Phase 3).
- Settings tab (6th tab): currently houses the language switch; future settings (theme, profile reset, public key display) will land here.

### Changed
- Event-driven save (#62): the 60-second auto-save poll and the manual `s` save key are gone. Persistence is now driven by an `App::dirty` flag set at the six mutation points (gacha pull, equip toggle, achievement claim, synthesis success / risky-failure, daily mission auto-claim). The main loop flushes to disk right after a key event or `on_tick` whenever `dirty` is true. Continuous values (`play_time`, SAN decay, rare cooldown) deliberately do not arm `dirty` — they are covered by startup / shutdown saves. `s` outside filter mode is now a no-op so `/`-filter typing of "s" works correctly.
- `Curion` rendering goes through `App::display_curion_name(&Curion)`, which formats as `"{Category} の {noun}"` in Ja, `"{english} ({Category})"` in En with a runtime `NounDatabase::english_for` lookup, and falls back to `"{ja-noun} ({Category-En})"` when the English entry is missing (synthesis-only nouns).

### Internal
- `Tab::COUNT` constant + propagation through `from_index` / `next` / `section_indices` / `handle_key` so adding a tab is a single-knob change.
- All-new `src/i18n.rs` (OnceLock-backed `t(key, lang)` table) with a debug-only `#[should_panic]` test that catches unregistered keys at the call site (release builds return `"?"`).
- 244 tests pass (203 baseline + 11 dirty-flag + 30 i18n coverage). `cargo clippy --all-targets -- -D warnings` clean.

## v0.2.0 - 2026-05-17

The "single-player addiction" release: every system needed to make a focused offline session feel rewarding is now in place.

### Added
- 図鑑機能 (#18): Collection tab dictionary listing every noun across all categories with locked `？？？` placeholders and per-category progress.
- デイリーミッション (#20): three date-seeded daily missions on the Dashboard with auto-claim and XP rewards.
- コンボシステム (#21): consecutive Rare+ pulls grant escalating XP multipliers and the "コンボマスター" title at combo 5.
- フレーバーテキスト (#22): every one of the 268 nouns now carries a sci-fi flavor line, surfaced in the Collection detail pane and dictionary.
- レア出現予告クールダウン (#25): 4-hour LineGauge cooldown on the Dashboard that boosts Rare+ probability up to 2x at full charge.
- 入手履歴 (#27): each curion remembers its global acquisition index (continues across synthesis consumption).
- 行動前確率表示 (#28): success rate gauges for synthesis recipes and live rarity probability percentages on the Dashboard.
- SAN 値 (#29): sanity stat with rarity-based gains, time-based decay, and a Dashboard Gauge with color thresholds.
- 寿命システム (#30): rarity-tiered expiration with Dashboard warnings and automatic pruning at login.
- 正規表現フィルタ (#31): `/` opens regex search across Collection (name / display name / rarity / category).
- きりの悪い数字設計 (#32): non-linear XP thresholds and achievement counts plus a "next milestone" indicator.
- Stats タブ仕上げ (#26): daily 30-day Sparkline for collection rate.
- 高リスク合成 (#35): per-recipe `success_rate` and `failure_mode` (NoLoss / LoseAll / Salvage) with SAFE/RISKY badges.
- 段階進化ガチャ (#36): 3-stage evolution lines tracked by collection counts, with "almost complete" highlights on the Dashboard.
- 部分公開レシピ (#37): per-recipe `Public` / `Partial` / `Unknown` visibility levels with progressively revealed labels.
- 意味空間ベクトル装備効果 (#38) と文字列→潜在ベクトル→ラベルの対称パイプライン (#39).
- **最新キュリオン名 bloom 演出** (jiwa): Dashboard の「最新キュリオン」表示が、新規生成の瞬間に grapheme 単位で暗(50,50,50)→白(255,255,255) に 220 ms フェードインする。中毒ループの「届いた感」を視線でキャッチできるように。Powered by the [`jiwa`](https://crates.io/crates/jiwa) crate, shared with `type-globe` and `gitpp`.

### Changed
- Docs reorganized: `DESIGN.md` moved to `docs/design.md`; `.claude/*.md` (vision / synthesis_and_p2p_design / p2p_detailed_design / addictive_ideas / implementation_roadmap) moved under `docs/` with lowercase filenames. The `.claude/` directory was removed.

## v0.1.3 - 2026-05-15

### Added
- Dashboard login bonus with escalating streak rewards, guaranteed ticket inventory, and local-date daily claim handling.

### Changed
- TUI visuals now follow `docs/design.md` more closely across Dashboard, Achievements, Stats, and Synthesis.
- Stats tab now includes sparkline/bar-chart based monitoring views instead of text-only summaries.

### Fixed
- Login streak / daily claim logic now uses a single local-day boundary derived from one timestamp.
- Documentation was synced with the shipped TUI layout and dashboard behavior.
