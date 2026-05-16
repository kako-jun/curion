use crate::curion::{Category, Curion, Rarity};
use chrono::NaiveDate;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::SeedableRng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

/// デイリーミッションの種類
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DailyMissionKind {
    /// 任意のキュリオンを N 個収集
    CollectAny(usize),
    /// 指定レアリティ以上のキュリオンを N 個獲得
    CollectRarityAtLeast(Rarity, usize),
    /// 合成を N 回成功させる
    SynthesizeSuccess(usize),
    /// N 種類の異なるカテゴリから収集
    CollectFromCategories(usize),
}

/// 単一のデイリーミッション
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyMission {
    /// 安定ID（例: "collect_any_10"）
    pub id: String,
    /// 表示用説明
    pub description: String,
    /// 種別
    pub kind: DailyMissionKind,
    /// 目標値
    pub target: usize,
    /// 現在の進捗
    pub current: usize,
    /// 達成時に付与する XP
    pub reward_xp: u32,
    /// 有効期限（その日の日付）
    pub expires_at: NaiveDate,
    /// 報酬受取済み
    pub claimed: bool,
}

impl DailyMission {
    /// 達成しているか
    pub fn is_completed(&self) -> bool {
        self.current >= self.target
    }

    /// 進捗率 (0.0..=1.0)
    pub fn progress_ratio(&self) -> f64 {
        if self.target == 0 {
            return 1.0;
        }
        (self.current as f64 / self.target as f64).clamp(0.0, 1.0)
    }
}

/// デイリーミッションのテンプレート（種ごとに生成される雛形）
struct MissionTemplate {
    id: &'static str,
    description: &'static str,
    kind: DailyMissionKind,
    target: usize,
    reward_xp: u32,
}

fn templates() -> Vec<MissionTemplate> {
    vec![
        MissionTemplate {
            id: "collect_any_10",
            description: "10個のキュリオンを収集",
            kind: DailyMissionKind::CollectAny(10),
            target: 10,
            reward_xp: 100,
        },
        MissionTemplate {
            id: "collect_rare_at_least_3",
            description: "Rare 以上を 3 個獲得",
            kind: DailyMissionKind::CollectRarityAtLeast(Rarity::Rare, 3),
            target: 3,
            reward_xp: 200,
        },
        MissionTemplate {
            id: "synthesize_success_1",
            description: "合成を 1 回成功させる",
            kind: DailyMissionKind::SynthesizeSuccess(1),
            target: 1,
            reward_xp: 300,
        },
        MissionTemplate {
            id: "collect_from_categories_5",
            description: "5 種類の異なるカテゴリから収集",
            kind: DailyMissionKind::CollectFromCategories(5),
            target: 5,
            reward_xp: 150,
        },
    ]
}

fn seed_for_date(date: NaiveDate) -> [u8; 32] {
    // 日付文字列をハッシュ化して 32 byte シードを得る。
    // 「curion-daily-mission/YYYY-MM-DD」を素材にすることで、他用途と衝突しないようにする。
    let key = format!("curion-daily-mission/{}", date.format("%Y-%m-%d"));
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    let mut seed = [0u8; 32];
    seed.copy_from_slice(&digest);
    seed
}

/// デイリーミッション管理
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DailyMissionManager {
    /// 今日のミッション 3 本
    pub missions: Vec<DailyMission>,
    /// 最後にミッションを生成した日付（None なら未生成）
    pub generated_date: Option<NaiveDate>,
    /// 今日カテゴリ進捗 (`CollectFromCategories`) 用のユニーク集合
    #[serde(default)]
    pub unique_categories_today: HashSet<Category>,
}

impl DailyMissionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// 日付ベースの seed を使って 4 テンプレから 3 つを選び、`DailyMission` を返す
    pub fn generate_for_date(date: NaiveDate) -> Vec<DailyMission> {
        let mut rng = StdRng::from_seed(seed_for_date(date));
        let mut all = templates();
        all.shuffle(&mut rng);
        all.into_iter()
            .take(3)
            .map(|tpl| DailyMission {
                id: tpl.id.to_string(),
                description: tpl.description.to_string(),
                kind: tpl.kind,
                target: tpl.target,
                current: 0,
                reward_xp: tpl.reward_xp,
                expires_at: date,
                claimed: false,
            })
            .collect()
    }

    /// 日付が変わったら新しい 3 本を生成する。同じ日付なら何もしない。
    pub fn ensure_today_missions(&mut self, today: NaiveDate) {
        if self.generated_date == Some(today) {
            return;
        }
        self.missions = Self::generate_for_date(today);
        self.generated_date = Some(today);
        self.unique_categories_today.clear();
    }

    /// キュリオン獲得を記録し、各ミッションの進捗を更新する
    pub fn record_curion_acquired(&mut self, curion: &Curion) {
        // CollectFromCategories のためにカテゴリ集合を更新
        self.unique_categories_today.insert(curion.category.clone());
        let unique_count = self.unique_categories_today.len();

        for mission in self.missions.iter_mut() {
            if mission.claimed {
                continue;
            }
            match &mission.kind {
                DailyMissionKind::CollectAny(_) => {
                    if mission.current < mission.target {
                        mission.current += 1;
                    }
                }
                DailyMissionKind::CollectRarityAtLeast(min_rarity, _) => {
                    if rarity_rank(&curion.rarity) >= rarity_rank(min_rarity)
                        && mission.current < mission.target
                    {
                        mission.current += 1;
                    }
                }
                DailyMissionKind::CollectFromCategories(_) => {
                    mission.current = unique_count.min(mission.target);
                }
                DailyMissionKind::SynthesizeSuccess(_) => {}
            }
        }
    }

    /// 合成成功を記録する
    pub fn record_synthesis_success(&mut self) {
        for mission in self.missions.iter_mut() {
            if mission.claimed {
                continue;
            }
            if matches!(mission.kind, DailyMissionKind::SynthesizeSuccess(_))
                && mission.current < mission.target
            {
                mission.current += 1;
            }
        }
    }

    /// 達成済みかつ未受取のミッションを「受取済み」にして返す。
    /// XP 付与は呼び出し側 (`GameState`) で行う。
    pub fn claim_completed(&mut self) -> Vec<DailyMission> {
        let mut claimed = Vec::new();
        for mission in self.missions.iter_mut() {
            if !mission.claimed && mission.is_completed() {
                mission.claimed = true;
                claimed.push(mission.clone());
            }
        }
        claimed
    }
}

fn rarity_rank(r: &Rarity) -> u8 {
    match r {
        Rarity::Common => 0,
        Rarity::Rare => 1,
        Rarity::Epic => 2,
        Rarity::Legendary => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use uuid::Uuid;

    fn make_curion(category: Category, rarity: Rarity) -> Curion {
        Curion::new(
            Uuid::new_v4(),
            "test".to_string(),
            category,
            rarity,
            0.5,
            0.5,
        )
    }

    #[test]
    fn generate_for_date_deterministic() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let a = DailyMissionManager::generate_for_date(date);
        let b = DailyMissionManager::generate_for_date(date);
        assert_eq!(a.len(), 3);
        assert_eq!(b.len(), 3);
        let a_ids: Vec<_> = a.iter().map(|m| m.id.as_str()).collect();
        let b_ids: Vec<_> = b.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(a_ids, b_ids);
    }

    #[test]
    fn generate_for_date_differs_per_day() {
        // すべての日付で同一順では困るので、別日のシードが少なくとも一回は別の順序を出すことを確認。
        let d1 = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut seen = std::collections::HashSet::new();
        for delta in 0..14 {
            let d = d1
                .checked_add_signed(chrono::Duration::days(delta))
                .unwrap();
            let ids: Vec<_> = DailyMissionManager::generate_for_date(d)
                .into_iter()
                .map(|m| m.id)
                .collect();
            seen.insert(ids);
        }
        assert!(seen.len() > 1, "2週間で順序がまったく変わらないのは異常");
    }

    #[test]
    fn ensure_today_missions_only_regenerates_on_date_change() {
        let mut mgr = DailyMissionManager::new();
        let d1 = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        mgr.ensure_today_missions(d1);
        let snapshot_ids: Vec<_> = mgr.missions.iter().map(|m| m.id.clone()).collect();
        // 同日呼び出し: 何も変わらない
        if let Some(m) = mgr.missions.first_mut() {
            m.current = 5;
        }
        mgr.ensure_today_missions(d1);
        assert_eq!(mgr.missions[0].current, 5);
        // 翌日: ミッション再生成、カテゴリ集合もクリア
        mgr.unique_categories_today.insert(Category::Animal);
        mgr.ensure_today_missions(d2);
        assert!(mgr.unique_categories_today.is_empty());
        let new_ids: Vec<_> = mgr.missions.iter().map(|m| m.id.clone()).collect();
        // 日が変われば current は 0 にリセットされている
        assert!(mgr.missions.iter().all(|m| m.current == 0));
        // ID 自体は同じこともある（4 から 3 を選ぶ）が、生成日は更新される
        assert_eq!(mgr.generated_date, Some(d2));
        let _ = (snapshot_ids, new_ids);
    }

    #[test]
    fn record_curion_acquired_increments() {
        let mut mgr = DailyMissionManager::new();
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        mgr.ensure_today_missions(date);

        // 全テンプレに対する進捗を一気に検証するため、4 種から選ばれた 3 本に対し、
        // 該当する種別の進捗が動くことだけを保証する。
        let before: Vec<usize> = mgr.missions.iter().map(|m| m.current).collect();
        let curion = make_curion(Category::Animal, Rarity::Epic);
        mgr.record_curion_acquired(&curion);
        let after: Vec<usize> = mgr.missions.iter().map(|m| m.current).collect();

        for (i, mission) in mgr.missions.iter().enumerate() {
            match &mission.kind {
                DailyMissionKind::CollectAny(_) => assert_eq!(after[i], before[i] + 1),
                DailyMissionKind::CollectRarityAtLeast(min, _) => {
                    if rarity_rank(min) <= rarity_rank(&Rarity::Epic) {
                        assert_eq!(after[i], before[i] + 1);
                    }
                }
                DailyMissionKind::CollectFromCategories(_) => assert_eq!(after[i], 1),
                DailyMissionKind::SynthesizeSuccess(_) => assert_eq!(after[i], before[i]),
            }
        }
    }

    #[test]
    fn collect_from_categories_counts_unique() {
        let mut mgr = DailyMissionManager::new();
        // テンプレに依存しないよう、ミッションを手動でセット
        mgr.missions = vec![DailyMission {
            id: "collect_from_categories_5".to_string(),
            description: "5 種類".to_string(),
            kind: DailyMissionKind::CollectFromCategories(5),
            target: 5,
            current: 0,
            reward_xp: 150,
            expires_at: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            claimed: false,
        }];
        mgr.generated_date = Some(NaiveDate::from_ymd_opt(2026, 5, 16).unwrap());

        // 同じカテゴリを 3 回獲得 → 1 のまま
        for _ in 0..3 {
            mgr.record_curion_acquired(&make_curion(Category::Animal, Rarity::Common));
        }
        assert_eq!(mgr.missions[0].current, 1);
        // 別カテゴリ → 2
        mgr.record_curion_acquired(&make_curion(Category::Plant, Rarity::Common));
        assert_eq!(mgr.missions[0].current, 2);
    }

    #[test]
    fn claim_completed_returns_finished_missions_once() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut mgr = DailyMissionManager::new();
        mgr.missions = vec![DailyMission {
            id: "collect_any_10".to_string(),
            description: "10 個".to_string(),
            kind: DailyMissionKind::CollectAny(10),
            target: 10,
            current: 10,
            reward_xp: 100,
            expires_at: date,
            claimed: false,
        }];
        mgr.generated_date = Some(date);

        let claimed = mgr.claim_completed();
        assert_eq!(claimed.len(), 1);
        assert_eq!(claimed[0].reward_xp, 100);
        // 2 回目は重複付与されない
        let again = mgr.claim_completed();
        assert!(again.is_empty());
        assert!(mgr.missions[0].claimed);
    }

    #[test]
    fn record_synthesis_success_progresses_only_synthesize_kind() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut mgr = DailyMissionManager::new();
        mgr.missions = vec![
            DailyMission {
                id: "synthesize_success_1".to_string(),
                description: "合成 1 回".to_string(),
                kind: DailyMissionKind::SynthesizeSuccess(1),
                target: 1,
                current: 0,
                reward_xp: 300,
                expires_at: date,
                claimed: false,
            },
            DailyMission {
                id: "collect_any_10".to_string(),
                description: "10 個".to_string(),
                kind: DailyMissionKind::CollectAny(10),
                target: 10,
                current: 0,
                reward_xp: 100,
                expires_at: date,
                claimed: false,
            },
        ];
        mgr.generated_date = Some(date);

        mgr.record_synthesis_success();
        assert_eq!(mgr.missions[0].current, 1);
        assert_eq!(mgr.missions[1].current, 0);
    }

    // -----------------------------------------------------------------
    // Issue #20 追加テスト群
    // 1 テスト 1 観点で粒度を細かく切り、回帰検知の精度を上げる。
    // -----------------------------------------------------------------

    #[test]
    fn test_generate_for_date_is_deterministic() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let first = DailyMissionManager::generate_for_date(date);
        let second = DailyMissionManager::generate_for_date(date);
        assert_eq!(first.len(), 3);
        let first_ids: Vec<_> = first.iter().map(|m| m.id.clone()).collect();
        let second_ids: Vec<_> = second.iter().map(|m| m.id.clone()).collect();
        assert_eq!(
            first_ids, second_ids,
            "同じ日付なら必ず同じ id 順序で 3 本が並ぶこと"
        );
    }

    #[test]
    fn test_generate_for_date_distinct_missions() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let missions = DailyMissionManager::generate_for_date(date);
        let mut ids: Vec<_> = missions.iter().map(|m| m.id.clone()).collect();
        ids.sort();
        let unique_count = {
            let mut dedup = ids.clone();
            dedup.dedup();
            dedup.len()
        };
        assert_eq!(unique_count, ids.len(), "同テンプレ×3 にならないこと");
        assert_eq!(unique_count, 3);
    }

    #[test]
    fn test_generate_for_date_different_days_can_differ() {
        // 連続 7 日生成し、最低 1 種類以上は別の順序になることを期待する。
        // テンプレ 4 本から 3 本選ぶので、確率的にはほぼ確実に複数の組み合わせが出る。
        let base = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut seen = std::collections::HashSet::new();
        for delta in 0..7 {
            let d = base
                .checked_add_signed(chrono::Duration::days(delta))
                .unwrap();
            let ids: Vec<_> = DailyMissionManager::generate_for_date(d)
                .into_iter()
                .map(|m| m.id)
                .collect();
            seen.insert(ids);
        }
        assert!(
            seen.len() > 1,
            "7 日連続で生成パターンが完全に同一なのは異常 (got {} 種類)",
            seen.len()
        );
    }

    #[test]
    fn test_ensure_today_missions_replaces_on_date_change() {
        let mut mgr = DailyMissionManager::new();
        let d1 = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let d2 = NaiveDate::from_ymd_opt(2026, 5, 17).unwrap();
        mgr.ensure_today_missions(d1);
        assert_eq!(mgr.generated_date, Some(d1));
        // d1 のミッションを少し進めておく
        mgr.missions[0].current = 3;
        mgr.unique_categories_today.insert(Category::Animal);

        mgr.ensure_today_missions(d2);
        assert_eq!(
            mgr.generated_date,
            Some(d2),
            "日付が変われば generated_date が更新される"
        );
        assert!(
            mgr.missions.iter().all(|m| m.current == 0),
            "再生成されているので current は 0"
        );
        assert!(
            mgr.unique_categories_today.is_empty(),
            "カテゴリ集合も翌日でクリアされる"
        );
    }

    #[test]
    fn test_ensure_today_missions_no_op_same_day() {
        let mut mgr = DailyMissionManager::new();
        let d1 = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        mgr.ensure_today_missions(d1);
        // 既存ミッションの状態を書き換え、同日呼び出しで保持されるか確認
        mgr.missions[0].current = 7;
        mgr.missions[0].claimed = true;
        let before_ids: Vec<_> = mgr.missions.iter().map(|m| m.id.clone()).collect();

        mgr.ensure_today_missions(d1);

        let after_ids: Vec<_> = mgr.missions.iter().map(|m| m.id.clone()).collect();
        assert_eq!(before_ids, after_ids);
        assert_eq!(mgr.missions[0].current, 7);
        assert!(mgr.missions[0].claimed);
    }

    #[test]
    fn test_record_curion_acquired_increments_collect_any() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut mgr = DailyMissionManager::new();
        mgr.missions = vec![DailyMission {
            id: "collect_any_10".to_string(),
            description: "10 個".to_string(),
            kind: DailyMissionKind::CollectAny(10),
            target: 10,
            current: 0,
            reward_xp: 100,
            expires_at: date,
            claimed: false,
        }];
        mgr.generated_date = Some(date);

        mgr.record_curion_acquired(&make_curion(Category::Animal, Rarity::Common));
        assert_eq!(mgr.missions[0].current, 1);
    }

    #[test]
    fn test_record_curion_acquired_rarity_filter() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut mgr = DailyMissionManager::new();
        mgr.missions = vec![DailyMission {
            id: "collect_rare_at_least_3".to_string(),
            description: "Rare 以上 3 個".to_string(),
            kind: DailyMissionKind::CollectRarityAtLeast(Rarity::Rare, 3),
            target: 3,
            current: 0,
            reward_xp: 200,
            expires_at: date,
            claimed: false,
        }];
        mgr.generated_date = Some(date);

        // Common は弾かれる
        mgr.record_curion_acquired(&make_curion(Category::Animal, Rarity::Common));
        assert_eq!(mgr.missions[0].current, 0, "Common はカウントしない");

        // Rare はカウントされる
        mgr.record_curion_acquired(&make_curion(Category::Animal, Rarity::Rare));
        assert_eq!(mgr.missions[0].current, 1);

        // Epic も上位互換でカウントされる
        mgr.record_curion_acquired(&make_curion(Category::Animal, Rarity::Epic));
        assert_eq!(mgr.missions[0].current, 2);
    }

    #[test]
    fn test_record_curion_acquired_categories_unique() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut mgr = DailyMissionManager::new();
        mgr.missions = vec![DailyMission {
            id: "collect_from_categories_5".to_string(),
            description: "5 種類".to_string(),
            kind: DailyMissionKind::CollectFromCategories(5),
            target: 5,
            current: 0,
            reward_xp: 150,
            expires_at: date,
            claimed: false,
        }];
        mgr.generated_date = Some(date);

        // 同カテゴリ複数獲得 → current は 1 のまま
        mgr.record_curion_acquired(&make_curion(Category::Animal, Rarity::Common));
        mgr.record_curion_acquired(&make_curion(Category::Animal, Rarity::Rare));
        mgr.record_curion_acquired(&make_curion(Category::Animal, Rarity::Common));
        assert_eq!(mgr.missions[0].current, 1);

        // 別カテゴリ獲得 → +1
        mgr.record_curion_acquired(&make_curion(Category::Plant, Rarity::Common));
        assert_eq!(mgr.missions[0].current, 2);
    }

    #[test]
    fn test_record_synthesis_success_increments() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut mgr = DailyMissionManager::new();
        mgr.missions = vec![
            DailyMission {
                id: "synthesize_success_1".to_string(),
                description: "合成 1 回".to_string(),
                kind: DailyMissionKind::SynthesizeSuccess(1),
                target: 1,
                current: 0,
                reward_xp: 300,
                expires_at: date,
                claimed: false,
            },
            DailyMission {
                id: "collect_any_10".to_string(),
                description: "10 個".to_string(),
                kind: DailyMissionKind::CollectAny(10),
                target: 10,
                current: 0,
                reward_xp: 100,
                expires_at: date,
                claimed: false,
            },
        ];
        mgr.generated_date = Some(date);

        mgr.record_synthesis_success();
        assert_eq!(mgr.missions[0].current, 1, "合成ミッションのみ +1");
        assert_eq!(mgr.missions[1].current, 0, "他種別は変化しない");
    }

    #[test]
    fn test_claim_completed_returns_only_unclaimed_completed() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut mgr = DailyMissionManager::new();
        mgr.missions = vec![
            // 達成済み + 未受取 → 返ってくる
            DailyMission {
                id: "collect_any_10".to_string(),
                description: "10 個".to_string(),
                kind: DailyMissionKind::CollectAny(10),
                target: 10,
                current: 10,
                reward_xp: 100,
                expires_at: date,
                claimed: false,
            },
            // 達成済み + 受取済 → 返らない
            DailyMission {
                id: "synthesize_success_1".to_string(),
                description: "合成 1 回".to_string(),
                kind: DailyMissionKind::SynthesizeSuccess(1),
                target: 1,
                current: 1,
                reward_xp: 300,
                expires_at: date,
                claimed: true,
            },
            // 未達成 → 返らない
            DailyMission {
                id: "collect_rare_at_least_3".to_string(),
                description: "Rare 以上 3 個".to_string(),
                kind: DailyMissionKind::CollectRarityAtLeast(Rarity::Rare, 3),
                target: 3,
                current: 1,
                reward_xp: 200,
                expires_at: date,
                claimed: false,
            },
        ];
        mgr.generated_date = Some(date);

        let claimed = mgr.claim_completed();
        assert_eq!(claimed.len(), 1, "未 claim の達成済み 1 本だけ返る");
        assert_eq!(claimed[0].id, "collect_any_10");
    }

    #[test]
    fn test_claim_completed_marks_claimed() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut mgr = DailyMissionManager::new();
        mgr.missions = vec![DailyMission {
            id: "collect_any_10".to_string(),
            description: "10 個".to_string(),
            kind: DailyMissionKind::CollectAny(10),
            target: 10,
            current: 10,
            reward_xp: 100,
            expires_at: date,
            claimed: false,
        }];
        mgr.generated_date = Some(date);

        let _ = mgr.claim_completed();
        assert!(mgr.missions[0].claimed, "claim_completed 後は claimed=true");
        // 二度呼んでも空（idempotent）
        assert!(mgr.claim_completed().is_empty());
    }

    #[test]
    fn test_completed_progress_not_incremented_further() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut mgr = DailyMissionManager::new();
        mgr.missions = vec![
            DailyMission {
                id: "collect_any_10".to_string(),
                description: "10 個".to_string(),
                kind: DailyMissionKind::CollectAny(10),
                target: 10,
                current: 10,
                reward_xp: 100,
                expires_at: date,
                claimed: true, // 既に受取済み
            },
            DailyMission {
                id: "synthesize_success_1".to_string(),
                description: "合成 1 回".to_string(),
                kind: DailyMissionKind::SynthesizeSuccess(1),
                target: 1,
                current: 1,
                reward_xp: 300,
                expires_at: date,
                claimed: true, // 既に受取済み
            },
        ];
        mgr.generated_date = Some(date);

        // claimed なミッションは record 系で current が動かない
        mgr.record_curion_acquired(&make_curion(Category::Animal, Rarity::Common));
        mgr.record_synthesis_success();
        assert_eq!(mgr.missions[0].current, 10);
        assert_eq!(mgr.missions[1].current, 1);
    }

    #[test]
    fn test_save_load_roundtrip() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 16).unwrap();
        let mut mgr = DailyMissionManager::new();
        mgr.ensure_today_missions(date);
        mgr.record_curion_acquired(&make_curion(Category::Animal, Rarity::Rare));
        mgr.record_curion_acquired(&make_curion(Category::Plant, Rarity::Common));
        mgr.record_synthesis_success();

        let json = serde_json::to_string(&mgr).expect("serialize");
        let restored: DailyMissionManager = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.generated_date, mgr.generated_date);
        assert_eq!(restored.missions.len(), mgr.missions.len());
        for (a, b) in restored.missions.iter().zip(mgr.missions.iter()) {
            assert_eq!(a.id, b.id);
            assert_eq!(a.current, b.current);
            assert_eq!(a.target, b.target);
            assert_eq!(a.claimed, b.claimed);
        }
        assert_eq!(
            restored.unique_categories_today,
            mgr.unique_categories_today
        );
    }
}
