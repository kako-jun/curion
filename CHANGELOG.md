# Changelog

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
