use anyhow::Result;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Tabs},
    Frame,
};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::curion::Rarity;
use crate::generator::CurionGenerator;
use crate::player::GameState;

/// タブの種類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Dashboard,
    Collection,
    Achievements,
    Stats,
    Synthesis,
}

impl Tab {
    pub fn title(&self) -> &str {
        match self {
            Tab::Dashboard => "Dashboard",
            Tab::Collection => "Collection",
            Tab::Achievements => "Achievements",
            Tab::Stats => "Stats",
            Tab::Synthesis => "Synthesis",
        }
    }
}

/// 合成UI状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthesisUIState {
    SelectingFirst,  // 1つ目の材料を選択中
    SelectingSecond, // 2つ目の材料を選択中
}

/// アプリケーション状態
pub struct App {
    pub game_state: GameState,
    pub current_tab: Tab,
    pub scroll: usize,
    pub guid_timer: Instant,
    pub guid_interval: Duration,
    pub generator: CurionGenerator,
    pub save_message: Option<(String, Instant)>,
    // 合成UI用
    pub synthesis_state: SynthesisUIState,
    pub selected_first_curion: Option<usize>, // コレクションのインデックス
    pub synthesis_scroll: usize,
}

impl App {
    pub fn new(game_state: GameState) -> Self {
        let generator = CurionGenerator::new("data/nouns")
            .expect("Failed to load noun database");

        Self {
            game_state,
            current_tab: Tab::Dashboard,
            scroll: 0,
            guid_timer: Instant::now(),
            guid_interval: Duration::from_secs(30), // 30秒ごと
            generator,
            save_message: None,
            synthesis_state: SynthesisUIState::SelectingFirst,
            selected_first_curion: None,
            synthesis_scroll: 0,
        }
    }

    pub fn set_tab(&mut self, index: usize) {
        self.current_tab = match index {
            0 => Tab::Dashboard,
            1 => Tab::Collection,
            2 => Tab::Achievements,
            3 => Tab::Stats,
            4 => Tab::Synthesis,
            _ => self.current_tab,
        };
        self.scroll = 0;
    }

    pub fn next_tab(&mut self) {
        self.current_tab = match self.current_tab {
            Tab::Dashboard => Tab::Collection,
            Tab::Collection => Tab::Achievements,
            Tab::Achievements => Tab::Stats,
            Tab::Stats => Tab::Synthesis,
            Tab::Synthesis => Tab::Dashboard,
        };
        self.scroll = 0;
    }

    pub fn scroll_up(&mut self) {
        if self.current_tab == Tab::Synthesis && self.synthesis_state == SynthesisUIState::SelectingSecond {
            self.synthesis_scroll = self.synthesis_scroll.saturating_sub(1);
        } else {
            self.scroll = self.scroll.saturating_sub(1);
        }
    }

    pub fn scroll_down(&mut self) {
        if self.current_tab == Tab::Synthesis && self.synthesis_state == SynthesisUIState::SelectingSecond {
            self.synthesis_scroll = self.synthesis_scroll.saturating_add(1);
        } else {
            self.scroll = self.scroll.saturating_add(1);
        }
    }

    pub fn handle_enter(&mut self) -> Result<()> {
        match self.current_tab {
            Tab::Achievements => {
                // 報酬受け取り処理
                let achievable = self.game_state.achievement_manager.get_achievable();
                if let Some((achievement, _)) = achievable.get(self.scroll) {
                    let achievement_id = achievement.id.clone();
                    self.game_state.claim_achievement_reward(&achievement_id);
                }
            }
            Tab::Synthesis => {
                self.handle_synthesis_enter()?;
            }
            _ => {}
        }
        Ok(())
    }

    pub fn handle_escape(&mut self) {
        if self.current_tab == Tab::Synthesis {
            match self.synthesis_state {
                SynthesisUIState::SelectingSecond => {
                    // 2つ目の選択から1つ目に戻る
                    self.synthesis_state = SynthesisUIState::SelectingFirst;
                    self.selected_first_curion = None;
                    self.synthesis_scroll = 0;
                }
                _ => {}
            }
        }
    }

    fn handle_synthesis_enter(&mut self) -> Result<()> {
        use crate::synthesis::SynthesisAttemptResult;

        match self.synthesis_state {
            SynthesisUIState::SelectingFirst => {
                // 1つ目の材料を選択
                if self.scroll < self.game_state.player.collection.len() {
                    self.selected_first_curion = Some(self.scroll);
                    self.synthesis_state = SynthesisUIState::SelectingSecond;
                    self.synthesis_scroll = 0;
                }
            }
            SynthesisUIState::SelectingSecond => {
                // 2つ目の材料を選択して合成実行
                if let Some(first_idx) = self.selected_first_curion {
                    if let Some(first_curion) = self.game_state.player.collection.get(first_idx).cloned() {
                        // 候補を検索
                        let candidates = self.game_state.synthesis_manager.find_possible_second_ingredients(
                            &first_curion,
                            &self.game_state.player.collection,
                        );

                        if let Some(candidate) = candidates.get(self.synthesis_scroll) {
                            // 2つ目のキュリオンを見つける
                            if let Some(second_curion) = self.game_state.player.collection
                                .iter()
                                .find(|c| c.noun == candidate.noun && c.id != first_curion.id)
                                .cloned()
                            {
                                // 合成実行
                                let ingredients = vec![first_curion.clone(), second_curion.clone()];
                                let result = self.game_state.synthesis_manager.try_synthesize(ingredients)?;

                                match result {
                                    SynthesisAttemptResult::Success {
                                        curion,
                                        recipe_name,
                                        first_discovery,
                                    } => {
                                        // 使用した材料を削除
                                        self.game_state.player.collection.retain(|c| {
                                            c.id != first_curion.id && c.id != second_curion.id
                                        });

                                        // 結果を追加
                                        self.game_state.add_curion(curion.clone());

                                        // メッセージ表示
                                        let message = if first_discovery {
                                            format!("✨ Discovered: {}!", recipe_name)
                                        } else {
                                            format!("Created: {}", curion.noun)
                                        };
                                        self.save_message = Some((message, Instant::now()));

                                        // 状態をリセット
                                        self.synthesis_state = SynthesisUIState::SelectingFirst;
                                        self.selected_first_curion = None;
                                        self.synthesis_scroll = 0;
                                        self.scroll = 0;
                                    }
                                    SynthesisAttemptResult::DiscoveryFailed { hint } => {
                                        self.save_message = Some((hint, Instant::now()));
                                    }
                                    SynthesisAttemptResult::NoRecipe => {
                                        self.save_message = Some(("No recipe found".to_string(), Instant::now()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn generate_curion(&mut self) -> Result<()> {
        let guid = Uuid::new_v4();
        let curion = self.generator.generate_from_guid(guid)?;
        self.game_state.add_curion(curion);
        self.guid_timer = Instant::now(); // タイマーをリセット
        Ok(())
    }

    pub fn on_tick(&mut self) {
        // 自動GUID生成
        if self.guid_timer.elapsed() >= self.guid_interval {
            let _ = self.generate_curion();
        }

        // プレイ時間を更新（0.25秒ごと）
        self.game_state.player.add_play_time(1);

        // セーブメッセージの有効期限チェック（3秒）
        if let Some((_, timestamp)) = self.save_message {
            if timestamp.elapsed() > Duration::from_secs(3) {
                self.save_message = None;
            }
        }
    }

    pub fn show_save_message(&mut self) {
        self.save_message = Some(("💾 Saved!".to_string(), Instant::now()));
    }

    pub fn guid_progress(&self) -> f64 {
        let elapsed = self.guid_timer.elapsed().as_secs_f64();
        let total = self.guid_interval.as_secs_f64();
        (elapsed / total).min(1.0)
    }

    pub fn guid_remaining_seconds(&self) -> u64 {
        let elapsed = self.guid_timer.elapsed();
        self.guid_interval.saturating_sub(elapsed).as_secs()
    }
}

/// メイン描画関数
pub fn draw(f: &mut Frame<'_>, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),      // タブバー
            Constraint::Percentage(100), // メインコンテンツ
        ])
        .split(f.area());

    // タブバーを描画
    draw_tabs(f, app, chunks[0]);

    // 現在のタブに応じてコンテンツを描画
    match app.current_tab {
        Tab::Dashboard => draw_dashboard(f, app, chunks[1]),
        Tab::Collection => draw_collection(f, app, chunks[1]),
        Tab::Achievements => draw_achievements(f, app, chunks[1]),
        Tab::Stats => draw_stats(f, app, chunks[1]),
        Tab::Synthesis => draw_synthesis(f, app, chunks[1]),
    }

    // セーブメッセージを表示
    if let Some((message, _)) = &app.save_message {
        let area = Rect {
            x: f.area().width.saturating_sub(20),
            y: 1,
            width: 18,
            height: 1,
        };
        let save_text = Paragraph::new(message.as_str())
            .style(Style::default().fg(Color::Green).add_modifier(Modifier::BOLD));
        f.render_widget(save_text, area);
    }
}

/// タブバーを描画
fn draw_tabs(f: &mut Frame<'_>, app: &App, area: Rect) {
    let tabs = vec!["Dashboard", "Collection", "Achievements", "Stats", "Synthesis"];
    let index = match app.current_tab {
        Tab::Dashboard => 0,
        Tab::Collection => 1,
        Tab::Achievements => 2,
        Tab::Stats => 3,
        Tab::Synthesis => 4,
    };

    let tabs_widget = Tabs::new(tabs)
        .block(Block::default().borders(Borders::ALL).title("Curion"))
        .select(index)
        .style(Style::default().fg(Color::White))
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        );

    f.render_widget(tabs_widget, area);
}

/// ダッシュボードを描画
fn draw_dashboard(f: &mut Frame<'_>, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage(45), // 上半分：現在の状況
            Constraint::Percentage(55), // 下半分：煽り情報
        ])
        .split(area);

    draw_dashboard_top(f, app, chunks[0]);
    draw_dashboard_bottom(f, app, chunks[1]);
}

/// ダッシュボード上半分（現在の状況）
fn draw_dashboard_top(f: &mut Frame<'_>, app: &App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),  // GUID生成タイマー
            Constraint::Length(3),  // 基本統計
            Constraint::Length(2),  // 最新キュリオン
            Constraint::Length(6),  // レアリティ分布
            Constraint::Min(4),     // カテゴリ分布
        ])
        .split(area);

    // GUIDタイマー
    let progress = app.guid_progress();
    let remaining = app.guid_remaining_seconds();
    let gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title("次のキュリオン生成まで"))
        .gauge_style(Style::default().fg(Color::Cyan).bg(Color::Black))
        .percent((progress * 100.0) as u16)
        .label(format!("{}秒", remaining));
    f.render_widget(gauge, chunks[0]);

    // 基本統計
    let player = &app.game_state.player;
    let stats_text = vec![
        Line::from(vec![
            Span::styled("総獲得数: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} 個", player.total_acquired()),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled("今日の獲得: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{} 個", player.today_acquired),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled("レベル: ", Style::default().fg(Color::Gray)),
            Span::styled(
                format!("{}", player.level),
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ),
        ]),
    ];
    let stats = Paragraph::new(stats_text)
        .block(Block::default().borders(Borders::ALL).title("統計"))
        .alignment(Alignment::Left);
    f.render_widget(stats, chunks[1]);

    // 最新キュリオン
    if let Some(curion) = player.latest_curion() {
        let rarity_color = get_rarity_color(&curion.rarity);
        let rarity_stars = get_rarity_stars(&curion.rarity);
        let latest_text = vec![
            Line::from(vec![
                Span::styled(rarity_stars, Style::default().fg(rarity_color)),
                Span::raw(" "),
                Span::styled(
                    format!("[{:?}]", curion.rarity),
                    Style::default().fg(rarity_color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    curion.display_name(),
                    Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                ),
            ]),
        ];
        let latest = Paragraph::new(latest_text)
            .block(Block::default().borders(Borders::ALL).title("最新キュリオン"));
        f.render_widget(latest, chunks[2]);
    }

    // レアリティ分布
    let mut rarity_items = Vec::new();
    for rarity in [Rarity::Common, Rarity::Rare, Rarity::Epic, Rarity::Legendary] {
        let count = player.count_by_rarity(rarity);
        let percentage = if player.total_acquired() > 0 {
            (count * 100) / player.total_acquired()
        } else {
            0
        };
        let bar_width = (percentage as f64 / 100.0 * 30.0) as usize;
        let bar = "█".repeat(bar_width) + &"░".repeat(30 - bar_width);

        rarity_items.push(
            Line::from(vec![
                Span::styled(
                    format!("{:<9}", format!("{:?}", rarity)),
                    Style::default().fg(get_rarity_color(&rarity)),
                ),
                Span::raw(" "),
                Span::styled(bar, Style::default().fg(get_rarity_color(&rarity))),
                Span::raw(format!("  {} ({}%)", count, percentage)),
            ])
        );
    }
    let rarity_widget = Paragraph::new(rarity_items)
        .block(Block::default().borders(Borders::ALL).title("レアリティ分布"));
    f.render_widget(rarity_widget, chunks[3]);

    // カテゴリ分布（簡易版）
    let category_text = player.category_stats
        .iter()
        .map(|(cat, stats)| {
            format!("{}: {}個", cat.as_str(), stats.count)
        })
        .collect::<Vec<_>>()
        .join("  ");
    let category_widget = Paragraph::new(category_text)
        .block(Block::default().borders(Borders::ALL).title("カテゴリ分布"));
    f.render_widget(category_widget, chunks[4]);
}

/// ダッシュボード下半分（煽り情報）
fn draw_dashboard_bottom(f: &mut Frame<'_>, app: &App, area: Rect) {
    let almost_complete = app.game_state.get_almost_complete_achievements(10);

    let mut items: Vec<ListItem> = Vec::new();

    for (name, progress, ratio) in almost_complete {
        let remaining = progress.remaining();
        let percentage = progress.progress_percentage();
        let bar_width = ((ratio * 40.0) as usize).min(40);
        let bar = "█".repeat(bar_width) + &"░".repeat(40 - bar_width);

        // 進捗率に応じてアイコンと色を決定
        let (icon, color) = if ratio >= 0.95 {
            ("🔥", Color::Red)
        } else if ratio >= 0.80 {
            ("⭐", Color::Yellow)
        } else if ratio >= 0.50 {
            ("📦", Color::Cyan)
        } else {
            ("", Color::Gray)
        };

        let urgency = if ratio >= 0.95 {
            "緊急！ "
        } else if ratio >= 0.80 {
            "もうすぐ！ "
        } else {
            ""
        };

        items.push(ListItem::new(vec![
            Line::from(vec![
                Span::raw(icon),
                Span::raw(" "),
                Span::styled(
                    format!("{}あと {} で「{}」達成！", urgency, remaining, name),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(bar, Style::default().fg(color)),
                Span::raw(format!("  {}% ({}/{})", percentage, progress.current, progress.target)),
            ]),
            Line::from(""),
        ]));
    }

    // レベルアップ情報を追加
    let player = &app.game_state.player;
    let xp_remaining = player.xp_for_next_level() - player.xp;
    let xp_ratio = player.xp_progress_ratio();
    let xp_bar_width = ((xp_ratio * 40.0) as usize).min(40);
    let xp_bar = "█".repeat(xp_bar_width) + &"░".repeat(40 - xp_bar_width);

    items.push(ListItem::new(vec![
        Line::from(vec![
            Span::styled(
                format!("🏆 次のレベルまで: あと {} XP (Lv.{} → Lv.{})", xp_remaining, player.level, player.level + 1),
                Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::raw("    "),
            Span::styled(xp_bar, Style::default().fg(Color::Magenta)),
            Span::raw(format!("  {}% ({}/{})", player.xp_progress_percentage(), player.xp, player.xp_for_next_level())),
        ]),
    ]));

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("🎯 もうすぐ達成できる目標 (あと少し！)"),
        );

    f.render_widget(list, area);
}

/// コレクション一覧を描画
fn draw_collection(f: &mut Frame<'_>, app: &App, area: Rect) {
    let player = &app.game_state.player;
    let collection = &player.collection;

    let items: Vec<ListItem> = collection
        .iter()
        .rev()
        .enumerate()
        .skip(app.scroll)
        .take(area.height as usize - 3)
        .map(|(i, curion)| {
            let rarity_color = get_rarity_color(&curion.rarity);
            let rarity_stars = get_rarity_stars(&curion.rarity);
            let index = collection.len() - i;

            let interest_bar_width = ((curion.interest * 10.0) as usize).min(10);
            let interest_bar = "█".repeat(interest_bar_width) + &"░".repeat(10 - interest_bar_width);

            let beauty_bar_width = ((curion.beauty * 10.0) as usize).min(10);
            let beauty_bar = "█".repeat(beauty_bar_width) + &"░".repeat(10 - beauty_bar_width);

            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!("#{:<4}", index), Style::default().fg(Color::Gray)),
                    Span::styled(rarity_stars, Style::default().fg(rarity_color)),
                    Span::raw(" "),
                    Span::styled(
                        format!("[{:<9}]", format!("{:?}", curion.rarity)),
                        Style::default().fg(rarity_color),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("{:<20}", curion.display_name()),
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        curion.acquired_at.format("%Y-%m-%d %H:%M").to_string(),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("      興味度: "),
                    Span::styled(interest_bar, Style::default().fg(Color::Cyan)),
                    Span::raw(format!(" {:.0}%", curion.interest * 100.0)),
                    Span::raw("  美しさ: "),
                    Span::styled(beauty_bar, Style::default().fg(Color::Magenta)),
                    Span::raw(format!(" {:.0}%", curion.beauty * 100.0)),
                ]),
                Line::from(""),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("コレクション [{} / {}]", collection.len(), collection.len())),
        );

    f.render_widget(list, area);
}

/// 実績一覧を描画
fn draw_achievements(f: &mut Frame<'_>, app: &App, area: Rect) {
    let achievements = app.game_state.achievement_manager.get_sorted_by_progress();

    let items: Vec<ListItem> = achievements
        .iter()
        .skip(app.scroll)
        .take(area.height as usize - 3)
        .map(|(achievement, progress)| {
            let icon = if progress.unlocked && !progress.claimed {
                "✅💰"
            } else if progress.unlocked {
                "✅"
            } else {
                "🔒"
            };

            let ratio = progress.progress_ratio();
            let bar_width = ((ratio * 30.0) as usize).min(30);
            let bar_color = if ratio >= 1.0 {
                Color::Green
            } else if ratio >= 0.8 {
                Color::Yellow
            } else if ratio >= 0.5 {
                Color::Cyan
            } else {
                Color::Gray
            };
            let bar = "█".repeat(bar_width) + &"░".repeat(30 - bar_width);

            let mut lines = vec![
                Line::from(vec![
                    Span::raw(icon),
                    Span::raw(" "),
                    Span::styled(
                        format!("[{}] {}", achievement.icon, achievement.name),
                        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::raw(&achievement.description),
                ]),
            ];

            if !progress.unlocked {
                lines.push(Line::from(vec![
                    Span::raw("    "),
                    Span::styled(bar, Style::default().fg(bar_color)),
                    Span::raw(format!("  {}% ({}/{})", progress.progress_percentage(), progress.current, progress.target)),
                ]));
            }

            lines.push(Line::from(vec![
                Span::raw("    報酬: "),
                Span::styled(
                    format!("{} XP", achievement.reward_xp),
                    Style::default().fg(Color::Yellow),
                ),
                achievement.reward_title.as_ref().map(|title| {
                    Span::styled(
                        format!(", 称号「{}」", title),
                        Style::default().fg(Color::Magenta),
                    )
                }).unwrap_or_else(|| Span::raw("")),
                progress.unlocked_at.map(|date| {
                    Span::styled(
                        format!("  解除日: {}", date.format("%Y-%m-%d")),
                        Style::default().fg(Color::DarkGray),
                    )
                }).unwrap_or_else(|| Span::raw("")),
            ]));

            lines.push(Line::from(""));

            ListItem::new(lines)
        })
        .collect();

    let unlocked = app.game_state.achievement_manager.get_unlocked_count();
    let total = app.game_state.achievement_manager.get_total_count();
    let percentage = app.game_state.achievement_manager.get_unlock_percentage();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("実績: {} / {} 解除済み ({}%)", unlocked, total, percentage)),
        );

    f.render_widget(list, area);
}

/// 統計情報を描画
fn draw_stats(f: &mut Frame<'_>, app: &App, area: Rect) {
    let player = &app.game_state.player;

    let hours = player.total_play_time / 3600;
    let minutes = (player.total_play_time % 3600) / 60;

    let mut lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("┌─ 基本情報 ─────────────────────┐", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("│ レベル:          "),
            Span::styled(format!("{:<15}", player.level), Style::default().fg(Color::Yellow)),
            Span::raw("│"),
        ]),
        Line::from(vec![
            Span::raw("│ 総プレイ時間:    "),
            Span::styled(format!("{}時間 {}分{:<5}", hours, minutes, ""), Style::default().fg(Color::Green)),
            Span::raw("│"),
        ]),
        Line::from(vec![
            Span::raw("│ 初回プレイ:      "),
            Span::styled(format!("{:<15}", player.first_played_at.format("%Y-%m-%d")), Style::default().fg(Color::White)),
            Span::raw("│"),
        ]),
        Line::from(vec![
            Span::raw("│ 連続ログイン:    "),
            Span::styled(format!("{}日{:<12}", player.consecutive_login_days, ""), Style::default().fg(Color::Magenta)),
            Span::raw("│"),
        ]),
        Line::from(vec![
            Span::styled("└────────────────────────────────┘", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("┌─ 収集統計 ─────────────────────┐", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::raw("│ 総獲得数:        "),
            Span::styled(format!("{}個{:<12}", player.total_acquired(), ""), Style::default().fg(Color::Yellow)),
            Span::raw("│"),
        ]),
        Line::from(vec![
            Span::raw("│ 今日の獲得:      "),
            Span::styled(format!("{}個{:<12}", player.today_acquired, ""), Style::default().fg(Color::Green)),
            Span::raw("│"),
        ]),
        Line::from(vec![
            Span::raw("│ 最高日間獲得:    "),
            Span::styled(format!("{}個{:<12}", player.max_daily_acquired, ""), Style::default().fg(Color::Red)),
            Span::raw("│"),
        ]),
        Line::from(vec![
            Span::raw("│ 平均日間獲得:    "),
            Span::styled(format!("{:.1}個{:<10}", player.average_daily_acquired(), ""), Style::default().fg(Color::Cyan)),
            Span::raw("│"),
        ]),
        Line::from(vec![
            Span::raw("│ 獲得レート:      "),
            Span::styled(format!("{:.1}個/時間{:<6}", player.acquisition_rate_per_hour(), ""), Style::default().fg(Color::Magenta)),
            Span::raw("│"),
        ]),
        Line::from(vec![
            Span::styled("└────────────────────────────────┘", Style::default().fg(Color::Cyan)),
        ]),
        Line::from(""),
        Line::from(vec![
            Span::styled("┌─ レアリティ別統計 ─────────────┐", Style::default().fg(Color::Cyan)),
        ]),
    ];

    for rarity in [Rarity::Common, Rarity::Rare, Rarity::Epic, Rarity::Legendary] {
        let count = player.count_by_rarity(rarity);
        let percentage = if player.total_acquired() > 0 {
            (count * 1000 / player.total_acquired()) as f64 / 10.0
        } else {
            0.0
        };

        let most_recent = player.rarity_stats.get(&rarity)
            .and_then(|s| s.most_recent.as_ref())
            .map(|s| s.as_str())
            .unwrap_or("-");

        lines.push(Line::from(vec![
            Span::raw("│ "),
            Span::styled(
                format!("{:<9}", format!("{:?}:", rarity)),
                Style::default().fg(get_rarity_color(&rarity)),
            ),
            Span::raw(format!(" {:>4}個 ({:>5.1}%)", count, percentage)),
            Span::raw(format!(" 最新: {:<6}", most_recent)),
            Span::raw("│"),
        ]));
    }

    lines.push(Line::from(vec![
        Span::styled("└────────────────────────────────┘", Style::default().fg(Color::Cyan)),
    ]));

    let paragraph = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("プレイヤー統計"))
        .alignment(Alignment::Left);

    f.render_widget(paragraph, area);
}

/// レアリティに対応する色を取得
fn get_rarity_color(rarity: &Rarity) -> Color {
    match rarity {
        Rarity::Common => Color::White,
        Rarity::Rare => Color::Cyan,
        Rarity::Epic => Color::Magenta,
        Rarity::Legendary => Color::Yellow,
    }
}

/// レアリティに対応する星を取得
fn get_rarity_stars(rarity: &Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "★",
        Rarity::Rare => "★★",
        Rarity::Epic => "★★★",
        Rarity::Legendary => "★★★★",
    }
}

/// 合成タブを描画
fn draw_synthesis(f: &mut Frame<'_>, app: &App, area: Rect) {

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // ヘッダー
            Constraint::Percentage(100), // コンテンツ
        ])
        .split(area);

    // ヘッダー
    let header = Paragraph::new(format!(
        "Synthesis Lab | Discovered: {}/{}",
        app.game_state.synthesis_manager.discovered_count(),
        app.game_state.synthesis_manager.total_recipe_count()
    ))
    .block(Block::default().borders(Borders::ALL))
    .style(Style::default().fg(Color::Cyan));
    f.render_widget(header, chunks[0]);

    // コンテンツエリアを左右に分割
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // 左: 材料選択
            Constraint::Percentage(50), // 右: レシピ情報
        ])
        .split(chunks[1]);

    match app.synthesis_state {
        SynthesisUIState::SelectingFirst => {
            // 1つ目の材料を選択中
            draw_first_ingredient_selection(f, app, content_chunks[0]);

            // 右側にヘルプを表示
            let help = Paragraph::new("← Select first ingredient\n\nUse ↑↓ to navigate\nPress Enter to select")
                .block(Block::default().borders(Borders::ALL).title("Help"))
                .style(Style::default().fg(Color::Gray));
            f.render_widget(help, content_chunks[1]);
        }
        SynthesisUIState::SelectingSecond => {
            // 1つ目の材料を表示
            if let Some(first_idx) = app.selected_first_curion {
                if let Some(first_curion) = app.game_state.player.collection.get(first_idx) {
                    draw_selected_first(f, first_curion, content_chunks[0]);

                    // 2つ目の候補を表示
                    draw_second_ingredient_candidates(f, app, first_curion, first_idx, content_chunks[1]);
                }
            }
        }
    }
}

/// 1つ目の材料選択画面
fn draw_first_ingredient_selection(f: &mut Frame<'_>, app: &App, area: Rect) {
    let collection = &app.game_state.player.collection;

    if collection.is_empty() {
        let empty = Paragraph::new("No curions in collection")
            .block(Block::default().borders(Borders::ALL).title("Ingredient 1"))
            .style(Style::default().fg(Color::Red));
        f.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = collection
        .iter()
        .enumerate()
        .map(|(i, curion)| {
            let style = if i == app.scroll {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            ListItem::new(format!(
                "{} {} ({})",
                get_rarity_stars(&curion.rarity),
                curion.noun,
                format!("{:?}", curion.category)
            ))
            .style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Select Ingredient 1"))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    f.render_widget(list, area);
}

/// 選択済みの1つ目の材料を表示
fn draw_selected_first(f: &mut Frame<'_>, curion: &crate::curion::Curion, area: Rect) {
    let text = format!(
        "Ingredient 1:\n\n{} {}\nCategory: {:?}\nRarity: {}",
        get_rarity_stars(&curion.rarity),
        curion.noun,
        curion.category,
        format!("{:?}", curion.rarity)
    );

    let widget = Paragraph::new(text)
        .block(Block::default().borders(Borders::ALL).title("Selected"))
        .style(Style::default().fg(Color::Green));

    f.render_widget(widget, area);
}

/// 2つ目の材料候補を表示
fn draw_second_ingredient_candidates(
    f: &mut Frame<'_>,
    app: &App,
    first_curion: &crate::curion::Curion,
    _first_idx: usize,
    area: Rect,
) {
    // 候補を検索
    let candidates = app.game_state.synthesis_manager.find_possible_second_ingredients(
        first_curion,
        &app.game_state.player.collection,
    );

    if candidates.is_empty() {
        let empty = Paragraph::new("No possible combinations\n\nPress Esc to go back")
            .block(Block::default().borders(Borders::ALL).title("Ingredient 2"))
            .style(Style::default().fg(Color::Red));
        f.render_widget(empty, area);
        return;
    }

    let items: Vec<ListItem> = candidates
        .iter()
        .enumerate()
        .map(|(i, candidate)| {
            let style = if i == app.synthesis_scroll {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let result_text = if let Some(ref result) = candidate.result_preview {
                format!("→ {}", result)
            } else {
                "→ ???".to_string()
            };

            let discovered_mark = if candidate.is_discovered { "✓" } else { "?" };

            ListItem::new(format!(
                "{} {} (×{}) {} {}",
                discovered_mark,
                candidate.noun,
                candidate.available_count,
                result_text,
                format!("{:?}", candidate.category)
            ))
            .style(style)
        })
        .collect();

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("Select Ingredient 2"))
        .highlight_style(Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD));

    f.render_widget(list, area);
}
