# Curion - Overview

## What is Curion?

Curion is an SF-themed collection game that runs in a terminal (TUI). The name comes from "particles of curiosity" -- fictional particles that represent everything in the world as collectible items.

Players collect curions generated deterministically from GUIDs, in a mechanism inspired by Barcode Battler: a GUID is issued periodically, hashed, and the hash determines the curion's category, name, rarity, and attributes. Every noun in the world -- animals, plants, colors, elements, abstract concepts -- is a potential curion.

## Core Mechanics

- **GUID-based deterministic generation**: UUID v4 + SHA-256 hashing produces curions. The same GUID always yields the same curion.
- **Barcode Battler-style mapping**: Hash bytes are sliced into fields that determine category (9 types), specific noun, rarity (Common/Rare/Epic/Legendary), and numeric attributes (interest, rarity score, beauty).
- **Collection**: Duplicates are allowed. Filter and sort by category, rarity, or acquisition date.
- **Synthesis**: Combine two curions to create new ones via a recipe database (e.g., Water + Fire = Steam). 15 base recipes, with synthesis-exclusive curions like Flame Dragon and Ice Phoenix.
- **Achievements**: 40+ achievements across collection milestones, streaks, rarity goals, and special combos. An XP/level system rewards progress.
- **Nostr-based P2P trade** (planned): Decentralized curion exchange via Nostr relays, with multi-profile support already in place.

## Design Philosophy

### Cookie Clicker-style addictiveness

The UI is deliberately designed to create a "just one more" loop. The bottom half of the Dashboard shows progress bars for nearby goals -- "2 more for Color Collector!", "16 more for 250 milestone!" -- with urgency color-coding (red at 95%+, yellow at 80%+). Multiple goals are always visible so the player never runs out of motivation.

### TUI metaverse

Curion aims to be a metaverse you can experience without VR -- just a terminal. The world is text-based but rich: colored rarity indicators, animated generation effects, and a multi-tab interface (Dashboard, Collection, Achievements, Stats) built with ratatui.

### Nostr-based decentralization

Player identity uses Nostr keypairs. Trading will happen over Nostr relays, making the system fully peer-to-peer with no central server. Regional/location-based curions are planned for a later phase.

## Tech Stack

| Component | Technology |
|---|---|
| Language | Rust (edition 2021) |
| TUI framework | ratatui 0.29 + crossterm 0.28 |
| P2P protocol | nostr-sdk 0.37 |
| CLI | clap 4.5 |
| Async runtime | tokio 1.42 |
| Serialization | serde + serde_json |
| GUID | uuid 1.11 |
| Hashing | sha2 0.10 |
| Time | chrono 0.4 |
| Random | rand 0.8 |
