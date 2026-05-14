# DESIGN.md

curion — Design System

## 1. Visual Theme & Atmosphere

SF collection game running in a terminal. The visual language borrows from Linux system monitors (glances, btop, htop): dense information panels, color-coded status, live progress bars, and sparklines — all inside a ratatui TUI.

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
| Special | `Magenta` | UNIQUE rarity, synthesis success |

Rarity color mapping:

| Rarity | Color |
|---|---|
| COM | `Gray` |
| RARE | `Cyan` |
| EPIC | `Yellow` |
| LEGENDARY | `Red` |
| UNIQUE | `Magenta` |

## 3. Typography Rules

Terminal monospace only. No font selection — inherits the user's terminal font.

- All labels: UPPERCASE for section headers, Title Case for item names
- Numbers: right-aligned in columns
- Percentages: always shown with `%` suffix
- Rarity labels: always in brackets `[RARE]`, colored by rarity

## 4. Component Usage

### Progress Bars (ratatui `Gauge` / `LineGauge`)

Used for any quantity with a known maximum.

| Context | Widget | Color rule |
|---|---|---|
| XP bar | `Gauge` | `Cyan` → `Yellow` at 75% → `Red` at 95% |
| Achievement progress | `LineGauge` | `Gray` → `Green` when complete |
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

## 8. Agent Prompt Guide

### ratatui Quick Reference

```rust
// Rarity color
fn rarity_color(rarity: &Rarity) -> Color {
    match rarity {
        Rarity::Com       => Color::Gray,
        Rarity::Rare      => Color::Cyan,
        Rarity::Epic      => Color::Yellow,
        Rarity::Legendary => Color::Red,
        Rarity::Unique    => Color::Magenta,
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
