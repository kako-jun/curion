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

## Latent Vector Pipeline (Issue #39)

Curion 生成は「文字列から直接 noun を引く」のではなく、潜在ベクトル経由の
対称パイプラインで行う。

```
seed bytes  →  SHA-256 × 2 round  →  16-dim f32 latent vector  →  curion
                                            │
                                            ├─→  category  (dim 0)
                                            ├─→  rarity    (dims 1..4)
                                            ├─→  noun      (nearest noun prototype, 全 16-dim cosine × weight)
                                            ├─→  interest  (dims 8..12)
                                            └─→  beauty    (dims 12..16)
```

- 各 noun には `prototype_for_noun(name)` で deterministic に作られる prototype
  vector があり、latent との cosine similarity が最大 (× noun.weight) の noun が
  「最も近いラベル」として採用される。
- `latent_from_seed(seed)` と `prototype_for_noun(name)` は異なるドメインタグ
  (`curion:latent:seed:v1` / `curion:latent:noun_prototype:v1`) で hash するので、
  seed と noun 名が偶然一致しても衝突しない。
- `CurionGenerator::generate_from_guid(guid)` は `guid.as_bytes()` を seed として
  この pipeline に委譲する後方互換 API。`generate_with_bonus(guid, bonus)` は
  Issue #25 の roll-shift モデル (最大 -0.3) を latent パイプライン上で再現する。
- 将来 Issue #38 の装備効果・消費効果も同じ latent vector の別投影として
  導出する想定 (curion 本体 = latent、noun名や効果はそのラベル)。

実装: `src/latent.rs` (純粋関数群)、`src/generator.rs::generate_from_seed_bytes_with_bonus`。

## Synthesis System

Combine two owned curions to create a new one.

- Recipes are defined in `data/recipes/basic_recipes.json` (17 recipes: 15 base + 2 high-risk).
- Recipe format: `material_a + material_b -> result` (matched by category + name).
- Synthesis-exclusive curions exist (e.g., Flame Dragon, Ice Phoenix) -- obtainable only through synthesis.
- Smart synthesis UI suggests valid combinations from the player's inventory.
- Discovered recipes are tracked and shown in the UI.

### High-risk Synthesis (Issue #35)

Each recipe has two independent probability dials:

- `discovery_rate` (existing) — first-time-only roll. Failing this returns
  `DiscoveryFailed` without consuming ingredients. Once passed, the recipe is
  marked discovered and the roll is skipped on subsequent attempts.
- `success_rate` (new, default `1.0`) — execution-time roll applied **every**
  attempt, including discovered recipes. `success_rate < 0.95` flags the recipe
  as high-risk (`is_high_risk()`).

Failure behaviour is selected per recipe via `failure_mode`:

| Mode | Effect on inventory | Output |
|---|---|---|
| `NoLoss` (default) | nothing lost | nothing gained |
| `LoseAll` | both ingredients deleted | nothing |
| `Salvage { fallback_rarity }` | both ingredients deleted | 1 salvage curion at `fallback_rarity`, named "`<first>の残骸`", inheriting the first ingredient's category |

The displayed success probability is `discovery_factor * success_rate`, where
`discovery_factor` is `discovery_rate` for undiscovered recipes and `1.0` for
discovered ones. UI shows `[SAFE]` / `[RISKY]` badges plus the failure-mode
label on both the recipe list and the ingredient-2 candidate list.

`SynthesisAttemptResult::HighRiskFailure { recipe_name, lost_ingredients,
salvage, failure_mode }` is returned on a failed risk roll. The UI layer is
responsible for removing `lost_ingredients` (matched by `id`) and adding any
`salvage` curion. The internal `try_synthesize_with_rolls(ingredients,
discovery_roll, risk_roll)` API allows deterministic testing of the two-stage
roll without random sources.

Recipe JSON is backward compatible: existing recipes without `success_rate` /
`failure_mode` deserialize to `1.0` / `NoLoss`, so all 15 legacy recipes remain
100% safe.

### Partial Recipe Visibility (Issue #37)

Each recipe carries a `visibility: RecipeVisibility` (default `Public`,
`#[serde(default)]` so legacy JSON is unchanged):

- `Public`  — ingredients and result are fully displayed from the start.
- `Partial` — only the first ingredient name is shown; the rest of the
  ingredients and the result are masked with `?`. Recipe `name` is still shown.
  Example list line: `光 + ? -> ?` (with description hidden behind the same mask).
- `Unknown` — recipe shown only as `未確認レシピ #NN` where `NN` is the 1-origin
  recipe index, zero-padded to 2 digits. Description, ingredients, result, and
  the recipe `name` are all hidden.

A recipe is always rendered as Public the moment it becomes discovered,
regardless of its `visibility`. Discovery transitions are unchanged: passing
`discovery_rate` flips `is_discovered = true` and the row reveals full text on
the next render.

Progress indicator (per recipe row, except Unknown rows):

- `進捗: N/M` where `N = satisfied IngredientRequirement count`, `M = total`.
- `✓` appended in green when `all_satisfied == true`.
- Otherwise `(あと K 種)` is appended where `K = total - satisfied`, calculated
  via `SynthesisRecipe::remaining_categories(&collection)`.

Logic-layer APIs (UI-independent, in `synthesis.rs`):

- `SynthesisRecipe::ingredient_progress(&[Curion]) -> IngredientProgress`
- `SynthesisRecipe::remaining_categories(&[Curion]) -> usize`
- `SynthesisRecipe::display_label(&[Curion], is_discovered, index) -> String`

Color treatment in the recipe list:

- Public / discovered: white recipe name (default).
- Partial: recipe name in `COLOR_LABEL` (dark gray) to signal "obfuscated".
- Unknown: row in `Color::DarkGray` throughout.
- When `all_satisfied == true` and `!discovered`, the name is bumped to
  `COLOR_SUCCESS` to tease the player ("you have everything; try synthesizing").

Current bundled high-visibility recipes (`data/recipes/basic_recipes.json`):

| Recipe | Visibility |
|---|---|
| `recipe_014` (陰陽 / Advanced Legendary) | `partial` |
| `recipe_016` (禁断の神 / Advanced Legendary, LoseAll) | `unknown` |
| `recipe_017` (黒い太陽 / Conceptual Epic, Salvage) | `partial` |

## Lifespan System (Issue #30)

Each curion has a finite lifespan tied to its rarity. Curions left in the
collection past their lifespan are auto-removed at the next launch (treated
as natural decay). Synthesizing a curion does **not** count as natural decay
— "using it up" is its proper send-off.

| Rarity | Lifespan |
|---|---|
| Common | 3 days |
| Rare | 7 days |
| Epic | 14 days |
| Legendary | 30 days |

- `Curion::lifespan_days: Option<u32>` is set at acquisition time via
  `lifespan_for_rarity(rarity)` in `src/curion.rs`. New curions always carry
  `Some(...)`; legacy saves without the field deserialize to `None` (immortal)
  for backward compatibility.
- `Curion::expires_at()` = `acquired_at + lifespan_days`. Returns `None`
  for immortal curions.
- `Curion::is_expired(now)` = `now > expires_at()`. Always `false` for
  immortal curions.
- `Curion::days_remaining(now)` returns `(expires_at - now).num_days()`.
  Negative values mean already-expired.
- `Player::prune_expired(now) -> Vec<Curion>` removes expired curions from
  the collection and returns the removed list. Stats (`rarity_stats`,
  `category_stats`) are intentionally left untouched — they are cumulative
  acquisition histories, not current-inventory views.
- `GameState::prune_expired_curions(now)` delegates to the player and is
  invoked right after `process_login()` in `main.rs`, `plain.rs`, and
  `interactive.rs`. The returned list drives:
  - TUI: `App::show_expired_curions_message` toast (6-second display)
  - `--plain` mode: a `=== 寿命で消えたキュリオン (N 個) ===` section
  - Interactive (REPL) mode: a yellow-titled list
- Dashboard Overview shows `⚠ 期限切れ間近 (残り 1 日以下): N 個` while at
  least one curion is one day from expiring; otherwise the row is blank but
  still occupies one terminal line for layout stability.
- Synthesis consumes ingredients via the existing inventory removal path
  and does not interact with lifespan — using a curion for synthesis simply
  retires it before natural decay can occur.

## Evolution Lines (Issue #36)

Selected nouns form 3-stage evolution chains that act as a meta progression
layer on top of the regular collection. Each chain is data-driven via
`data/evolutions/lines.json` (embedded with `include_str!`).

### Data shape

```json
{
  "id": "fish_dragon",
  "display_name": "魚 → 蛇 → 龍",
  "stages": [
    { "stage": 1, "noun": "魚", "required_count": 10 },
    { "stage": 2, "noun": "蛇", "required_count": 3 },
    { "stage": 3, "noun": "龍", "required_count": 1 }
  ]
}
```

- `noun` must match `Curion::noun` exactly (lookup is by string equality).
- `required_count` on stage N is the **count of stage-N nouns** needed to
  unlock stage N+1. The final stage's `required_count` represents the
  number of stage-N curions needed to consider the chain "complete".
- Stages are listed in ascending `stage` order (1..=N).

### Progress calculation

`EvolutionDatabase::calculate_progress(collection)` is a pure function over
`&[Curion]`. For each line it returns:

| Field | Meaning |
|---|---|
| `current_stage` | Highest stage reached (0 if no member nouns owned) |
| `next_stage_required` | Count needed at the current stage to unlock the next stage (`None` once complete) |
| `next_stage_noun` | Noun that unlocks at the next stage (`None` once complete) |
| `remaining_to_next` | `required - owned`, saturating at 0 |
| `progress_ratio` | `owned / required`, clamped to `[0.0, 1.0]` |

Reaching stage k (for k ≥ 2) requires the prior stage's `required_count` to
be met **and** at least one stage-k noun to be owned. Stage 1 is considered
reached as soon as one stage-1 noun is owned.

### Bundled evolution lines

| ID | Chain |
|---|---|
| `fish_dragon` | 魚 (×10) → 蛇 (×3) → 龍 (×1) |
| `bamboo_pine_forest` | 竹 (×8) → 松 (×3) → 森 (×1) |
| `fire_flame_phoenix` | 火 (×7) → 炎 (×3) → 鳳凰 (×1) |
| `water_ice_whale` | 水 (×9) → 氷 (×3) → 鯨 (×1) |
| `light_star_sun` | 光 (×12) → 星 (×4) → 太陽 (×1) |

### Out of scope (deferred)

- Synthesis-success-triggers-evolution and time-based evolution are out of
  scope for this issue. The current implementation is purely
  collection-count driven. The `EvolutionLine` schema is forward-compatible
  with adding such triggers later.

## Achievement System

40+ achievements across 4 categories:

### Collection achievements
- Total count milestones (Issue #32 cliffhanger numbers): **10, 27, 51, 103, 247, 501, 1001**
- Rarity milestones (Issue #32):
  - Rare: 10 / 47 / 103
  - Epic: 5 / 23 / 51
  - Legendary: 1 / 7 / 23 / 47 / 101
- Category completion: collect all nouns in a category

### Streak achievements
- Consecutive login (Issue #32): 3, 7, 14, **33**, **101** days
- Play time (Issue #32): 1h, **11h**, **47h**, **103h**, **503h**

### Cliffhanger numbers (Issue #32 backward compatibility)
- 実績進捗は `HashMap<AchievementId, AchievementProgress>` で保存される。
- 旧セーブに残った `total_50` `total_100` 等の ID は新版では再評価されない (実害なし、
  解除済みフラグは無視されたまま放置)。
- 新規 ID (`total_27` 等) は起動時に `register_default_achievements` から空 Progress として
  作成され、現在の所持数で再判定される。
- マイグレーション不要。旧解除フラグは消えるが、現在のカウントで即時再達成される。

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

Level N → N+1 XP requirement uses a **non-linear "cliffhanger" table** (Issue #32):

| Lv | XP   | Lv | XP    | Lv | XP    | Lv | XP    |
|----|------|----|-------|----|-------|----|-------|
| 1  | 100  | 6  | 1820  | 11 | 6170  | 16 | 13680 |
| 2  | 270  | 7  | 2450  | 12 | 7400  | 17 | 15600 |
| 3  | 510  | 8  | 3210  | 13 | 8770  | 18 | 17680 |
| 4  | 870  | 9  | 4080  | 14 | 10260 | 19 | 19920 |
| 5  | 1280 | 10 | 5060  | 15 | 11900 | 20 | 22320 |

- Lv.21 以降は `last + (last / 10) * 1.18` 風の漸近指数で外挿される。
- 旧式の `level * 100` (キリのいい等差) は廃止。「あと 50 で切りがいい所まで」を意図的に避け、
  常に半端な残量にして「あと少し感」を維持する。

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
- **Rare cooldown LineGauge** (Issue #25):
  - Fills over 4 hours since the last collection
  - Cyan while filling: `RARE COOLDOWN [████░░░░░░░░] 2:35 remaining`
  - Yellow when full: `RARE COOLDOWN [████████████] ⚡ レア出現確率上昇中!`
  - The progress (0.0..=1.0) is fed to `CurionGenerator::generate_with_bonus`, which
    subtracts up to 0.3 from the rarity roll to shift Common rolls into Rare/Epic.
  - `Player::last_collection_at` (`#[serde(default) = None]`) holds the timestamp;
    `add_curion` refreshes it. `None` (= fresh save / legacy save) is treated as
    fully charged so the very first acquisition starts with the bonus.
- **RARE 出現確率** (Issue #28):
  - Cooldown progress 反映済みの現在のレアリティ別出現確率を 1 行で表示
  - 例: `RARE出現確率: 12.3%  (Common 47.7% / Rare 30.0% / Epic 9.0% / Legendary 1.0%)`
  - 「レア以上 = Rare + Epic + Legendary」を強調色で出す
  - 計算ロジックは `crate::cooldown::current_rarity_probabilities(progress)` に閉じ込め、
    UI は値を読み取って整形するだけ。
  - generator の roll-shift モデルと整合: 累積確率境界 (0.01 / 0.10 / 0.40) に
    `0.3 * progress` を加算 → クランプして 4 帯の確率を算出。
- **Next milestone hint** (Issue #32):
  `next milestone: ⭐ コレクター Lv.3 (あと 4 個)` 形式の 1 行表示。
  XP / 未解除実績 (TotalCount / RarityCount / CategoryCount / SpecificNoun / ConsecutiveLogin
  / PlayTime) のうち、残量が最も小さい候補を 1 つ選ぶ。残量 0 は除外。
  全マイルストーン達成済みなら `全マイルストーン達成済み` を表示する。
- Quick stats: total collected, today's count, level, **COMBO: N**
  - Combo counts consecutive Rare/Epic/Legendary acquisitions; Common resets to 0
  - XP multiplier on `add_curion`: combo 2 = 1.5x, 3-4 = 2.0x, 5+ = 3.0x (XP is `(base * multiplier) as u32`, truncated)
  - At combo 5 the title `コンボマスター` is granted once (no duplicates)
  - `combo_count` is shown in label color when 0/1, Rare color at 2, Epic color at 3-4, Legendary color + `🔥 コンボマスター!` at 5+
  - `max_combo` is recorded for future stats display
  - Save compatibility: `combo_count` / `max_combo` use `#[serde(default)]`
- **SAN value (正気度) LineGauge** (Issue #29):
  - 0.0..=100.0 の `f64` を `Player::san` に持つ。初期値 100.0。
  - 表示: `SAN [████████░░░░] 67.5 / 100` 形式の LineGauge を常時 1 行で表示。
  - 色: SAN >= 80 → Cyan / 50..80 → Yellow / 30..50 → Red / < 30 → Magenta + `⚠ 異常状態` ラベル付記。
  - 変動: Common 収集 +0.5 / Rare +2.0 / Epic +5.0 / Legendary +15.0 / 合成成功 +3.0 /
    時間経過 -0.1 per minute (放置で減少)。境界は `[0.0, 100.0]` でクランプ。
  - 変動ロジックは `src/san.rs` のピュア関数 (`san_gain_for_acquisition`,
    `apply_decay`, `apply_gain`, `san_state`) に閉じ、`ui.rs` は値を読み取って描画するだけ。
  - Save compatibility: `san` は `#[serde(default = "default_san")]` (= 100.0 で復元)。
- **Lifespan warning** (Issue #30):
  - 残り寿命 1 日以下のキュリオン数を 1 行で表示: `⚠ 期限切れ間近 (残り 1 日以下): N 個`
  - 0 個のときは空行扱い (レイアウトは予約)
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
- 1 日に 3 本のミッションが並ぶ。日付ベースのシードで同じ日には全プレイヤー同じ 3 本が選ばれる
- ミッションテンプレ (4 種から 3 つランダム抽選):
  - 「10 個のキュリオンを収集」: +100 XP
  - 「Rare 以上を 3 個獲得」: +200 XP
  - 「合成を 1 回成功させる」: +300 XP
  - 「5 種類の異なるカテゴリから収集」: +150 XP
- 報酬は達成検知のたびに自動で XP 付与され、トーストで通知する
- 進捗・達成状態は `daily_mission_manager` に保存され、日付が変わると自動でリセット
- リセットは端末ローカル日付 0:00 基準。タイトルにリセットまでの残時間 (HH:MM) を表示
- 起動時 (`GameState::process_login`) では、`ensure_today_missions` の**前**に
  `auto_claim_daily_missions()` を呼んで「前日達成・未受取」のミッション XP を救済する
- 合成成功で生まれたキュリオンも収集系ミッション (CollectAny / CollectRarityAtLeast /
  CollectFromCategories) の進捗にカウントされる仕様 (`add_curion` 内の
  `record_curion_acquired` が走るため)
- 各ミッションの XP / target は仮置きであり、プレイバランス調整時に変更される予定

### Tab 2: Collection

Left pane sections:
- Owned List
- Encyclopedia

**Owned List right pane:**
- Scrollable list of all owned curions
- Display format: `#ID stars [Rarity] Category Name  Date  残 N 日` (Issue #30 残り寿命を右端に追記)
  - 残り 0 日以下: 赤 + `寿命: ! まもなく消滅`
  - 残り 1〜3 日: 黄色 `残 N 日`
  - それ以上: 薄いグレー `残 N 日`
  - 寿命なし (旧セーブ): `寿命: --`
- Attribute bars (interest, beauty) shown inline
- Bottom detail pane (`Constraint::Length(4)`, Issue #22 + #27): two lines for the curion currently at the top of the visible list. Wraps if the flavor exceeds the line width.
  - Line 1: SF flavor text (Issue #22). Missing flavor falls back to `(フレーバー未登録)`.
  - Line 2: acquisition history (Issue #27) — `入手: YYYY-MM-DD HH:MM  (通算 N回目の収集)` in local TZ. Legacy save curions without `acquisition_index` show `(履歴情報なし)` instead of the count.

**Encyclopedia right pane:**
- Two-column layout: category list (left) + noun entries for the focused category (right)
- Each category row shows `name owned/total` (unique count vs. database total)
- Header above noun entries shows overall progress `総数: owned/total (NN.N%) | カテゴリ: owned/total (NN.N%)`
- Acquired noun row: `name stars [Rarity] YYYY-MM-DD ×count` (uses highest acquired rarity and latest acquisition date)
  - Second line (small, `Color::DarkGray`): the noun's flavor text, indented two spaces. If the flavor exceeds the line width it is truncated by display width and `…` is appended (Issue #22).
- Unacquired noun row: displays `？？？` only (encourages completion). Flavor is hidden until acquired.
- Noun ordering follows the embedded JSON data order; no rearrangement based on ownership
- Each entry consumes 1 line (locked) or 2 lines (acquired + flavor). The dictionary scroll unit remains the noun entry count; the visible window is clipped at the line level.
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
- Timeline: first play, last play, total play time, streak gauges, and a daily `Sparkline` over the last 30 days (`Player::daily_acquisition_counts(30, now)`)

### Tab 5: Synthesis

Left pane sections:
- Recipe List
- Synthesize

Right pane behavior:
- Recipe List: discovered/undiscovered recipe index
- Synthesize: two-step ingredient selection flow

**Synthesis success probability (Issue #28):**
- Each recipe row in the Recipe List shows its success probability with a 10-cell bar:
  - Undiscovered: cyan `合成確率:  78% [████████░░]` (= `discovery_rate`)
  - Discovered:   green `合成確率: 100% [██████████]` (= 1.0)
- During Ingredient 2 selection, each candidate is suffixed with the actual recipe's
  success probability for the (Ingredient 1, candidate) pair, e.g.
  `... 水 (×3) → 蒸気 元素 — 合成確率 78% [████████░░]`.
- 確率は `SynthesisRecipe::success_probability(is_discovered)` /
  `SynthesisManager::success_probability_for_recipe(&recipe)` がロジック層で確定し、
  UI 層は値を整形するだけ。

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
| / | Collection: open regex filter input (Issue #31). Filters Owned List and Encyclopedia by noun / `{category} の {noun}` / rarity label (`COMMON`/`RARE`/`EPIC`/`LEGENDARY`) / category name. Type the pattern live; Enter keeps the filter and exits input mode; Esc clears the filter and exits input mode; Backspace deletes one character. Invalid regex shows a red error inline without crashing. |

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
