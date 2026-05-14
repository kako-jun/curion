use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, List, ListItem, Paragraph, Tabs},
    Frame,
};
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::curion::Rarity;
use crate::generator::CurionGenerator;
use crate::player::GameState;

// ── Style constants ──────────────────────────────────────────────

const COLOR_COMMON: Color = Color::White;
const COLOR_RARE: Color = Color::Cyan;
const COLOR_EPIC: Color = Color::Magenta;
const COLOR_LEGENDARY: Color = Color::Yellow;
const COLOR_BORDER: Color = Color::Cyan;
const COLOR_LABEL: Color = Color::Gray;
const COLOR_BAR_HOT: Color = Color::Red;
const COLOR_BAR_WARM: Color = Color::Yellow;
const COLOR_BAR_COOL: Color = Color::Cyan;
const COLOR_BAR_COLD: Color = Color::Gray;

fn rarity_color(rarity: &Rarity) -> Color {
    match rarity {
        Rarity::Common => COLOR_COMMON,
        Rarity::Rare => COLOR_RARE,
        Rarity::Epic => COLOR_EPIC,
        Rarity::Legendary => COLOR_LEGENDARY,
    }
}

fn rarity_stars(rarity: &Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "★",
        Rarity::Rare => "★★",
        Rarity::Epic => "★★★",
        Rarity::Legendary => "★★★★",
    }
}

fn progress_color(ratio: f64) -> Color {
    if ratio >= 0.95 {
        COLOR_BAR_HOT
    } else if ratio >= 0.80 {
        COLOR_BAR_WARM
    } else if ratio >= 0.50 {
        COLOR_BAR_COOL
    } else {
        COLOR_BAR_COLD
    }
}

fn bar(filled_ratio: f64, width: usize) -> String {
    let filled = ((filled_ratio * width as f64) as usize).min(width);
    "█".repeat(filled) + &"░".repeat(width - filled)
}

// ── Types ────────────────────────────────────────────────────────

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

    fn index(&self) -> usize {
        match self {
            Tab::Dashboard => 0,
            Tab::Collection => 1,
            Tab::Achievements => 2,
            Tab::Stats => 3,
            Tab::Synthesis => 4,
        }
    }

    fn from_index(i: usize) -> Option<Tab> {
        match i {
            0 => Some(Tab::Dashboard),
            1 => Some(Tab::Collection),
            2 => Some(Tab::Achievements),
            3 => Some(Tab::Stats),
            4 => Some(Tab::Synthesis),
            _ => None,
        }
    }

    fn next(&self) -> Tab {
        Tab::from_index((self.index() + 1) % 5).unwrap_or(Tab::Dashboard)
    }
}

/// 合成UI状態
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthesisUIState {
    SelectingFirst,
    SelectingSecond,
}

// ── App ──────────────────────────────────────────────────────────

/// アプリケーション状態
pub struct App {
    pub game_state: GameState,
    pub current_tab: Tab,
    pub detail_scroll: usize,
    pub section_indices: [usize; 5],
    pub guid_timer: Instant,
    pub guid_interval: Duration,
    pub generator: CurionGenerator,
    pub save_message: Option<(String, Instant)>,
    pub synthesis_state: SynthesisUIState,
    pub selected_first_curion: Option<usize>,
    pub synthesis_scroll: usize,
}

impl App {
    pub fn new(game_state: GameState) -> Self {
        let generator =
            CurionGenerator::new().unwrap_or_else(|e| panic!("Failed to load noun database: {e}"));

        Self {
            game_state,
            current_tab: Tab::Dashboard,
            detail_scroll: 0,
            section_indices: [0; 5],
            guid_timer: Instant::now(),
            guid_interval: Duration::from_secs(30),
            generator,
            save_message: None,
            synthesis_state: SynthesisUIState::SelectingFirst,
            selected_first_curion: None,
            synthesis_scroll: 0,
        }
    }

    // ── Key handling ─────────────────────────────────────────────

    /// キー入力を処理する。`true` を返したらアプリ終了。
    pub fn handle_key(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Esc => {
                self.handle_escape();
                if self.current_tab != Tab::Synthesis
                    || self.synthesis_state == SynthesisUIState::SelectingFirst
                {
                    return Ok(true);
                }
            }
            KeyCode::Char(c @ '1'..='5') => {
                if let Some(tab) = Tab::from_index((c as usize) - ('1' as usize)) {
                    self.set_tab(tab);
                }
            }
            KeyCode::Tab => self.next_tab(),
            KeyCode::Char(' ') => self.generate_curion()?,
            KeyCode::Char('k') => self.previous_section(),
            KeyCode::Char('j') => self.next_section(),
            KeyCode::Up => self.scroll_up(),
            KeyCode::Down => self.scroll_down(),
            KeyCode::Enter => self.handle_enter()?,
            _ => {}
        }
        Ok(false)
    }

    /// 手動セーブ用（SaveManager を持たないので呼び出し元で save してから呼ぶ）
    pub fn handle_save_key(&mut self) {
        self.show_save_message();
    }

    fn set_tab(&mut self, tab: Tab) {
        self.current_tab = tab;
        self.detail_scroll = 0;
    }

    fn next_tab(&mut self) {
        self.current_tab = self.current_tab.next();
        self.detail_scroll = 0;
    }

    fn previous_section(&mut self) {
        let max_index = self.current_sections().len().saturating_sub(1);
        let current = self.current_section_index();
        self.section_indices[self.current_tab.index()] = current.saturating_sub(1).min(max_index);
        self.detail_scroll = 0;
    }

    fn next_section(&mut self) {
        let max_index = self.current_sections().len().saturating_sub(1);
        let current = self.current_section_index();
        self.section_indices[self.current_tab.index()] = (current + 1).min(max_index);
        self.detail_scroll = 0;
    }

    fn scroll_up(&mut self) {
        if self.current_tab == Tab::Synthesis
            && self.synthesis_state == SynthesisUIState::SelectingSecond
        {
            self.synthesis_scroll = self.synthesis_scroll.saturating_sub(1);
        } else {
            self.detail_scroll = self.detail_scroll.saturating_sub(1);
        }
    }

    fn scroll_down(&mut self) {
        if self.current_tab == Tab::Synthesis
            && self.synthesis_state == SynthesisUIState::SelectingSecond
        {
            self.synthesis_scroll = self.synthesis_scroll.saturating_add(1);
        } else {
            self.detail_scroll = self.detail_scroll.saturating_add(1);
        }
    }

    fn handle_enter(&mut self) -> Result<()> {
        match self.current_tab {
            Tab::Achievements => {
                if self.current_section_index() == 0 {
                    let achievable = self.game_state.achievement_manager.get_achievable();
                    if let Some((achievement, _)) = achievable.get(self.detail_scroll) {
                        let achievement_id = achievement.id.clone();
                        self.game_state.claim_achievement_reward(&achievement_id);
                    }
                }
            }
            Tab::Synthesis => {
                self.handle_synthesis_enter()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_escape(&mut self) {
        if self.current_tab == Tab::Synthesis {
            if let SynthesisUIState::SelectingSecond = self.synthesis_state {
                self.synthesis_state = SynthesisUIState::SelectingFirst;
                self.selected_first_curion = None;
                self.synthesis_scroll = 0;
            }
        }
    }

    fn handle_synthesis_enter(&mut self) -> Result<()> {
        use crate::synthesis::SynthesisAttemptResult;

        match self.synthesis_state {
            SynthesisUIState::SelectingFirst => {
                if self.detail_scroll < self.game_state.player.collection.len() {
                    self.selected_first_curion = Some(self.detail_scroll);
                    self.synthesis_state = SynthesisUIState::SelectingSecond;
                    self.synthesis_scroll = 0;
                }
            }
            SynthesisUIState::SelectingSecond => {
                if let Some(first_idx) = self.selected_first_curion {
                    if let Some(first_curion) =
                        self.game_state.player.collection.get(first_idx).cloned()
                    {
                        let candidates = self
                            .game_state
                            .synthesis_manager
                            .find_possible_second_ingredients(
                                &first_curion,
                                &self.game_state.player.collection,
                            );

                        if let Some(candidate) = candidates.get(self.synthesis_scroll) {
                            if let Some(second_curion) = self
                                .game_state
                                .player
                                .collection
                                .iter()
                                .find(|c| c.noun == candidate.noun && c.id != first_curion.id)
                                .cloned()
                            {
                                let ingredients = vec![first_curion.clone(), second_curion.clone()];
                                let result = self
                                    .game_state
                                    .synthesis_manager
                                    .try_synthesize(ingredients)?;

                                match result {
                                    SynthesisAttemptResult::Success {
                                        curion,
                                        recipe_name,
                                        first_discovery,
                                    } => {
                                        self.game_state.player.collection.retain(|c| {
                                            c.id != first_curion.id && c.id != second_curion.id
                                        });
                                        self.game_state.add_curion(curion.clone());

                                        let message = if first_discovery {
                                            format!("✨ Discovered: {recipe_name}!")
                                        } else {
                                            format!("Created: {}", curion.noun)
                                        };
                                        self.save_message = Some((message, Instant::now()));

                                        self.synthesis_state = SynthesisUIState::SelectingFirst;
                                        self.selected_first_curion = None;
                                        self.synthesis_scroll = 0;
                                        self.detail_scroll = 0;
                                    }
                                    SynthesisAttemptResult::DiscoveryFailed { hint } => {
                                        self.save_message = Some((hint, Instant::now()));
                                    }
                                    SynthesisAttemptResult::NoRecipe => {
                                        self.save_message =
                                            Some(("No recipe found".to_string(), Instant::now()));
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
        self.guid_timer = Instant::now();
        Ok(())
    }

    pub fn on_tick(&mut self) {
        if self.guid_timer.elapsed() >= self.guid_interval {
            let _ = self.generate_curion();
        }
        self.game_state.player.add_play_time(1);

        if let Some((_, timestamp)) = self.save_message {
            if timestamp.elapsed() > Duration::from_secs(3) {
                self.save_message = None;
            }
        }
    }

    pub fn show_save_message(&mut self) {
        self.save_message = Some(("💾 Saved!".to_string(), Instant::now()));
    }

    fn guid_progress(&self) -> f64 {
        let elapsed = self.guid_timer.elapsed().as_secs_f64();
        let total = self.guid_interval.as_secs_f64();
        (elapsed / total).min(1.0)
    }

    fn guid_remaining_seconds(&self) -> u64 {
        let elapsed = self.guid_timer.elapsed();
        self.guid_interval.saturating_sub(elapsed).as_secs()
    }

    fn current_sections(&self) -> &'static [&'static str] {
        match self.current_tab {
            Tab::Dashboard => &["概要", "ログインボーナス", "デイリーミッション"],
            Tab::Collection => &["所持一覧", "図鑑"],
            Tab::Achievements => &["達成可能", "進行中", "達成済み"],
            Tab::Stats => &["レアリティ", "カテゴリ", "時系列"],
            Tab::Synthesis => &["レシピ一覧", "合成実行"],
        }
    }

    fn current_section_index(&self) -> usize {
        let max_index = self.current_sections().len().saturating_sub(1);
        self.section_indices[self.current_tab.index()].min(max_index)
    }

    // ── Rendering ────────────────────────────────────────────────

    pub fn ui(&self, f: &mut Frame<'_>) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // タブバー
                Constraint::Min(0),    // コンテンツ
                Constraint::Length(1), // help_line
            ])
            .split(f.area());

        self.render_tabs(f, chunks[0]);
        self.render_navigation(f, chunks[1]);

        self.render_help_line(f, chunks[2]);

        if let Some((message, _)) = &self.save_message {
            let area = Rect {
                x: f.area().width.saturating_sub(20),
                y: 1,
                width: 18,
                height: 1,
            };
            let save_text = Paragraph::new(message.as_str()).style(
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            );
            f.render_widget(save_text, area);
        }
    }

    fn render_help_line(&self, f: &mut Frame<'_>, area: Rect) {
        let help = match self.current_tab {
            Tab::Achievements if self.current_section_index() == 0 => Line::from(vec![
                Span::styled(" j/k ", Style::default().fg(Color::Black).bg(COLOR_RARE)),
                Span::raw(" 左ペイン  "),
                Span::styled(
                    " ↑/↓ ",
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::raw(" 実績選択  "),
                Span::styled(" Enter ", Style::default().fg(Color::Black).bg(COLOR_EPIC)),
                Span::raw(" 報酬受取  "),
                Span::styled(" Space ", Style::default().fg(Color::Black).bg(COLOR_RARE)),
                Span::raw(" 生成  "),
                Span::styled(
                    " Tab/1-5 ",
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::raw(" タブ  "),
                Span::styled(" s ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
                Span::raw(" 保存  "),
                Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
                Span::raw(" 終了"),
            ]),
            Tab::Synthesis => Line::from(vec![
                Span::styled(" j/k ", Style::default().fg(Color::Black).bg(COLOR_RARE)),
                Span::raw(" 左ペイン  "),
                Span::styled(
                    " ↑/↓ ",
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::raw(" 候補選択  "),
                Span::styled(" Enter ", Style::default().fg(Color::Black).bg(COLOR_EPIC)),
                Span::raw(" 合成  "),
                Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" 戻る  "),
                Span::styled(" Space ", Style::default().fg(Color::Black).bg(COLOR_RARE)),
                Span::raw(" 生成  "),
                Span::styled(
                    " Tab/1-5 ",
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::raw(" タブ  "),
                Span::styled(" s ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
                Span::raw(" 保存  "),
                Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
                Span::raw(" 終了"),
            ]),
            _ => Line::from(vec![
                Span::styled(" j/k ", Style::default().fg(Color::Black).bg(COLOR_RARE)),
                Span::raw(" 左ペイン  "),
                Span::styled(
                    " ↑/↓ ",
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::raw(" 詳細スクロール  "),
                Span::styled(" Space ", Style::default().fg(Color::Black).bg(COLOR_RARE)),
                Span::raw(" 生成  "),
                Span::styled(
                    " Tab/1-5 ",
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::raw(" タブ  "),
                Span::styled(" s ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
                Span::raw(" 保存  "),
                Span::styled(" q ", Style::default().fg(Color::Black).bg(Color::DarkGray)),
                Span::raw(" 終了"),
            ]),
        };

        let help_widget = Paragraph::new(help).style(Style::default().bg(Color::Black));
        f.render_widget(help_widget, area);
    }

    fn render_tabs(&self, f: &mut Frame<'_>, area: Rect) {
        let tabs = vec![
            "Dashboard",
            "Collection",
            "Achievements",
            "Stats",
            "Synthesis",
        ];

        let tabs_widget = Tabs::new(tabs)
            .block(Block::default().borders(Borders::ALL).title("Curion"))
            .select(self.current_tab.index())
            .style(Style::default().fg(Color::White))
            .highlight_style(Style::default().fg(COLOR_RARE).add_modifier(Modifier::BOLD));

        f.render_widget(tabs_widget, area);
    }

    fn render_navigation(&self, f: &mut Frame<'_>, area: Rect) {
        let panes = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(20), Constraint::Min(0)])
            .split(area);

        self.render_left_pane(f, panes[0]);
        self.render_current_section(f, panes[1]);
    }

    fn render_left_pane(&self, f: &mut Frame<'_>, area: Rect) {
        let items: Vec<ListItem> = self
            .current_sections()
            .iter()
            .enumerate()
            .map(|(index, title)| {
                let is_selected = index == self.current_section_index();
                let prefix = if is_selected { "> " } else { "  " };
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(COLOR_RARE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(format!("{prefix}{title}")).style(style)
            })
            .collect();

        let block = Block::default()
            .title(self.current_tab.title())
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(COLOR_RARE));
        let list = List::new(items).block(block);
        f.render_widget(list, area);
    }

    fn render_current_section(&self, f: &mut Frame<'_>, area: Rect) {
        match self.current_tab {
            Tab::Dashboard => self.render_dashboard_section(f, area),
            Tab::Collection => self.render_collection_section(f, area),
            Tab::Achievements => self.render_achievements_section(f, area),
            Tab::Stats => self.render_stats_section(f, area),
            Tab::Synthesis => self.render_synthesis_section(f, area),
        }
    }

    fn render_dashboard_section(&self, f: &mut Frame<'_>, area: Rect) {
        match self.current_section_index() {
            0 => self.render_dashboard_overview(f, area),
            1 => self.render_login_bonus_placeholder(f, area),
            2 => self.render_daily_mission_placeholder(f, area),
            _ => self.render_dashboard_overview(f, area),
        }
    }

    fn render_dashboard_overview(&self, f: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(area);

        self.render_dashboard_top(f, chunks[0]);
        self.render_dashboard_bottom(f, chunks[1]);
    }

    fn render_login_bonus_placeholder(&self, f: &mut Frame<'_>, area: Rect) {
        let player = &self.game_state.player;
        let lines = vec![
            Line::from(vec![
                Span::styled("連続ログイン", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::styled(
                    format!("{} 日", player.consecutive_login_days),
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from("次の Issue #19 で報酬テーブルと受取処理を実装します。"),
            Line::from("この枠は 3 層構造で固定済みなので、次は中身を載せるだけです。"),
        ];
        let widget = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("ログインボーナス"),
        );
        f.render_widget(widget, area);
    }

    fn render_daily_mission_placeholder(&self, f: &mut Frame<'_>, area: Rect) {
        let player = &self.game_state.player;
        let ratio = (player.today_acquired.min(10) as f64) / 10.0;
        let gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_type(BorderType::Rounded)
                    .title("デイリーミッション"),
            )
            .gauge_style(Style::default().fg(progress_color(ratio)))
            .ratio(ratio)
            .label(format!(
                "{} / 10 collected today",
                player.today_acquired.min(10)
            ));
        f.render_widget(gauge, area);
    }

    fn render_dashboard_top(&self, f: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Length(2),
                Constraint::Length(6),
                Constraint::Min(4),
            ])
            .split(area);

        // GUID timer
        let progress = self.guid_progress();
        let remaining = self.guid_remaining_seconds();
        let gauge = Gauge::default()
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("次のキュリオン生成まで"),
            )
            .gauge_style(Style::default().fg(COLOR_RARE).bg(Color::Black))
            .percent((progress * 100.0) as u16)
            .label(format!("{remaining}秒"));
        f.render_widget(gauge, chunks[0]);

        // Basic stats
        let player = &self.game_state.player;
        let stats_text = vec![Line::from(vec![
            Span::styled("総獲得数: ", Style::default().fg(COLOR_LABEL)),
            Span::styled(
                format!("{} 個", player.total_acquired()),
                Style::default()
                    .fg(COLOR_LEGENDARY)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
            Span::styled("今日の獲得: ", Style::default().fg(COLOR_LABEL)),
            Span::styled(
                format!("{} 個", player.today_acquired),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled("レベル: ", Style::default().fg(COLOR_LABEL)),
            Span::styled(
                format!("{}", player.level),
                Style::default().fg(COLOR_EPIC).add_modifier(Modifier::BOLD),
            ),
        ])];
        let stats = Paragraph::new(stats_text)
            .block(Block::default().borders(Borders::ALL).title("統計"))
            .alignment(Alignment::Left);
        f.render_widget(stats, chunks[1]);

        // Latest curion
        if let Some(curion) = player.latest_curion() {
            let color = rarity_color(&curion.rarity);
            let stars = rarity_stars(&curion.rarity);
            let latest_text = vec![Line::from(vec![
                Span::styled(stars, Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(
                    format!("[{:?}]", curion.rarity),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(
                    curion.display_name(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ])];
            let latest = Paragraph::new(latest_text).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("最新キュリオン"),
            );
            f.render_widget(latest, chunks[2]);
        }

        // Rarity distribution
        let mut rarity_items = Vec::new();
        for rarity in [
            Rarity::Common,
            Rarity::Rare,
            Rarity::Epic,
            Rarity::Legendary,
        ] {
            let count = player.count_by_rarity(rarity);
            let percentage = if player.total_acquired() > 0 {
                (count * 100) / player.total_acquired()
            } else {
                0
            };
            let color = rarity_color(&rarity);

            rarity_items.push(Line::from(vec![
                Span::styled(
                    format!("{:<9}", format!("{:?}", rarity)),
                    Style::default().fg(color),
                ),
                Span::raw(" "),
                Span::styled(
                    bar(percentage as f64 / 100.0, 30),
                    Style::default().fg(color),
                ),
                Span::raw(format!("  {count} ({percentage}%)")),
            ]));
        }
        let rarity_widget = Paragraph::new(rarity_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("レアリティ分布"),
        );
        f.render_widget(rarity_widget, chunks[3]);

        // Category distribution
        let category_text = player
            .category_stats
            .iter()
            .map(|(cat, stats)| format!("{}: {}個", cat.as_str(), stats.count))
            .collect::<Vec<_>>()
            .join("  ");
        let category_widget = Paragraph::new(category_text)
            .block(Block::default().borders(Borders::ALL).title("カテゴリ分布"));
        f.render_widget(category_widget, chunks[4]);
    }

    fn render_dashboard_bottom(&self, f: &mut Frame<'_>, area: Rect) {
        let almost_complete = self.game_state.get_almost_complete_achievements(10);

        let mut items: Vec<ListItem> = Vec::new();

        for (name, progress, ratio) in almost_complete {
            let remaining = progress.remaining();
            let percentage = progress.progress_percentage();
            let color = progress_color(ratio);

            let icon = if ratio >= 0.95 {
                "🔥"
            } else if ratio >= 0.80 {
                "⭐"
            } else if ratio >= 0.50 {
                "📦"
            } else {
                ""
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
                        format!("{urgency}あと {remaining} で「{name}」達成！"),
                        Style::default().fg(color).add_modifier(Modifier::BOLD),
                    ),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(bar(ratio, 40), Style::default().fg(color)),
                    Span::raw(format!(
                        "  {}% ({}/{})",
                        percentage, progress.current, progress.target
                    )),
                ]),
                Line::from(""),
            ]));
        }

        // Level-up info
        let player = &self.game_state.player;
        let xp_remaining = player.xp_for_next_level() - player.xp;
        let xp_ratio = player.xp_progress_ratio();

        items.push(ListItem::new(vec![
            Line::from(vec![Span::styled(
                format!(
                    "🏆 次のレベルまで: あと {} XP (Lv.{} → Lv.{})",
                    xp_remaining,
                    player.level,
                    player.level + 1
                ),
                Style::default().fg(COLOR_EPIC).add_modifier(Modifier::BOLD),
            )]),
            Line::from(vec![
                Span::raw("    "),
                Span::styled(bar(xp_ratio, 40), Style::default().fg(COLOR_EPIC)),
                Span::raw(format!(
                    "  {}% ({}/{})",
                    player.xp_progress_percentage(),
                    player.xp,
                    player.xp_for_next_level()
                )),
            ]),
        ]));

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title("🎯 もうすぐ達成できる目標 (あと少し！)"),
        );

        f.render_widget(list, area);
    }

    fn render_collection_section(&self, f: &mut Frame<'_>, area: Rect) {
        match self.current_section_index() {
            0 => self.render_collection_list(f, area),
            1 => self.render_collection_dictionary(f, area),
            _ => self.render_collection_list(f, area),
        }
    }

    fn render_collection_list(&self, f: &mut Frame<'_>, area: Rect) {
        let player = &self.game_state.player;
        let collection = &player.collection;

        let items: Vec<ListItem> = collection
            .iter()
            .rev()
            .enumerate()
            .skip(self.detail_scroll)
            .take(area.height as usize - 3)
            .map(|(i, curion)| {
                let color = rarity_color(&curion.rarity);
                let stars = rarity_stars(&curion.rarity);
                let index = collection.len() - i;

                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(format!("#{index:<4}"), Style::default().fg(COLOR_LABEL)),
                        Span::styled(stars, Style::default().fg(color)),
                        Span::raw(" "),
                        Span::styled(
                            format!("[{:<9}]", format!("{:?}", curion.rarity)),
                            Style::default().fg(color),
                        ),
                        Span::raw(" "),
                        Span::styled(
                            format!("{:<20}", curion.display_name()),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(" "),
                        Span::styled(
                            curion.acquired_at.format("%Y-%m-%d %H:%M").to_string(),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]),
                    Line::from(vec![
                        Span::raw("      興味度: "),
                        Span::styled(bar(curion.interest, 10), Style::default().fg(COLOR_RARE)),
                        Span::raw(format!(" {:.0}%", curion.interest * 100.0)),
                        Span::raw("  美しさ: "),
                        Span::styled(bar(curion.beauty, 10), Style::default().fg(COLOR_EPIC)),
                        Span::raw(format!(" {:.0}%", curion.beauty * 100.0)),
                    ]),
                    Line::from(""),
                ])
            })
            .collect();

        let list = List::new(items).block(Block::default().borders(Borders::ALL).title(format!(
            "コレクション [{} / {}]",
            collection.len(),
            collection.len()
        )));

        f.render_widget(list, area);
    }

    fn render_collection_dictionary(&self, f: &mut Frame<'_>, area: Rect) {
        let player = &self.game_state.player;
        let mut lines = vec![
            Line::from(vec![
                Span::styled("総所持数", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::styled(
                    format!("{} entries", player.total_acquired()),
                    Style::default()
                        .fg(COLOR_LEGENDARY)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
        ];

        for category in [
            crate::curion::Category::Animal,
            crate::curion::Category::Plant,
            crate::curion::Category::Color,
            crate::curion::Category::Object,
            crate::curion::Category::Concept,
            crate::curion::Category::Element,
            crate::curion::Category::Food,
            crate::curion::Category::Phenomenon,
            crate::curion::Category::Abstract,
        ] {
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<8}", category.as_str()),
                    Style::default().fg(COLOR_RARE),
                ),
                Span::raw(" "),
                Span::raw(format!(
                    "{:>3} total / {:>3} unique",
                    player.count_by_category(&category),
                    player.unique_count_by_category(&category)
                )),
            ]));
        }

        let widget = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("図鑑"),
        );
        f.render_widget(widget, area);
    }

    fn render_achievements_section(&self, f: &mut Frame<'_>, area: Rect) {
        match self.current_section_index() {
            0 => {
                let achievable = self.game_state.achievement_manager.get_achievable();
                self.render_achievement_list(f, area, "達成可能", achievable.into_iter().collect());
            }
            1 => {
                let in_progress = self
                    .game_state
                    .achievement_manager
                    .get_sorted_by_progress()
                    .into_iter()
                    .filter(|(_, progress)| !progress.unlocked)
                    .collect();
                self.render_achievement_list(f, area, "進行中", in_progress);
            }
            2 => {
                let unlocked = self
                    .game_state
                    .achievement_manager
                    .get_sorted_by_progress()
                    .into_iter()
                    .filter(|(_, progress)| progress.unlocked)
                    .collect();
                self.render_achievement_list(f, area, "達成済み", unlocked);
            }
            _ => {}
        }
    }

    fn render_achievement_list(
        &self,
        f: &mut Frame<'_>,
        area: Rect,
        title: &str,
        achievements: Vec<(
            &crate::achievement::Achievement,
            &crate::achievement::AchievementProgress,
        )>,
    ) {
        let items: Vec<ListItem> = achievements
            .iter()
            .skip(self.detail_scroll)
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
                let bar_color = progress_color(ratio);

                let mut lines = vec![
                    Line::from(vec![
                        Span::raw(icon),
                        Span::raw(" "),
                        Span::styled(
                            format!("[{}] {}", achievement.icon, achievement.name),
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(vec![Span::raw("    "), Span::raw(&achievement.description)]),
                ];

                if !progress.unlocked {
                    lines.push(Line::from(vec![
                        Span::raw("    "),
                        Span::styled(bar(ratio, 30), Style::default().fg(bar_color)),
                        Span::raw(format!(
                            "  {}% ({}/{})",
                            progress.progress_percentage(),
                            progress.current,
                            progress.target
                        )),
                    ]));
                }

                lines.push(Line::from(vec![
                    Span::raw("    報酬: "),
                    Span::styled(
                        format!("{} XP", achievement.reward_xp),
                        Style::default().fg(COLOR_LEGENDARY),
                    ),
                    achievement
                        .reward_title
                        .as_ref()
                        .map(|title| {
                            Span::styled(
                                format!(", 称号「{title}」"),
                                Style::default().fg(COLOR_EPIC),
                            )
                        })
                        .unwrap_or_else(|| Span::raw("")),
                    progress
                        .unlocked_at
                        .map(|date| {
                            Span::styled(
                                format!("  解除日: {}", date.format("%Y-%m-%d")),
                                Style::default().fg(Color::DarkGray),
                            )
                        })
                        .unwrap_or_else(|| Span::raw("")),
                ]));

                lines.push(Line::from(""));

                ListItem::new(lines)
            })
            .collect();

        let unlocked = self.game_state.achievement_manager.get_unlocked_count();
        let total = self.game_state.achievement_manager.get_total_count();
        let percentage = self.game_state.achievement_manager.get_unlock_percentage();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(format!(
                    "{title} | {unlocked} / {total} 解除済み ({percentage}%)"
                )),
        );

        f.render_widget(list, area);
    }

    fn render_stats_section(&self, f: &mut Frame<'_>, area: Rect) {
        match self.current_section_index() {
            0 => self.render_stats_rarity(f, area),
            1 => self.render_stats_category(f, area),
            2 => self.render_stats_timeline(f, area),
            _ => self.render_stats_rarity(f, area),
        }
    }

    fn render_stats_rarity(&self, f: &mut Frame<'_>, area: Rect) {
        let player = &self.game_state.player;

        let hours = player.total_play_time / 3600;
        let minutes = (player.total_play_time % 3600) / 60;

        let mut lines = vec![
            Line::from(""),
            Line::from(vec![Span::styled(
                "┌─ 基本情報 ─────────────────────┐",
                Style::default().fg(COLOR_BORDER),
            )]),
            Line::from(vec![
                Span::raw("│ レベル:          "),
                Span::styled(
                    format!("{:<15}", player.level),
                    Style::default().fg(COLOR_LEGENDARY),
                ),
                Span::raw("│"),
            ]),
            Line::from(vec![
                Span::raw("│ 総プレイ時間:    "),
                Span::styled(
                    format!("{}時間 {}分{:<5}", hours, minutes, ""),
                    Style::default().fg(Color::Green),
                ),
                Span::raw("│"),
            ]),
            Line::from(vec![
                Span::raw("│ 初回プレイ:      "),
                Span::styled(
                    format!("{:<15}", player.first_played_at.format("%Y-%m-%d")),
                    Style::default().fg(Color::White),
                ),
                Span::raw("│"),
            ]),
            Line::from(vec![
                Span::raw("│ 連続ログイン:    "),
                Span::styled(
                    format!("{}日{:<12}", player.consecutive_login_days, ""),
                    Style::default().fg(COLOR_EPIC),
                ),
                Span::raw("│"),
            ]),
            Line::from(vec![Span::styled(
                "└────────────────────────────────┘",
                Style::default().fg(COLOR_BORDER),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "┌─ 収集統計 ─────────────────────┐",
                Style::default().fg(COLOR_BORDER),
            )]),
            Line::from(vec![
                Span::raw("│ 総獲得数:        "),
                Span::styled(
                    format!("{}個{:<12}", player.total_acquired(), ""),
                    Style::default().fg(COLOR_LEGENDARY),
                ),
                Span::raw("│"),
            ]),
            Line::from(vec![
                Span::raw("│ 今日の獲得:      "),
                Span::styled(
                    format!("{}個{:<12}", player.today_acquired, ""),
                    Style::default().fg(Color::Green),
                ),
                Span::raw("│"),
            ]),
            Line::from(vec![
                Span::raw("│ 最高日間獲得:    "),
                Span::styled(
                    format!("{}個{:<12}", player.max_daily_acquired, ""),
                    Style::default().fg(COLOR_BAR_HOT),
                ),
                Span::raw("│"),
            ]),
            Line::from(vec![
                Span::raw("│ 平均日間獲得:    "),
                Span::styled(
                    format!("{:.1}個{:<10}", player.average_daily_acquired(), ""),
                    Style::default().fg(COLOR_RARE),
                ),
                Span::raw("│"),
            ]),
            Line::from(vec![
                Span::raw("│ 獲得レート:      "),
                Span::styled(
                    format!("{:.1}個/時間{:<6}", player.acquisition_rate_per_hour(), ""),
                    Style::default().fg(COLOR_EPIC),
                ),
                Span::raw("│"),
            ]),
            Line::from(vec![Span::styled(
                "└────────────────────────────────┘",
                Style::default().fg(COLOR_BORDER),
            )]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "┌─ レアリティ別統計 ─────────────┐",
                Style::default().fg(COLOR_BORDER),
            )]),
        ];

        for rarity in [
            Rarity::Common,
            Rarity::Rare,
            Rarity::Epic,
            Rarity::Legendary,
        ] {
            let count = player.count_by_rarity(rarity);
            let percentage = if player.total_acquired() > 0 {
                (count * 1000 / player.total_acquired()) as f64 / 10.0
            } else {
                0.0
            };

            let most_recent = player
                .rarity_stats
                .get(&rarity)
                .and_then(|s| s.most_recent.as_ref())
                .map(|s| s.as_str())
                .unwrap_or("-");

            lines.push(Line::from(vec![
                Span::raw("│ "),
                Span::styled(
                    format!("{:<9}", format!("{:?}:", rarity)),
                    Style::default().fg(rarity_color(&rarity)),
                ),
                Span::raw(format!(" {count:>4}個 ({percentage:>5.1}%)")),
                Span::raw(format!(" 最新: {most_recent:<6}")),
                Span::raw("│"),
            ]));
        }

        lines.push(Line::from(vec![Span::styled(
            "└────────────────────────────────┘",
            Style::default().fg(COLOR_BORDER),
        )]));

        let paragraph = Paragraph::new(lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("プレイヤー統計"),
            )
            .alignment(Alignment::Left);

        f.render_widget(paragraph, area);
    }

    fn render_stats_category(&self, f: &mut Frame<'_>, area: Rect) {
        let player = &self.game_state.player;
        let mut lines = Vec::new();

        for category in [
            crate::curion::Category::Animal,
            crate::curion::Category::Plant,
            crate::curion::Category::Color,
            crate::curion::Category::Object,
            crate::curion::Category::Concept,
            crate::curion::Category::Element,
            crate::curion::Category::Food,
            crate::curion::Category::Phenomenon,
            crate::curion::Category::Abstract,
        ] {
            let total = player.count_by_category(&category);
            let unique = player.unique_count_by_category(&category);
            lines.push(Line::from(vec![
                Span::styled(
                    format!("{:<8}", category.as_str()),
                    Style::default().fg(COLOR_RARE),
                ),
                Span::raw(" "),
                Span::raw(format!("{total:>3} total / {unique:>3} unique")),
            ]));
        }

        let widget = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("カテゴリ"),
        );
        f.render_widget(widget, area);
    }

    fn render_stats_timeline(&self, f: &mut Frame<'_>, area: Rect) {
        let player = &self.game_state.player;
        let lines = vec![
            Line::from(vec![
                Span::styled("初回プレイ", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::raw(player.first_played_at.format("%Y-%m-%d %H:%M").to_string()),
            ]),
            Line::from(vec![
                Span::styled("最終プレイ", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::raw(player.last_played_at.format("%Y-%m-%d %H:%M").to_string()),
            ]),
            Line::from(vec![
                Span::styled("総プレイ時間", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::raw(format!("{} sec", player.total_play_time)),
            ]),
            Line::from(""),
            Line::from("Sparkline 系の時系列可視化は Issue #26 で追加します。"),
        ];

        let widget = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("時系列"),
        );
        f.render_widget(widget, area);
    }

    fn render_synthesis_section(&self, f: &mut Frame<'_>, area: Rect) {
        match self.current_section_index() {
            0 => self.render_recipe_list(f, area),
            1 => self.render_synthesis(f, area),
            _ => self.render_recipe_list(f, area),
        }
    }

    fn render_synthesis(&self, f: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(3), Constraint::Percentage(100)])
            .split(area);

        // Header
        let header = Paragraph::new(format!(
            "Synthesis Lab | Discovered: {}/{}",
            self.game_state.synthesis_manager.discovered_count(),
            self.game_state.synthesis_manager.total_recipe_count()
        ))
        .block(Block::default().borders(Borders::ALL))
        .style(Style::default().fg(COLOR_RARE));
        f.render_widget(header, chunks[0]);

        // Content: left/right split
        let content_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        match self.synthesis_state {
            SynthesisUIState::SelectingFirst => {
                self.render_first_ingredient_selection(f, content_chunks[0]);

                let help = Paragraph::new(
                    "← Select first ingredient\n\nUse ↑↓ to navigate\nPress Enter to select",
                )
                .block(Block::default().borders(Borders::ALL).title("Help"))
                .style(Style::default().fg(COLOR_LABEL));
                f.render_widget(help, content_chunks[1]);
            }
            SynthesisUIState::SelectingSecond => {
                if let Some(first_idx) = self.selected_first_curion {
                    if let Some(first_curion) = self.game_state.player.collection.get(first_idx) {
                        self.render_selected_first(f, first_curion, content_chunks[0]);
                        self.render_second_ingredient_candidates(
                            f,
                            first_curion,
                            content_chunks[1],
                        );
                    }
                }
            }
        }
    }

    fn render_first_ingredient_selection(&self, f: &mut Frame<'_>, area: Rect) {
        let collection = &self.game_state.player.collection;

        if collection.is_empty() {
            let empty = Paragraph::new("No curions in collection")
                .block(Block::default().borders(Borders::ALL).title("Ingredient 1"))
                .style(Style::default().fg(COLOR_BAR_HOT));
            f.render_widget(empty, area);
            return;
        }

        let items: Vec<ListItem> = collection
            .iter()
            .enumerate()
            .map(|(i, curion)| {
                let style = if i == self.detail_scroll {
                    Style::default()
                        .fg(COLOR_LEGENDARY)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(format!(
                    "{} {} ({:?})",
                    rarity_stars(&curion.rarity),
                    curion.noun,
                    curion.category
                ))
                .style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Select Ingredient 1"),
            )
            .highlight_style(
                Style::default()
                    .fg(COLOR_LEGENDARY)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_widget(list, area);
    }

    fn render_recipe_list(&self, f: &mut Frame<'_>, area: Rect) {
        let items: Vec<ListItem> = self
            .game_state
            .synthesis_manager
            .recipe_db()
            .all_recipes()
            .iter()
            .skip(self.detail_scroll)
            .take(area.height as usize - 3)
            .map(|recipe| {
                let discovered = self.game_state.synthesis_manager.is_discovered(&recipe.id);
                let marker = if discovered { "✓" } else { "?" };
                let preview = if discovered {
                    recipe.result.noun.clone()
                } else {
                    "???".to_string()
                };
                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(
                            marker,
                            Style::default().fg(if discovered {
                                Color::Green
                            } else {
                                Color::DarkGray
                            }),
                        ),
                        Span::raw(" "),
                        Span::styled(
                            &recipe.name,
                            Style::default()
                                .fg(Color::White)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]),
                    Line::from(format!("    {} -> {}", recipe.description, preview)),
                    Line::from(""),
                ])
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title("レシピ一覧"),
        );
        f.render_widget(list, area);
    }

    fn render_selected_first(&self, f: &mut Frame<'_>, curion: &crate::curion::Curion, area: Rect) {
        let text = format!(
            "Ingredient 1:\n\n{} {}\nCategory: {:?}\nRarity: {:?}",
            rarity_stars(&curion.rarity),
            curion.noun,
            curion.category,
            curion.rarity
        );

        let widget = Paragraph::new(text)
            .block(Block::default().borders(Borders::ALL).title("Selected"))
            .style(Style::default().fg(Color::Green));

        f.render_widget(widget, area);
    }

    fn render_second_ingredient_candidates(
        &self,
        f: &mut Frame<'_>,
        first_curion: &crate::curion::Curion,
        area: Rect,
    ) {
        let candidates = self
            .game_state
            .synthesis_manager
            .find_possible_second_ingredients(first_curion, &self.game_state.player.collection);

        if candidates.is_empty() {
            let empty = Paragraph::new("No possible combinations\n\nPress Esc to go back")
                .block(Block::default().borders(Borders::ALL).title("Ingredient 2"))
                .style(Style::default().fg(COLOR_BAR_HOT));
            f.render_widget(empty, area);
            return;
        }

        let items: Vec<ListItem> = candidates
            .iter()
            .enumerate()
            .map(|(i, candidate)| {
                let style = if i == self.synthesis_scroll {
                    Style::default()
                        .fg(COLOR_LEGENDARY)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let result_text = if let Some(ref result) = candidate.result_preview {
                    format!("→ {result}")
                } else {
                    "→ ???".to_string()
                };

                let discovered_mark = if candidate.is_discovered { "✓" } else { "?" };

                ListItem::new(format!(
                    "{} {} (×{}) {} {:?}",
                    discovered_mark,
                    candidate.noun,
                    candidate.available_count,
                    result_text,
                    candidate.category
                ))
                .style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Select Ingredient 2"),
            )
            .highlight_style(
                Style::default()
                    .fg(COLOR_LEGENDARY)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_widget(list, area);
    }
}
