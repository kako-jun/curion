# Curion - SF Collection Game

TUIで動くSFコレクションゲーム。GUIDベースの決定論的生成でキュリオン（興味の粒子）を収集する。
Nostrベースの分散型トレード機能あり。

## ドキュメント

| ファイル | 内容 | 言語 |
|---|---|---|
| `README.md` | エンドユーザー向けの使い方 | 日本語 |
| `docs/overview.md` | ゲームコンセプト・設計思想 | 英語 |
| `docs/spec.md` | コアメカニクス仕様・UI設計・データ構造 | 英語 |
| `docs/roadmap.md` | 完了済み・残タスク（内部運用メモ） | 日本語 |
| `CLAUDE.md` | AI向け内部ドキュメント（このファイル） | 日本語 |
| `.claude/vision.md` | ゲームビジョン（中毒性・世界観の深堀り） | 英語 |
| `.claude/synthesis_and_p2p_design.md` | 合成・P2P交換の設計 | 英語 |
| `.claude/p2p_detailed_design.md` | P2P詳細設計 | 英語 |
| `.claude/addictive_ideas.md` | 中毒性向上アイデア集 | 英語 |
| `.claude/implementation_roadmap.md` | 実装ロードマップ詳細 | 英語 |

## ソース構成

```
src/
├── main.rs              # エントリーポイント、CLI引数パース
├── curion.rs            # Curion構造体、Rarity/Category enum
├── generator.rs         # UUID→SHA-256→キュリオン生成
├── player.rs            # プレイヤー状態、ゲームループ
├── achievement.rs       # 40+実績の定義・判定
├── synthesis.rs         # 合成レシピ管理、SynthesisManager
├── nostr_identity.rs    # Nostr keypair生成、プロファイル管理
├── save.rs              # JSONセーブ/ロード、自動セーブ
└── ui.rs                # ratatui TUI（4タブ、ダッシュボード、煽りUI）

data/
├── nouns/               # 名詞データベース（9カテゴリ、250+語）
│   ├── animals.json     # 67個
│   ├── abstracts.json   # 52個
│   └── ...              # plants, colors, objects, concepts, elements, foods, phenomena
└── recipes/
    └── basic_recipes.json  # 15レシピ
```

## 主要な設計判断

- **決定論的生成**: 同じGUIDからは常に同じキュリオンが生成される。再現性とフェアネスのため
- **Cookie Clicker的煽りUI**: 画面下半分に「あとX個で達成」を常時表示。達成率95%以上は赤色強調
- **JSON保存**: SQLiteではなくJSONを採用。単一プレイヤー前提でシンプルさ優先
- **Nostr keypairベースのID**: 中央サーバー不要。将来のP2Pトレードの基盤
- **多重起動防止**: 同一プロファイルの複数起動を防止する設計
- **`--profile`でマルチアカウント**: 複数のコレクションを独立管理可能

## 技術スタック

Rust (edition 2021) / ratatui 0.29 / crossterm 0.28 / nostr-sdk 0.37 / clap 4.5 / tokio 1.42

## 開発ルール

- `cargo clippy` で警告ゼロを維持
- 全ての公開APIにドキュメントコメント
- コミットメッセージは英語
