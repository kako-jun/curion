mod achievement;
mod curion;
mod generator;
mod player;
mod save;
mod ui;

use anyhow::Result;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::{Duration, Instant};

use crate::save::SaveManager;
use crate::ui::App;

fn main() -> Result<()> {
    // セーブマネージャーを初期化
    let save_manager = SaveManager::new()?;

    // ゲーム状態をロード（存在しない場合は新規作成）
    let game_state = save_manager.load()?;

    // ターミナルのセットアップ
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(game_state);

    // メインループ
    let tick_rate = Duration::from_millis(250);
    let res = run_app(&mut terminal, &mut app, tick_rate, &save_manager);

    // ターミナルを元に戻す
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // 終了時にセーブ
    if let Err(err) = save_manager.save(&app.game_state) {
        eprintln!("Failed to save game state: {:?}", err);
    }

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    Ok(())
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    tick_rate: Duration,
    save_manager: &SaveManager,
) -> Result<()> {
    let mut last_tick = Instant::now();
    let mut last_save = Instant::now();
    let auto_save_interval = Duration::from_secs(60); // 1分ごとに自動セーブ

    loop {
        terminal.draw(|f| ui::draw(f, app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                    KeyCode::Char('1') => app.set_tab(0),
                    KeyCode::Char('2') => app.set_tab(1),
                    KeyCode::Char('3') => app.set_tab(2),
                    KeyCode::Char('4') => app.set_tab(3),
                    KeyCode::Tab => app.next_tab(),
                    KeyCode::Char(' ') => app.generate_curion()?,
                    KeyCode::Char('s') => {
                        // 手動セーブ
                        save_manager.save(&app.game_state)?;
                        app.show_save_message();
                    }
                    KeyCode::Up | KeyCode::Char('k') => app.scroll_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.scroll_down(),
                    KeyCode::Enter => app.handle_enter()?,
                    _ => {}
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            app.on_tick();
            last_tick = Instant::now();
        }

        // 自動セーブ
        if last_save.elapsed() >= auto_save_interval {
            save_manager.save(&app.game_state)?;
            last_save = Instant::now();
        }
    }
}
