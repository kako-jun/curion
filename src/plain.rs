//! --plain モード: TUI なしで全操作をプレーンテキスト出力する。
//! フリーザ様（Claude）がデバッグ・テストに使う。
//!
//! Usage:
//!   curion --plain status         # 現在の状態を表示
//!   curion --plain collect        # キュリオンを1個生成（タイマー無視）
//!   curion --plain collect 5      # キュリオンを5個生成
//!   curion --plain collection     # コレクション一覧
//!   curion --plain achievements   # 実績一覧
//!   curion --plain synthesize <idx1> <idx2>  # コレクション番号で合成
//!   curion --plain help           # コマンド一覧

use anyhow::Result;

use crate::curion::Rarity;
use crate::generator::CurionGenerator;
use crate::nostr_identity::ProfileManager;
use crate::save::SaveManager;

/// --plain モードのエントリポイント
pub fn run_plain_mode(profile_manager: &ProfileManager, args: &[String]) -> Result<()> {
    let save_manager = SaveManager::new_with_profile(profile_manager)?;
    let mut game_state = save_manager.load()?;
    let login_bonus = game_state.process_login();
    // Issue #30: 期限切れキュリオンを起動時に削除し、ユーザーに通知する。
    let expired = game_state.prune_expired_curions(chrono::Utc::now());
    save_manager.save(&game_state)?;
    let generator = CurionGenerator::new()?;

    if let Some(reward) = &login_bonus {
        println!("=== Login Bonus ===");
        for line in reward.summary_lines() {
            println!("  {line}");
        }
        println!();
    }

    if !expired.is_empty() {
        println!("=== 寿命で消えたキュリオン ({} 個) ===", expired.len());
        for c in &expired {
            println!("  - [{}] {}", rarity_tag(c.rarity), c.display_name());
        }
        println!();
    }

    let cmd = args.first().map(|s| s.as_str()).unwrap_or("status");

    match cmd {
        "status" | "s" => {
            cmd_status(&game_state);
        }

        "collect" | "c" => {
            let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
            for _ in 0..n {
                let guid = uuid::Uuid::new_v4();
                let curion = generator.generate_from_guid(guid)?;
                let rarity_label = rarity_tag(curion.rarity);
                println!(
                    "[{}] {} ({}) - interest:{:.2} beauty:{:.2}",
                    rarity_label,
                    curion.noun,
                    curion.category.as_str(),
                    curion.interest,
                    curion.beauty
                );
                game_state.add_curion(curion);
            }
            save_manager.save(&game_state)?;
            println!("--- saved ({} total)", game_state.player.total_acquired());
        }

        "collection" | "col" => {
            cmd_collection(&game_state);
        }

        "achievements" | "ach" | "a" => {
            cmd_achievements(&game_state);
        }

        "synthesize" | "syn" => {
            let idx1: usize = args
                .get(1)
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or_else(|| anyhow::anyhow!("Usage: synthesize <idx1> <idx2>"))?
                .saturating_sub(1);
            let idx2: usize = args
                .get(2)
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or_else(|| anyhow::anyhow!("Usage: synthesize <idx1> <idx2>"))?
                .saturating_sub(1);

            let col_len = game_state.player.collection.len();
            if idx1 >= col_len || idx2 >= col_len {
                println!("Error: index out of range (collection has {col_len} items)");
                return Ok(());
            }
            if idx1 == idx2 {
                println!("Error: cannot synthesize an item with itself");
                return Ok(());
            }

            let a = game_state.player.collection[idx1].clone();
            let b = game_state.player.collection[idx2].clone();
            let a_noun = a.noun.clone();
            let b_noun = b.noun.clone();

            use crate::synthesis::SynthesisAttemptResult;
            match game_state.synthesis_manager.try_synthesize(vec![a, b])? {
                SynthesisAttemptResult::Success {
                    curion,
                    recipe_name,
                    first_discovery,
                } => {
                    let flag = if first_discovery {
                        " [FIRST DISCOVERY!]"
                    } else {
                        ""
                    };
                    println!(
                        "[SUCCESS]{} {} + {} => {} [{}] (recipe: {})",
                        flag,
                        a_noun,
                        b_noun,
                        curion.noun,
                        rarity_tag(curion.rarity),
                        recipe_name
                    );
                    game_state.add_curion(curion);
                    // 素材を削除（大きいインデックスから）
                    let (hi, lo) = if idx1 > idx2 {
                        (idx1, idx2)
                    } else {
                        (idx2, idx1)
                    };
                    game_state.player.collection.remove(hi);
                    game_state.player.collection.remove(lo);
                    save_manager.save(&game_state)?;
                }
                SynthesisAttemptResult::DiscoveryFailed { hint } => {
                    println!("[HINT] {hint}");
                }
                SynthesisAttemptResult::NoRecipe => {
                    println!("[NO RECIPE] no matching recipe for {a_noun} + {b_noun}");
                }
                SynthesisAttemptResult::HighRiskFailure {
                    recipe_name,
                    lost_ingredients,
                    salvage,
                    failure_mode,
                } => {
                    // Issue #35: 高リスク失敗。LoseAll/Salvage の場合は素材を削除する。
                    // インデックスベースで削除すると順序が変わるので、id ベースで除去する。
                    for ci in &lost_ingredients {
                        game_state.player.collection.retain(|c| c.id != ci.id);
                    }
                    let mode_label = match &failure_mode {
                        crate::synthesis::FailureMode::LoseAll => "lose_all",
                        crate::synthesis::FailureMode::Salvage { .. } => "salvage",
                        crate::synthesis::FailureMode::NoLoss => "no_loss",
                    };
                    println!(
                        "[HIGH-RISK FAIL] {a_noun} + {b_noun} => x (recipe: {recipe_name}, mode: {mode_label})"
                    );
                    if let Some(s) = salvage {
                        println!("  Salvage: {} [{}]", s.noun, rarity_tag(s.rarity),);
                        game_state.add_curion(s);
                    }
                    save_manager.save(&game_state)?;
                }
            }
        }

        "help" | "h" | "--help" => {
            println!("curion --plain <command> [args]");
            println!();
            println!("  status                     Show current state");
            println!("  collect [n]                Generate n curions (default: 1)");
            println!("  collection                 List collected curions");
            println!("  achievements               List achievements");
            println!("  synthesize <idx1> <idx2>   Synthesize two curions by index");
            println!("  help                       Show this help");
        }

        other => {
            println!("Unknown command: {other}. Try: curion --plain help");
        }
    }

    Ok(())
}

fn cmd_status(game_state: &crate::player::GameState) {
    let p = &game_state.player;
    println!("=== Curion Status ===");
    println!("Level       : {}", p.level);
    println!("XP          : {} / {}", p.xp, p.xp_for_next_level());
    println!("Total       : {}", p.total_acquired());
    println!("Unique      : {}", unique_count(p));
    println!("Login streak: {} days", p.consecutive_login_days);
    println!(
        "Claimed today: {}",
        if p.login_bonus_claimed_today {
            "yes"
        } else {
            "no"
        }
    );
    println!(
        "Tickets     : common={} rare={} epic={}",
        p.guaranteed_tickets.common, p.guaranteed_tickets.rare, p.guaranteed_tickets.epic
    );
    println!();
    println!("--- Rarity breakdown ---");
    for rarity in [
        Rarity::Legendary,
        Rarity::Epic,
        Rarity::Rare,
        Rarity::Common,
    ] {
        let count = p.count_by_rarity(rarity);
        if count > 0 {
            println!("  {:10} : {}", rarity_tag(rarity), count);
        }
    }
    println!();
    println!("--- Almost complete achievements ---");
    for (name, _progress, ratio) in game_state.get_almost_complete_achievements(5) {
        println!("  [{:3.0}%] {}", ratio * 100.0, name);
    }
}

fn cmd_collection(game_state: &crate::player::GameState) {
    let col = &game_state.player.collection;
    if col.is_empty() {
        println!("(empty)");
        return;
    }
    println!(
        "{:>4}  {:12}  {:10}  {:9}  int    bty",
        "#", "noun", "category", "rarity"
    );
    println!("{}", "-".repeat(60));
    for (i, c) in col.iter().enumerate() {
        println!(
            "{:>4}  {:12}  {:10}  {:9}  {:.2}   {:.2}",
            i + 1,
            c.noun,
            c.category.as_str(),
            rarity_tag(c.rarity),
            c.interest,
            c.beauty,
        );
    }
}

fn cmd_achievements(game_state: &crate::player::GameState) {
    let all = game_state.achievement_manager.get_all_achievements();
    let achievable = game_state.achievement_manager.get_achievable();
    println!("=== Achievements ({} total) ===", all.len());
    println!();
    println!("--- Claimable now ---");
    if achievable.is_empty() {
        println!("  (none)");
    }
    for (ach, _) in &achievable {
        println!("  [READY] {}: {}", ach.id, ach.name);
    }
    println!();
    println!("--- In progress (top 10) ---");
    for (name, progress, ratio) in game_state.get_almost_complete_achievements(10) {
        println!(
            "  [{:3.0}%] {} (current: {})",
            ratio * 100.0,
            name,
            progress.current
        );
    }
}

fn rarity_tag(rarity: Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "COM",
        Rarity::Rare => "RARE",
        Rarity::Epic => "EPIC",
        Rarity::Legendary => "LGND",
    }
}

fn unique_count(player: &crate::player::Player) -> usize {
    use std::collections::HashSet;
    player
        .collection
        .iter()
        .map(|c| &c.noun)
        .collect::<HashSet<_>>()
        .len()
}
