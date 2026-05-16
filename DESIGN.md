# DESIGN.md

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
    - Paragraph: 総獲得数 / 今日の獲得 / レベル (one line, inline)
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
  - Gauge: 今日の収集進捗 (Cyan, turns Yellow when complete)
  - Target: 10 curions/day
  - Label format: [████░░░░]  N / 10 collected today
```

### Collection Tab

Left pane sections: `所持一覧 / 図鑑`

Right pane layout:

```
所持一覧 (section 0):
  - Scrollable list of collected curions (newest first)
  - Each item (3 lines):
    Line 1: #N  ★★  [RARE]  名前                 2025-01-01 12:00
    Line 2:       興味度: [██████░░░░]  60%  美しさ: [████░░░░░░]  40%
    Line 3: (blank separator)
  - Color: rarity color on stars and rarity label
  - Timestamp: DarkGray

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
        Locked:   "？？？" (COLOR_LABEL)
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
  - Sparkline: COLLECTION RATE (16 buckets, Cyan)
```

### Synthesis Tab

Left pane sections: `レシピ一覧 / 合成実行`

Right pane layout:

```
レシピ一覧 (section 0):
  - Scrollable list of all recipes
  - Each item (3 lines):
    Line 1: ✓/? RecipeName (bold White)
    Line 2:     Description -> ResultName (discovered) or ??? (undiscovered)
    Line 3: (blank)
  - ✓ = Green (discovered), ? = DarkGray (undiscovered)

合成実行 (section 1):
  Top header (3 lines): "合成実行" focused_block — shows "Synthesis Lab | Discovered: N/M" in Cyan

  Phase A — SelectingFirst:
    Left half: "Select Ingredient 1" (focused_block) — scrollable curion list
    Right half: "Help" (unfocused_block) — instructions in DarkGray

  Phase B — SelectingSecond:
    Left half: "Selected" (unfocused_block) — first ingredient details in Green
    Right half: "Select Ingredient 2" (focused_block) — candidate list

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

### Daily Mission (Issue #20)

```
Section in Dashboard tab, section index 2.
Current placeholder: single Gauge (10 curions/day target).

Full implementation target:
  - List of 3 daily missions (e.g., collect N curions, collect 1 RARE, perform synthesis)
  - Each mission: LineGauge per mission + reward XP display
  - "Complete" badge in Green when all done
  - Reset countdown: when next reset occurs (midnight)
```

### Collection 図鑑 Filter (Issue #31)

```
Section in Collection tab, section index 1.
Current state: category summary only.

Full implementation target:
  - Top: filter input line (fg White, bg DarkGray input box)
  - Below: filtered curion list or category breakdown
  - Unknown entries: fg(DarkGray), name = "???"
  - Match highlight: bold
```

### High-risk Synthesis (Issue #35)

```
New section in Synthesis tab: "高リスク合成"

Layout:
  - Show success probability as Gauge (Red when <30%, Yellow <60%, Green ≥60%)
  - Failure consequence: "失敗時: 素材ロスト" in Red
  - Confirm dialog before execution: "本当に合成しますか？ (y/n)"
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

### Regex Filter (Issue #31)

```
Applied in Collection tab 図鑑 and potentially Synthesis レシピ一覧.

Input line style:
  - fg(White) bg(DarkGray)
  - Prefix: " / " (vim-style search prompt)
  - Clear with Esc

Result highlight: Bold on matched text portion.
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
