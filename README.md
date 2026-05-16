# curion

[日本語](README.ja.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A sci-fi collection game in your terminal — gather "particles of curiosity" generated from GUIDs.

## Why curion?

Remember Barcode Battler? You'd scan a random barcode and get a character with stats you
couldn't predict. Curion brings that same thrill to your terminal, except instead of barcodes
you have GUIDs, and instead of warriors you collect *curions* — fictional particles that
represent every noun imaginable.

A cat, a supernova, the color indigo, the concept of justice — they're all curions waiting to
appear. Every 30 seconds a new GUID is issued, the hash is crunched, and something lands in
your collection. You don't control what you get. You just keep watching, combining, and
chasing that next Legendary drop.

It's Cookie Clicker logic applied to a gacha cabinet that runs in a 80x24 terminal.
No ads, no microtransactions, no internet required. Just one more spin.

## Features

### Core

- **GUID-based deterministic generation** — each GUID maps to a specific curion via SHA-256 hashing; same GUID always yields the same result
- **268 collectible nouns** across 9 categories (Animal, Plant, Color, Object, Concept, Element, Food, Phenomenon, Abstract), each with SF flavor text (#22)
- **4-tier rarity system** — Common, Rare, Epic, Legendary
- **5-tab TUI** built with ratatui — Dashboard, Collection, Achievements, Stats, Synthesis
- **Three-pane navigation** — tab bar + left section list + right content pane, aligned with the future PWA information architecture
- **Nostr identity** — each player gets a keypair for future decentralized trading
- **Auto-save** every 60 seconds, manual save with `s`
- Single binary, no runtime dependencies

### Dashboard

- **Daily login bonus** — first launch of the day auto-claims escalating streak rewards, including guaranteed tickets at day 3 / 5 / 7
- **Daily missions (#20)** — three date-seeded missions per day (4 templates), XP auto-claimed on completion, daily reset at local midnight
- **Rare cooldown LineGauge (#25)** — 4-hour bar that boosts Rare+ probability up to +0.3 when full
- **Combo counter (#21)** — consecutive Rare+ pulls escalate the XP multiplier (1.5x → 2.0x → 3.0x), with a "コンボマスター" title at combo 5
- **Live rarity probability (#28)** — current per-rarity drop rates shown alongside the cooldown bar
- **Next milestone hint (#32)** — closest unfinished XP / achievement / streak goal surfaced as one line
- **SAN gauge (#29)** — sanity stat that gains from rare pulls and decays over time, with state colors and an `⚠ 異常状態` flag below 30
- **Lifespan warning (#30)** — count of curions expiring within 24 hours
- **Evolution progress (#36)** — top 3 evolution lines sorted by "almost done" urgency
- **Equipped curion summary (#38)** — one-line readout of the currently equipped curion and its derived effects

### Collection

- **Owned list with flavor text and acquisition history (#22 / #27)** — bottom detail pane shows flavor + `通算 N回目の収集`
- **Encyclopedia (図鑑, #18)** — every noun in every category, locked entries shown as `？？？` with per-category progress
- **Regex filter (#31)** — `/` opens a live regex search over name / display name / rarity / category
- **Per-curion lifespan display (#30)** — red / yellow / gray `残 N 日` on each owned row
- **Equip a curion (#38)** — press `e` to equip the focused curion; Phase 1 effects (XP multiplier / SAN decay modifier) are applied immediately

### Stats

- **Rarity / Category BarCharts** — DESIGN-compliant per-rarity palette
- **Daily Sparkline** — last 30 days of acquisitions (Issue #26)
- **Recent acquisitions Sparkline** — 16-bucket short window for at-a-glance pace

### Synthesis

- **17 recipes** — 15 base recipes plus 2 high-risk additions (recipe_016 禁断の神 / recipe_017 黒い太陽). See `data/recipes/basic_recipes.json`.
- **SAFE / RISKY badges and a success-probability gauge (#28 / #35)** — every recipe row shows its current odds with a 10-cell bar
- **Public / Partial / Unknown recipe visibility (#37)** — high-rarity recipes are masked progressively until discovered
- **High-risk failure modes (#35)** — `NoLoss` / `LoseAll` / `Salvage` per recipe, with appropriate toast feedback

### Achievements

- **Non-linear thresholds (#32)** — collection / streak / playtime milestones use intentionally awkward numbers (e.g. 27 / 47 / 103) to keep "あと少し" alive
- **Available / In progress / Unlocked tabs** — claimable rewards are surfaced for one-keystroke `Enter` collection

## Quick Start

```bash
cargo install curion
curion
```

Or build from source:

```bash
git clone https://github.com/kako-jun/curion.git
cd curion
cargo run
```

## How It Works

```
GUID issued (every 30s)
    |
    v
SHA-256 hash
    |
    +--> Category (9 types)
    +--> Noun (268 words)
    +--> Rarity (Common / Rare / Epic / Legendary)
    +--> Attributes (curiosity, rarity score, beauty)
    |
    v
Curion added to your collection
    |
    +--> Achievements checked
    +--> Synthesis unlocked when you have matching ingredients
```

You can also press `Space` to spend a GUID immediately instead of waiting for the timer.

## Gameplay

```
┌─ Dashboard / Collection / Achievements / Stats / Synthesis ─────┐
├─ Overview / Login / Daily ─┬─ Right pane content ───────────────┤
│ > Overview                 │ Next GUID in: 18s                  │
│   Login Bonus              │ [████████████████░░░░░░] 40%       │
│   Daily Mission            │ Latest: [Legendary] Dragon         │
│                            │ Total: 142 / Rare: 41 / Epic: 14   │
├────────────────────────────┴─────────────────────────────────────┤
│ help_line                                                    │
└──────────────────────────────────────────────────────────────┘
```

### Key Bindings

| Key | Action |
|---|---|
| `1`-`5` | Switch to tab (Dashboard / Collection / Achievements / Stats / Synthesis) |
| `Tab` | Next tab |
| `Space` | Generate a curion now (spends the current GUID) |
| `j` / `k` | Move between left-pane sections |
| `Up` / `Down` | Scroll or select inside the right pane |
| `Enter` | Claim achievement reward / Select synthesis ingredient |
| `Esc` | Cancel synthesis selection / Quit |
| `s` | Manual save |
| `q` | Quit |

## Synthesis

Select two curions from your collection and combine them. If they match a recipe, a new
curion is created and the ingredients are consumed.

| Recipe | Ingredients | Result |
|---|---|---|
| Steam | Water + Fire | Steam |
| Mud | Earth + Water | Mud |
| Lava | Fire + Earth | Lava |
| ... | ... | ... |

17 recipes total (15 base + 2 high-risk: `recipe_016` 禁断の神 / `recipe_017` 黒い太陽),
spanning Intuitive, Conceptual, Biological, Cooking, Abstract, Chaos Mix, and Advanced
Legendary tiers. See `data/recipes/basic_recipes.json` for the full list. Discover them
by experimenting.

## Documentation

- [Game Spec](docs/spec.md) — full game design document
- [Design System](docs/design.md) — color palette, widget rules, per-tab layout

## Roadmap

- **v0.3.0** — P2P curion trading (#5) and `mypace` WebSocket integration (#4)
- **v0.4.0** — Time / region-limited curions (#34)
- **Long-term** — PWA / WASM split (preparation done in #23)

## License

MIT
