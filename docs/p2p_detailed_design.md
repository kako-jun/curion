# Curion - P2P交換システム詳細設計

**最終更新**: 2025-11-17
**状態**: 設計フェーズ（実装待ち）

## 概要

NostrプロトコルをベースとしたP2P（Peer-to-Peer）キュリオン交換システムの詳細設計。

### 設計原則
1. **分散型** - 中央サーバー不要
2. **透明性** - 全ての交換履歴が公開
3. **セキュリティ** - 暗号署名で所有権証明
4. **シンプル** - 物々交換のみ（金銭取引なし）
5. **公平性** - 同時交換で詐欺防止

---

## アーキテクチャ

### システム構成

```
┌─────────────────┐         ┌─────────────────┐
│  Player A       │         │  Player B       │
│  (alice)        │         │  (bob)          │
│                 │         │                 │
│  - Curion List  │         │  - Curion List  │
│  - Nostr Keys   │         │  - Nostr Keys   │
│  - Trade Offers │         │  - Trade Offers │
└────────┬────────┘         └────────┬────────┘
         │                           │
         │  WebSocket (Nostr)        │
         │                           │
         └──────────┬────────────────┘
                    │
         ┌──────────▼──────────┐
         │   Nostr Relay       │
         │                     │
         │  - Event Storage    │
         │  - Pub/Sub          │
         │  - Session Mgmt     │
         └─────────────────────┘
```

### コンポーネント

1. **NostrClient** - Relay接続管理
2. **TradeManager** - 交換ロジック
3. **OfferStore** - オファーキャッシュ
4. **SessionManager** - セッション管理
5. **TradeUI** - 交換UI

---

## データ構造

### TradeOffer

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeOffer {
    /// オファーID (UUIDv4)
    pub id: String,

    /// 提供者の公開鍵
    pub offerer_pubkey: String,

    /// 提供するキュリオン
    pub offering: Vec<CurionRef>,

    /// 希望するキュリオン
    pub wanting: Vec<CurionWant>,

    /// オファー状態
    pub status: TradeStatus,

    /// 作成日時
    pub created_at: DateTime<Utc>,

    /// 有効期限
    pub expires_at: DateTime<Utc>,

    /// メッセージ
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurionRef {
    /// キュリオンID
    pub curion_id: String,

    /// 名詞
    pub noun: String,

    /// レアリティ
    pub rarity: Rarity,

    /// カテゴリ
    pub category: Category,

    /// 所有証明 (署名)
    pub ownership_proof: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurionWant {
    /// 特定の名詞（オプション）
    pub specific_noun: Option<String>,

    /// カテゴリ（オプション）
    pub category: Option<Category>,

    /// レアリティ（オプション）
    pub rarity: Option<Rarity>,

    /// 必要数
    pub count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeStatus {
    Open,        // 募集中
    Negotiating, // 交渉中
    Agreed,      // 合意済み
    Completed,   // 完了
    Cancelled,   // キャンセル
    Expired,     // 期限切れ
}
```

### Nostr Events

#### Event Kind 定義

```rust
const KIND_TRADE_OFFER: u16 = 30000;      // トレードオファー
const KIND_TRADE_REQUEST: u16 = 30001;    // 交換申し込み
const KIND_TRADE_ACCEPT: u16 = 30002;     // 交換承認
const KIND_TRADE_EXECUTE: u16 = 30003;    // 交換実行
const KIND_TRADE_COMPLETE: u16 = 30004;   // 交換完了
const KIND_TRADE_CANCEL: u16 = 30005;     // キャンセル
```

#### Event 構造例

**オファー投稿 (KIND: 30000)**
```json
{
  "kind": 30000,
  "pubkey": "alice_pubkey",
  "created_at": 1699999999,
  "tags": [
    ["d", "offer_id"],
    ["offer", "龍", "Legendary", "Animal"],
    ["want", "鳳凰", "Legendary", "Animal"],
    ["expires", "1700086399"]
  ],
  "content": "龍と鳳凰を交換してくれる方募集！",
  "sig": "signature"
}
```

**交換申し込み (KIND: 30001)**
```json
{
  "kind": 30001,
  "pubkey": "bob_pubkey",
  "created_at": 1700000000,
  "tags": [
    ["e", "offer_event_id"],
    ["p", "alice_pubkey"],
    ["offer", "鳳凰", "Legendary", "Animal"]
  ],
  "content": "交換希望します！",
  "sig": "signature"
}
```

---

## 交換フロー

### フロー図

```
Alice                    Relay                    Bob
  │                        │                        │
  │ 1. Create Offer        │                        │
  ├───────────────────────►│                        │
  │    (KIND: 30000)       │                        │
  │                        │                        │
  │                        │   2. Subscribe         │
  │                        │◄───────────────────────┤
  │                        │      (filter: offers)  │
  │                        │                        │
  │                        │   3. Receive Offer     │
  │                        ├───────────────────────►│
  │                        │                        │
  │                        │   4. Request Trade     │
  │                        │◄───────────────────────┤
  │   5. Receive Request   │    (KIND: 30001)       │
  │◄───────────────────────┤                        │
  │                        │                        │
  │ 6. Accept Trade        │                        │
  ├───────────────────────►│                        │
  │    (KIND: 30002)       │                        │
  │                        │   7. Receive Accept    │
  │                        ├───────────────────────►│
  │                        │                        │
  │ 8. Execute Trade       │   9. Execute Trade     │
  ├───────────────────────►│◄───────────────────────┤
  │    (KIND: 30003)       │    (KIND: 30003)       │
  │                        │                        │
  │                        │  10. Verify & Complete │
  │◄───────────────────────┼───────────────────────►│
  │    (KIND: 30004)       │    (KIND: 30004)       │
  │                        │                        │
  │ 11. Update Collection  │  12. Update Collection │
  │                        │                        │
```

### 詳細ステップ

#### Step 1: オファー作成 (Alice)
```rust
pub fn create_offer(
    &mut self,
    offering: Vec<Curion>,
    wanting: Vec<CurionWant>,
    expires_in_hours: u64,
) -> Result<TradeOffer> {
    // 1. 所有権確認
    for curion in &offering {
        if !self.player.owns(curion) {
            return Err(anyhow!("You don't own this curion"));
        }
    }

    // 2. オファー作成
    let offer = TradeOffer {
        id: Uuid::new_v4().to_string(),
        offerer_pubkey: self.identity.public_key.clone(),
        offering: offering.iter().map(|c| c.to_ref(&self.identity)).collect(),
        wanting,
        status: TradeStatus::Open,
        created_at: Utc::now(),
        expires_at: Utc::now() + Duration::hours(expires_in_hours),
        message: None,
    };

    // 3. Nostr Eventとして送信
    self.publish_offer(&offer)?;

    Ok(offer)
}
```

#### Step 2: オファー検索 (Bob)
```rust
pub fn search_offers(
    &self,
    filter: OfferFilter,
) -> Result<Vec<TradeOffer>> {
    // Relayから最新のオファーを取得
    let events = self.relay_client.query_events(
        Filter::new()
            .kind(Kind::Custom(30000))
            .since(Timestamp::now() - Duration::hours(24))
    )?;

    // フィルタリング
    let offers: Vec<TradeOffer> = events
        .into_iter()
        .filter_map(|e| parse_offer_event(&e))
        .filter(|o| filter.matches(o))
        .collect();

    Ok(offers)
}
```

#### Step 3: 交換申し込み (Bob)
```rust
pub fn request_trade(
    &mut self,
    offer: &TradeOffer,
    my_offering: Vec<Curion>,
) -> Result<()> {
    // 1. 要求を満たすか確認
    if !offer.wants_match(&my_offering) {
        return Err(anyhow!("Your offer doesn't match requirements"));
    }

    // 2. 所有権確認
    for curion in &my_offering {
        if !self.player.owns(curion) {
            return Err(anyhow!("You don't own this curion"));
        }
    }

    // 3. 交換申し込みEvent送信
    self.publish_trade_request(&offer, &my_offering)?;

    Ok(())
}
```

#### Step 4-5: 承認 (Alice)
```rust
pub fn accept_trade_request(
    &mut self,
    request: &TradeRequest,
) -> Result<()> {
    // 1. まだ有効なオファーか確認
    if self.my_offer.status != TradeStatus::Open {
        return Err(anyhow!("Offer is no longer available"));
    }

    // 2. 承認Event送信
    self.publish_trade_accept(request)?;

    // 3. 状態を「交渉中」に更新
    self.my_offer.status = TradeStatus::Negotiating;

    Ok(())
}
```

#### Step 6-7: 交換実行 (両者)
```rust
pub fn execute_trade(
    &mut self,
    trade_session: &TradeSession,
) -> Result<()> {
    // 1. 両者の署名確認
    if !trade_session.verify_signatures() {
        return Err(anyhow!("Invalid signatures"));
    }

    // 2. 交換実行Event送信（同時）
    let execute_event = self.create_execute_event(trade_session)?;
    self.relay_client.publish(execute_event)?;

    // 3. 相手の実行Eventを待機
    let timeout = Duration::from_secs(30);
    let partner_event = self.wait_for_partner_execute(trade_session, timeout)?;

    // 4. 相手のEvent検証
    if !self.verify_partner_execute(&partner_event) {
        return Err(anyhow!("Partner execute failed"));
    }

    // 5. 交換実行（ローカル）
    self.swap_curions(trade_session)?;

    // 6. 完了Event送信
    self.publish_trade_complete(trade_session)?;

    Ok(())
}
```

---

## セキュリティ設計

### 所有権証明

```rust
pub struct OwnershipProof {
    /// キュリオンID
    curion_id: String,

    /// 所有者の公開鍵
    owner_pubkey: String,

    /// タイムスタンプ
    timestamp: DateTime<Utc>,

    /// 署名 (秘密鍵で curion_id + timestamp に署名)
    signature: String,
}

impl OwnershipProof {
    pub fn create(curion: &Curion, keys: &Keys) -> Result<Self> {
        let timestamp = Utc::now();
        let message = format!("{}:{}", curion.id, timestamp.timestamp());
        let signature = keys.sign_message(&message)?;

        Ok(Self {
            curion_id: curion.id.clone(),
            owner_pubkey: keys.public_key().to_hex(),
            timestamp,
            signature: signature.to_hex(),
        })
    }

    pub fn verify(&self, public_key: &PublicKey) -> bool {
        let message = format!("{}:{}", self.curion_id, self.timestamp.timestamp());
        verify_signature(public_key, &self.signature, &message)
    }
}
```

### 二重交換防止

```rust
pub struct TradeHistory {
    /// 交換済みキュリオンのID → 交換日時
    traded_curions: HashMap<String, DateTime<Utc>>,
}

impl TradeHistory {
    pub fn can_trade(&self, curion_id: &str) -> bool {
        // 1時間以内に交換済みなら拒否
        if let Some(traded_at) = self.traded_curions.get(curion_id) {
            let elapsed = Utc::now() - *traded_at;
            if elapsed < Duration::hours(1) {
                return false;
            }
        }
        true
    }

    pub fn mark_traded(&mut self, curion_id: String) {
        self.traded_curions.insert(curion_id, Utc::now());
    }
}
```

### セッション管理（重複接続防止）

```rust
pub struct SessionManager {
    /// 接続中のpubkey → 接続時刻
    active_sessions: HashMap<String, DateTime<Utc>>,
}

impl SessionManager {
    /// 新しい接続を試みる
    pub fn try_connect(&mut self, pubkey: &str) -> Result<()> {
        // すでに接続中か確認
        if let Some(connected_at) = self.active_sessions.get(pubkey) {
            let elapsed = Utc::now() - *connected_at;

            // 5分以内なら拒否
            if elapsed < Duration::minutes(5) {
                return Err(anyhow!(
                    "This account is already connected from another session"
                ));
            }

            // 古い接続はタイムアウトとみなす
            self.active_sessions.remove(pubkey);
        }

        // 新しいセッションを記録
        self.active_sessions.insert(pubkey.to_string(), Utc::now());

        Ok(())
    }

    /// 切断
    pub fn disconnect(&mut self, pubkey: &str) {
        self.active_sessions.remove(pubkey);
    }

    /// ハートビート（接続維持）
    pub fn heartbeat(&mut self, pubkey: &str) {
        if self.active_sessions.contains_key(pubkey) {
            self.active_sessions.insert(pubkey.to_string(), Utc::now());
        }
    }
}
```

---

## UI設計

### Exchange タブ (Tab 6)

```
┌─────────────────────────────────────────────────────────────┐
│ Exchange [🔄 3 players online]                              │
├─────────────────────────────────────────────────────────────┤
│ My Offer:                                                   │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ Offering: ★★★ 龍 (Animal)                              ││
│ │ Wanting:  ★★★★ 鳳凰 (Animal) or ★★★ 麒麟 (Animal)    ││
│ │ Status: Open | Created: 5 min ago | Expires: 23h 55m   ││
│ │ [Edit] [Cancel]                                         ││
│ └─────────────────────────────────────────────────────────┘│
│                                                             │
│ Available Offers:                                           │
│ ┌─────────────────────────────────────────────────────────┐│
│ │ @alice (Japan) - 2 min ago                              ││
│ │ Offering: ★★ 鯉 ×3                                     ││
│ │ Wanting:  ★★★ Any Epic                                 ││
│ │ [Request Trade]                                         ││
│ └─────────────────────────────────────────────────────────┘│
│ ┌─────────────────────────────────────────────────────────┐│
│ │ @bob (USA) - 10 min ago                                 ││
│ │ Offering: ★★★★ Eagle (Animal, Regional)               ││
│ │ Wanting:  ★★★ 桜 (Plant, Japan Regional)              ││
│ │ [Request Trade]                                         ││
│ └─────────────────────────────────────────────────────────┘│
│                                                             │
│ [Create Offer] [Refresh] [Filter]                          │
└─────────────────────────────────────────────────────────────┘
```

### 操作フロー

1. **Tab/6** - Exchangeタブを開く
2. **n** - 新しいオファー作成
3. **r** - リフレッシュ
4. **f** - フィルター設定
5. **Enter** - 選択したオファーに申し込み
6. **e** - 自分のオファーを編集
7. **c** - 自分のオファーをキャンセル

---

## テスト計画

### ユニットテスト

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offer_creation() {
        // オファー作成テスト
    }

    #[test]
    fn test_ownership_proof() {
        // 所有権証明テスト
    }

    #[test]
    fn test_duplicate_connection() {
        // 重複接続防止テスト
    }

    #[test]
    fn test_trade_execution() {
        // 交換実行テスト
    }
}
```

### 統合テスト

```bash
# ローカルNostr relay起動
docker run -p 7777:7777 nostr-relay

# 3つのプロファイルで起動
cargo run -- --profile alice &
cargo run -- --profile bob &
cargo run -- --profile carol &

# 交換テスト実行
# 1. Aliceがオファー作成
# 2. Bobがオファーに申し込み
# 3. 交換実行
# 4. 両者のコレクション確認
```

---

## ローカルテスト環境

### Nostr Relay セットアップ

```bash
# Option 1: Docker
docker run -d \
  --name curion-relay \
  -p 7777:7777 \
  scsibug/nostr-rs-relay

# Option 2: Binary
git clone https://github.com/scsibug/nostr-rs-relay
cd nostr-rs-relay
cargo build --release
./target/release/nostr-rs-relay --port 7777
```

### 設定ファイル

```toml
# config.toml
[relay]
url = "ws://localhost:7777"
timeout_seconds = 30

[session]
duplicate_timeout_minutes = 5
heartbeat_interval_seconds = 60

[trade]
offer_default_expiry_hours = 24
execute_timeout_seconds = 30
```

---

## 実装タスク

### Phase 5.1: Relay接続
- [ ] `NostrClient` struct実装
- [ ] Relay URL設定
- [ ] 接続/切断ロジック
- [ ] 再接続ロジック
- [ ] イベント送受信テスト

### Phase 5.2: オファー機能
- [ ] `TradeOffer` struct実装
- [ ] オファー作成関数
- [ ] オファーEvent送信
- [ ] オファー検索関数
- [ ] オファーキャッシュ

### Phase 5.3: マッチング
- [ ] `TradeRequest` struct実装
- [ ] 申し込み関数
- [ ] 承認関数
- [ ] マッチング通知

### Phase 5.4: 交換実行
- [ ] 交換実行関数
- [ ] 所有権検証
- [ ] 同時交換ロジック
- [ ] 履歴記録

### Phase 5.5: UI実装
- [ ] Exchangeタブ追加
- [ ] オファー作成UI
- [ ] オファーリスト表示
- [ ] 交換実行UI
- [ ] ステータス表示

### Phase 5.6: セキュリティ
- [ ] セッション管理実装
- [ ] 二重交換防止
- [ ] タイムアウト処理
- [ ] エラーハンドリング

---

## 参考資料

- [Nostr Protocol](https://github.com/nostr-protocol/nostr)
- [Nostr NIPs](https://github.com/nostr-protocol/nips)
- [nostr-sdk Documentation](https://docs.rs/nostr-sdk/)
- [nostr-rs-relay](https://github.com/scsibug/nostr-rs-relay)

---

**次のステップ**: Phase 5.1から実装開始
