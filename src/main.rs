mod achievement;
mod cooldown;
mod curion;
mod daily_mission;
mod equipment;
mod evolution;
mod generator;
mod interactive;
mod latent;
mod nostr_identity;
mod plain;
mod player;
mod san;
mod save;
mod semantic;
mod synthesis;
mod ui;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::{Duration, Instant};

use crate::nostr_identity::ProfileManager;
use crate::save::SaveManager;
use crate::ui::App;

/// Curion - A SF collection game where you gather particles of curiosity
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Profile name for multi-player debug testing
    #[arg(short, long)]
    profile: Option<String>,

    /// Start in interactive (REPL) mode instead of TUI
    #[arg(short, long)]
    interactive: bool,

    /// Plain text mode (no TUI). Useful for debugging and scripting.
    /// Usage: curion --plain [status|collect [n]|collection|achievements|synthesize <i> <j>|help]
    #[arg(long)]
    plain: bool,

    /// Subcommand args for --plain mode
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    plain_args: Vec<String>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let profile_manager = ProfileManager::new(args.profile)?;
    let identity = profile_manager.load_or_generate_identity()?;

    println!(
        "🎮 Starting Curion with profile: {}",
        profile_manager.profile_name()
    );
    println!("🔑 Your public key: {}", identity.public_key);

    if args.interactive {
        // インタラクティブ（REPL）モード
        interactive::run_interactive_mode(&profile_manager)?;
    } else if args.plain {
        // プレーンテキストモード（TUI なし、デバッグ用）
        plain::run_plain_mode(&profile_manager, &args.plain_args)?;
    } else {
        // TUIモード（デフォルト）
        run_tui(&profile_manager)?;
    }

    Ok(())
}

/// TUIモードを起動する
pub fn run_tui(profile_manager: &ProfileManager) -> Result<()> {
    let save_manager = SaveManager::new_with_profile(profile_manager)?;
    let mut game_state = save_manager.load()?;
    let login_bonus = game_state.process_login();
    // Issue #30: 起動時に期限切れキュリオンを自動削除し、UI に通知メッセージを残す。
    let expired = game_state.prune_expired_curions(chrono::Utc::now());
    save_manager.save(&game_state)?;

    // Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(game_state);
    // 起動時に達成済みデイリーミッションがあれば自動受取（前回終了時に達成→未受取で残っていたケース）
    app.flush_daily_mission_rewards();
    if let Some(reward) = login_bonus {
        app.show_login_bonus_message(&reward);
    }
    // Issue #30: 期限切れで削除されたキュリオンを 1 行トーストで知らせる。
    if !expired.is_empty() {
        app.show_expired_curions_message(&expired);
    }

    let res = run_app(&mut terminal, &mut app, &save_manager);

    // Terminal teardown
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Save on exit
    if let Err(err) = save_manager.save(&app.game_state) {
        eprintln!("Failed to save game state: {err:?}");
    }

    if let Err(err) = res {
        eprintln!("Error: {err:?}");
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    save_manager: &SaveManager,
) -> Result<()> {
    let tick_rate = Duration::from_millis(250);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| app.ui(f))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if app.handle_key(key.code)? {
                    return Ok(());
                }
                // Issue #62: mutation 駆動の即時永続化。
                // key event を契機に状態が変わったときだけ save する (ポーリングは廃止)。
                // dirty を立てる点は src/ui.rs の各 handler 側で管理する。
                if app.dirty {
                    save_manager.save(&app.game_state)?;
                    app.dirty = false;
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
            // Issue #62: on_tick は guid_timer 満了で generate_curion を内部呼出するため
            // 間接的に dirty が立つことがある。key event を待たずに save しないと、
            // 無操作中に取得した curion がハードシャットダウンで失われる。
            if app.dirty {
                save_manager.save(&app.game_state)?;
                app.dirty = false;
            }
        }
    }
}
