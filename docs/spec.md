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
| Rarity | Common (70%), Rare (20%), Epic (8%), Legendary (2%) |
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
| Daily login | 50 |

Level N -> N+1 requires N * 100 XP.

## TUI Layout

4 tabs, switched via Tab key or number keys 1-4.

### Tab 1: Dashboard

Split into two halves:

**Upper half -- Current status:**
- GUID generation countdown (progress bar)
- Quick stats: total collected, today's count, play time
- Latest curion acquired
- Rarity distribution (horizontal bar chart)
- Category distribution (compact bar chart)

**Lower half -- Goal nudges ("almost there!"):**
- Lists goals sorted by proximity to completion
- Priority display by completion percentage:
  - 95%+: red "Urgent!" badge
  - 80-94%: yellow star badge
  - 50-79%: normal display
  - 30-49%: gray
  - Below 30%: hidden

### Tab 2: Collection

- Scrollable list of all owned curions
- Filter by rarity and category
- Sort by: newest, rarity, category
- Detail view on Enter
- Display format: `#ID stars [Rarity] Category Name  Date`
- Attribute bars (interest, beauty) shown inline

### Tab 3: Achievements

- List of all achievements with unlock status
- Unlocked: checkmark, unlock date, XP reward
- Locked: progress bar, required count, reward preview
- Claimable rewards highlighted

### Tab 4: Stats

- Basic info: level, total play time, first/last play, login streak
- Collection stats: total, daily, peak day, average rate, rate per hour
- Rarity breakdown with most frequent item per tier
- Category completion percentages with bar charts

## Key Bindings

### Global
| Key | Action |
|---|---|
| Tab / 1-4 | Switch tabs |
| q / Esc | Quit |
| ? | Help |

### Dashboard
| Key | Action |
|---|---|
| Space | Manual GUID generation (with cooldown) |
| r | Refresh display |

### Collection
| Key | Action |
|---|---|
| Up/Down or j/k | Scroll |
| Enter | Detail view |
| f | Filter settings |
| s | Sort settings |

### Achievements
| Key | Action |
|---|---|
| Up/Down or j/k | Scroll |
| Enter | Claim reward (unlocked only) |

## Visual Design

### Rarity colors
| Rarity | Color |
|---|---|
| Common | White/Gray |
| Rare | Blue/Cyan |
| Epic | Magenta/Purple |
| Legendary | Yellow/Gold |

### Progress bar color ramp
| Range | Color |
|---|---|
| 0-29% | Gray |
| 30-49% | White |
| 50-79% | Blue |
| 80-94% | Yellow |
| 95-99% | Orange |
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
