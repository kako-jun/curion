# Curion - Roadmap

## 完了済みフェーズ

### Phase 1: コア機能
- [x] プロジェクト初期化
- [x] 名詞データベース (9カテゴリ、250+名詞)
- [x] GUID生成システム
- [x] キュリオン構造体 (Rarity, Category, 属性)
- [x] プレイヤー状態管理
- [x] 実績システム (40+実績)

### Phase 2: UI/UX
- [x] Ratatui TUI実装
- [x] 5タブシステム (Dashboard, Collection, Achievements, Stats, Synthesis)
- [x] 3層ナビゲーション (タブバー → 左ペイン → 右ペイン)
- [x] Cookie Clicker風のダッシュボード
- [x] 進捗バー・統計表示
- [x] docs/design.md 準拠のビジュアル強化（Gauge / LineGauge / Sparkline / BarChart / border states）
- [x] 自動セーブ (60秒間隔)
- [x] docs/design.md を実装参照基準へ拡張（全タブレイアウト原則・状態定義・将来画面プレースホルダ規則）

### Phase 3: 合成システム
- [x] 合成レシピデータベース
- [x] SynthesisManager実装
- [x] レシピ発見システム
- [x] 15種類の基本レシピ
- [x] 合成限定キュリオン (炎龍、氷鳳凰など)
- [x] スマート合成UI (候補提案システム)
- [x] Abstractカテゴリ追加 (52個の抽象概念)

### Phase 4: アイデンティティ
- [x] Nostr keypair生成
- [x] プロファイルシステム (multi-account対応)
- [x] 多重起動防止設計
- [x] CLI引数パース (`--profile`)

## 未実装機能

### P2P交換システム（優先度: 高）
- [ ] Nostr relay接続管理
- [ ] トレードオファー作成
- [ ] オファー検索・マッチング
- [ ] 交換実行・検証
- [ ] 交換UI (6番目のタブ)
- [ ] セッション管理 (重複接続防止)
- [ ] ローカルテスト用relay設定

### 地域限定システム（優先度: 中）
- [ ] 地域データベース
- [ ] IP/GPS位置判定
- [ ] 地域限定キュリオン追加
- [ ] 旅行者ボーナス

### 拡張機能（優先度: 低）
- [x] デイリーミッション
- [x] ログインボーナス
- [ ] ガチャシステム
- [x] コンボシステム
- [x] 図鑑機能
- [x] フレーバーテキスト (#22)
- [x] キュリオン入手履歴 (#27) — 入手日時 + 通算入手回数を Collection 詳細ペインに表示
- [x] コレクション正規表現絞り込み (#31) — Collection タブで `/` から正規表現を入力し、所持一覧と図鑑をリアルタイムフィルタ
- [x] レア出現予告クールダウン (#25) — 収集後 4 時間でレア確率が段階的に上昇、Dashboard 概要に LineGauge 表示
- [x] きりの悪い数字設計 (#32) — XP 閾値を非線形テーブル化、実績閾値を割り切れない値にずらし、Dashboard に「next milestone (あと N)」を常時表示
- [x] 行動前に成功確率を表示する (#28) — Synthesis レシピ一覧と Ingredient 2 候補に「合成確率 NN% [████████░░]」を表示し、Dashboard 概要には cooldown 込みの現在のレアリティ別出現確率を 1 行で表示 (計算は `cooldown::current_rarity_probabilities` / `SynthesisRecipe::success_probability` でロジック層に閉じる)
- [x] Stats タブ実装 (#26) — レアリティ BarChart (Gray/Cyan/Yellow/Red)、カテゴリ BarChart、時系列タブの直近 30 日 Sparkline。日次集計は `Player::daily_acquisition_counts(days, now)` の純粋関数に閉じてあり、UI 非依存にテスト可能
- [x] SAN 値パラメータ (#29) — 正気度 (0.0..=100.0) を `Player::san` に追加。Common +0.5 / Rare +2.0 / Epic +5.0 / Legendary +15.0 / 合成成功 +3.0 / 時間経過 -0.1/min。Dashboard 概要に LineGauge を常時表示し、>= 80 Cyan / 50..80 Yellow / 30..50 Red / < 30 Magenta + `⚠ 異常状態` で警告。変動ロジックは `src/san.rs` のピュア関数 (`san_gain_for_acquisition` / `apply_decay` / `apply_gain` / `san_state`) に閉じて UI 非依存
- [x] 寿命システム (#30) — レアリティ別寿命 (Common 3 / Rare 7 / Epic 14 / Legendary 30 日) を `Curion::lifespan_days: Option<u32>` で保持し、起動時に `Player::prune_expired` で期限切れキュリオンを自動削除。削除分は TUI トースト (6 秒) / --plain / interactive モードで通知。Collection 一覧の各行に残り寿命を表示 (残 ≤ 0 = 赤、≤ 3 = 黄、それ以上 = グレー、寿命なし = `--`)。Dashboard 概要に「⚠ 期限切れ間近 (残り 1 日以下): N 個」を常時表示 (0 個なら空行)。合成消費は寿命を見ず「使ってあげること = 供養」として扱う。旧セーブは `lifespan_days = None` で復元され永遠扱い (後方互換)
- [x] 高リスク合成 (#35) — `SynthesisRecipe` に `success_rate` (実行時成功率、デフォルト 1.0) と `failure_mode` (`NoLoss` / `LoseAll` / `Salvage{fallback_rarity}`、デフォルト `NoLoss`) を追加。発見済みでも `success_rate < 1.0` なら毎回 risk roll が走り、失敗時は `SynthesisAttemptResult::HighRiskFailure` を返す。`try_synthesize_with_rolls(ingredients, discovery_roll, risk_roll)` を内部 API として切り出し、UI 非依存にテスト可能。Synthesis レシピ一覧と Ingredient 2 候補に `[SAFE]` / `[RISKY:失敗時挙動]` バッジを併記し、`success_probability(is_discovered)` は discovery × success_rate の積を返す。`basic_recipes.json` に「禁断の神」(混沌+秩序、25% LoseAll Legendary) と「黒い太陽」(光+影、50% Salvage Common) を追加
- [x] 部分公開レシピ (#37) — `SynthesisRecipe` に `visibility: RecipeVisibility` (`Public` / `Partial` / `Unknown`、デフォルト `Public`、`#[serde(default)]` で既存 JSON 互換) を追加。`Partial` は第一材料のみ名前表示し残材料と結果を `?` でマスク、`Unknown` は recipe.name すら隠して `未確認レシピ #NN` (1-origin 2 桁 0 埋め) で識別する。発見済みになれば visibility に関わらず常に完全表示。プレイヤー視点の充足状況を `IngredientProgress { satisfied, total, all_satisfied }` で表現し、`ingredient_progress()` / `remaining_categories()` / `display_label()` をロジック層で公開。Synthesis レシピ一覧の各行に「進捗: N/M ✓」または「進捗: N/M (あと K 種)」を表示し、Unknown は進捗を出さず行全体を DarkGray、Partial は recipe.name を `COLOR_LABEL`、Public/全材料揃いかつ未発見は `COLOR_SUCCESS` で煽る。`basic_recipes.json` で陰陽 (#014) と 黒い太陽 (#017) を `partial`、禁断の神 (#016) を `unknown` に設定
- [x] 文字列 → 潜在ベクトル → ラベル パイプライン (#39) — `seed bytes → SHA-256 × 2round → 16-dim f32 latent vector → 最寄りの noun prototype = curion名` の対称パイプラインを実装。各 noun には `prototype_for_noun(name)` で deterministic に作られる prototype vector があり、latent との cosine similarity × weight が最大の noun が選ばれる。rarity / interest / beauty も同じ latent の別投影 (dims 1..4 / 8..12 / 12..16) から派生し、「curion 本体 = 潜在ベクトル、noun名はラベル」という世界観を実体化。`CurionGenerator::generate_from_guid(guid)` は GUID バイト列を seed として新パイプラインに委譲する後方互換 API として残り、`generate_with_bonus(guid, bonus)` の Issue #25 roll-shift モデル (最大 -0.3) も latent 上で完全に再現。`generate_from_seed(str, guid)` / `generate_from_seed_bytes_with_bonus(bytes, guid, bonus)` を新公開 API として追加し、将来 #38 (装備/消費効果) の効果ベクトルを同じ latent から導出する基盤を構築。実装は `src/latent.rs` の純粋関数群 (`latent_from_seed` / `prototype_for_noun` / `cosine_similarity` / `project_unit`) と `src/generator.rs::generate_from_seed_bytes_with_bonus` に閉じ、UI 非依存にテスト可能
- [x] 段階進化ガチャ (#36) — `EvolutionLine { id, display_name, stages: Vec<EvolutionStage> }` をデータ駆動で `data/evolutions/lines.json` に定義し、`include_str!` で埋め込む。各 stage は `{ stage, noun, required_count }` を持ち、stage N の `required_count` 体集めると stage N+1 が解放される。`EvolutionDatabase::calculate_progress(collection)` は純粋関数で `EvolutionProgress { current_stage, next_stage_required, next_stage_noun, remaining_to_next, progress_ratio }` を返し、UI 非依存にテスト可能。Dashboard 概要に `sort_progress_by_urgency` で「あと少し感」順に並べた進化系列 トップ 3 を 1 行ずつ表示 (完成=Green+Bold ⭐、あと 1 個=Cyan+Bold、その他=Label color)。バンドル系列: 魚→蛇→龍 / 竹→松→森 / 火→炎→鳳凰 / 水→氷→鯨 / 光→星→太陽 の 5 本。合成成功・時間経過トリガでの進化は本 Issue ではスコープ外 (将来拡張余地は schema に確保済み)
- [ ] イベントシステム
- [ ] ランキング

## 短期目標（1-2週間）

1. **P2Pシステム基礎**
   - Nostr relay接続の実装
   - トレードオファーの基本構造
   - ローカルでの動作確認

2. **テスト環境構築**
   - ローカルNostr relayセットアップ
   - 複数プロファイルでの交換テスト
   - デバッグツール整備

## 中期目標（1-2ヶ月）

1. **P2P完全実装**
   - マッチングアルゴリズム
   - 交換実行・検証
   - 不正防止機能

2. **地域限定システム**
   - 位置情報判定
   - 地域別名詞データ追加
   - 地域限定合成レシピ

3. **コンテンツ拡充**
   - 名詞を500個まで拡張
   - レシピを100個まで拡張
   - 実績を100個まで拡張

## 長期目標（3-6ヶ月）

1. **公開リリース準備**
   - パフォーマンス最適化
   - セキュリティ監査
   - ドキュメント整備

2. **コミュニティ機能**
   - 公式Nostr relay運営
   - コミュニティイベント
   - ユーザー投票で新レシピ追加

3. **文化交流機能**
   - 漢字トレード推奨システム
   - 多言語対応
   - グローバルランキング
