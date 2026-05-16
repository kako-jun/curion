# Design

curion — Design System

## 1. Visual Theme & Atmosphere

SF collection game running in a terminal. The visual language borrows from Linux system monitors (glances, btop, htop): dense information panels, color-coded status, live progress bars, and sparklines — all inside a ratatui TUI.

Issue #17 establishes the 3-layer information architecture first. Some widget-level visuals in this document are target styles for the follow-up visual enhancement issue, not all fully implemented yet.

The player should feel like they are watching a scientific instrument, not a toy. Data is alive. Bars fill. Counters tick. Rare events pulse.

Inspirations: glances, btop, htop, FF14 cooldown indicators, Cookie Clicker saturation of numbers.

Dark terminal only. No light mode.

## 2. Color Palette & Roles

ratatui `Color` enum values.

| Role | Color | Usage |
|---|---|---|
| Background | `Reset` (terminal default) | Base background |
| Primary text | `White` | Default text |
| Dim text | `DarkGray` | Labels, inactive items |
| Accent | `Cyan` | Borders, focused elements, RARE rarity |
| Success | `Green` | Achieved, complete, login bonus received |
| Warning | `Yellow` | Near-complete (≥75%), EPIC rarity |
| Danger | `Red` | Urgent (≥95% achievement), LEGENDARY rarity |
| Muted | `Gray` | COM rarity, unfocused panels |

Rarity color mapping:

| Rarity | Label | Color |
|---|---|---|
| Common | `COM` | `Gray` |
| Rare | `RARE` | `Cyan` |
| Epic | `EPIC` | `Yellow` |
| Legendary | `LEG` | `Red` |

Labels are always 4 characters wide, left-aligned in `[XXXX]` brackets: `[COM ]`, `[RARE]`, `[EPIC]`, `[LEG ]`.
`Magenta` is reserved for synthesis success flash (not a rarity).

## 3. Typography Rules

Terminal monospace only. No font selection — inherits the user's terminal font.

- All labels: UPPERCASE for section headers, Title Case for item names
- Numbers: right-aligned in columns
- Percentages: always shown with `%` suffix
- Rarity labels: always in brackets `[RARE]` (4-char width: `[COM ]`, `[RARE]`, `[EPIC]`, `[LEG ]`), colored by rarity

## 4. Component Usage

### Progress Bars (ratatui `Gauge` / `LineGauge`)

Used for any quantity with a known maximum.

| Context | Widget | Color rule |
|---|---|---|
| XP bar | `Gauge` | `Cyan` → `Yellow` at 75% → `Red` at 95% |
| Achievement progress | `LineGauge` | `Gray` → `Green` when complete |
| Achievement urgency | `LineGauge` | `Gray` below 50%, `Cyan` 50-80%, `Yellow` 80-95%, `Red` ≥95% |
| Cooldown timer | `LineGauge` | `Cyan`, drains left to right |
| Daily mission | `Gauge` | `Yellow` when complete |
| Login streak | `LineGauge` | `Green` |

Label format: `[████████░░░░]  64%  (8:32 remaining)`

### Sparklines (ratatui `Sparkline`)

Used in Stats tab for time-series data.

| Context | Color |
|---|---|
| Collection rate over time | `Cyan` |
| Rarity distribution history | `Yellow` |

### Bar Charts (ratatui `BarChart`)

Used in Stats tab for categorical breakdowns.

| Context | Color |
|---|---|
| Rarity breakdown | Per-rarity color |
| Category breakdown | `Cyan` |

### Borders

- Focused panel: `Borders::ALL`, style `Cyan`, `BorderType::Rounded`
- Unfocused panel: `Borders::ALL`, style `DarkGray`, `BorderType::Plain`
- Tab bar: `BorderType::Plain`, `White`

### Lists

- Selected item: `bg(Cyan) fg(Black)` — inverse highlight
- Unselected: `fg(White)`
- Completed/achieved: `fg(Green)`
- Locked/unknown (図鑑未入手): `fg(DarkGray)`, name replaced with `???`

## 5. Layout Principles

3-layer hierarchy: Tab bar → Left pane (section list) → Right pane (content).

```
┌─ Tab bar (3 lines) ─────────────────────────────────────┐
├─ Left pane (width 20) ─┬─ Right pane (Min 0) ───────────┤
│  > 概要                │  [Gauge or content]             │
│    ログインボーナス     │                                 │
│    デイリー            │                                 │
├────────────────────────┴─────────────────────────────────┤
│ help_line (1 line)                                        │
└──────────────────────────────────────────────────────────┘
```

- Right pane should never be empty — always show something, even placeholder text
- Progress bars and gauges appear in-line within the right pane content, not as separate sections
- Numbers and bars coexist: show the raw number and the visual bar together

## 6. Motion & Feedback

- No smooth animations (terminal limitation)
- Cooldown bars redraw on each tick to simulate motion
- Achievement unlock: flash the right pane `Green` for one tick (block color invert)
- Rare curion obtained: display `[RARE]` in `Cyan`, bold

## 7. Do's and Don'ts

### Do

- Use `Gauge` or `LineGauge` for every numeric value that has a max
- Color-code rarity consistently everywhere (list, detail, achievement, stats)
- Show raw numbers alongside bars: `[████░░░░]  42 / 100`
- Use `Rounded` borders on the focused panel
- Use `Sparkline` in Stats tab for any time-series data
- Keep the right pane dense — system monitor style, not minimal

### Don't

- Use plain text where a bar would communicate the same thing better
- Show only percentages without raw numbers
- Use colors outside the defined palette
- Leave any pane empty without a placeholder

## 8. Tab Layout Principles

Each tab follows the 3-layer hierarchy. This section documents the exact layout for each tab, so new features can be added without visual drift.

### Dashboard Tab

Left pane sections: `概要 / ログインボーナス / デイリーミッション`

Right pane layout:

```
概要 (section 0):
  Top 45% — Gauges + dense stats
    - Gauge: 次のキュリオン生成まで (Cyan)
    - Gauge: XP (color-shifts Cyan→Yellow→Red by ratio)
    - LineGauge: RARE COOLDOWN (Issue #25)
      - 4 時間で満タン。filling 中は Cyan、満タンで Yellow に変色
      - filling: `RARE COOLDOWN [████░░░░░░░░] H:MM remaining`
      - full:    `RARE COOLDOWN [████████████] ⚡ レア出現確率上昇中!`
      - 進捗値は `CurionGenerator::generate_with_bonus` に渡されて roll を最大 0.3 だけ引き下げる
      - `Player::last_collection_at` に最終収集時刻を記録 (旧セーブ・新規セーブは None=フルチャージ扱い)
    - Paragraph: RARE 出現確率 (Issue #28, 1 行)
      - `RARE出現確率: 12.3%  (Common 47.7% / Rare 30.0% / Epic 9.0% / Legendary 1.0%)`
      - cooldown progress 反映済み。レア以上は Cyan+Bold (満タン時は Epic 色)、内訳は Label color
      - 確率は `crate::cooldown::current_rarity_probabilities(progress)` がロジック層で算出
      - generator の roll-shift モデル (累積境界 0.01 / 0.10 / 0.40 + `0.3*progress` シフト) と整合
    - LineGauge: SAN 値 (正気度) (Issue #29, 1 行)
      - `SAN [████████░░░░] 67.5 / 100`
      - 色: >= 80 Cyan / 50..80 Yellow / 30..50 Red / < 30 Magenta + `⚠ 異常状態` ラベル付記
      - 変動: Common +0.5 / Rare +2.0 / Epic +5.0 / Legendary +15.0 / 合成成功 +3.0 / 時間経過 -0.1/min
      - 計算は `crate::san` のピュア関数 (`san_gain_for_acquisition` / `apply_decay` / `apply_gain` / `san_state`) に閉じ、UI は値を読んで描画するだけ
      - 旧セーブ互換: `Player::san` は `#[serde(default = "default_san")]` (= 100.0 で復元)
    - Paragraph: 総獲得数 / 今日の獲得 / レベル / COMBO (one line, inline)
      - COMBO: N — Common でリセット、Rare 以上で +1
      - 表示色: 0/1=Label, 2=Rare, 3-4=Epic, 5+=Legendary + `🔥 コンボマスター!`
      - XP 倍率: 2x=1.5, 3-4x=2.0, 5+=3.0 を `add_curion` で適用 (切り捨て)
      - combo 5 到達で称号「コンボマスター」を 1 回だけ付与
    - Paragraph: next milestone (Issue #32, 1 行)
      - `next milestone: ⭐ コレクター Lv.3 (あと 4 個)` 形式
      - XP / 未解除実績の残量から最小値を選ぶ (`GameState::next_milestone`)
      - label=Label, ラベル本文=Epic+Bold, `(あと N)`=Legendary+Bold
      - 全達成なら `全マイルストーン達成済み` (Label color)
    - Paragraph: 進化進捗 (Issue #36, 最大 3 行)
      - 「あと少し感」順 (`sort_progress_by_urgency`) で最大 3 系列を 1 行ずつ表示
      - 例: `進化: 魚 → 蛇 → 龍  Stage 2 (あと 蛇 ×2 で次段階)`
      - 完成: `進化: 魚 → 蛇 → 龍  ⭐ 完成` (Green+Bold)
      - 残り 1 個は `(あと ○ ×1 で次段階)` を Cyan+Bold で強調
      - Stage 表示は Stage 0 が Label color、Stage 1+ が Epic 色
      - 計算は `crate::evolution::EvolutionDatabase::calculate_progress` に閉じる純粋関数
    - Paragraph: 最新キュリオン (rarity color)
    - Paragraph: レアリティ分布 (bar per rarity, per-rarity color)
    - Paragraph: カテゴリ分布 (compact one-liner)
  Bottom 55% — almost-complete achievement list + level-up line
    - Focused block "🎯 もうすぐ達成できる目標"
    - Each item: urgency icon + bar + raw numbers

ログインボーナス (section 1):
  - LineGauge: STREAK (Green, X/7 days)
  - LineGauge: REWARD CYCLE (Cyan, Day N)
  - Paragraph: 連続ログイン / 状態 / 今日の報酬 / 次回予告 / 所持チケット

デイリーミッション (section 2):
  - Focused block title: "デイリーミッション (リセットまで HH:MM)"
  - 3 missions stacked vertically. Each mission uses 4 lines:
    Line 1: 🎯 / ✅ + description (white / green-bold when completed)
    Line 2:   [████████░░░░]  N / target  (gauge color: Cyan progress, Green complete)
    Line 3:   報酬: +N XP  (or "[✅ +N XP claimed]" when claimed, Green)
    Line 4:   (blank separator)
  - Templates (4 種から日付シードで 3 つ抽選):
    - "10 個のキュリオンを収集": +100 XP
    - "Rare 以上を 3 個獲得": +200 XP
    - "合成を 1 回成功させる": +300 XP
    - "5 種類の異なるカテゴリから収集": +150 XP
  - 報酬は自動付与（達成判定時に XP 加算 + トースト通知）
```

### Collection Tab

Left pane sections: `所持一覧 / 図鑑`

Right pane layout:

```
所持一覧 (section 0):
  - Vertical split: top = scrollable list (Constraint::Min(3)), bottom = detail pane (Constraint::Length(4))
  - List: collected curions (newest first), each item is 3 lines:
    Line 1: #N  ★★  [RARE]  名前                 2025-01-01 12:00
    Line 2:       興味度: [██████░░░░]  60%  美しさ: [████░░░░░░]  40%
    Line 3: (blank separator)
  - Color: rarity color on stars and rarity label
  - Timestamp: DarkGray
  - Detail pane (Issue #22 + #27): unfocused_block("詳細: {noun}") with two body lines for
    the curion currently at the top of the visible list. `Wrap { trim: true }` is applied.
    Line 1: SF flavor text (Color::Gray); missing flavor falls back to `(フレーバー未登録)`.
    Line 2 (Issue #27, Color::DarkGray): acquisition history —
      `入手: YYYY-MM-DD HH:MM  (通算 N回目の収集)`, where the timestamp is rendered in the
      local TZ. Legacy save curions without `acquisition_index` show `(履歴情報なし)`.

図鑑 (section 1):
  - Two-column layout inside the focused_block("図鑑")
    - Left (Constraint::Length(22)): unfocused_block("Categories"), one row per category
      Row format: "> 名前    owned/total" (selected: COLOR_RARE bg, black fg, bold)
    - Right (Constraint::Min(0)): unfocused_block titled
      "全体: O/T (P.P%) | カテゴリ: o/t (p.p%)"
      Body lists each noun in the focused category (DB order):
        Acquired: "noun        ★★   [RARE] YYYY-MM-DD ×count"
                  (rarity color on stars+label, white bold noun, DarkGray date,
                   COLOR_SUCCESS count)
                  Flavor line (Issue #22, only when acquired and flavor is present):
                  "  {flavor}" (indented 2 cells, COLOR_LABEL = DarkGray).
                  If the flavor exceeds inner width, it is truncated at display-cell
                  width and `…` is appended (see `truncate_display`).
        Locked:   "？？？" (COLOR_LABEL) — flavor hidden until acquired.
      Each entry consumes 1 or 2 lines. dictionary_scroll counts entries (not lines);
      the visible window is clipped at line granularity to fit `inner.height`.
  - Key bindings inside this section:
    - ↑/↓: move category focus (resets dictionary_scroll)
    - PgUp/PgDn: scroll noun list within focused category by 10
  - When feature #31 (Regex Filter) is added:
    - Filter input line at top of right pane
    - Filtered results below
```

### Achievements Tab

Left pane sections: `達成可能 / 進行中 / 達成済み`

Right pane layout:

```
Common block title: "{section} | {unlocked} / {total} 解除済み ({pct}%)"

Each achievement card (6 lines height, unfocused_block border):
  Line 1: [icon] ✅💰 / ✅ / 🔒  [emoji] Achievement Name (bold)
  Line 2:   Description text
  Line 3: LineGauge: [████░░░░] 75% (15/20) — Green when complete, Gray in progress
  Line 4: 報酬: N XP, 称号「...」 (Legendary/Epic), 解除日: YYYY-MM-DD (DarkGray)
  Lines 5-6: (padding/blank)

- 達成可能: unlocked && !claimed (ready to claim with Enter)
- 進行中: !unlocked
- 達成済み: unlocked && claimed
```

### Stats Tab

Left pane sections: `レアリティ / カテゴリ / 時系列`

Right pane layout:

```
レアリティ (section 0):
  - Paragraph PLAYER block: Level / Total / Rate / Avg per day
  - Sparkline: RECENT ACQUISITIONS (16 buckets, Cyan)
  - BarChart: RARITY BREAKDOWN — COM/RARE/EPIC/LEG bars, per-rarity color

カテゴリ (section 1):
  - BarChart: CATEGORY BREAKDOWN — 9 categories, Cyan
  - Paragraph CATEGORY DETAIL: per-category count / unique

時系列 (section 2):
  - Paragraph SESSION: 初回/最終プレイ, 総プレイ時間
  - LineGauge: LOGIN STREAK (Green, X/30 days)
  - LineGauge: TODAY VS BEST (Cyan, today/max)
  - Sparkline: DAILY (last 30 days, Cyan) — `Player::daily_acquisition_counts(30, Utc::now())` の戻り値を表示
```

### Synthesis Tab

Left pane sections: `レシピ一覧 / 合成実行`

Right pane layout:

```
レシピ一覧 (section 0):
  - Scrollable list of all recipes
  - Each item (5 lines, Issue #37 で +1 行):
    Line 1: ✓/? RecipeName (bold)
              - Public:   White
              - Partial:  DarkGray (薄め)
              - Unknown:  DarkGray + 行全体 "未確認レシピ #NN" 表示で recipe.name は出さない
              - 全材料揃いかつ未発見: Green (煽り強調)
    Line 2:     Description -> display_label (Public/Partial/discovered)
              - Public/discovered: 完全表示 "水 + 火 → 蒸気"
              - Partial: 第一材料のみ表示し、残材料と結果は ? でマスク "光 + ? → ?"
              - Unknown: 行ごと "(??? の手がかりはまだ無い)" に置換 (DarkGray)
    Line 3:     進捗: N/M ✓ もしくは 進捗: N/M (あと K 種) (Issue #37)
              - all_satisfied=true なら ✓ + Green
              - 揃ってなければ COLOR_LABEL (DarkGray)
              - Unknown は進捗を出さない (材料の正体がバレるため空行)
              - 残数 K は `SynthesisRecipe::remaining_categories(&collection)`
    Line 4:     合成確率: NN% [████████░░] (Issue #28)
              - undiscovered: Cyan ratio bar + cyan percentage (= discovery_rate)
              - discovered:   Green 100% + green full bar (確定成功)
              - 10-cell bar, percentage は `round()` で整数化
              - RISKY のときは Red バーで [RISKY] バッジ + 失敗時挙動を表示 (Issue #35)
    Line 5: (blank)
  - ✓ = Green (discovered), ? = DarkGray (undiscovered)

合成実行 (section 1):
  Top header (3 lines): "合成実行" focused_block — shows "Synthesis Lab | Discovered: N/M" in Cyan

  Phase A — SelectingFirst:
    Left half: "Select Ingredient 1" (focused_block) — scrollable curion list
    Right half: "Help" (unfocused_block) — instructions in DarkGray

  Phase B — SelectingSecond:
    Left half: "Selected" (unfocused_block) — first ingredient details in Green
    Right half: "Select Ingredient 2" (focused_block) — candidate list
      - 各候補行末尾に「合成確率 NN% [████████░░]」を 8-cell bar 付きで表示 (Issue #28)
      - 確率値は (Ingredient 1, 候補) ペアで最初にマッチするレシピを基準
      - 計算ロジックは `SynthesisManager::success_probability_for_recipe`

  Active selection highlight: bg(Cyan) fg(Black) Bold
  Empty state: fg(Red), "No curions / No possible combinations"
```

---

## 9. State Definitions

Every right-pane section must handle all four states. Never leave a pane blank.

### Empty State

When the underlying data collection is empty (e.g., no curions collected yet):

```
Paragraph with centered message:
  Style: fg(DarkGray)
  Text: "まだキュリオンがありません"
  Sub-text: hint for action, e.g., "スペースキーでキュリオンを生成"
```

For generic data-empty states (future screens):

```
  Text: "まだデータがありません"  or  "No data yet"
```

### Loading State

Not currently applicable (all data is local/synchronous). Reserved for future online features.

```
Paragraph: "Loading..." — fg(Cyan), blinking modifier if supported
```

### Locked State

For content the player has not yet unlocked (future feature: high-rarity synthesis, stage evolution):

```
Name:  "???"  — fg(DarkGray)
Stats: all bars at 0, fg(DarkGray)
Icon:  🔒
```

### Unknown / Undiscovered State

For synthesis recipes and collection図鑑 entries not yet obtained:

```
Recipe name:  visible (recipe.name)
Result:       "???"  — fg(DarkGray)
Marker:       "?"    — fg(DarkGray)
```

---

## 10. Placeholder Rules for Future Screens

These screens are defined in the roadmap but not yet implemented. When implementing, use these rules as the starting point.

### Daily Mission (Issue #20) — IMPLEMENTED

Dashboard tab section 2 で 3 本のデイリーミッションを表示する。
詳細レイアウト・テンプレートは「Tab Layout Principles / Dashboard Tab / デイリーミッション」参照。
日付ベース seed (SHA256(`curion-daily-mission/YYYY-MM-DD`)) で 4 テンプレから 3 つを抽選し、
報酬は達成時に自動付与される。

### Collection 図鑑 Filter (Issue #31) — IMPLEMENTED

```
Applied to both Owned List (section 0) and Encyclopedia (section 1) in the Collection tab.

Input mode:
  - Enter with `/`. The input line appears above the list with style fg(White) bg(DarkGray)
    and prefix " / ". A reversed-background space acts as the caret while typing.
  - Backspace deletes one char. Enter exits input mode but keeps the filter active.
  - Esc exits input mode AND clears the filter completely.

Filter targets (per curion / noun):
  - noun (e.g. `魚`)
  - display_name `{category} の {noun}` (e.g. `動物 の 魚`)
  - rarity label `COMMON` / `RARE` / `EPIC` / `LEGENDARY`
  - category name (e.g. `動物`)

Behaviour:
  - Owned list title becomes `コレクション [N matched / M total]` while filter is active.
  - Encyclopedia hides nouns that don't match; categories list stays as-is so the
    overall completion percentage remains comparable.
  - Locked nouns (`？？？`) still appear when their noun name / display_name / category
    matches; they cannot match by rarity since none has been acquired yet.
  - Invalid regex (e.g. `[`) shows `! invalid regex: …` in red on the prompt line
    without applying any filter and without crashing.
```

### Lifespan System (Issue #30) — IMPLEMENTED

```
レアリティ別に有限寿命を持たせ、放置されたキュリオンを「自然消滅」させる。
合成消費は寿命を見ない (使ってあげる = 供養)。

寿命日数:
  - Common: 3 日 / Rare: 7 日 / Epic: 14 日 / Legendary: 30 日

Curion field:
  - `lifespan_days: Option<u32>` (新規 = Some, 旧セーブ = None で永遠)

Pruning:
  - 起動時 (`process_login` 直後) に `prune_expired_curions(now)` を呼ぶ
  - 削除分は TUI トースト / --plain セクション / interactive REPL 出力で通知
  - 6 秒トースト (通常 3 秒より長め、見逃さないように)

Dashboard Overview 表示:
  - "⚠ 期限切れ間近 (残り 1 日以下): N 個" を 1 行で警告 (0 個なら空行)

Collection Owned List 表示:
  - 各行右端に残り寿命: `残 N 日`
  - 残 ≤ 0: 赤 + `寿命: ! まもなく消滅`
  - 残 ≤ 3: 黄色
  - それ以上: 薄いグレー
  - 寿命なし: `寿命: --`
```

### High-risk Synthesis (Issue #35) — IMPLEMENTED

```
高リスクレシピは既存の Synthesis タブにインライン表示する (専用 section は作らない)。

Data model (src/synthesis.rs):
  SynthesisRecipe に 2 フィールドを追加 (どちらも #[serde(default)] で後方互換)
    - success_rate: f64       // 実行時成功率。発見済みでも毎回 roll
                              //   省略時 1.0 (既存レシピは挙動不変)
    - failure_mode: FailureMode
        - NoLoss                       (保険: 失敗しても素材を失わない、既存互換 = default)
        - LoseAll                      (素材全消滅)
        - Salvage { fallback_rarity }  (素材を失い、代わりに低レアの残骸 curion 1 個獲得)

  HIGH_RISK_THRESHOLD = 0.95
  is_high_risk(): success_rate < 0.95
  success_probability(is_discovered):
    base = if is_discovered { 1.0 } else { discovery_rate }
    return base * success_rate    // discovery と success_rate の AND

Execution flow (try_synthesize / try_synthesize_with_rolls):
  1. find_matching_recipes → 最初の 1 件を採用
  2. !is_discovered なら discovery_roll で discovery_rate 判定
     失敗 → DiscoveryFailed (素材は消費しない、risk roll まで到達しない)
     成功 → discover_recipe で発見済みに昇格
  3. success_rate < 1.0 かつ risk_roll > success_rate → HighRiskFailure を返す
     - failure_mode に応じて lost_ingredients / salvage を構築:
         NoLoss   → lost_ingredients = [], salvage = None
         LoseAll  → lost_ingredients = ingredients 全件, salvage = None
         Salvage  → lost_ingredients = ingredients 全件,
                    salvage = Some(最初の材料の名詞+カテゴリを継承 + 「〜の残骸」, fallback_rarity)
  4. 上記をすべて通過 → Success

UI side (handle_synthesis_enter):
  HighRiskFailure を受け取ったら
    - lost_ingredients を id 一致で player.collection から削除
    - salvage があれば add_curion で追加 (収集系ミッション進捗にも乗る)
    - 失敗モード別トースト:
        LoseAll → "💥 失敗: <recipe_name> (素材消滅)"
        Salvage → "💔 失敗: <recipe_name> (残骸を獲得)"
        NoLoss  → "⚠ 失敗: <recipe_name> (保険発動)"
    - synthesis_state を SelectingFirst に戻す

Display (render_recipe_list / render_second_ingredient_candidates):
  - SAFE   バッジ: 緑 (COLOR_SUCCESS)
  - RISKY  バッジ: 赤 (COLOR_BAR_HOT) + "失敗時: 素材消滅 / 残骸獲得 / 保険" の付記
  - 合成確率バーも RISKY なら赤系で表示し、視覚的に "危険" を強調

Sample recipes in data/recipes/basic_recipes.json:
  recipe_016 「禁断の神」 混沌 + 秩序 → 神 (Legendary)
    discovery_rate 0.5 / success_rate 0.25 / failure_mode LoseAll
  recipe_017 「黒い太陽」 光 + 影 → 黒い太陽 (Epic)
    discovery_rate 0.6 / success_rate 0.50 / failure_mode Salvage(Common)
```

### Stage Evolution Gacha (Issue #36)

```
New section: "段階進化"

Layout:
  - Show current stage of selected curion (Stage 1 → Stage 2 → ...)
  - LineGauge: 進化ゲージ (how close to next stage)
  - "ガチャ" action button (Enter key)
  - Success: flash Magenta on right pane for one tick
```

### Regex Filter (Issue #31) — IMPLEMENTED for Collection tab

```
Applied in Collection tab Owned List + 図鑑. See "Collection 図鑑 Filter (Issue #31)" above
for the canonical spec. Synthesis レシピ一覧 への展開は今後の課題。

Input line style:
  - fg(White) bg(DarkGray)
  - Prefix: " / " (vim-style search prompt)
  - Caret = reversed-background space while in input mode
  - Inline error in red (fg Red, bold) for invalid regex

Esc behaviour: clears the filter and exits input mode at the same time.
```

---

## 11. Agent Prompt Guide

### ratatui Quick Reference

```rust
// Rarity color
fn rarity_color(rarity: &Rarity) -> Color {
    match rarity {
        Rarity::Common    => Color::Gray,
        Rarity::Rare      => Color::Cyan,
        Rarity::Epic      => Color::Yellow,
        Rarity::Legendary => Color::Red,
    }
}

// Rarity label (4-char width, use with format!("[{label:<4}]"))
fn rarity_label(rarity: &Rarity) -> &'static str {
    match rarity {
        Rarity::Common    => "COM",
        Rarity::Rare      => "RARE",
        Rarity::Epic      => "EPIC",
        Rarity::Legendary => "LEG",
    }
}

// XP gauge
Gauge::default()
    .block(Block::default().title("XP"))
    .gauge_style(Style::default().fg(Color::Cyan))
    .ratio(xp as f64 / xp_max as f64)
    .label(format!("{} / {}", xp, xp_max));

// Achievement line gauge
LineGauge::default()
    .gauge_style(Style::default().fg(Color::Green))
    .ratio(current as f64 / target as f64);

// Focused border
Block::default()
    .borders(Borders::ALL)
    .border_type(BorderType::Rounded)
    .border_style(Style::default().fg(Color::Cyan));
```

### Color Emotion Reference

- **Cyan:** Active, alive, RARE — the default "interesting" signal
- **Green:** Done, achieved, safe
- **Yellow:** Close, exciting, EPIC — "almost there"
- **Red:** Urgent, legendary, "drop everything"
- **Magenta:** Unique, synthesis result, once-in-a-session moment
- **Gray:** Common, background noise, already seen
- **DarkGray:** Inactive, locked, not yet
