# Curion - Technical Specification

## GUID Generation System

- A new UUID v4 is issued at regular intervals (configurable cooldown).
- Players can also trigger generation manually (Space key, with cooldown).
- The UUID is hashed with SHA-256 to produce a deterministic 32-byte digest.
- Digest bytes are sliced to determine curion attributes.

## Curion Attributes

Each curion has:

| Field | Derivation |
|---|---|
| Category | Hash bytes mapped to one of 9 categories |
| Name | Index into the category's noun list |
| Rarity | Common/COM (70%), Rare/RARE (20%), Epic/EPIC (8%), Legendary/LEG (2%) |
| Interest | Numeric 0-100, from hash bytes |
| Rarity Score | Numeric 0-100, from hash bytes |
| Beauty | Numeric 0-100, from hash bytes |

### Categories (9)

| Category | Noun count | Examples |
|---|---|---|
| Animals | 67 | Dragon, Cat, Fish |
| Plants | ~30 | Sakura, Rose |
| Colors | ~25 | Red, Gold |
| Objects | ~30 | Book, Sword |
| Concepts | ~20 | Dream, Time |
| Elements | ~20 | Fire, Water, Gold |
| Foods | ~20 | Rice, Apple |
| Phenomena | ~20 | Thunder, Rainbow |
| Abstracts | 52 | Love, Freedom, Chaos |

Noun data is stored in `data/nouns/*.json` (one file per category).

## Synthesis System

Combine two owned curions to create a new one.

- Recipes are defined in `data/recipes/basic_recipes.json` (15 base recipes).
- Recipe format: `material_a + material_b -> result` (matched by category + name).
- Synthesis-exclusive curions exist (e.g., Flame Dragon, Ice Phoenix) -- obtainable only through synthesis.
- Smart synthesis UI suggests valid combinations from the player's inventory.
- Discovered recipes are tracked and shown in the UI.

## Achievement System

40+ achievements across 4 categories:

### Collection achievements
- Total count milestones: 10, 50, 100, 250, 500, 1000
- Rarity milestones: 10, 50, 100 per rarity tier
- Category completion: collect all nouns in a category

### Streak achievements
- Consecutive login: 3, 7, 14, 30, 100 days
- Play time: 1h, 10h, 50h, 100h, 500h

### Special achievements
- "Gold Rush": 10 Gold curions
- "Dragon Incarnation": 5 Dragon curions
- "Perfectionist": complete all categories
- "Legendary Collector": 100 Legendary curions

### Combo achievements
- "Lucky Streak": 3 consecutive Rare+ pulls
- "Golden Hour": 10 curions in 1 hour

### XP and Levels

| Source | XP |
|---|---|
| Common curion | 10 |
| Rare curion | 25 |
| Epic curion | 50 |
| Legendary curion | 200 |
| Achievement unlock | Per-achievement reward |
| Daily login | 50-1500+ (streak escalates) |

Level N -> N+1 requires N * 100 XP.

## TUI Layout

5 tabs, switched via Tab key or number keys 1-5.

Each tab now uses a 3-layer layout:

- Top: global tab bar
- Left pane: section list for the current tab
- Right pane: content for the selected section
- Bottom: one-line help bar

### Tab 1: Dashboard

Left pane sections:
- Overview
- Login Bonus
- Daily Mission

**Overview right pane:**
- GUID generation countdown (progress bar)
- XP bar with rarity-aligned warning colors
- Quick stats: total collected, today's count, level
- Latest curion acquired
- Rarity distribution (horizontal text bars)
- Category distribution (compact text summary)

**Overview lower area -- Goal nudges ("almost there!"):**
- Lists goals sorted by proximity to completion
- Priority display by completion percentage:
  - 95%+: red "Urgent!" badge
  - 80-94%: yellow star badge
  - 50-79%: normal display
  - 30-49%: gray
  - Below 30%: hidden

**Login Bonus right pane:**
- Shows current consecutive login days
- Auto-claims once per day on the first launch of the day
- Displays today's reward, next reward, and guaranteed-ticket inventory
- Daily reset is based on the local system date, not UTC
- Reward ladder:
  - Day 1: 50 XP
  - Day 2: 100 XP
  - Day 3: 200 XP + Common guaranteed ticket
  - Day 5: 500 XP + Rare guaranteed ticket
  - Day 7: 1500 XP + Epic guaranteed ticket + title
  - Day 8+: XP keeps escalating; ticket rewards continue on 3/5/7 day cadence
- Guaranteed tickets are inventory-only for now; spending flow belongs to a future issue

**Daily Mission right pane:**
- Placeholder gauge for today's collection count; dedicated mission logic remains future work

### Tab 2: Collection

Left pane sections:
- Owned List
- Encyclopedia

**Owned List right pane:**
- Scrollable list of all owned curions
- Display format: `#ID stars [Rarity] Category Name  Date`
- Attribute bars (interest, beauty) shown inline

**Encyclopedia right pane:**
- Two-column layout: category list (left) + noun entries for the focused category (right)
- Each category row shows `name owned/total` (unique count vs. database total)
- Header above noun entries shows overall progress `総数: owned/total (NN.N%) | カテゴリ: owned/total (NN.N%)`
- Acquired noun row: `name stars [Rarity] YYYY-MM-DD ×count` (uses highest acquired rarity and latest acquisition date)
- Unacquired noun row: displays `？？？` only (encourages completion)
- Noun ordering follows the embedded JSON data order; no rearrangement based on ownership
- Key bindings inside this section:
  - `↑/↓` move focus between categories (resets noun scroll to 0)
  - `PgUp/PgDn` scroll the noun list within the focused category

### Tab 3: Achievements

Left pane sections:
- Claimable
- In Progress
- Completed

Right pane behavior:
- Claimable: rewards can be claimed with Enter
- In Progress: locked achievements with `LineGauge` overlays
- Completed: unlocked achievements with unlock date and reward history

### Tab 4: Stats

Left pane sections:
- Rarity
- Category
- Timeline

Right pane behavior:
- Rarity: player summary + recent-acquisition `Sparkline` + rarity `BarChart`
- Category: category `BarChart` plus total / unique breakdown
- Timeline: first play, last play, total play time, streak gauges, and collection-rate `Sparkline`

### Tab 5: Synthesis

Left pane sections:
- Recipe List
- Synthesize

Right pane behavior:
- Recipe List: discovered/undiscovered recipe index
- Synthesize: two-step ingredient selection flow

## Key Bindings

### Global
| Key | Action |
|---|---|
| Tab / 1-5 | Switch tabs |
| q / Esc | Quit |
| ? | Help |
| j / k | Move selection in the left pane |
| Up / Down | Scroll or select inside the right pane |

### Dashboard
| Key | Action |
|---|---|
| Space | Manual GUID generation (with cooldown) |
| r | Refresh display |

### Collection / Achievements / Stats / Synthesis
| Key | Action |
|---|---|
| Up/Down | Scroll or select content in the right pane (Collection > Encyclopedia: move category focus) |
| PgUp/PgDn | Collection > Encyclopedia: scroll noun list within focused category |
| Enter | Claim reward / start synthesis step |

## Visual Design

### Rarity colors
| Rarity | Color |
|---|---|
| Common | Gray |
| Rare | Cyan |
| Epic | Yellow |
| Legendary | Red |

### Progress bar color ramp
| Range | Color |
|---|---|
| 0-29% | Gray |
| 30-49% | Gray |
| 50-79% | Cyan |
| 80-94% | Yellow |
| 95-99% | Red |
| 100%+ | Green (claimable) |

### Animations
- Curion generation: fade-in effect
- Level up: screen flash
- Achievement unlock: celebration effect
- Legendary pull: rainbow effect

## Data Storage

- Save directory: `~/.curion/`
- Auto-save every 60 seconds
- Format: JSON (serde_json serialization)
- Multi-profile support via `--profile` CLI argument
- Duplicate connection prevention per profile
