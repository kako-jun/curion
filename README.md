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

- **GUID-based deterministic generation** — each GUID maps to a specific curion via SHA-256 hashing; same GUID always yields the same result
- **268 collectible nouns** across 9 categories (Animal, Plant, Color, Object, Concept, Element, Food, Phenomenon, Abstract)
- **4-tier rarity system** — Common (60%), Rare (30%), Epic (9%), Legendary (1%)
- **Synthesis** — combine two curions to create something new; 15 recipes (water + fire = steam, dream + light = hope, ...)
- **39 achievements** — milestone counts, rarity hunts, category completions, login streaks, playtime goals, and special challenges
- **5-tab TUI** built with ratatui — Dashboard, Collection, Achievements, Stats, Synthesis
- **Nostr identity** — each player gets a keypair for future decentralized trading
- **Auto-save** every 60 seconds, manual save with `s`
- Single binary, no runtime dependencies

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
┌─────────────────────────────────────────────────────────────────┐
│  Dashboard  │  Collection  │  Achievements  │  Stats  │  Synthesis  │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Next GUID in: 18s                                              │
│  [████████████████░░░░░░░░░░░░░░░░░░░░░░]  40%                 │
│                                                                 │
│  Latest:                                                        │
│    ⭐ Dragon    [Legendary]  Animal    curiosity: 97            │
│    💙 Cobalt    [Rare]       Color     curiosity: 64            │
│    ⚪ Bread     [Common]     Food      curiosity: 23            │
│    💜 Quartz    [Epic]       Element   curiosity: 81            │
│                                                                 │
│  Total: 142  |  Common: 85  Rare: 41  Epic: 14  Legendary: 2   │
│                                                                 │
│  Achievements: 12/39 unlocked                                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Key Bindings

| Key | Action |
|---|---|
| `1`-`5` | Switch to tab (Dashboard / Collection / Achievements / Stats / Synthesis) |
| `Tab` | Next tab |
| `Space` | Generate a curion now (spends the current GUID) |
| `j` / `k` or Arrow keys | Scroll up / down |
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

15 recipes total, spanning Intuitive, Conceptual, Biological, Cooking, Abstract, and Chaos
Mix types. Discover them by experimenting.

## Documentation

- [Game Spec](GAME_SPEC.md) — full game design document

## Roadmap

- P2P curion trading via Nostr relay
- More noun categories and synthesis recipes
- Cross-platform binary releases

## License

MIT
