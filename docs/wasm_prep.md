# Curion - WASM 対応整理ノート (Issue #23)

将来の PWA / ブラウザ版 (curion.llll-ll.com など) を見据え、ロジック層と
ターミナル/IO 層の境界を明文化する。本ドキュメントは「現状の整理」と「将来
crate を分割するときの設計」の両方を扱う。本 PR (Issue #23) では crate 分割は
行わず、`src/` 内のファイル単位の境界を保つだけにとどめる。

## 1. レイヤ境界の定義

`src/` 配下のファイルを以下 2 層に明確に区分する。ロジック層は UI/IO 依存を
一切持たず、`wasm32-unknown-unknown` ターゲットでも (理論上) コンパイル可能な
ピュア Rust に閉じる。

### ロジック層 (UI/IO 非依存・WASM 互換)

| ファイル | 役割 |
|---|---|
| `src/curion.rs` | `Curion` 構造体、`Rarity` / `Category` 列挙、属性計算 |
| `src/player.rs` | `Player` 状態管理 (コレクション・XP・SAN・装備・ミッション) |
| `src/generator.rs` | 名詞 DB + latent パイプライン経由の Curion 生成 |
| `src/synthesis.rs` | レシピ DB と合成判定 (`try_synthesize_with_rolls`) |
| `src/achievement.rs` | 実績判定 (`AchievementManager`) |
| `src/latent.rs` | seed → 16-dim latent vector → noun prototype の純粋関数群 |
| `src/semantic.rs` | latent → 10 タグ `SemanticProfile` 投影 |
| `src/equipment.rs` | `EquipmentSlot` / `EquipmentEffect` (装備効果) |
| `src/cooldown.rs` | レア出現クールダウン (Issue #25) |
| `src/san.rs` | SAN 値の増減ロジック (`apply_decay` / `apply_gain` ほか) |
| `src/daily_mission.rs` | デイリーミッション生成 (deterministic seed) |
| `src/evolution.rs` | 進化系列 (`EvolutionDatabase::calculate_progress`) |

これら 12 ファイルが満たす不変条件:

- `use std::fs` / `use std::path` / `use std::env` を一切含まない
- `ratatui` / `crossterm` / `rustyline` / `nostr_sdk` / `tokio` / `dirs` の
  import を一切含まない
- `std::io` も含まない (`println!` / `eprintln!` を直接使わない)
- 外部ファイル参照は `include_str!` のみ (data/nouns/*.json,
  data/recipes/basic_recipes.json, data/evolutions/lines.json) で、ビルド時
  埋め込みなので WASM ターゲットでも問題なく動く
- 依存 crate は `serde` / `serde_json` / `chrono` / `uuid` / `rand` /
  `sha2` / `anyhow` / `regex` のみ。すべて wasm32 ターゲットで動くものに限定
  (chrono は `Utc::now()` を使う箇所が wasm32 で `js-sys` 経由になるが、
   本 PR では対応 feature flag は追加しない — 現状の cargo build を壊さない
   範囲に留める)

### IO / UI 層 (ターミナル専用・WASM 非対応)

| ファイル | 役割 |
|---|---|
| `src/main.rs` | エントリポイント、CLI 引数 (`clap`)、tokio runtime |
| `src/ui.rs` | Ratatui によるタブ UI 描画とイベント処理 |
| `src/plain.rs` | `--plain` モード (非インタラクティブ実行) |
| `src/interactive.rs` | `rustyline` ベースの REPL |
| `src/save.rs` | セーブ/ロード (`std::fs`, `dirs::home_dir`, JSON 永続化) |
| `src/nostr_identity.rs` | Nostr keypair の永続化 (`std::fs`, `nostr_sdk`) |

これらは `std::fs` / `dirs` / `ratatui` / `crossterm` / `rustyline` /
`nostr_sdk` 等のネイティブ専用 crate に直接依存する。PWA 化するときは
WASM 版で **別実装** に差し替える必要がある (永続化 → IndexedDB、
UI → DOM / Canvas、Nostr → ブラウザ用 nostr-tools 等)。

## 2. 検証方法

### 2-1. ロジック層から IO/UI crate import の混入を grep で監視

```bash
# 禁止 import が無いことの確認 (出力ゼロが期待値)
grep -rn "use std::fs\|use std::path\|use std::env" \
  src/curion.rs src/player.rs src/generator.rs src/synthesis.rs \
  src/achievement.rs src/latent.rs src/semantic.rs src/equipment.rs \
  src/cooldown.rs src/san.rs src/daily_mission.rs src/evolution.rs

grep -rn "ratatui\|crossterm\|rustyline\|nostr_sdk\|tokio\|dirs::" \
  src/curion.rs src/player.rs src/generator.rs src/synthesis.rs \
  src/achievement.rs src/latent.rs src/semantic.rs src/equipment.rs \
  src/cooldown.rs src/san.rs src/daily_mission.rs src/evolution.rs
```

2026-05-17 時点で両方 0 行 (san.rs 内に "ratatui や Player への参照を持たない"
というコメントが 1 件あるが、これは説明文なので OK)。新しいロジックファイルを
追加するときは、この grep をローカルで回して 0 行であることを確認する。

### 2-2. cargo check で通常ターゲットを保つ

```bash
cargo build --release
cargo test --all
cargo clippy --all-targets -- -D warnings
```

WASM ターゲットでの `cargo build --target wasm32-unknown-unknown` は ratatui /
tokio / nostr-sdk が WASM 非対応なので **現時点では失敗する**。これは想定内
であり、本 PR では「ロジック層のみを抜き出した WASM ビルド」も対象外。将来
crate 分割を行うときに `curion-core` 単独で `--target wasm32-unknown-unknown`
が通ることを目標とする (下記 3-1)。

## 3. 将来の crate 分割設計 (未実施・参考用)

PWA 化のフェーズに入ったら、以下のように crate を分割する想定。本 PR では
実施しないが、設計の地図として残しておく。

### 3-1. `curion-core` (純粋ロジック・WASM 互換)

```
curion-core/
  src/
    lib.rs            # pub mod curion; pub mod player; ...
    curion.rs
    player.rs
    generator.rs
    synthesis.rs
    achievement.rs
    latent.rs
    semantic.rs
    equipment.rs
    cooldown.rs
    san.rs
    daily_mission.rs
    evolution.rs
  data/               # include_str! 用 JSON
```

依存: `serde` / `chrono` (with `wasmbind` feature) / `uuid` (with `js`
feature for v4 on wasm) / `rand` / `sha2` / `anyhow` / `regex` のみ。

検証: `cargo build --target wasm32-unknown-unknown -p curion-core` が通ること。

### 3-2. `curion-tui` (ターミナル UI バイナリ)

```
curion-tui/
  src/
    main.rs
    ui.rs
    plain.rs
    interactive.rs
    save.rs
    nostr_identity.rs
```

依存: `curion-core` に加え、`ratatui` / `crossterm` / `rustyline` /
`nostr-sdk` / `tokio` / `dirs` / `clap`。

### 3-3. `curion-web` (将来の PWA フロントエンド・別リポでも可)

`curion-core` を `wasm-pack build --target web` で WASM パッケージ化し、
TypeScript フロント (Vite + Solid/Svelte あたり) から呼び出す。永続化は
IndexedDB、Nostr は `nostr-tools` (npm) を使う。

### 3-4. 移行ステップ (チェックリスト)

将来分割を実施するときの作業順:

1. `cargo new --lib curion-core` で空 crate を作る
2. 12 ロジックファイルを `curion-core/src/` に move し、`lib.rs` で pub mod 化
3. `data/` も `curion-core/data/` に move し、`include_str!` のパスを更新
4. 既存 `src/` をワークスペース構成にして `curion-tui` に rename
5. `curion-tui` の `Cargo.toml` で `curion-core = { path = "../curion-core" }`
6. `cargo build --release` (ネイティブ) を通す
7. `cd curion-core && cargo build --target wasm32-unknown-unknown` を試す
8. chrono / uuid / rand に必要な wasm feature flag を追加
9. すべて通ったら `curion-core` を crates.io 公開も検討

## 4. 本 PR (Issue #23) のスコープ

- [x] ロジック層 12 ファイルの WASM 互換性を grep で audit
- [x] `std::fs` / `std::path` / `std::env` の混入が無いことを確認
- [x] `ratatui` / `crossterm` / `rustyline` / `nostr_sdk` / `tokio` / `dirs`
      の import が混入していないことを確認
- [x] `#[cfg(not(target_arch = "wasm32"))]` の追加が不要なことを確認
- [x] `docs/wasm_prep.md` (本ファイル) を新規作成
- [x] `docs/roadmap.md` に #23 を追記

スコープ外 (将来 PR で対応):

- `curion-core` / `curion-tui` への crate 分割
- chrono / uuid / rand への wasm feature 追加
- `cargo build --target wasm32-unknown-unknown` 通過
- PWA フロントエンド実装
