# Curion - プロジェクト管理ドキュメント

**最終更新**: 2025-11-17

## プロジェクト概要

**Curion** は「中毒性のあるSFコレクションゲーム」
TUIで動作する、Nostrベースの分散型メタバース

### コンセプト
- VRなしでTUIで遊べるメタバース
- GUIDベースの決定論的生成（Barcode Battler風）
- 全ての名詞がコレクション対象
- 現実では手に入らないものを収集
- 擬似的にもう一つの世界・人生を体験

### コアメカニクス
1. **GUID生成** - UUID v4 + SHA-256でキュリオンを生成
2. **合成システム** - 直感的な素材組み合わせ（水+火→蒸気）
3. **実績システム** - 40+の実績でモチベーション維持
4. **P2P交換** - Nostr relayを使った分散型トレード
5. **地域限定** - 位置情報連携で地域固有キュリオン

---

## 現在の実装状況

### ✅ 完了した機能

#### フェーズ1: コア機能
- [x] プロジェクト初期化
- [x] 名詞データベース (9カテゴリ、250+名詞)
- [x] GUID生成システム
- [x] キュリオン構造体 (Rarity, Category, 属性)
- [x] プレイヤー状態管理
- [x] 実績システム (40+実績)

#### フェーズ2: UI/UX
- [x] Ratatui TUI実装
- [x] 4タブシステム (Dashboard, Collection, Achievements, Stats)
- [x] Cookie Clicker風のダッシュボード
- [x] 進捗バー・統計表示
- [x] 自動セーブ (60秒間隔)

#### フェーズ3: 合成システム
- [x] 合成レシピデータベース
- [x] SynthesisManager実装
- [x] レシピ発見システム
- [x] 15種類の基本レシピ
- [x] 合成限定キュリオン (炎龍、氷鳳凰など)
- [x] スマート合成UI (候補提案システム)
- [x] Abstractカテゴリ追加 (52個の抽象概念)

#### フェーズ4: アイデンティティ
- [x] Nostr keypair生成
- [x] プロファイルシステム (multi-account対応)
- [x] 多重起動防止設計
- [x] CLI引数パース (`--profile`)

### 🚧 進行中の機能

なし（次のフェーズ準備中）

### 📋 未実装の機能

#### P2P交換システム (優先度: 高)
- [ ] Nostr relay接続管理
- [ ] トレードオファー作成
- [ ] オファー検索・マッチング
- [ ] 交換実行・検証
- [ ] 交換UI (6番目のタブ)
- [ ] セッション管理 (重複接続防止)
- [ ] ローカルテスト用relay設定

#### 地域限定システム (優先度: 中)
- [ ] 地域データベース
- [ ] IP/GPS位置判定
- [ ] 地域限定キュリオン追加
- [ ] 旅行者ボーナス

#### 拡張機能 (優先度: 低)
- [ ] デイリーミッション
- [ ] ログインボーナス
- [ ] ガチャシステム
- [ ] コンボシステム
- [ ] 図鑑機能
- [ ] イベントシステム
- [ ] ランキング

---

## 技術スタック

### 言語・フレームワーク
- **Rust** (edition 2021)
- **Ratatui** 0.29 - TUI framework
- **Crossterm** 0.28 - Terminal control

### 主要ライブラリ
- **nostr-sdk** 0.37 - Nostr protocol
- **clap** 4.5 - CLI parsing
- **tokio** 1.42 - Async runtime
- **serde/serde_json** - Serialization
- **uuid** 1.11 - GUID generation
- **sha2** 0.10 - Hashing
- **chrono** 0.4 - Time handling
- **rand** 0.8 - Random generation

### データ構造
- JSONベースのデータストレージ
- `~/.curion/` - 設定・セーブディレクトリ
- `data/nouns/` - 名詞データベース
- `data/recipes/` - 合成レシピ

---

## ディレクトリ構造

```
curion/
├── src/
│   ├── main.rs              # エントリーポイント
│   ├── curion.rs            # キュリオン構造体
│   ├── generator.rs         # GUID生成
│   ├── player.rs            # プレイヤー・ゲーム状態
│   ├── achievement.rs       # 実績システム
│   ├── synthesis.rs         # 合成システム
│   ├── nostr_identity.rs    # Nostrアイデンティティ
│   ├── save.rs              # セーブ管理
│   └── ui.rs                # TUI実装
├── data/
│   ├── nouns/               # 名詞データ (9カテゴリ)
│   │   ├── animals.json     # 67個
│   │   ├── plants.json
│   │   ├── colors.json
│   │   ├── objects.json
│   │   ├── concepts.json
│   │   ├── elements.json
│   │   ├── foods.json
│   │   ├── phenomena.json
│   │   └── abstracts.json   # 52個
│   └── recipes/
│       └── basic_recipes.json  # 15レシピ
├── .claude/
│   ├── addictive_ideas.md         # 機能アイデア集
│   ├── synthesis_and_p2p_design.md # 合成・P2P設計
│   ├── vision.md                   # ゲームビジョン
│   ├── implementation_roadmap.md   # 実装ロードマップ
│   └── p2p_detailed_design.md      # P2P詳細設計
├── CLAUDE.md            # このファイル (プロジェクト管理)
├── GAME_SPEC.md         # ゲーム仕様書
├── UI_DESIGN.md         # UI設計書
├── README.md            # プロジェクト説明
└── Cargo.toml           # Rust依存関係
```

---

## 次のステップ

### 短期目標 (1-2週間)

1. **P2Pシステム基礎**
   - Nostr relay接続の実装
   - トレードオファーの基本構造
   - ローカルでの動作確認

2. **テスト環境構築**
   - ローカルNostr relayセットアップ
   - 複数プロファイルでの交換テスト
   - デバッグツール整備

### 中期目標 (1-2ヶ月)

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

### 長期目標 (3-6ヶ月)

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

---

## 開発ルール

### コーディング規約
- Rustのベストプラクティスに従う
- `cargo clippy` で警告ゼロを目指す
- 全ての公開APIにドキュメントコメント
- テストコード推奨

### コミットメッセージ
- 英語で記述
- 詳細な変更内容を含める
- 関連する設計ドキュメントを参照

### ブランチ戦略
- `main`: 安定版
- `claude/*`: 開発ブランチ
- 機能完成後にPR

---

## 参考リンク

### 設計ドキュメント
- [ゲーム仕様書](GAME_SPEC.md)
- [UI設計書](UI_DESIGN.md)
- [ゲームビジョン](.claude/vision.md)
- [合成・P2P設計](.claude/synthesis_and_p2p_design.md)

### 外部リソース
- [Nostr Protocol](https://github.com/nostr-protocol/nostr)
- [Ratatui Documentation](https://ratatui.rs/)
- [Rust Book](https://doc.rust-lang.org/book/)

---

## 変更履歴

### 2025-11-17
- Nostrアイデンティティシステム実装
- プロファイルシステム追加
- 多重起動防止設計完了

### 2025-11-16
- スマート合成UI実装
- 候補提案システム完成
- Abstractカテゴリ追加 (52個)

### 2025-11-16 (初期)
- 合成システム実装
- レシピデータベース構築
- 合成限定キュリオン追加

### 2025-11-15
- プロジェクト開始
- コア機能実装
- TUI基礎完成
