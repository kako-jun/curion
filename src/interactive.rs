use anyhow::Result;
use rustyline::completion::{Completer, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::validate::Validator;
use rustyline::{Context, Editor, Helper};
use std::borrow::Cow;
use std::time::Instant;
use uuid::Uuid;

use crate::curion::{Category, Rarity};
use crate::generator::CurionGenerator;
use crate::nostr_identity::ProfileManager;
use crate::player::GameState;
use crate::save::SaveManager;
use crate::synthesis::SynthesisAttemptResult;

/// 静的コマンド一覧
const COMMANDS: &[&str] = &[
    "generate",
    "gen",
    "synthesize",
    "synth",
    "collection",
    "col",
    "achievements",
    "ach",
    "stats",
    "tui",
    "help",
    "?",
    "exit",
    "quit",
];

/// カテゴリ名一覧（補完用）
const CATEGORY_NAMES: &[&str] = &[
    "Animals",
    "Plants",
    "Colors",
    "Objects",
    "Concepts",
    "Elements",
    "Foods",
    "Phenomena",
    "Abstracts",
];

/// CurionHelper: rustyline の補完・ヒント・ハイライトを提供
pub struct CurionHelper {
    commands: Vec<String>,
    /// 動的に更新されるキュリオン名（合成の素材選択用）
    curion_names: Vec<String>,
}

impl CurionHelper {
    pub fn new() -> Self {
        let mut commands: Vec<String> = COMMANDS.iter().map(|s| s.to_string()).collect();
        for name in CATEGORY_NAMES {
            commands.push(name.to_string());
        }
        CurionHelper {
            commands,
            curion_names: Vec::new(),
        }
    }

    /// 所持キュリオン名を更新する
    pub fn update_curion_names(&mut self, names: Vec<String>) {
        self.curion_names = names;
    }
}

impl Completer for CurionHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        _ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let input = &line[..pos];

        // スペースが含まれている場合、最後の単語だけを補完対象にする
        // （例: "synth Wa" → "Wa" を補完）
        let (start, word) = if let Some(space_pos) = input.rfind(' ') {
            (space_pos + 1, &input[space_pos + 1..])
        } else {
            (0, input)
        };

        if word.is_empty() && start > 0 {
            // コマンドの後にスペースがある場合、キュリオン名を候補にする
            let candidates: Vec<Pair> = self
                .curion_names
                .iter()
                .map(|name| Pair {
                    display: name.clone(),
                    replacement: name.clone(),
                })
                .collect();
            return Ok((start, candidates));
        }

        let candidates: Vec<Pair> = if start == 0 {
            // 最初の単語 → コマンド補完
            self.commands
                .iter()
                .filter(|cmd| cmd.starts_with(word))
                .map(|cmd| Pair {
                    display: cmd.clone(),
                    replacement: cmd.clone(),
                })
                .collect()
        } else {
            // 2番目以降 → キュリオン名 + カテゴリ名を補完
            let mut pairs: Vec<Pair> = Vec::new();
            for name in &self.curion_names {
                if name.starts_with(word) {
                    pairs.push(Pair {
                        display: name.clone(),
                        replacement: name.clone(),
                    });
                }
            }
            for name in CATEGORY_NAMES {
                if name.starts_with(word) {
                    pairs.push(Pair {
                        display: name.to_string(),
                        replacement: name.to_string(),
                    });
                }
            }
            pairs
        };

        Ok((start, candidates))
    }
}

impl Hinter for CurionHelper {
    type Hint = String;

    fn hint(&self, line: &str, pos: usize, _ctx: &Context<'_>) -> Option<String> {
        if pos < line.len() {
            return None;
        }

        // 最初の単語のみヒントを出す（コマンド補完）
        if !line.contains(' ') {
            for cmd in &self.commands {
                if cmd.starts_with(line) && cmd != line {
                    return Some(cmd[line.len()..].to_string());
                }
            }
        }

        None
    }
}

impl Highlighter for CurionHelper {
    fn highlight<'l>(&self, line: &'l str, _pos: usize) -> Cow<'l, str> {
        // 入力の最初の単語がコマンドにマッチしたらシアン色
        let first_word = line.split_whitespace().next().unwrap_or("");
        if self.commands.iter().any(|c| c == first_word) {
            Cow::Owned(format!("\x1b[1;36m{line}\x1b[0m"))
        } else {
            Cow::Borrowed(line)
        }
    }

    fn highlight_char(&self, _line: &str, _pos: usize, _forced: bool) -> bool {
        true
    }
}

impl Validator for CurionHelper {}

impl Helper for CurionHelper {}

/// インタラクティブモードを起動する
pub fn run_interactive_mode(profile_manager: &ProfileManager) -> Result<()> {
    let save_manager = SaveManager::new_with_profile(profile_manager)?;
    let mut game_state = save_manager.load()?;
    let generator = CurionGenerator::new()?;

    // ログイン処理
    let login_bonus = game_state.process_login();
    // Issue #30: 期限切れキュリオンを削除して通知する。
    let expired = game_state.prune_expired_curions(chrono::Utc::now());
    save_manager.save(&game_state)?;

    let mut helper = CurionHelper::new();
    update_helper_names(&mut helper, &game_state);

    let mut rl = Editor::new()?;
    rl.set_helper(Some(helper));

    let history_file = dirs::home_dir()
        .map(|mut path| {
            path.push(".curion_history");
            path
        })
        .unwrap_or_else(|| std::path::PathBuf::from(".curion_history"));

    let _ = rl.load_history(&history_file);

    println!("\x1b[1;36mcurion\x1b[0m - A SF collection game");
    println!(
        "Type '\x1b[1;33mhelp\x1b[0m' for available commands, '\x1b[1;33mexit\x1b[0m' to quit\n"
    );

    if let Some(reward) = login_bonus {
        println!("\x1b[1;32mLogin Bonus\x1b[0m");
        for line in reward.summary_lines() {
            println!("  {line}");
        }
        println!();
    }

    // Issue #30: 期限切れで消えたキュリオンを通知
    if !expired.is_empty() {
        println!(
            "\x1b[1;33m寿命で消えたキュリオン ({} 個)\x1b[0m",
            expired.len()
        );
        for c in &expired {
            println!("  - {}", c.display_name());
        }
        println!();
    }

    // 起動時に簡易ステータスを表示
    print_brief_status(&game_state);

    let session_start = Instant::now();

    loop {
        let readline = rl.readline("\x1b[1;36mcurion>\x1b[0m ");
        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                let _ = rl.add_history_entry(trimmed);

                let parts: Vec<&str> = trimmed.split_whitespace().collect();
                let cmd = parts[0];

                match cmd {
                    "exit" | "quit" => {
                        // プレイ時間を加算
                        game_state
                            .player
                            .add_play_time(session_start.elapsed().as_secs());
                        // 終了時にセーブ
                        if let Err(e) = save_manager.save(&game_state) {
                            eprintln!("Save failed: {e:?}");
                        } else {
                            println!("Game saved.");
                        }
                        println!("Goodbye!");
                        break;
                    }
                    "help" | "?" => {
                        show_help();
                    }
                    "generate" | "gen" => {
                        cmd_generate(&generator, &mut game_state)?;
                        // ヘルパーのキュリオン名を更新
                        if let Some(h) = rl.helper_mut() {
                            update_helper_names(h, &game_state);
                        }
                    }
                    "synthesize" | "synth" => {
                        if parts.len() < 3 {
                            println!("Usage: synth <name1> <name2>");
                            println!("  Example: synth 水 火");
                        } else {
                            cmd_synthesize(parts[1], parts[2], &mut game_state)?;
                            // ヘルパーのキュリオン名を更新
                            if let Some(h) = rl.helper_mut() {
                                update_helper_names(h, &game_state);
                            }
                        }
                    }
                    "collection" | "col" => {
                        cmd_collection(&game_state);
                    }
                    "achievements" | "ach" => {
                        cmd_achievements(&game_state);
                    }
                    "stats" => {
                        cmd_stats(&game_state, session_start.elapsed().as_secs());
                    }
                    "tui" => {
                        // プレイ時間を加算してセーブ
                        game_state
                            .player
                            .add_play_time(session_start.elapsed().as_secs());
                        if let Err(e) = save_manager.save(&game_state) {
                            eprintln!("Save failed: {e:?}");
                        }
                        println!("Switching to TUI mode...");
                        let _ = rl.save_history(&history_file);
                        // TUI モードを起動
                        crate::run_tui(profile_manager)?;
                        return Ok(());
                    }
                    _ => {
                        println!("Unknown command: {cmd}. Type 'help' for available commands.");
                    }
                }
            }
            Err(ReadlineError::Interrupted) => {
                println!("^C");
                continue;
            }
            Err(ReadlineError::Eof) => {
                // Ctrl-D: セーブして終了
                game_state
                    .player
                    .add_play_time(session_start.elapsed().as_secs());
                if let Err(e) = save_manager.save(&game_state) {
                    eprintln!("Save failed: {e:?}");
                } else {
                    println!("Game saved.");
                }
                println!("^D");
                break;
            }
            Err(err) => {
                eprintln!("Error: {err:?}");
                break;
            }
        }
    }

    let _ = rl.save_history(&history_file);
    Ok(())
}

/// ヘルパーのキュリオン名を更新
fn update_helper_names(helper: &mut CurionHelper, game_state: &GameState) {
    let mut names: Vec<String> = game_state
        .player
        .collection
        .iter()
        .map(|c| c.noun.clone())
        .collect();
    names.sort();
    names.dedup();
    helper.update_curion_names(names);
}

/// 簡易ステータス表示
fn print_brief_status(game_state: &GameState) {
    let player = &game_state.player;
    println!(
        "  Level: {}  XP: {}/{}  Collection: {}  Achievements: {}/{}",
        player.level,
        player.xp,
        player.xp_for_next_level(),
        player.total_acquired(),
        game_state.achievement_manager.get_unlocked_count(),
        game_state.achievement_manager.get_total_count(),
    );
    println!();
}

/// generate コマンド
fn cmd_generate(generator: &CurionGenerator, game_state: &mut GameState) -> Result<()> {
    let guid = Uuid::new_v4();
    let curion = generator.generate_from_guid(guid)?;

    let rarity_color = match curion.rarity {
        Rarity::Common => "\x1b[37m",    // white
        Rarity::Rare => "\x1b[34m",      // blue
        Rarity::Epic => "\x1b[35m",      // magenta
        Rarity::Legendary => "\x1b[33m", // yellow
    };

    println!(
        "  Generated: {}{:?}\x1b[0m {} [{}] (interest: {:.0}%, beauty: {:.0}%)",
        rarity_color,
        curion.rarity,
        curion.noun,
        curion.category.as_str(),
        curion.interest * 100.0,
        curion.beauty * 100.0,
    );

    let newly_unlocked = game_state.add_curion(curion);
    for achievement_id in &newly_unlocked {
        println!("  \x1b[1;33m★ Achievement unlocked: {achievement_id}\x1b[0m");
    }

    Ok(())
}

/// synthesize コマンド
fn cmd_synthesize(name1: &str, name2: &str, game_state: &mut GameState) -> Result<()> {
    // 名前からキュリオンを検索
    let first = game_state
        .player
        .collection
        .iter()
        .find(|c| c.noun == name1)
        .cloned();
    let second = game_state
        .player
        .collection
        .iter()
        .find(|c| {
            c.noun == name2 && {
                // 同名の場合は別のインスタンスを選ぶ
                if let Some(ref f) = first {
                    c.id != f.id
                } else {
                    true
                }
            }
        })
        .cloned();

    let first = match first {
        Some(c) => c,
        None => {
            println!("  '{name1}' is not in your collection.");
            return Ok(());
        }
    };
    let second = match second {
        Some(c) => c,
        None => {
            println!("  '{name2}' is not in your collection.");
            return Ok(());
        }
    };

    let first_id = first.id.clone();
    let second_id = second.id.clone();
    let ingredients = vec![first, second];

    match game_state.synthesis_manager.try_synthesize(ingredients)? {
        SynthesisAttemptResult::Success {
            curion,
            recipe_name,
            first_discovery,
        } => {
            // 使用した材料を削除
            game_state
                .player
                .collection
                .retain(|c| c.id != first_id && c.id != second_id);

            if first_discovery {
                println!("  \x1b[1;33m✨ New discovery: {recipe_name}!\x1b[0m");
            }

            let rarity_color = match curion.rarity {
                Rarity::Common => "\x1b[37m",
                Rarity::Rare => "\x1b[34m",
                Rarity::Epic => "\x1b[35m",
                Rarity::Legendary => "\x1b[33m",
            };

            println!(
                "  Result: {}{:?}\x1b[0m {} [{}]",
                rarity_color,
                curion.rarity,
                curion.noun,
                curion.category.as_str(),
            );

            game_state.add_curion(curion);
        }
        SynthesisAttemptResult::NoRecipe => {
            println!("  No recipe found for {name1} + {name2}.");
        }
        SynthesisAttemptResult::DiscoveryFailed { hint } => {
            println!("  {hint}");
        }
        SynthesisAttemptResult::HighRiskFailure {
            recipe_name,
            lost_ingredients,
            salvage,
            failure_mode,
        } => {
            // Issue #35: 高リスク合成失敗の演出 (赤表示)
            // 失われた材料を collection から除去 (NoLoss モードでは lost_ingredients が空)
            for ci in &lost_ingredients {
                game_state.player.collection.retain(|c| c.id != ci.id);
            }
            let mode_label = match failure_mode {
                crate::synthesis::FailureMode::LoseAll => "素材消滅",
                crate::synthesis::FailureMode::Salvage { .. } => "残骸を獲得",
                crate::synthesis::FailureMode::NoLoss => "保険発動",
            };
            println!("  \x1b[1;31m💥 失敗: {recipe_name} ({mode_label})\x1b[0m");
            if let Some(s) = salvage {
                println!(
                    "  Salvage: \x1b[37m{:?}\x1b[0m {} [{}]",
                    s.rarity,
                    s.noun,
                    s.category.as_str(),
                );
                game_state.add_curion(s);
            }
        }
    }

    Ok(())
}

/// collection コマンド
fn cmd_collection(game_state: &GameState) {
    let collection = &game_state.player.collection;
    if collection.is_empty() {
        println!("  Your collection is empty. Try 'generate' to get your first curion!");
        return;
    }

    // カテゴリ別にグループ化
    let categories = [
        Category::Animal,
        Category::Plant,
        Category::Color,
        Category::Object,
        Category::Concept,
        Category::Element,
        Category::Food,
        Category::Phenomenon,
        Category::Abstract,
    ];

    println!(
        "  \x1b[1;36m=== Collection ({} total) ===\x1b[0m",
        collection.len()
    );

    for category in &categories {
        let items: Vec<_> = collection
            .iter()
            .filter(|c| &c.category == category)
            .collect();
        if items.is_empty() {
            continue;
        }

        println!("  \x1b[1m[{}]\x1b[0m ({})", category.as_str(), items.len());
        for curion in &items {
            let rarity_color = match curion.rarity {
                Rarity::Common => "\x1b[37m",
                Rarity::Rare => "\x1b[34m",
                Rarity::Epic => "\x1b[35m",
                Rarity::Legendary => "\x1b[33m",
            };
            println!(
                "    {}{:?}\x1b[0m {}",
                rarity_color, curion.rarity, curion.noun
            );
        }
    }
}

/// achievements コマンド
fn cmd_achievements(game_state: &GameState) {
    let manager = &game_state.achievement_manager;

    println!(
        "  \x1b[1;36m=== Achievements ({}/{}) ===\x1b[0m",
        manager.get_unlocked_count(),
        manager.get_total_count(),
    );

    let sorted = manager.get_sorted_by_progress();
    for (achievement, progress) in sorted {
        let status = if progress.unlocked {
            if progress.claimed {
                "\x1b[32m✓\x1b[0m"
            } else {
                "\x1b[33m★\x1b[0m"
            }
        } else {
            " "
        };

        println!(
            "  {} {} {} ({}/{}  {:.0}%)",
            status,
            achievement.icon,
            achievement.name,
            progress.current,
            progress.target,
            progress.progress_ratio() * 100.0,
        );
    }
}

/// stats コマンド
fn cmd_stats(game_state: &GameState, session_seconds: u64) {
    let player = &game_state.player;
    let total_time = player.total_play_time + session_seconds;
    let hours = total_time / 3600;
    let minutes = (total_time % 3600) / 60;

    println!("  \x1b[1;36m=== Stats ===\x1b[0m");
    println!("  Level: {}", player.level);
    println!("  XP: {}/{}", player.xp, player.xp_for_next_level());
    println!("  Total collected: {}", player.total_acquired());
    println!("  Play time: {hours}h {minutes}m");
    println!("  Days played: {}", player.days_played());
    println!(
        "  Consecutive login: {} days",
        player.consecutive_login_days
    );
    println!("  Today acquired: {}", player.today_acquired);
    println!("  Avg daily: {:.1}", player.average_daily_acquired());

    println!("  \x1b[1m--- Rarity Distribution ---\x1b[0m");
    for rarity in [
        Rarity::Common,
        Rarity::Rare,
        Rarity::Epic,
        Rarity::Legendary,
    ] {
        let count = player.count_by_rarity(rarity);
        let rarity_color = match rarity {
            Rarity::Common => "\x1b[37m",
            Rarity::Rare => "\x1b[34m",
            Rarity::Epic => "\x1b[35m",
            Rarity::Legendary => "\x1b[33m",
        };
        println!("    {rarity_color}{rarity:?}\x1b[0m: {count}");
    }

    println!("  \x1b[1m--- Recipes ---\x1b[0m");
    println!(
        "  Discovered: {}/{}",
        game_state.synthesis_manager.discovered_count(),
        game_state.synthesis_manager.total_recipe_count(),
    );
}

/// ヘルプ表示
fn show_help() {
    println!("\n\x1b[1;36mAvailable Commands:\x1b[0m");
    println!("  \x1b[1;33mgenerate\x1b[0m (gen)           Generate a new curion");
    println!("  \x1b[1;33msynthesize\x1b[0m (synth) <a> <b>  Synthesize two curions");
    println!("  \x1b[1;33mcollection\x1b[0m (col)         Show your collection");
    println!("  \x1b[1;33machievements\x1b[0m (ach)       Show achievements");
    println!("  \x1b[1;33mstats\x1b[0m                    Show statistics");
    println!("  \x1b[1;33mtui\x1b[0m                      Switch to TUI mode");
    println!("  \x1b[1;33mhelp\x1b[0m, \x1b[1;33m?\x1b[0m                 Show this help");
    println!("  \x1b[1;33mexit\x1b[0m, \x1b[1;33mquit\x1b[0m              Exit the game");
    println!("\n\x1b[1;36mTips:\x1b[0m");
    println!("  - Use \x1b[1;33mTab\x1b[0m for auto-completion (commands & curion names)");
    println!("  - Use \x1b[1;33m↑/↓\x1b[0m arrows for command history");
    println!();
}
