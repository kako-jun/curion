use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Bar, BarChart, BarGroup, Block, BorderType, Borders, Gauge, LineGauge, List, ListItem,
        Paragraph, Sparkline, Tabs,
    },
    Frame,
};
use std::time::{Duration, Instant};
use unicode_width::UnicodeWidthStr;
use uuid::Uuid;

use jiwa::{RevealHandle, RevealOpts, Rgb};

use crate::cooldown::{cooldown_progress, current_rarity_probabilities, remaining_seconds};
use crate::curion::{Category, Curion, Rarity};
use crate::evolution::{sort_progress_by_urgency, EvolutionDatabase};
use crate::generator::CurionGenerator;
use crate::player::{GameState, LoginBonusReward};
use crate::san::{san_state, SanState, SAN_MAX};
use crate::semantic::SemanticProfile;

// ── Style constants ──────────────────────────────────────────────

const COLOR_COMMON: Color = Color::Gray;
const COLOR_RARE: Color = Color::Cyan;
const COLOR_EPIC: Color = Color::Yellow;
const COLOR_LEGENDARY: Color = Color::Red;
const COLOR_BORDER: Color = Color::Cyan;
const COLOR_LABEL: Color = Color::DarkGray;
const COLOR_BAR_HOT: Color = Color::Red;
const COLOR_BAR_WARM: Color = Color::Yellow;
const COLOR_BAR_COOL: Color = Color::Cyan;
const COLOR_BAR_COLD: Color = Color::Gray;
const COLOR_SUCCESS: Color = Color::Green;
const RECENT_ACTIVITY_BUCKETS: usize = 16;

// TODO: Category::iter() (strum) 化を将来検討
const ALL_CATEGORIES: [Category; 9] = [
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

/// 文字列を表示幅 (East Asian Wide を 2 セル換算) で `target` セルまで右側を空白埋めする。
/// 既に target 以上の幅があれば、入力をそのまま返す（切り詰めない）。
fn pad_display(s: &str, target: usize) -> String {
    let width = UnicodeWidthStr::width(s);
    if width >= target {
        s.to_string()
    } else {
        let mut out = String::with_capacity(s.len() + (target - width));
        out.push_str(s);
        for _ in 0..(target - width) {
            out.push(' ');
        }
        out
    }
}

/// 文字列を表示幅 `max_width` セル以内に収まるように切り詰め、超える場合は末尾に `…` を付与する。
/// `max_width` が 1 以下なら入力をそのまま返す（フォールバック）。
fn truncate_display(s: &str, max_width: usize) -> String {
    if max_width <= 1 || UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }
    // 末尾の "…" は表示幅 1
    let budget = max_width.saturating_sub(1);
    let mut acc = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let w = UnicodeWidthStr::width(ch.to_string().as_str());
        if used + w > budget {
            break;
        }
        acc.push(ch);
        used += w;
    }
    acc.push('…');
    acc
}

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

fn rarity_rank(rarity: &Rarity) -> u8 {
    match rarity {
        Rarity::Common => 0,
        Rarity::Rare => 1,
        Rarity::Epic => 2,
        Rarity::Legendary => 3,
    }
}

fn rarity_label(rarity: &Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "COM",
        Rarity::Rare => "RARE",
        Rarity::Epic => "EPIC",
        Rarity::Legendary => "LEG",
    }
}

/// 残り寿命を表す Span を返す (Issue #30)。
///
/// - 残り 0 日以下: 赤 + `!` 警告アイコン (まもなく消滅)
/// - 残り 1〜3 日: 黄色
/// - それ以上: 薄いグレー
/// - 寿命なし (`lifespan_days = None`、旧セーブ): `--`
pub(crate) fn lifespan_span_for(curion: &Curion, now: chrono::DateTime<chrono::Utc>) -> Span<'_> {
    match curion.days_remaining(now) {
        None => Span::styled("寿命: --", Style::default().fg(Color::DarkGray)),
        Some(d) if d <= 0 => Span::styled(
            "寿命: ! まもなく消滅".to_string(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ),
        Some(d) if d <= 3 => Span::styled(format!("残 {d} 日"), Style::default().fg(Color::Yellow)),
        Some(d) => Span::styled(format!("残 {d} 日"), Style::default().fg(Color::DarkGray)),
    }
}

/// Issue #31: 正規表現フィルタの長尺レアリティラベル。
///
/// `rarity_label` は `[COM ]` 等の固定 4 桁表示用に短縮されているため、ユーザーが
/// `RARE` `COMMON` `EPIC` `LEGENDARY` のような自然なキーワードで絞り込める語彙で
/// マッチさせる。
fn rarity_filter_label(rarity: &Rarity) -> &'static str {
    match rarity {
        Rarity::Common => "COMMON",
        Rarity::Rare => "RARE",
        Rarity::Epic => "EPIC",
        Rarity::Legendary => "LEGENDARY",
    }
}

/// Issue #31: キュリオン 1 件が正規表現にマッチするか判定する。
///
/// 対象フィールド:
/// - `noun` (日本語名)
/// - `display_name()` (例: `動物 の 魚`)
/// - レアリティラベル (`COMMON` / `RARE` / `EPIC` / `LEGENDARY`)
/// - カテゴリ名 (`動物` 等)
fn match_curion(re: &regex::Regex, curion: &Curion) -> bool {
    if re.is_match(&curion.noun) {
        return true;
    }
    if re.is_match(&curion.display_name()) {
        return true;
    }
    if re.is_match(rarity_filter_label(&curion.rarity)) {
        return true;
    }
    if re.is_match(curion.category.as_str()) {
        return true;
    }
    false
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

fn xp_bar_color(ratio: f64) -> Color {
    if ratio >= 0.95 {
        COLOR_BAR_HOT
    } else if ratio >= 0.75 {
        COLOR_BAR_WARM
    } else {
        COLOR_BAR_COOL
    }
}

fn bar(filled_ratio: f64, width: usize) -> String {
    let filled = ((filled_ratio * width as f64) as usize).min(width);
    "█".repeat(filled) + &"░".repeat(width - filled)
}

fn focused_block<'a, T>(title: T) -> Block<'a>
where
    T: Into<Line<'a>>,
{
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(COLOR_BORDER))
}

fn unfocused_block<'a, T>(title: T) -> Block<'a>
where
    T: Into<Line<'a>>,
{
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(COLOR_LABEL))
}

fn tab_block<'a, T>(title: T) -> Block<'a>
where
    T: Into<Line<'a>>,
{
    Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(Color::White))
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
    /// `save_message` の表示秒数 (デフォルト 3 秒)。デイリーミッション集約表示など、
    /// 通常より長く出したいときに変更する。
    pub save_message_duration: Duration,
    pub synthesis_state: SynthesisUIState,
    pub selected_first_curion: Option<usize>,
    pub synthesis_scroll: usize,
    /// 図鑑モードでフォーカス中のカテゴリ index
    pub dictionary_category_index: usize,
    /// 図鑑モードで選択カテゴリ内の名詞リスト縦スクロール
    pub dictionary_scroll: usize,
    /// Issue #31: Collection タブの正規表現フィルタ
    ///
    /// `/` キーで `filter_mode = true` に入り、入力モードになる。Esc で抜けて
    /// `filter_text` と `compiled_filter` も両方クリアする。Enter で入力モードだけ
    /// 抜けて、フィルタ自体は維持する。
    pub filter_mode: bool,
    /// Collection タブの正規表現フィルタの入力中文字列
    pub filter_text: String,
    /// Collection タブの正規表現フィルタが無効パターンだったときのエラーメッセージ
    pub filter_error: Option<String>,
    /// コンパイル成功した正規表現。`filter_text` が空 or 無効パターンのときは `None`
    pub compiled_filter: Option<regex::Regex>,
    /// Issue #36: 段階進化ガチャの埋め込みデータベース。Dashboard の進化進捗表示で参照する。
    pub evolution_db: EvolutionDatabase,
    /// Dashboard「最新キュリオン」表示の reveal アニメーション。新しいキュリオンが
    /// 生成された瞬間に start し、display_name を 1 文字ずつ暗→明にフェードインで
    /// 浮かび上がらせる。生成体験に「届いた感」を出すための演出。
    latest_reveal: Option<RevealHandle>,
}

impl App {
    pub fn new(game_state: GameState) -> Self {
        let generator =
            CurionGenerator::new().unwrap_or_else(|e| panic!("Failed to load noun database: {e}"));
        let evolution_db = EvolutionDatabase::load_embedded()
            .unwrap_or_else(|e| panic!("Failed to load evolution database: {e}"));

        Self {
            game_state,
            current_tab: Tab::Dashboard,
            detail_scroll: 0,
            section_indices: [0; 5],
            guid_timer: Instant::now(),
            guid_interval: Duration::from_secs(30),
            generator,
            save_message: None,
            save_message_duration: Duration::from_secs(3),
            synthesis_state: SynthesisUIState::SelectingFirst,
            selected_first_curion: None,
            synthesis_scroll: 0,
            dictionary_category_index: 0,
            dictionary_scroll: 0,
            filter_mode: false,
            filter_text: String::new(),
            filter_error: None,
            compiled_filter: None,
            evolution_db,
            latest_reveal: None,
        }
    }

    /// Dashboard「最新キュリオン」名を bloom させるための reveal プリセット。
    /// 暗いグレーから真っ白へ ~200 ms でフェード、字送りは 40 ms。
    fn latest_reveal_opts() -> RevealOpts {
        RevealOpts {
            char_interval: Duration::from_millis(40),
            fade_duration: Duration::from_millis(220),
            fade_from: Rgb(50, 50, 50),
            fade_to: Rgb(255, 255, 255),
        }
    }

    // ── Key handling ─────────────────────────────────────────────

    /// キー入力を処理する。`true` を返したらアプリ終了。
    pub fn handle_key(&mut self, key: KeyCode) -> Result<bool> {
        // Issue #31: Collection タブで正規表現フィルタ入力中はキー入力をフィルタ専用に取る。
        // 入力モード中は `q`/`1`-`5` 等の通常キーもフィルタ文字として扱う必要があるため、
        // 通常ハンドラより前に分岐する。
        if self.filter_mode {
            return self.handle_filter_key(key);
        }
        match key {
            KeyCode::Char('q') => return Ok(true),
            KeyCode::Char('/') if self.current_tab == Tab::Collection => {
                self.enter_filter_mode();
            }
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
            KeyCode::PageUp => self.page_up(),
            KeyCode::PageDown => self.page_down(),
            KeyCode::Enter => self.handle_enter()?,
            // Issue #38: Collection 所持一覧で `e` を押すと、現在 focus 中の curion を
            // 装備/解除する (詳細ペインに表示中のもの)。装備中の curion を再度 `e` で
            // 解除。Collection タブの「所持一覧」セクション以外では no-op。
            KeyCode::Char('e')
                if self.current_tab == Tab::Collection && self.current_section_index() == 0 =>
            {
                self.handle_equip_toggle();
            }
            _ => {}
        }
        Ok(false)
    }

    /// Issue #38: 現在 focus 中の Curion を装備/解除する。
    ///
    /// 「focus 中の Curion」は `render_collection_list` と同じロジックで決める:
    /// - フィルタ適用中ならフィルタ後リスト、それ以外は full collection
    /// - リストは新しい順 (rev) で表示しているので、focus 位置を反転して index を算出
    fn handle_equip_toggle(&mut self) {
        let collection = &self.game_state.player.collection;
        let filtered: Vec<&Curion> = match &self.compiled_filter {
            Some(re) => collection.iter().filter(|c| match_curion(re, c)).collect(),
            None => collection.iter().collect(),
        };
        if filtered.is_empty() {
            return;
        }
        let focus_index_rev = self.detail_scroll.min(filtered.len().saturating_sub(1));
        let focus_index = filtered.len() - 1 - focus_index_rev;
        let id = match filtered.get(focus_index).map(|c| c.id.clone()) {
            Some(id) => id,
            None => return,
        };
        self.game_state.player.toggle_equip(&id);
    }

    /// Issue #31: 正規表現フィルタ入力モード中のキー処理。
    ///
    /// - Esc: 入力モードを抜け、フィルタも全クリア (filter_text / compiled / error)
    /// - Enter: 入力モードのみ抜ける。フィルタは維持
    /// - Backspace: 1 文字削除
    /// - 印字可能文字: filter_text に追記
    fn handle_filter_key(&mut self, key: KeyCode) -> Result<bool> {
        match key {
            KeyCode::Esc => {
                self.exit_filter_mode_and_clear();
            }
            KeyCode::Enter => {
                // フィルタは維持したまま入力モードだけ抜ける
                self.filter_mode = false;
            }
            KeyCode::Backspace => {
                self.filter_text.pop();
                self.recompile_filter();
            }
            KeyCode::Char(c) => {
                self.filter_text.push(c);
                self.recompile_filter();
            }
            _ => {}
        }
        Ok(false)
    }

    /// Issue #31: `/` キーでフィルタ入力モードに入る。入力初期化のため
    /// `filter_text` / `compiled_filter` / `filter_error` はすべてクリアする。
    fn enter_filter_mode(&mut self) {
        self.filter_mode = true;
        self.filter_text.clear();
        self.compiled_filter = None;
        self.filter_error = None;
    }

    /// Issue #31: フィルタ入力モードを抜け、適用中のフィルタもすべて解除する。
    fn exit_filter_mode_and_clear(&mut self) {
        self.filter_mode = false;
        self.filter_text.clear();
        self.compiled_filter = None;
        self.filter_error = None;
    }

    /// Issue #31: `filter_text` を Regex にコンパイルし直す。
    ///
    /// - 空文字列: フィルタ無効 (None)
    /// - コンパイル成功: `compiled_filter = Some(re)`, `filter_error = None`
    /// - コンパイル失敗: `compiled_filter = None`, `filter_error = Some(msg)`
    fn recompile_filter(&mut self) {
        // フィルタが変わるたび、表示中スクロールを先頭に戻す。
        // フィルタで件数が減ったあと前のスクロール位置のまま空表示にしないため。
        self.detail_scroll = 0;
        self.dictionary_scroll = 0;
        if self.filter_text.is_empty() {
            self.compiled_filter = None;
            self.filter_error = None;
            return;
        }
        match regex::Regex::new(&self.filter_text) {
            Ok(re) => {
                self.compiled_filter = Some(re);
                self.filter_error = None;
            }
            Err(e) => {
                self.compiled_filter = None;
                self.filter_error = Some(format!("invalid regex: {e}"));
            }
        }
    }

    fn is_collection_dictionary(&self) -> bool {
        self.current_tab == Tab::Collection && self.current_section_index() == 1
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
        if self.is_collection_dictionary() {
            // 図鑑モード: ↑/↓ はカテゴリ移動
            if self.dictionary_category_index > 0 {
                self.dictionary_category_index -= 1;
                self.dictionary_scroll = 0;
            }
        } else if self.current_tab == Tab::Synthesis
            && self.synthesis_state == SynthesisUIState::SelectingSecond
        {
            self.synthesis_scroll = self.synthesis_scroll.saturating_sub(1);
        } else {
            self.detail_scroll = self.detail_scroll.saturating_sub(1);
        }
    }

    fn scroll_down(&mut self) {
        if self.is_collection_dictionary() {
            // 図鑑モード: ↑/↓ はカテゴリ移動
            let max_index = ALL_CATEGORIES.len().saturating_sub(1);
            if self.dictionary_category_index < max_index {
                self.dictionary_category_index += 1;
                self.dictionary_scroll = 0;
            }
        } else if self.current_tab == Tab::Synthesis
            && self.synthesis_state == SynthesisUIState::SelectingSecond
        {
            self.synthesis_scroll = self.synthesis_scroll.saturating_add(1);
        } else {
            self.detail_scroll = self.detail_scroll.saturating_add(1);
        }
    }

    fn page_up(&mut self) {
        if self.is_collection_dictionary() {
            self.dictionary_scroll = self.dictionary_scroll.saturating_sub(10);
        }
    }

    fn page_down(&mut self) {
        if self.is_collection_dictionary() {
            self.dictionary_scroll = self.dictionary_scroll.saturating_add(10);
        }
    }

    fn handle_enter(&mut self) -> Result<()> {
        match self.current_tab {
            Tab::Achievements if self.current_section_index() == 0 => {
                let achievable = self.game_state.achievement_manager.get_achievable();
                if let Some((achievement, _)) = achievable.get(self.detail_scroll) {
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
                                        // 合成成功で生まれた新キュリオンも収集系ミッション (CollectAny /
                                        // CollectFromCategories / CollectRarityAtLeast) の進捗に乗せる。
                                        // add_curion 内で record_curion_acquired が走るため、
                                        // ここでは合成ミッション固有の進捗 (SynthesizeSuccess) だけを更新する。
                                        self.game_state.record_synthesis_success();
                                        self.flush_daily_mission_rewards();

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
                                    // Issue #35: 高リスク合成失敗の処理。
                                    // - lost_ingredients を collection から id 一致で除去
                                    // - salvage curion があれば add_curion で追加 (Curion 加算系
                                    //   ミッションも record_curion_acquired 経由で進捗する)
                                    // - 失敗モードに応じた赤系トーストを表示
                                    SynthesisAttemptResult::HighRiskFailure {
                                        recipe_name,
                                        lost_ingredients,
                                        salvage,
                                        failure_mode,
                                    } => {
                                        for ci in &lost_ingredients {
                                            self.game_state
                                                .player
                                                .collection
                                                .retain(|c| c.id != ci.id);
                                        }
                                        if let Some(s) = salvage {
                                            self.game_state.add_curion(s);
                                            self.flush_daily_mission_rewards();
                                        }
                                        let msg = match failure_mode {
                                            crate::synthesis::FailureMode::LoseAll => {
                                                format!("💥 失敗: {recipe_name} (素材消滅)")
                                            }
                                            crate::synthesis::FailureMode::Salvage { .. } => {
                                                format!("💔 失敗: {recipe_name} (残骸を獲得)")
                                            }
                                            crate::synthesis::FailureMode::NoLoss => {
                                                format!("⚠ 失敗: {recipe_name} (保険発動)")
                                            }
                                        };
                                        self.save_message = Some((msg, Instant::now()));

                                        self.synthesis_state = SynthesisUIState::SelectingFirst;
                                        self.selected_first_curion = None;
                                        self.synthesis_scroll = 0;
                                        self.detail_scroll = 0;
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
        // Issue #25: 収集後 X 時間でレア確率が段階的に上昇する。
        // クールダウン満了時 (progress=1.0) に最大ボーナスでロールする。
        let progress = cooldown_progress(
            self.game_state.player.last_collection_at,
            chrono::Utc::now(),
        );
        let curion = self.generator.generate_with_bonus(guid, progress)?;
        let revealed_name = curion.display_name();
        self.game_state.add_curion(curion);
        self.latest_reveal = Some(RevealHandle::start(
            &revealed_name,
            Self::latest_reveal_opts(),
        ));
        self.flush_daily_mission_rewards();
        self.guid_timer = Instant::now();
        Ok(())
    }

    /// 達成済みデイリーミッションを自動受取し、結果をトーストへ流す。
    /// 起動時 (main.rs) からも呼べるよう pub。
    pub fn flush_daily_mission_rewards(&mut self) {
        let claimed = self.game_state.auto_claim_daily_missions();
        if claimed.is_empty() {
            return;
        }
        let (msg, duration) = if claimed.len() == 1 {
            let m = &claimed[0];
            (
                format!("🎯 [Mission] {} +{} XP", m.description, m.reward_xp),
                Duration::from_secs(3),
            )
        } else {
            // 複数件達成時は description を全件表示する。プレイヤーに「どれが達成したか」を
            // 認識してもらうため、改行で区切って 5 秒間表示する。
            let total_xp: u32 = claimed.iter().map(|m| m.reward_xp).sum();
            let lines: Vec<String> = claimed
                .iter()
                .map(|m| format!("  + {} (+{} XP)", m.description, m.reward_xp))
                .collect();
            (
                format!(
                    "🎯 [Mission] {} 件達成! 合計 +{} XP\n{}",
                    claimed.len(),
                    total_xp,
                    lines.join("\n")
                ),
                Duration::from_secs(5),
            )
        };
        self.save_message = Some((msg, Instant::now()));
        self.save_message_duration = duration;
    }

    pub fn on_tick(&mut self) {
        if self.guid_timer.elapsed() >= self.guid_interval {
            let _ = self.generate_curion();
        }
        self.game_state.player.add_play_time(1);

        if let Some((_, timestamp)) = self.save_message {
            if timestamp.elapsed() > self.save_message_duration {
                self.save_message = None;
                // 次回のメッセージは標準秒数に戻す
                self.save_message_duration = Duration::from_secs(3);
            }
        }
    }

    pub fn show_save_message(&mut self) {
        self.save_message = Some(("💾 Saved!".to_string(), Instant::now()));
    }

    pub fn show_login_bonus_message(&mut self, reward: &LoginBonusReward) {
        self.save_message = Some((
            format!("🎁 Day {} +{} XP", reward.day, reward.xp),
            Instant::now(),
        ));
    }

    /// 期限切れで消えたキュリオンを 1 行トーストで表示する (Issue #30)。
    ///
    /// 表示時間は通常より長めの 6 秒に延長する (普段見ない情報なので拾わせる)。
    pub fn show_expired_curions_message(&mut self, expired: &[Curion]) {
        if expired.is_empty() {
            return;
        }
        let preview: String = expired
            .iter()
            .take(3)
            .map(|c| c.display_name())
            .collect::<Vec<_>>()
            .join(", ");
        let more = if expired.len() > 3 {
            format!(" 他 {} 個", expired.len() - 3)
        } else {
            String::new()
        };
        self.save_message_duration = Duration::from_secs(6);
        self.save_message = Some((format!("🕯 寿命で消滅: {}{}", preview, more), Instant::now()));
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

    fn collection_count_by_category(&self, category: &Category) -> usize {
        self.game_state
            .player
            .collection
            .iter()
            .filter(|curion| &curion.category == category)
            .count()
    }

    fn collection_unique_count_by_category(&self, category: &Category) -> usize {
        use std::collections::BTreeSet;

        self.game_state
            .player
            .collection
            .iter()
            .filter(|curion| &curion.category == category)
            .map(|curion| curion.noun.as_str())
            .collect::<BTreeSet<_>>()
            .len()
    }

    fn recent_acquisition_buckets(&self, bucket_count: usize) -> Vec<u64> {
        let player = &self.game_state.player;
        if bucket_count == 0 {
            return Vec::new();
        }

        if player.collection.is_empty() {
            return vec![0; bucket_count];
        }

        let now = chrono::Utc::now();
        let span_seconds = (now - player.first_played_at)
            .num_seconds()
            .max(bucket_count as i64);
        let bucket_span = ((span_seconds as f64) / bucket_count as f64)
            .ceil()
            .max(1.0);
        let mut buckets = vec![0_u64; bucket_count];

        for curion in &player.collection {
            let elapsed = (curion.acquired_at - player.first_played_at)
                .num_seconds()
                .clamp(0, span_seconds);
            let index = ((elapsed as f64) / bucket_span).floor() as usize;
            buckets[index.min(bucket_count.saturating_sub(1))] += 1;
        }

        buckets
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
            // 複数件メッセージは改行を含むため、行数と最大幅から動的にサイズを決める
            let lines: Vec<&str> = message.split('\n').collect();
            let max_w = lines
                .iter()
                .map(|l| l.chars().count() as u16)
                .max()
                .unwrap_or(18);
            // 描画幅は端末幅に収まる範囲で、最低 18・最大 60 とする
            let desired_w = max_w.saturating_add(2).clamp(18, 60);
            let width = desired_w.min(f.area().width);
            let height = (lines.len() as u16).clamp(1, 8);
            let area = Rect {
                x: f.area().width.saturating_sub(width),
                y: 1,
                width,
                height,
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
        // Issue #31: フィルタ入力中は専用ヘルプを出す
        if self.filter_mode {
            let help = Line::from(vec![
                Span::styled(" filter ", Style::default().fg(Color::Black).bg(COLOR_EPIC)),
                Span::raw(" 正規表現入力中  "),
                Span::styled(" Enter ", Style::default().fg(Color::Black).bg(COLOR_RARE)),
                Span::raw(" 入力確定 (フィルタ維持)  "),
                Span::styled(" Esc ", Style::default().fg(Color::Black).bg(Color::Gray)),
                Span::raw(" 解除  "),
                Span::styled(
                    " Backspace ",
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::raw(" 1 文字削除"),
            ]);
            let help_widget = Paragraph::new(help).style(Style::default().bg(Color::Black));
            f.render_widget(help_widget, area);
            return;
        }
        let help = match self.current_tab {
            Tab::Collection if self.current_section_index() == 1 => Line::from(vec![
                Span::styled(" j/k ", Style::default().fg(Color::Black).bg(COLOR_RARE)),
                Span::raw(" 左ペイン  "),
                Span::styled(
                    " ↑/↓ ",
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::raw(" カテゴリ移動  "),
                Span::styled(
                    " PgUp/PgDn ",
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::raw(" 名詞スクロール  "),
                Span::styled(" / ", Style::default().fg(Color::Black).bg(COLOR_EPIC)),
                Span::raw(" 絞り込み  "),
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
            Tab::Collection if self.current_section_index() == 0 => Line::from(vec![
                Span::styled(" j/k ", Style::default().fg(Color::Black).bg(COLOR_RARE)),
                Span::raw(" 左ペイン  "),
                Span::styled(
                    " ↑/↓ ",
                    Style::default().fg(Color::Black).bg(Color::DarkGray),
                ),
                Span::raw(" スクロール  "),
                Span::styled(" / ", Style::default().fg(Color::Black).bg(COLOR_EPIC)),
                Span::raw(" 絞り込み  "),
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
            .block(tab_block("Curion"))
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

        let block = unfocused_block(self.current_tab.title());
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
            2 => self.render_daily_missions(f, area),
            _ => self.render_dashboard_overview(f, area),
        }
    }

    fn render_dashboard_overview(&self, f: &mut Frame<'_>, area: Rect) {
        let block = focused_block("概要");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(45), Constraint::Percentage(55)])
            .split(inner);

        self.render_dashboard_top(f, chunks[0]);
        self.render_dashboard_bottom(f, chunks[1]);
    }

    fn render_login_bonus_placeholder(&self, f: &mut Frame<'_>, area: Rect) {
        let player = &self.game_state.player;
        let today_reward = player.current_login_bonus_reward();
        let next_reward = player.next_login_bonus_reward();
        let status_color = if player.login_bonus_claimed_today {
            Color::Green
        } else {
            Color::Yellow
        };
        let status_text = if player.login_bonus_claimed_today {
            "受取済み"
        } else {
            "未受取"
        };
        let block = focused_block("ログインボーナス");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),
                Constraint::Length(2),
                Constraint::Min(0),
            ])
            .split(inner);

        let mut lines = vec![
            Line::from(vec![
                Span::styled("連続ログイン", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::styled(
                    format!("{} 日", player.consecutive_login_days),
                    Style::default()
                        .fg(COLOR_SUCCESS)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("   "),
                Span::styled("状態", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::styled(
                    status_text,
                    Style::default()
                        .fg(status_color)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("今日の報酬", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::styled(
                    format!("Day {} / {} XP", today_reward.day, today_reward.xp),
                    Style::default().fg(COLOR_EPIC).add_modifier(Modifier::BOLD),
                ),
            ]),
        ];

        for summary in today_reward.summary_lines().into_iter().skip(1) {
            lines.push(Line::from(format!("  + {summary}")));
        }

        lines.extend([
            Line::from(""),
            Line::from(vec![
                Span::styled("次回予告", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::styled(
                    format!("Day {} / {} XP", next_reward.day, next_reward.xp),
                    Style::default().fg(COLOR_RARE),
                ),
            ]),
        ]);

        for summary in next_reward.summary_lines().into_iter().skip(1) {
            lines.push(Line::from(format!("  -> {summary}")));
        }

        lines.extend([
            Line::from(""),
            Line::from(vec![
                Span::styled("所持チケット", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::raw(format!(
                    "Common {} / Rare {} / Epic {}",
                    player.guaranteed_tickets.common,
                    player.guaranteed_tickets.rare,
                    player.guaranteed_tickets.epic
                )),
            ]),
        ]);

        let streak_ratio = (player.consecutive_login_days.min(7) as f64) / 7.0;
        let streak = LineGauge::default()
            .ratio(streak_ratio)
            .label(format!(
                "STREAK [{}] {:>2}/7 days",
                bar(streak_ratio, 10),
                player.consecutive_login_days.min(7)
            ))
            .filled_style(Style::default().fg(COLOR_SUCCESS))
            .unfilled_style(Style::default().fg(COLOR_LABEL));
        f.render_widget(streak, chunks[0]);

        let reward_cycle = ((today_reward.day - 1) % 7 + 1) as f64 / 7.0;
        let cycle = LineGauge::default()
            .ratio(reward_cycle)
            .label(format!(
                "REWARD CYCLE [{}] Day {}",
                bar(reward_cycle, 10),
                today_reward.day
            ))
            .filled_style(Style::default().fg(COLOR_RARE))
            .unfilled_style(Style::default().fg(COLOR_LABEL));
        f.render_widget(cycle, chunks[1]);

        let widget = Paragraph::new(lines);
        f.render_widget(widget, chunks[2]);
    }

    fn render_daily_missions(&self, f: &mut Frame<'_>, area: Rect) {
        use chrono::Local;

        let missions = &self.game_state.player.daily_mission_manager.missions;

        // タイトル: 翌 0:00 までの残り時間を chrono::Duration で計算し、
        // 分単位で切り上げる (秒未満を切り捨てると「00:00」になるエッジを避ける)
        let now = Local::now();
        let tomorrow = now.date_naive() + chrono::Duration::days(1);
        let next_midnight = tomorrow
            .and_hms_opt(0, 0, 0)
            .and_then(|nd| nd.and_local_timezone(Local).single())
            .unwrap_or(now);
        let remaining = next_midnight.signed_duration_since(now);
        let total_secs = remaining.num_seconds().max(0);
        // 秒を切り上げて分にする (例: 59 秒残り → 1 分扱い)
        let remaining_minutes = (total_secs + 59) / 60;
        let hh = remaining_minutes / 60;
        let mm = remaining_minutes % 60;
        let title = format!("デイリーミッション (リセットまで {:02}:{:02})", hh, mm);

        let block = focused_block(title);
        let inner = block.inner(area);
        f.render_widget(block, area);

        if missions.is_empty() {
            let empty = Paragraph::new("まだデータがありません")
                .style(Style::default().fg(COLOR_LABEL))
                .alignment(Alignment::Center);
            f.render_widget(empty, inner);
            return;
        }

        // S3: 画面高さが足りない場合の簡易表示フォールバック。
        // 通常は 1 ミッションあたり 4 行 (icon+desc / gauge / reward / blank) なので、
        // 3 本で 12 行必要。inner.height < 12 のときは 1 ミッション 1 行に圧縮する。
        if inner.height < 12 {
            let constraints: Vec<Constraint> = std::iter::once(Constraint::Length(1))
                .chain(missions.iter().map(|_| Constraint::Length(1)))
                .collect();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints(constraints)
                .split(inner);
            // ヘッダ行
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    "デイリーミッション (簡易表示)",
                    Style::default().fg(COLOR_LABEL),
                ))),
                chunks[0],
            );
            for (i, mission) in missions.iter().enumerate() {
                if i + 1 >= chunks.len() {
                    break;
                }
                let completed = mission.is_completed();
                let icon = if mission.claimed {
                    "✅"
                } else if completed {
                    "🎯"
                } else {
                    "·"
                };
                let line = Line::from(vec![
                    Span::raw(icon),
                    Span::raw(" "),
                    Span::raw(mission.description.clone()),
                    Span::raw(format!(
                        " {}/{} +{}XP",
                        mission.current.min(mission.target),
                        mission.target,
                        mission.reward_xp
                    )),
                ]);
                f.render_widget(Paragraph::new(line), chunks[i + 1]);
            }
            return;
        }

        // 各ミッションは 4 行（icon+desc / gauge / reward / blank）
        let constraints: Vec<Constraint> = missions
            .iter()
            .flat_map(|_| {
                [
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                    Constraint::Length(1),
                ]
            })
            .collect();
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        for (i, mission) in missions.iter().enumerate() {
            let base = i * 4;
            let completed = mission.is_completed();
            let icon = if completed { "✅" } else { "🎯" };
            let title_line = Line::from(vec![
                Span::raw(icon),
                Span::raw(" "),
                Span::styled(
                    mission.description.clone(),
                    Style::default()
                        .fg(if completed {
                            COLOR_SUCCESS
                        } else {
                            Color::White
                        })
                        .add_modifier(if completed {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ]);
            f.render_widget(Paragraph::new(title_line), chunks[base]);

            let ratio = mission.progress_ratio();
            let gauge_color = if completed { COLOR_SUCCESS } else { COLOR_RARE };
            let gauge_line = Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("[{}]", bar(ratio, 12)),
                    Style::default().fg(gauge_color),
                ),
                Span::raw(format!(
                    "  {:>3} / {}",
                    mission.current.min(mission.target),
                    mission.target
                )),
            ]);
            f.render_widget(Paragraph::new(gauge_line), chunks[base + 1]);

            let reward_line = if mission.claimed {
                Line::from(vec![Span::styled(
                    format!("  [✅ +{} XP claimed]", mission.reward_xp),
                    Style::default().fg(COLOR_SUCCESS),
                )])
            } else {
                Line::from(vec![Span::styled(
                    format!("  報酬: +{} XP", mission.reward_xp),
                    Style::default().fg(COLOR_LABEL),
                )])
            };
            f.render_widget(Paragraph::new(reward_line), chunks[base + 2]);

            // blank chunk (chunks[base + 3]) は空行として残す
        }
    }

    fn render_dashboard_top(&self, f: &mut Frame<'_>, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(3), // 0: GUID timer
                Constraint::Length(3), // 1: XP gauge
                Constraint::Length(1), // 2: Rare cooldown
                Constraint::Length(1), // 3: RARE 出現確率 (Issue #28)
                Constraint::Length(1), // 4: SAN bar (Issue #29)
                Constraint::Length(2), // 5: stats (total/today/level/COMBO)
                Constraint::Length(1), // 6: next milestone (Issue #32)
                Constraint::Length(1), // 7: lifespan warning (Issue #30)
                Constraint::Length(1), // 8: equipment summary (Issue #38)
                Constraint::Length(3), // 9: evolution progress top 3 (Issue #36)
                Constraint::Length(3), // 10: latest curion
                Constraint::Length(6), // 11: rarity distribution
                Constraint::Min(4),    // 12: category distribution
            ])
            .split(area);

        // GUID timer
        let progress = self.guid_progress();
        let remaining = self.guid_remaining_seconds();
        let gauge = Gauge::default()
            .block(unfocused_block("次のキュリオン生成まで"))
            .gauge_style(Style::default().fg(COLOR_RARE).bg(Color::Black))
            .percent((progress * 100.0) as u16)
            .label(format!(
                "[{}] {:>2}%  ({remaining}秒)",
                bar(progress, 12),
                (progress * 100.0) as u16
            ));
        f.render_widget(gauge, chunks[0]);

        let xp_ratio = self.game_state.player.xp_progress_ratio();
        let xp_gauge = Gauge::default()
            .block(unfocused_block("XP"))
            .gauge_style(Style::default().fg(xp_bar_color(xp_ratio)))
            .ratio(xp_ratio)
            .label(format!(
                "[{}] {:>3}%  ({} / {})",
                bar(xp_ratio, 12),
                self.game_state.player.xp_progress_percentage(),
                self.game_state.player.xp,
                self.game_state.player.xp_for_next_level()
            ));
        f.render_widget(xp_gauge, chunks[1]);

        // Issue #25: レア出現予告クールダウン (LineGauge, 1 行)
        let cooldown_p = cooldown_progress(
            self.game_state.player.last_collection_at,
            chrono::Utc::now(),
        );
        let (cd_color, cd_label) = if cooldown_p >= 1.0 {
            (
                COLOR_EPIC,
                "RARE COOLDOWN ⚡ レア出現確率上昇中!".to_string(),
            )
        } else {
            let secs = remaining_seconds(
                self.game_state.player.last_collection_at,
                chrono::Utc::now(),
            );
            let mm = secs / 60;
            let hh = mm / 60;
            let m_rem = mm % 60;
            (
                COLOR_RARE,
                format!("RARE COOLDOWN {hh}:{m_rem:02} remaining"),
            )
        };
        let cooldown_gauge = LineGauge::default()
            .ratio(cooldown_p.clamp(0.0, 1.0))
            .label(cd_label)
            .filled_style(Style::default().fg(cd_color))
            .unfilled_style(Style::default().fg(COLOR_LABEL));
        f.render_widget(cooldown_gauge, chunks[2]);

        // Issue #28: 行動前にレア出現確率を表示
        // 現在の cooldown progress を反映した「現在の」レアリティ別確率を 1 行で出す。
        // 例: `RARE出現確率: 12.3% (Common 60.0% / Rare 30.0% / Epic 9.0% / Legendary 1.0%)`
        let rarity_probs = current_rarity_probabilities(cooldown_p);
        let rare_pct = rarity_probs.rare_or_higher() * 100.0;
        let rare_probability_line = Line::from(vec![
            Span::styled("RARE出現確率: ", Style::default().fg(COLOR_LABEL)),
            Span::styled(
                format!("{:.1}%", rare_pct),
                Style::default()
                    .fg(if cooldown_p >= 1.0 {
                        COLOR_EPIC
                    } else {
                        COLOR_RARE
                    })
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "  (Common {:.1}% / Rare {:.1}% / Epic {:.1}% / Legendary {:.1}%)",
                    rarity_probs.common * 100.0,
                    rarity_probs.rare * 100.0,
                    rarity_probs.epic * 100.0,
                    rarity_probs.legendary * 100.0,
                ),
                Style::default().fg(COLOR_LABEL),
            ),
        ]);
        f.render_widget(Paragraph::new(rare_probability_line), chunks[3]);

        // Issue #29: SAN 値 (正気度) バー
        // ロジックは `crate::san` に閉じ、ここでは値を読んで描画するだけ。
        let san = self.game_state.player.san;
        let san_ratio = (san / SAN_MAX).clamp(0.0, 1.0);
        let state = san_state(san);
        let (san_color, san_warn) = match state {
            SanState::Healthy => (Color::Cyan, ""),
            SanState::Slight => (Color::Yellow, ""),
            SanState::Warning => (Color::Red, ""),
            SanState::Critical => (Color::Magenta, "  ⚠ 異常状態"),
        };
        let san_label = format!(
            "SAN [{}] {:>5.1} / {:.0}{}",
            bar(san_ratio, 12),
            san,
            SAN_MAX,
            san_warn
        );
        let san_gauge = LineGauge::default()
            .ratio(san_ratio)
            .label(san_label)
            .filled_style(Style::default().fg(san_color))
            .unfilled_style(Style::default().fg(COLOR_LABEL));
        f.render_widget(san_gauge, chunks[4]);

        // Basic stats
        let player = &self.game_state.player;
        // COMBO 表示: コンボ中はレアリティ色で強調、5+ は Legendary + 称号アイコン
        let (combo_color, combo_suffix) = match player.combo_count {
            0 | 1 => (COLOR_LABEL, String::new()),
            2 => (COLOR_RARE, String::new()),
            3 | 4 => (COLOR_EPIC, String::new()),
            _ => (COLOR_LEGENDARY, "  🔥 コンボマスター!".to_string()),
        };
        let combo_value_style = if player.combo_count >= 2 {
            Style::default()
                .fg(combo_color)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(combo_color)
        };
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
            Span::raw("  "),
            Span::styled("COMBO: ", Style::default().fg(COLOR_LABEL)),
            Span::styled(format!("{}", player.combo_count), combo_value_style),
            Span::styled(
                combo_suffix,
                Style::default()
                    .fg(COLOR_LEGENDARY)
                    .add_modifier(Modifier::BOLD),
            ),
        ])];
        let stats = Paragraph::new(stats_text)
            .block(unfocused_block("統計"))
            .alignment(Alignment::Left);
        f.render_widget(stats, chunks[5]);

        // Issue #32: 次のマイルストーン表示 (= 「あと少し感」を常時演出)
        // XP / 各種実績の残量から最も小さいものを 1 行で出す。
        let milestone_line = if let Some(hint) = self.game_state.next_milestone() {
            Line::from(vec![
                Span::styled("next milestone: ", Style::default().fg(COLOR_LABEL)),
                Span::styled(
                    hint.label.clone(),
                    Style::default().fg(COLOR_EPIC).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(" (あと {})", hint.remaining),
                    Style::default()
                        .fg(COLOR_LEGENDARY)
                        .add_modifier(Modifier::BOLD),
                ),
            ])
        } else {
            Line::from(vec![Span::styled(
                "next milestone: 全マイルストーン達成済み",
                Style::default().fg(COLOR_LABEL),
            )])
        };
        let milestone_widget = Paragraph::new(vec![milestone_line]).alignment(Alignment::Left);
        f.render_widget(milestone_widget, chunks[6]);

        // Issue #30: 期限切れ間近 (残り 1 日以下) のキュリオン数を 1 行で警告。
        // 0 個のときも空行として描画して、レイアウト的に予約だけ残す
        // (Length(1) を消費しないと下のチャンクが詰まって見えなくなるため)。
        let now_utc = chrono::Utc::now();
        let near_expiry_count = player
            .collection
            .iter()
            .filter(|c| match c.days_remaining(now_utc) {
                Some(d) => d <= 1,
                None => false,
            })
            .count();
        let lifespan_line = if near_expiry_count > 0 {
            Line::from(vec![
                Span::styled(
                    "⚠ 期限切れ間近 (残り 1 日以下): ",
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{} 個", near_expiry_count),
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                ),
            ])
        } else {
            Line::from("")
        };
        f.render_widget(Paragraph::new(lifespan_line), chunks[7]);

        // Issue #38: 装備中 Curion のサマリを 1 行で表示。
        // 装備なしなら「装備: なし」、装備ありなら「装備: <display_name> (XP +N% / SAN ...)」。
        let equip_line: Line = match player.equipped_curion() {
            Some(c) => {
                let effect = player.current_equipment_effect();
                Line::from(vec![
                    Span::styled("装備: ", Style::default().fg(COLOR_LABEL)),
                    Span::styled(
                        c.display_name(),
                        Style::default().fg(COLOR_EPIC).add_modifier(Modifier::BOLD),
                    ),
                    Span::raw("  "),
                    Span::styled(effect.summary_line(), Style::default().fg(COLOR_SUCCESS)),
                ])
            }
            None => Line::from(vec![
                Span::styled("装備: ", Style::default().fg(COLOR_LABEL)),
                Span::styled("なし", Style::default().fg(COLOR_LABEL)),
            ]),
        };
        f.render_widget(Paragraph::new(equip_line), chunks[8]);

        // Issue #36: 段階進化ガチャ — 進化系列のトップ 3 を 1 行ずつ表示。
        // 「あと N 個で次段階」の期待感を Dashboard に常時演出する。
        // 計算ロジックは `crate::evolution::EvolutionDatabase::calculate_progress` に閉じ、
        // UI 側は値を読んで色付けするだけ。
        let mut evo_progress = self.evolution_db.calculate_progress(&player.collection);
        sort_progress_by_urgency(&mut evo_progress);
        let evo_lines: Vec<Line<'_>> = evo_progress
            .iter()
            .take(3)
            .map(|p| {
                if p.is_complete() {
                    Line::from(vec![
                        Span::styled("進化: ", Style::default().fg(COLOR_LABEL)),
                        Span::styled(
                            p.line.display_name.clone(),
                            Style::default().fg(Color::White),
                        ),
                        Span::styled(
                            "  ⭐ 完成",
                            Style::default()
                                .fg(Color::Green)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ])
                } else {
                    // 「あと 1 個」は強調色 (Cyan + Bold)、それ以外は Label color。
                    let remaining_style = if p.is_almost_complete() {
                        Style::default().fg(COLOR_RARE).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(COLOR_LABEL)
                    };
                    let stage_label = format!("Stage {}", p.current_stage);
                    let stage_color = if p.current_stage == 0 {
                        COLOR_LABEL
                    } else {
                        COLOR_EPIC
                    };
                    let next_text = match (p.next_stage_noun, p.next_stage_required) {
                        (Some(noun), Some(_req)) => {
                            format!(" (あと {} ×{} で次段階)", noun, p.remaining_to_next)
                        }
                        _ => String::new(),
                    };
                    Line::from(vec![
                        Span::styled("進化: ", Style::default().fg(COLOR_LABEL)),
                        Span::styled(
                            p.line.display_name.clone(),
                            Style::default().fg(Color::White),
                        ),
                        Span::raw("  "),
                        Span::styled(stage_label, Style::default().fg(stage_color)),
                        Span::styled(next_text, remaining_style),
                    ])
                }
            })
            .collect();
        let evo_widget = if evo_lines.is_empty() {
            Paragraph::new(Line::from(""))
        } else {
            Paragraph::new(evo_lines)
        };
        f.render_widget(evo_widget, chunks[9]);

        // Latest curion — Issue: jiwa を使った名前の bloom 演出。生成直後に
        // grapheme 単位で暗→白へフェードイン、~200 ms 経つと通常表示に戻る。
        if let Some(curion) = player.latest_curion() {
            let color = rarity_color(&curion.rarity);
            let stars = rarity_stars(&curion.rarity);
            let label = rarity_label(&curion.rarity);
            let mut spans = vec![
                Span::styled(stars, Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(
                    format!("[{label}]"),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
            ];
            spans.extend(self.render_latest_name_spans(&curion.display_name()));
            let latest =
                Paragraph::new(vec![Line::from(spans)]).block(unfocused_block("最新キュリオン"));
            f.render_widget(latest, chunks[10]);
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
            let label = rarity_label(&rarity);

            rarity_items.push(Line::from(vec![
                Span::styled(format!("{label:<4}"), Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(
                    bar(percentage as f64 / 100.0, 30),
                    Style::default().fg(color),
                ),
                Span::raw(format!("  {count} ({percentage}%)")),
            ]));
        }
        let rarity_widget = Paragraph::new(rarity_items).block(unfocused_block("レアリティ分布"));
        f.render_widget(rarity_widget, chunks[11]);

        // Category distribution
        let lang = self.game_state.language;
        let category_text = ALL_CATEGORIES
            .iter()
            .filter_map(|category| {
                let count = self.collection_count_by_category(category);
                (count > 0).then(|| format!("{}: {}個", category.display(lang), count))
            })
            .collect::<Vec<_>>()
            .join("  ");
        let category_widget = Paragraph::new(category_text).block(unfocused_block("カテゴリ分布"));
        f.render_widget(category_widget, chunks[12]);
    }

    /// 最新キュリオン名を grapheme 単位の Span 列にする。reveal 未開始 or 完了済みは
    /// 通常の白太字で 1 つの Span にまとめる (Span 数が膨らむと描画が重いため)。
    fn render_latest_name_spans(&self, name: &str) -> Vec<Span<'static>> {
        let now = Instant::now();
        match self.latest_reveal.as_ref() {
            Some(reveal) if !reveal.is_done(now) => reveal
                .snapshot(now)
                .into_iter()
                .map(|g| {
                    Span::styled(
                        g.text,
                        Style::default()
                            .fg(Color::Rgb(g.color.0, g.color.1, g.color.2))
                            .add_modifier(Modifier::BOLD),
                    )
                })
                .collect(),
            _ => vec![Span::styled(
                name.to_string(),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )],
        }
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
                Span::styled(
                    bar(xp_ratio, 40),
                    Style::default().fg(xp_bar_color(xp_ratio)),
                ),
                Span::raw(format!(
                    "  {}% ({}/{})",
                    player.xp_progress_percentage(),
                    player.xp,
                    player.xp_for_next_level()
                )),
            ]),
        ]));

        let list = List::new(items).block(focused_block("🎯 もうすぐ達成できる目標"));

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

        if collection.is_empty() {
            let empty = Paragraph::new(vec![
                Line::from(""),
                Line::from(vec![Span::styled(
                    "まだキュリオンがありません",
                    Style::default().fg(COLOR_LABEL),
                )]),
                Line::from(""),
                Line::from(vec![Span::styled(
                    "スペースキーでキュリオンを生成",
                    Style::default().fg(COLOR_LABEL),
                )]),
            ])
            .block(focused_block("コレクション [0 個]"))
            .alignment(Alignment::Center);
            f.render_widget(empty, area);
            return;
        }

        // Issue #22: 上段=コレクションリスト、下段=選択中キュリオンの詳細
        // Issue #27 で詳細ペインにフレーバー + 入手履歴の 2 行構成にしたため、
        // 旧 Length(3) では 1 行しか入らない。Length(4) に拡張する
        // (上下 border 各 1 + 中身 2 行)。
        // Issue #31: 正規表現フィルタ入力中 or フィルタ適用中はリストの上に 1 行
        // 検索プロンプトを表示する。
        let show_filter_line =
            self.filter_mode || self.compiled_filter.is_some() || self.filter_error.is_some();
        // Issue #38: 詳細ペインに「意味タグ (上位 3)」と「装備状態」の 2 行を追加
        // (Length 4 → 6: border 2 + 中身 4 行)。
        let outer_split = if show_filter_line {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Min(3),
                    Constraint::Length(6),
                ])
                .split(area)
        } else {
            Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(0),
                    Constraint::Min(3),
                    Constraint::Length(6),
                ])
                .split(area)
        };
        let filter_area = outer_split[0];
        let list_area = outer_split[1];
        let detail_area = outer_split[2];

        if show_filter_line {
            self.render_filter_input(f, filter_area);
        }

        // Issue #31: 適用中の正規表現でフィルタ。
        // フィルタが None ならフルコレクションをそのまま使う。
        let filtered: Vec<&Curion> = match &self.compiled_filter {
            Some(re) => collection.iter().filter(|c| match_curion(re, c)).collect(),
            None => collection.iter().collect(),
        };

        let list_title = if self.compiled_filter.is_some() {
            format!(
                "コレクション [{} matched / {} total]",
                filtered.len(),
                collection.len(),
            )
        } else {
            format!("コレクション [{} 個]", collection.len())
        };

        let now_utc = chrono::Utc::now();
        let items: Vec<ListItem> = filtered
            .iter()
            .rev()
            .enumerate()
            .skip(self.detail_scroll)
            .take((list_area.height as usize).saturating_sub(2).max(1))
            .map(|(i, curion)| {
                let curion = *curion;
                let color = rarity_color(&curion.rarity);
                let stars = rarity_stars(&curion.rarity);
                let label = rarity_label(&curion.rarity);
                let index = filtered.len() - i;

                // Issue #30: 残り寿命を 1 行右側に表示。1 日以下で Red、3 日以下で
                // Yellow、それ以上は薄いグレー。寿命なし (旧セーブ) は `--`。
                let lifespan_span = lifespan_span_for(curion, now_utc);

                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(format!("#{index:<4}"), Style::default().fg(COLOR_LABEL)),
                        Span::styled(stars, Style::default().fg(color)),
                        Span::raw(" "),
                        Span::styled(format!("[{label:<4}]"), Style::default().fg(color)),
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
                        Span::raw("  "),
                        lifespan_span,
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

        let list = List::new(items).block(focused_block(list_title));

        f.render_widget(list, list_area);

        // 詳細ペイン: スクロール先頭（= 視野の最上段）にあるキュリオンのフレーバーを表示する
        // Issue #31: フィルタ適用中はフィルタ後リストの先頭を詳細対象にする
        let focused: Option<&Curion> = if filtered.is_empty() {
            None
        } else {
            let focus_index_rev = self.detail_scroll.min(filtered.len().saturating_sub(1));
            // filtered も collection と同様「古い順」のままなので、表示は逆順 (newest first)
            let focus_index = filtered.len() - 1 - focus_index_rev;
            filtered.get(focus_index).copied()
        };

        let flavor_text: String = focused
            .and_then(|c| {
                self.generator
                    .database()
                    .flavor_for(&c.noun)
                    .map(|s| s.to_string())
            })
            .unwrap_or_else(|| "(フレーバー未登録)".to_string());
        // Issue #38: 装備中の curion なら title に [装備中] マーカー
        let equipped_id = player.equipment.curion_id.as_deref();
        let is_focused_equipped = focused
            .map(|c| Some(c.id.as_str()) == equipped_id)
            .unwrap_or(false);
        let title = focused
            .map(|c| {
                if is_focused_equipped {
                    format!("詳細: {}  [装備中]", c.display_name())
                } else {
                    format!("詳細: {}", c.display_name())
                }
            })
            .unwrap_or_else(|| "詳細".to_string());

        // Issue #27: 入手日時 + 通算回数を 2 行目に併記
        let acquisition_line: Line = match focused {
            Some(c) => Line::from(vec![Span::styled(
                c.format_acquisition_detail(),
                Style::default().fg(Color::DarkGray),
            )]),
            None => Line::from(""),
        };

        // Issue #38: 意味タグ上位 3 + 装備状態を 2 行で表示
        let semantic_line: Line = match focused {
            Some(c) => {
                let profile = SemanticProfile::from_curion(c);
                let top = profile.dominant_tags(3);
                let mut spans = vec![Span::styled("意味タグ: ", Style::default().fg(COLOR_LABEL))];
                for (i, (tag, score)) in top.iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::raw(" / "));
                    }
                    let stars = if *score >= 0.66 {
                        "★★★"
                    } else if *score >= 0.33 {
                        "★★"
                    } else {
                        "★"
                    };
                    spans.push(Span::styled(
                        format!("{} {}", tag.label(), stars),
                        Style::default().fg(COLOR_EPIC),
                    ));
                }
                Line::from(spans)
            }
            None => Line::from(""),
        };

        let equip_line: Line = match focused {
            Some(_) if is_focused_equipped => Line::from(vec![Span::styled(
                "[装備中] e で解除",
                Style::default()
                    .fg(COLOR_SUCCESS)
                    .add_modifier(Modifier::BOLD),
            )]),
            Some(_) => Line::from(vec![Span::styled(
                "e で装備",
                Style::default().fg(COLOR_LABEL),
            )]),
            None => Line::from(""),
        };

        let paragraph = Paragraph::new(vec![
            Line::from(vec![Span::styled(
                flavor_text,
                Style::default().fg(Color::Gray),
            )]),
            acquisition_line,
            semantic_line,
            equip_line,
        ])
        .block(unfocused_block(title))
        .wrap(ratatui::widgets::Wrap { trim: true });
        f.render_widget(paragraph, detail_area);
    }

    fn render_collection_dictionary(&self, f: &mut Frame<'_>, area: Rect) {
        // Issue #31: 図鑑側にも検索プロンプトを表示する
        let show_filter_line =
            self.filter_mode || self.compiled_filter.is_some() || self.filter_error.is_some();
        let (filter_area, body_area) = if show_filter_line {
            let split = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(0)])
                .split(area);
            (Some(split[0]), split[1])
        } else {
            (None, area)
        };

        if let Some(area) = filter_area {
            self.render_filter_input(f, area);
        }

        let block = focused_block("図鑑");
        let inner = block.inner(body_area);
        f.render_widget(block, body_area);

        let panes = Layout::default()
            .direction(Direction::Horizontal)
            // Categories pane width (docs/design.md spec)
            .constraints([Constraint::Length(22), Constraint::Min(0)])
            .split(inner);

        self.render_dictionary_categories(f, panes[0]);
        self.render_dictionary_entries(f, panes[1]);
    }

    /// Issue #31: 正規表現フィルタの入力プロンプト 1 行を描画する。
    ///
    /// 表示形式:
    /// - 入力モード中: `/{filter_text}_`
    /// - 入力モード外でフィルタ適用中: `/{filter_text}  (Esc 解除)`
    /// - 無効パターン: 末尾に赤字で `! invalid regex: <reason>` を続ける
    fn render_filter_input(&self, f: &mut Frame<'_>, area: Rect) {
        let mut spans: Vec<Span> = Vec::new();
        let prompt_style = Style::default().fg(Color::White).bg(Color::DarkGray);
        let cursor_style = Style::default()
            .fg(Color::White)
            .bg(Color::DarkGray)
            .add_modifier(Modifier::REVERSED);

        spans.push(Span::styled(" / ", prompt_style));
        spans.push(Span::styled(self.filter_text.clone(), prompt_style));
        if self.filter_mode {
            spans.push(Span::styled(" ", cursor_style));
        } else if self.compiled_filter.is_some() || self.filter_error.is_some() {
            spans.push(Span::styled(
                "  (Esc 解除)",
                Style::default().fg(COLOR_LABEL),
            ));
        }
        if let Some(err) = &self.filter_error {
            spans.push(Span::raw("  "));
            spans.push(Span::styled(
                format!("! {err}"),
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));
        }

        let widget = Paragraph::new(Line::from(spans)).style(Style::default().bg(Color::Black));
        f.render_widget(widget, area);
    }

    fn render_dictionary_categories(&self, f: &mut Frame<'_>, area: Rect) {
        let focused_index = self.dictionary_category_index.min(ALL_CATEGORIES.len() - 1);

        let items: Vec<ListItem> = ALL_CATEGORIES
            .iter()
            .enumerate()
            .map(|(i, category)| {
                let (owned, total) = self.dictionary_category_counts(category);
                let is_selected = i == focused_index;
                let prefix = if is_selected { "> " } else { "  " };
                let style = if is_selected {
                    Style::default()
                        .fg(Color::Black)
                        .bg(COLOR_RARE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                // CJK 混在のためバイト幅ではなく表示セル幅でパディング
                let name = pad_display(category.display(self.game_state.language), 8);
                ListItem::new(format!("{prefix}{name} {owned:>3}/{total:<3}")).style(style)
            })
            .collect();

        let list = List::new(items).block(unfocused_block("Categories"));
        f.render_widget(list, area);
    }

    fn render_dictionary_entries(&self, f: &mut Frame<'_>, area: Rect) {
        let focused_index = self.dictionary_category_index.min(ALL_CATEGORIES.len() - 1);
        let category = &ALL_CATEGORIES[focused_index];

        let (total_owned, total_entries) = self.dictionary_total_counts();
        let total_pct = if total_entries > 0 {
            (total_owned as f64 / total_entries as f64) * 100.0
        } else {
            0.0
        };

        let (cat_owned, cat_total) = self.dictionary_category_counts(category);
        let cat_pct = if cat_total > 0 {
            (cat_owned as f64 / cat_total as f64) * 100.0
        } else {
            0.0
        };

        let title = format!(
            "全体: {total_owned}/{total_entries} ({total_pct:.1}%) | {cat_name}: {cat_owned}/{cat_total} ({cat_pct:.1}%)",
            cat_name = category.display(self.game_state.language),
        );

        let block = unfocused_block(title);
        let inner = block.inner(area);
        f.render_widget(block, area);

        let visible_height = inner.height as usize;
        if visible_height == 0 {
            return;
        }

        // 名詞リストを取得（DBの順序を維持）。DB に無いカテゴリは空スライスとして扱う。
        let nouns: &[crate::generator::NounEntry] = self
            .generator
            .database()
            .get_nouns(category)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);

        if nouns.is_empty() {
            let widget = Paragraph::new(vec![Line::from(Span::styled(
                "(このカテゴリは未対応)",
                Style::default().fg(COLOR_LABEL),
            ))]);
            f.render_widget(widget, inner);
            return;
        }

        // Issue #31: フィルタ適用中は match した名詞だけに絞る。
        let filtered_nouns: Vec<&crate::generator::NounEntry> = match &self.compiled_filter {
            Some(re) => nouns
                .iter()
                .filter(|e| self.match_noun_for_dictionary(re, category, &e.name))
                .collect(),
            None => nouns.iter().collect(),
        };

        // 最後のページが収まる位置までクランプ。1ページに収まる場合は 0。
        // 1 エントリあたり 1〜2 行（獲得済みで flavor 付きなら 2 行）。
        // 既存の挙動を維持するため、スクロール単位は引き続き「エントリ数」とする。
        let max_scroll = filtered_nouns.len().saturating_sub(visible_height);
        let scroll = self.dictionary_scroll.min(max_scroll);

        // 利用可能セル幅（罫線控除済み）から flavor の折り返し閾値を決める
        let inner_width = inner.width as usize;
        let mut lines: Vec<Line> = Vec::with_capacity(visible_height);
        for entry in filtered_nouns.iter().skip(scroll) {
            if lines.len() >= visible_height {
                break;
            }
            let entry_lines = self.render_dictionary_entry(entry, inner_width);
            for line in entry_lines {
                if lines.len() >= visible_height {
                    break;
                }
                lines.push(line);
            }
        }

        let widget = Paragraph::new(lines);
        f.render_widget(widget, inner);
    }

    /// Issue #31: 図鑑モードで名詞 1 個が正規表現にマッチするか判定する。
    ///
    /// 対象:
    /// - 名詞名
    /// - `{category} の {noun}` (display_name 相当)
    /// - カテゴリ名
    /// - 所持している場合: その名詞で所持しているキュリオンのレアリティラベル
    fn match_noun_for_dictionary(
        &self,
        re: &regex::Regex,
        category: &Category,
        noun_name: &str,
    ) -> bool {
        if re.is_match(noun_name) {
            return true;
        }
        if re.is_match(&format!("{} の {}", category.as_str(), noun_name)) {
            return true;
        }
        if re.is_match(category.as_str()) {
            return true;
        }
        // 所持していればレアリティラベルでもマッチさせる
        for curion in self.game_state.player.collection.iter() {
            if curion.noun == noun_name && re.is_match(rarity_filter_label(&curion.rarity)) {
                return true;
            }
        }
        false
    }

    /// Issue #22: 図鑑エントリを 1〜2 行で描画する。
    /// 獲得済みでフレーバーがあれば 2 行目に flavor を表示し、行幅を超える場合は表示セル幅で
    /// 切り詰めて末尾に `…` を付与する。`inner_width` は描画領域のセル幅。
    fn render_dictionary_entry(
        &self,
        entry: &crate::generator::NounEntry,
        inner_width: usize,
    ) -> Vec<Line<'_>> {
        let main_line = self.render_dictionary_line(&entry.name);

        let acquired = self
            .game_state
            .player
            .collection
            .iter()
            .any(|c| c.noun == entry.name);

        let mut out: Vec<Line<'_>> = vec![main_line];
        if acquired {
            if let Some(flavor) = entry.flavor.as_deref() {
                // インデント `  ` の表示幅を引いて切り詰める
                let indent = "  ";
                let budget = inner_width.saturating_sub(UnicodeWidthStr::width(indent));
                let body = truncate_display(flavor, budget);
                out.push(Line::from(vec![
                    Span::raw(indent),
                    Span::styled(body, Style::default().fg(COLOR_LABEL)),
                ]));
            }
        }
        out
    }

    fn render_dictionary_line(&self, noun_name: &str) -> Line<'_> {
        let player = &self.game_state.player;
        // この名詞の獲得済みインスタンスを集約
        let mut count: usize = 0;
        let mut highest_rarity: Option<Rarity> = None;
        let mut latest: Option<chrono::DateTime<chrono::Utc>> = None;

        for curion in player.collection.iter().filter(|c| c.noun == noun_name) {
            count += 1;
            highest_rarity = Some(match highest_rarity {
                Some(r) if rarity_rank(&r) >= rarity_rank(&curion.rarity) => r,
                _ => curion.rarity,
            });
            latest = Some(match latest {
                Some(d) if d >= curion.acquired_at => d,
                _ => curion.acquired_at,
            });
        }

        if count == 0 {
            // 未獲得
            Line::from(vec![Span::styled(
                pad_display("？？？", 12),
                Style::default().fg(COLOR_LABEL),
            )])
        } else {
            let rarity = highest_rarity.unwrap_or(Rarity::Common);
            let color = rarity_color(&rarity);
            let stars = rarity_stars(&rarity);
            let label = rarity_label(&rarity);
            let date = latest
                .map(|d| d.format("%Y-%m-%d").to_string())
                .unwrap_or_else(|| "-".to_string());

            Line::from(vec![
                Span::styled(
                    // CJK 混在のため表示セル幅でパディング
                    pad_display(noun_name, 12),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::styled(format!("{stars:<4}"), Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(format!("[{label:<4}]"), Style::default().fg(color)),
                Span::raw(" "),
                Span::styled(date, Style::default().fg(Color::DarkGray)),
                Span::raw(" "),
                Span::styled(format!("×{count}"), Style::default().fg(COLOR_SUCCESS)),
            ])
        }
    }

    fn dictionary_category_counts(&self, category: &Category) -> (usize, usize) {
        let total = self
            .generator
            .database()
            .get_nouns(category)
            .map(|nouns| nouns.len())
            .unwrap_or(0);
        // DB に存在しない noun を collection が持っていても owned > total にならないようクランプ
        let owned = self
            .collection_unique_count_by_category(category)
            .min(total);
        (owned, total)
    }

    fn dictionary_total_counts(&self) -> (usize, usize) {
        let mut total = 0;
        let mut owned = 0;
        for category in ALL_CATEGORIES.iter() {
            let (o, t) = self.dictionary_category_counts(category);
            owned += o;
            total += t;
        }
        (owned, total)
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
        let unlocked = self.game_state.achievement_manager.get_unlocked_count();
        let total = self.game_state.achievement_manager.get_total_count();
        let percentage = self.game_state.achievement_manager.get_unlock_percentage();

        let block = focused_block(format!(
            "{title} | {unlocked} / {total} 解除済み ({percentage}%)"
        ));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let item_height = 6_u16;
        let visible = achievements
            .iter()
            .skip(self.detail_scroll)
            .take((inner.height / item_height).max(1) as usize);

        for (index, (achievement, progress)) in visible.enumerate() {
            let y = inner.y + index as u16 * item_height;
            if y + item_height > inner.bottom() {
                break;
            }

            let item_rect = Rect {
                x: inner.x,
                y,
                width: inner.width,
                height: item_height,
            };
            let item_block = unfocused_block("");
            let item_inner = item_block.inner(item_rect);
            f.render_widget(item_block, item_rect);

            let icon = if progress.unlocked && !progress.claimed {
                "✅💰"
            } else if progress.unlocked {
                "✅"
            } else {
                "🔒"
            };

            let header = Paragraph::new(vec![
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
                Line::from(vec![Span::raw("  "), Span::raw(&achievement.description)]),
            ]);
            let header_area = Rect {
                x: item_inner.x,
                y: item_inner.y,
                width: item_inner.width,
                height: 2,
            };
            f.render_widget(header, header_area);

            let ratio = progress.progress_ratio();
            let gauge = LineGauge::default()
                .ratio(ratio)
                .label(format!(
                    "[{}] {:>3}% ({}/{})",
                    bar(ratio, 10),
                    progress.progress_percentage(),
                    progress.current,
                    progress.target
                ))
                .filled_style(Style::default().fg(if progress.unlocked {
                    COLOR_SUCCESS
                } else {
                    COLOR_BAR_COLD
                }))
                .unfilled_style(Style::default().fg(COLOR_LABEL));
            let gauge_area = Rect {
                x: item_inner.x + 1,
                y: item_inner.y + 2,
                width: item_inner.width.saturating_sub(2),
                height: 1,
            };
            f.render_widget(gauge, gauge_area);

            let reward = Paragraph::new(Line::from(vec![
                Span::raw("報酬: "),
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
            let reward_area = Rect {
                x: item_inner.x,
                y: item_inner.y + 3,
                width: item_inner.width,
                height: 1,
            };
            f.render_widget(reward, reward_area);
        }
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
        let block = focused_block("レアリティ");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Length(5),
                Constraint::Min(8),
            ])
            .split(inner);

        let summary = Paragraph::new(vec![
            Line::from(vec![
                Span::styled("LEVEL", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::styled(
                    format!("{}", player.level),
                    Style::default()
                        .fg(COLOR_LEGENDARY)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("    "),
                Span::styled("TOTAL", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::styled(
                    format!("{}", player.total_acquired()),
                    Style::default()
                        .fg(COLOR_SUCCESS)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("RATE", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::styled(
                    format!("{:.1}/h", player.acquisition_rate_per_hour()),
                    Style::default().fg(COLOR_RARE),
                ),
                Span::raw("    "),
                Span::styled("AVG/DAY", Style::default().fg(COLOR_LABEL)),
                Span::raw("  "),
                Span::styled(
                    format!("{:.1}", player.average_daily_acquired()),
                    Style::default().fg(COLOR_EPIC),
                ),
            ]),
        ])
        .block(unfocused_block("PLAYER"));
        f.render_widget(summary, chunks[0]);

        let recent = Sparkline::default()
            .block(unfocused_block("RECENT ACQUISITIONS"))
            .data(self.recent_acquisition_buckets(RECENT_ACTIVITY_BUCKETS))
            .style(Style::default().fg(COLOR_RARE));
        f.render_widget(recent, chunks[1]);

        let rarity_bars = vec![
            Bar::default()
                .label("COM".into())
                .value(player.count_by_rarity(Rarity::Common) as u64)
                .style(Style::default().fg(COLOR_COMMON)),
            Bar::default()
                .label("RARE".into())
                .value(player.count_by_rarity(Rarity::Rare) as u64)
                .style(Style::default().fg(COLOR_RARE)),
            Bar::default()
                .label("EPIC".into())
                .value(player.count_by_rarity(Rarity::Epic) as u64)
                .style(Style::default().fg(COLOR_EPIC)),
            Bar::default()
                .label("LEG".into())
                .value(player.count_by_rarity(Rarity::Legendary) as u64)
                .style(Style::default().fg(COLOR_LEGENDARY)),
        ];
        let rarity_chart = BarChart::default()
            .block(unfocused_block("RARITY BREAKDOWN"))
            .data(BarGroup::default().bars(&rarity_bars))
            .bar_width(6)
            .bar_gap(1)
            .value_style(
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            )
            .label_style(Style::default().fg(Color::White));
        f.render_widget(rarity_chart, chunks[2]);
    }

    fn render_stats_category(&self, f: &mut Frame<'_>, area: Rect) {
        let block = focused_block("カテゴリ");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(9), Constraint::Min(7)])
            .split(inner);

        let lang = self.game_state.language;
        let category_bars: Vec<Bar> = ALL_CATEGORIES
            .iter()
            .map(|category| {
                Bar::default()
                    .label(category.display(lang).into())
                    .value(self.collection_count_by_category(category) as u64)
                    .style(Style::default().fg(COLOR_RARE))
            })
            .collect();

        let chart = BarChart::default()
            .block(unfocused_block("CATEGORY BREAKDOWN"))
            .data(BarGroup::default().bars(&category_bars))
            .bar_width(5)
            .bar_gap(1)
            .value_style(Style::default().fg(Color::White))
            .label_style(Style::default().fg(Color::White));
        f.render_widget(chart, chunks[0]);

        let lines: Vec<Line> = ALL_CATEGORIES
            .iter()
            .map(|category| {
                let total = self.collection_count_by_category(category);
                let unique = self.collection_unique_count_by_category(category);
                Line::from(vec![
                    Span::styled(
                        format!("{:<8}", category.display(lang)),
                        Style::default().fg(COLOR_RARE),
                    ),
                    Span::raw(" "),
                    Span::raw(format!("{total:>3} 所持 / {unique:>3} 種類")),
                ])
            })
            .collect();

        let widget = Paragraph::new(lines).block(unfocused_block("CATEGORY DETAIL"));
        f.render_widget(widget, chunks[1]);
    }

    fn render_stats_timeline(&self, f: &mut Frame<'_>, area: Rect) {
        let player = &self.game_state.player;
        let block = focused_block("時系列");
        let inner = block.inner(area);
        f.render_widget(block, area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5),
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ])
            .split(inner);

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
                Span::raw(format!("{} 秒", player.total_play_time)),
            ]),
        ];

        let widget = Paragraph::new(lines).block(unfocused_block("SESSION"));
        f.render_widget(widget, chunks[0]);

        let login_ratio = (player.consecutive_login_days.min(30) as f64) / 30.0;
        let streak = LineGauge::default()
            .block(unfocused_block("LOGIN STREAK"))
            .ratio(login_ratio)
            .label(format!(
                "[{}] {:>2} / 30 days",
                bar(login_ratio, 12),
                player.consecutive_login_days.min(30)
            ))
            .filled_style(Style::default().fg(COLOR_SUCCESS))
            .unfilled_style(Style::default().fg(COLOR_LABEL));
        f.render_widget(streak, chunks[1]);

        let today_ratio = (player.today_acquired.min(player.max_daily_acquired.max(1)) as f64)
            / (player.max_daily_acquired.max(1) as f64);
        let today = LineGauge::default()
            .block(unfocused_block("TODAY VS BEST"))
            .ratio(today_ratio)
            .label(format!(
                "[{}] {:>2} / {}",
                bar(today_ratio, 12),
                player.today_acquired,
                player.max_daily_acquired.max(1)
            ))
            .filled_style(Style::default().fg(COLOR_RARE))
            .unfilled_style(Style::default().fg(COLOR_LABEL));
        f.render_widget(today, chunks[2]);

        // Issue #26: 直近 30 日の「日別」獲得数 Sparkline。
        // 既存の COLLECTION RATE (total span 16-bucket) を置き換え、btop 風の
        // 日次推移を表示する。Player 側の純粋関数を呼ぶことでテスト可能にしている。
        let data = player.daily_acquisition_counts(30, chrono::Utc::now());
        let sparkline = Sparkline::default()
            .block(unfocused_block("DAILY (last 30 days)"))
            .data(&data)
            .style(Style::default().fg(COLOR_RARE));
        f.render_widget(sparkline, chunks[3]);
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
        .block(focused_block("合成実行"))
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
                .block(unfocused_block("Help"))
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
                .block(focused_block("Ingredient 1"))
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
                        .fg(Color::Black)
                        .bg(COLOR_RARE)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                ListItem::new(format!(
                    "{} {} [{}]",
                    rarity_stars(&curion.rarity),
                    curion.noun,
                    rarity_label(&curion.rarity),
                ))
                .style(style)
            })
            .collect();

        let list = List::new(items)
            .block(focused_block("Select Ingredient 1"))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(COLOR_RARE)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_widget(list, area);
    }

    fn render_recipe_list(&self, f: &mut Frame<'_>, area: Rect) {
        // Issue #37: visibility に応じて未発見レシピの材料/結果/名前を隠す。
        // 発見済みは visibility に関わらず常に完全表示。
        let collection = &self.game_state.player.collection;
        let items: Vec<ListItem> = self
            .game_state
            .synthesis_manager
            .recipe_db()
            .all_recipes()
            .iter()
            .enumerate() // Unknown レシピの番号付けに使う (絶対 index)
            .skip(self.detail_scroll)
            .take(area.height as usize - 3)
            .map(|(idx, recipe)| {
                let discovered = self.game_state.synthesis_manager.is_discovered(&recipe.id);
                let visibility = recipe.visibility;
                // 発見済みは Public 扱い。
                let effective_visibility = if discovered {
                    crate::synthesis::RecipeVisibility::Public
                } else {
                    visibility
                };

                let marker = if discovered { "✓" } else { "?" };

                // 進捗 (材料の何種類が手元に揃っているか)
                let progress = recipe.ingredient_progress(collection);

                // Issue #28: 合成成功確率を LineGauge 風に表示
                // Issue #35: 高リスクレシピは赤系で SAFE/RISKY バッジを併記する
                let success_p = self
                    .game_state
                    .synthesis_manager
                    .success_probability_for_recipe(recipe);
                let success_pct = (success_p * 100.0).round() as u16;
                let is_risky = recipe.is_high_risk();

                // Issue #37: 公開状態に応じて色を切り替える。
                // - Public/discovered: 通常 (討伐済みは Success 色 / 未発見は RARE 色)
                // - Partial: ラベルは COLOR_LABEL (薄め) で出す
                // - Unknown: DarkGray (さらに暗く)
                // - 全材料揃いなら COLOR_SUCCESS でハイライト (発見手前の煽り)
                let name_color = if progress.all_satisfied && !discovered {
                    COLOR_SUCCESS
                } else {
                    match effective_visibility {
                        crate::synthesis::RecipeVisibility::Public => Color::White,
                        crate::synthesis::RecipeVisibility::Partial => COLOR_LABEL,
                        crate::synthesis::RecipeVisibility::Unknown => Color::DarkGray,
                    }
                };

                let probability_color = if is_risky {
                    COLOR_BAR_HOT
                } else if discovered {
                    COLOR_SUCCESS
                } else {
                    COLOR_RARE
                };
                let (badge_text, badge_color) = if is_risky {
                    ("[RISKY]", COLOR_BAR_HOT)
                } else {
                    ("[SAFE]", COLOR_SUCCESS)
                };
                // 失敗時挙動の説明文 (RISKY のみ表示)
                let failure_hint = if is_risky {
                    match &recipe.failure_mode {
                        crate::synthesis::FailureMode::LoseAll => " 失敗時: 素材消滅",
                        crate::synthesis::FailureMode::Salvage { .. } => " 失敗時: 残骸獲得",
                        crate::synthesis::FailureMode::NoLoss => " 失敗時: 保険",
                    }
                } else {
                    ""
                };
                let probability_line = Line::from(vec![
                    Span::raw("    "),
                    Span::styled(
                        badge_text,
                        Style::default()
                            .fg(badge_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled("合成確率: ", Style::default().fg(COLOR_LABEL)),
                    Span::styled(
                        format!("{success_pct:>3}% "),
                        Style::default()
                            .fg(probability_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{}]", bar(success_p, 10)),
                        Style::default().fg(probability_color),
                    ),
                    Span::styled(failure_hint, Style::default().fg(COLOR_BAR_HOT)),
                ]);

                // Issue #37: 名前と式の行。Unknown は recipe.name を出さず、display_label
                // の "未確認レシピ #NN" だけにする。Partial/Public は recipe.name は表示しつつ
                // 式部分 (材料 → 結果) を visibility に従って隠す。
                let display_label = recipe.display_label(collection, discovered, idx);
                let name_line =
                    if effective_visibility == crate::synthesis::RecipeVisibility::Unknown {
                        Line::from(vec![
                            Span::styled(marker, Style::default().fg(Color::DarkGray)),
                            Span::raw(" "),
                            Span::styled(
                                display_label.clone(),
                                Style::default().fg(name_color).add_modifier(Modifier::BOLD),
                            ),
                        ])
                    } else {
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
                                recipe.name.clone(),
                                Style::default().fg(name_color).add_modifier(Modifier::BOLD),
                            ),
                        ])
                    };

                // Issue #37: 進捗行。「進捗: 2/2 ✓」または「進捗: 1/2 (あと N 種)」。
                // Unknown は仕様上「存在しか分からない」ので進捗は出さず、空行とのトレードに
                // しないため進捗が 0/total の場合のみ常時 1 行で表示する。
                let progress_line =
                    if effective_visibility == crate::synthesis::RecipeVisibility::Unknown {
                        // Unknown は進捗を出さない (材料の正体がバレるため)
                        Line::from("")
                    } else {
                        let progress_text = if progress.all_satisfied {
                            format!("    進捗: {}/{}  ✓", progress.satisfied, progress.total)
                        } else {
                            // Issue #37: 残数算出は SynthesisRecipe::remaining_categories に集約。
                            let remaining = recipe.remaining_categories(collection);
                            format!(
                                "    進捗: {}/{} (あと {} 種)",
                                progress.satisfied, progress.total, remaining
                            )
                        };
                        let progress_color = if progress.all_satisfied {
                            COLOR_SUCCESS
                        } else {
                            COLOR_LABEL
                        };
                        Line::from(Span::styled(
                            progress_text,
                            Style::default().fg(progress_color),
                        ))
                    };

                // 説明 + 式 (Unknown は description も隠す)
                let body_line = match effective_visibility {
                    crate::synthesis::RecipeVisibility::Unknown => Line::from(Span::styled(
                        "    (??? の手がかりはまだ無い)".to_string(),
                        Style::default().fg(Color::DarkGray),
                    )),
                    _ => Line::from(Span::styled(
                        format!("    {} -> {}", recipe.description, display_label),
                        Style::default().fg(name_color),
                    )),
                };

                ListItem::new(vec![
                    name_line,
                    body_line,
                    progress_line,
                    probability_line,
                    Line::from(""),
                ])
            })
            .collect();

        let list = List::new(items).block(focused_block("レシピ一覧"));
        f.render_widget(list, area);
    }

    fn render_selected_first(&self, f: &mut Frame<'_>, curion: &crate::curion::Curion, area: Rect) {
        let text = format!(
            "Ingredient 1:\n\n{} {}\nCategory: {:?}\nRarity: [{}]",
            rarity_stars(&curion.rarity),
            curion.noun,
            curion.category,
            rarity_label(&curion.rarity),
        );

        let widget = Paragraph::new(text)
            .block(unfocused_block("Selected"))
            .style(Style::default().fg(COLOR_SUCCESS));

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
                .block(focused_block("Ingredient 2"))
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
                        .fg(Color::Black)
                        .bg(COLOR_RARE)
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

                // Issue #28: この候補で実際に通るレシピの成功確率を表示
                // (first + candidate) が複数レシピにマッチする可能性もあるので、
                // 「最初にヒットするレシピ」(try_synthesize の挙動と一致) の確率を見せる。
                // Issue #35: 高リスクレシピは [RISKY] バッジ + 失敗時挙動も併記。
                let probability_text = self
                    .first_matching_recipe_for_pair(first_curion, &candidate.noun)
                    .map(|recipe| {
                        let p = self
                            .game_state
                            .synthesis_manager
                            .success_probability_for_recipe(recipe);
                        let badge = if recipe.is_high_risk() {
                            let mode = match &recipe.failure_mode {
                                crate::synthesis::FailureMode::LoseAll => "素材消滅",
                                crate::synthesis::FailureMode::Salvage { .. } => "残骸獲得",
                                crate::synthesis::FailureMode::NoLoss => "保険",
                            };
                            format!(" [RISKY:{mode}]")
                        } else {
                            " [SAFE]".to_string()
                        };
                        format!(
                            " — 合成確率 {:>3}% [{}]{}",
                            (p * 100.0).round() as u16,
                            bar(p, 8),
                            badge,
                        )
                    })
                    .unwrap_or_default();

                ListItem::new(format!(
                    "{} {} (×{}) {} {}{}",
                    discovered_mark,
                    candidate.noun,
                    candidate.available_count,
                    result_text,
                    candidate.category.display(self.game_state.language),
                    probability_text,
                ))
                .style(style)
            })
            .collect();

        let list = List::new(items)
            .block(focused_block("Select Ingredient 2"))
            .highlight_style(
                Style::default()
                    .fg(Color::Black)
                    .bg(COLOR_RARE)
                    .add_modifier(Modifier::BOLD),
            );

        f.render_widget(list, area);
    }

    /// Issue #28: 1 つ目の材料と 2 つ目候補名詞のペアで最初にマッチするレシピを返す。
    /// `SynthesisManager::try_synthesize` の挙動 (matching_recipes[0]) と一致する。
    fn first_matching_recipe_for_pair(
        &self,
        first: &crate::curion::Curion,
        second_noun: &str,
    ) -> Option<&crate::synthesis::SynthesisRecipe> {
        // 2 つ目候補のプレースホルダ Curion を作って find_matching_recipes に渡す。
        // available_count などはチェックされないため、レアリティ/カテゴリは
        // 「同名詞のうち代表」として collection から拾う。
        let second_curion = self
            .game_state
            .player
            .collection
            .iter()
            .find(|c| c.noun == second_noun)?;

        let ingredients = vec![first.clone(), second_curion.clone()];
        self.game_state
            .synthesis_manager
            .recipe_db()
            .find_matching_recipes(&ingredients)
            .into_iter()
            .next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curion::Curion;
    use crate::synthesis::{RecipeDatabase, SynthesisManager};
    use uuid::Uuid;

    fn empty_app() -> App {
        let recipe_db = RecipeDatabase::load_embedded().expect("Failed to load recipe database");
        let synthesis_manager = SynthesisManager::new(recipe_db);
        let game_state = GameState::new(synthesis_manager);
        App::new(game_state)
    }

    fn make_curion(noun: &str, category: Category, rarity: Rarity) -> Curion {
        Curion::new(Uuid::new_v4(), noun.to_string(), category, rarity, 0.5, 0.5)
    }

    /// 図鑑モード（Collection タブの section 1）に切り替えるヘルパー
    fn enter_dictionary_mode(app: &mut App) {
        app.set_tab(Tab::Collection);
        app.next_section();
        debug_assert!(app.is_collection_dictionary());
    }

    #[test]
    fn test_rarity_rank_ordering() {
        assert!(rarity_rank(&Rarity::Common) < rarity_rank(&Rarity::Rare));
        assert!(rarity_rank(&Rarity::Rare) < rarity_rank(&Rarity::Epic));
        assert!(rarity_rank(&Rarity::Epic) < rarity_rank(&Rarity::Legendary));
    }

    #[test]
    fn test_dictionary_category_counts_empty_collection() {
        let app = empty_app();
        for category in ALL_CATEGORIES.iter() {
            let (owned, total) = app.dictionary_category_counts(category);
            assert_eq!(owned, 0, "empty collection should have owned=0");
            assert!(
                total > 0,
                "category {category:?} should have at least one noun in DB"
            );
        }
    }

    #[test]
    fn test_dictionary_category_counts_partial() {
        let mut app = empty_app();
        let curion = make_curion("テスト名詞A", Category::Animal, Rarity::Common);
        app.game_state.player.collection.push(curion);
        let (owned, _total) = app.dictionary_category_counts(&Category::Animal);
        assert_eq!(owned, 1, "1 種獲得時 owned=1");

        // 他のカテゴリは影響を受けない
        let (other_owned, _) = app.dictionary_category_counts(&Category::Plant);
        assert_eq!(other_owned, 0);
    }

    #[test]
    fn test_dictionary_category_counts_unique_dedup() {
        let mut app = empty_app();
        // 同名詞を 3 個追加
        for _ in 0..3 {
            app.game_state.player.collection.push(make_curion(
                "ダブり名詞",
                Category::Animal,
                Rarity::Common,
            ));
        }
        let (owned, _total) = app.dictionary_category_counts(&Category::Animal);
        assert_eq!(owned, 1, "同名詞 3 個でも unique 集計で owned=1");
    }

    #[test]
    fn test_dictionary_total_counts_aggregates_all_categories() {
        let app = empty_app();
        let (total_owned, total_total) = app.dictionary_total_counts();

        let mut sum_owned = 0usize;
        let mut sum_total = 0usize;
        for category in ALL_CATEGORIES.iter() {
            let (o, t) = app.dictionary_category_counts(category);
            sum_owned += o;
            sum_total += t;
        }
        assert_eq!(total_owned, sum_owned);
        assert_eq!(total_total, sum_total, "全カテゴリ合計が個別合計と一致する");
        assert!(total_total > 0);
    }

    #[test]
    fn test_dictionary_total_counts_zero_division_safe() {
        let app = empty_app();
        let (owned, total) = app.dictionary_total_counts();
        assert_eq!(owned, 0);
        assert!(total > 0);
        // pct を関数化していないので、(owned, total) から手動で計算してパニックしないことを確認
        let pct = if total == 0 {
            0.0
        } else {
            owned as f64 / total as f64
        };
        assert_eq!(pct, 0.0);
    }

    #[test]
    fn test_scroll_down_resets_dictionary_scroll_on_category_change() {
        let mut app = empty_app();
        enter_dictionary_mode(&mut app);
        app.dictionary_scroll = 7;
        assert_eq!(app.dictionary_category_index, 0);

        app.scroll_down();

        assert_eq!(app.dictionary_category_index, 1);
        assert_eq!(
            app.dictionary_scroll, 0,
            "カテゴリ移動で scroll が 0 にリセット"
        );
    }

    #[test]
    fn test_scroll_up_stops_at_first_category() {
        let mut app = empty_app();
        enter_dictionary_mode(&mut app);
        assert_eq!(app.dictionary_category_index, 0);

        app.scroll_up();

        assert_eq!(app.dictionary_category_index, 0, "先頭で ↑ しても 0 のまま");
    }

    #[test]
    fn test_scroll_down_stops_at_last_category() {
        let mut app = empty_app();
        enter_dictionary_mode(&mut app);
        let last = ALL_CATEGORIES.len() - 1;
        app.dictionary_category_index = last;

        app.scroll_down();

        assert_eq!(
            app.dictionary_category_index, last,
            "末尾で ↓ しても末尾のまま"
        );
    }

    #[test]
    fn test_page_down_advances_dictionary_scroll() {
        let mut app = empty_app();
        enter_dictionary_mode(&mut app);
        let before = app.dictionary_scroll;

        let quit = app
            .handle_key(KeyCode::PageDown)
            .expect("handle_key should succeed");
        assert!(!quit);

        assert_eq!(app.dictionary_scroll, before + 10, "PgDn で +10");
    }

    #[test]
    fn test_page_up_saturates_at_zero() {
        let mut app = empty_app();
        enter_dictionary_mode(&mut app);
        app.dictionary_scroll = 3;

        let quit = app
            .handle_key(KeyCode::PageUp)
            .expect("handle_key should succeed");
        assert!(!quit);

        assert_eq!(app.dictionary_scroll, 0, "scroll=3 で PgUp なら 0 で飽和");
    }

    #[test]
    fn test_page_keys_no_op_outside_dictionary() {
        let mut app = empty_app();
        app.set_tab(Tab::Synthesis);
        app.dictionary_scroll = 5;
        assert!(!app.is_collection_dictionary());

        let quit = app
            .handle_key(KeyCode::PageDown)
            .expect("handle_key should succeed");
        assert!(!quit);

        assert_eq!(
            app.dictionary_scroll, 5,
            "図鑑モード外では PgDn は dictionary_scroll を変えない"
        );
    }

    #[test]
    fn test_pad_display_ascii_only() {
        // ASCII のみ: バイト数 == 表示幅
        assert_eq!(pad_display("abc", 6), "abc   ");
    }

    #[test]
    fn test_pad_display_cjk_only() {
        // CJK のみ: 各文字 2 セル幅。3 文字 = 6 セル -> target 8 で残り 2 セル分の空白
        let out = pad_display("動植物", 8);
        assert_eq!(UnicodeWidthStr::width(out.as_str()), 8);
        assert!(out.starts_with("動植物"));
        assert!(out.ends_with("  "));
    }

    #[test]
    fn test_pad_display_mixed() {
        // 混在: "abc動物" = 3 + 4 = 7 セル -> target 10 で 3 セル空白
        let out = pad_display("abc動物", 10);
        assert_eq!(UnicodeWidthStr::width(out.as_str()), 10);
        assert!(out.starts_with("abc動物"));
    }

    #[test]
    fn test_pad_display_already_over_target() {
        // 既に target 超え: 切り詰めずそのまま返す
        let s = "abcdefghij";
        assert_eq!(pad_display(s, 5), s);
        // 表示幅で target == width の境界も同様
        let cjk = "あい"; // 4 セル
        assert_eq!(pad_display(cjk, 4), cjk);
    }

    // ── Issue #22: truncate_display ─────────────────────────────────────

    #[test]
    fn test_truncate_with_ellipsis_basic_ascii() {
        // 半角 ASCII。max_width = 5 なら 4 文字 + "…" で表示幅 5
        let out = truncate_display("abcdefghij", 5);
        assert_eq!(out, "abcd…");
        assert_eq!(UnicodeWidthStr::width(out.as_str()), 5);
    }

    #[test]
    fn test_truncate_with_ellipsis_cjk() {
        // 全角は 1 文字 = 2 セル。max_width = 5 なら 全角 2 + "…"(1) = 5
        let out = truncate_display("あいうえお", 5);
        assert_eq!(out, "あい…");
        assert_eq!(UnicodeWidthStr::width(out.as_str()), 5);
    }

    #[test]
    fn test_truncate_noop_when_within_budget() {
        // 表示幅 <= max_width なら入力そのまま
        assert_eq!(truncate_display("abc", 10), "abc");
        assert_eq!(truncate_display("あい", 4), "あい");
    }

    #[test]
    fn test_truncate_with_tiny_budget_passthrough() {
        // max_width <= 1 はフォールバック (切り詰めない)
        assert_eq!(truncate_display("abcdef", 0), "abcdef");
        assert_eq!(truncate_display("abcdef", 1), "abcdef");
    }

    #[test]
    fn test_dictionary_owned_clamps_when_collection_has_unknown_noun() {
        let mut app = empty_app();
        // DB に存在しない名詞を含むコレクションを構築
        app.game_state.player.collection.push(make_curion(
            "存在しない名詞ZZZ",
            Category::Animal,
            Rarity::Common,
        ));
        let (owned, total) = app.dictionary_category_counts(&Category::Animal);
        assert!(
            owned <= total,
            "owned ({owned}) は total ({total}) を超えてはならない (DB 外名詞は集計対象外)"
        );
    }

    #[test]
    fn test_scroll_up_resets_dictionary_scroll_on_category_change() {
        let mut app = empty_app();
        enter_dictionary_mode(&mut app);
        // 末尾カテゴリへ移動してから ↑ で戻る
        let last = ALL_CATEGORIES.len() - 1;
        app.dictionary_category_index = last;
        app.dictionary_scroll = 11;

        app.scroll_up();

        assert_eq!(app.dictionary_category_index, last - 1);
        assert_eq!(
            app.dictionary_scroll, 0,
            "↑ でのカテゴリ移動でも scroll が 0 にリセット"
        );
    }

    // ── Issue #31: Collection 正規表現フィルタ ──────────────────────

    #[test]
    fn test_match_curion_by_noun() {
        let curion = make_curion("魚", Category::Animal, Rarity::Common);
        let re = regex::Regex::new("^魚$").unwrap();
        assert!(match_curion(&re, &curion), "noun が完全一致でマッチする");
    }

    #[test]
    fn test_match_curion_by_display_name() {
        let curion = make_curion("猫", Category::Animal, Rarity::Common);
        let re = regex::Regex::new("動物 の").unwrap();
        assert!(
            match_curion(&re, &curion),
            "display_name `動物 の 猫` の部分文字列でマッチする"
        );
    }

    #[test]
    fn test_match_curion_by_rarity_label() {
        let rare = make_curion("X", Category::Animal, Rarity::Rare);
        let common = make_curion("Y", Category::Animal, Rarity::Common);
        let re = regex::Regex::new("RARE").unwrap();
        assert!(match_curion(&re, &rare), "RARE は match");
        // "RARE" は "COMMON" にはマッチしないが、"RARE" だけにマッチさせるため
        // common の他フィールドにヒットしないことも確認
        assert!(!match_curion(&re, &common), "COMMON にはマッチしない");
    }

    #[test]
    fn test_match_curion_by_category() {
        let curion = make_curion("X", Category::Animal, Rarity::Common);
        let re = regex::Regex::new("動物").unwrap();
        assert!(match_curion(&re, &curion), "カテゴリ名 `動物` でマッチする");
    }

    #[test]
    fn test_match_curion_no_match() {
        let curion = make_curion("猫", Category::Animal, Rarity::Common);
        // 名詞 / display_name / rarity / category のどれにもヒットしないパターン
        let re = regex::Regex::new("XYZ_NOT_PRESENT").unwrap();
        assert!(
            !match_curion(&re, &curion),
            "全フィールドにマッチしないパターンは false"
        );
    }

    #[test]
    fn test_match_curion_invalid_pattern_is_rejected() {
        // `[` は閉じ括弧がないので不正。clippy::invalid_regex を避けるため動的文字列で渡す。
        let invalid_pat = String::from("[");
        let result = regex::Regex::new(&invalid_pat);
        assert!(result.is_err(), "不正な正規表現は Regex::new でエラー");

        // App 側で compiled_filter = None, filter_error = Some になることを確認
        let mut app = empty_app();
        app.set_tab(Tab::Collection);
        app.filter_mode = true;
        app.filter_text = invalid_pat;
        app.recompile_filter();
        assert!(
            app.compiled_filter.is_none(),
            "不正パターンで compiled_filter は None"
        );
        assert!(
            app.filter_error.is_some(),
            "不正パターンで filter_error が立つ"
        );
    }

    #[test]
    fn test_filter_mode_key_flow() {
        let mut app = empty_app();
        app.set_tab(Tab::Collection);
        assert!(!app.filter_mode);

        // `/` で入力モード
        let quit = app
            .handle_key(KeyCode::Char('/'))
            .expect("handle_key should succeed");
        assert!(!quit);
        assert!(app.filter_mode, "`/` で filter_mode = true");

        // 何か文字を入れる
        app.handle_key(KeyCode::Char('R')).unwrap();
        app.handle_key(KeyCode::Char('A')).unwrap();
        assert_eq!(app.filter_text, "RA");
        assert!(app.compiled_filter.is_some(), "正規表現コンパイル成功");

        // Esc で全クリア
        app.handle_key(KeyCode::Esc).unwrap();
        assert!(!app.filter_mode, "Esc で filter_mode = false");
        assert_eq!(app.filter_text, "", "Esc で filter_text もクリア");
        assert!(
            app.compiled_filter.is_none(),
            "Esc で compiled_filter もクリア"
        );
        assert!(app.filter_error.is_none());
    }

    #[test]
    fn test_filter_mode_accepts_japanese() {
        let mut app = empty_app();
        app.set_tab(Tab::Collection);
        app.handle_key(KeyCode::Char('/')).unwrap();
        for c in ['動', '物'] {
            app.handle_key(KeyCode::Char(c)).unwrap();
        }
        assert_eq!(app.filter_text, "動物");
        let re = app.compiled_filter.as_ref().expect("regex compiled");
        let curion = crate::curion::Curion::new(
            uuid::Uuid::nil(),
            "犬".to_string(),
            crate::curion::Category::Animal,
            crate::curion::Rarity::Common,
            0.5,
            0.5,
        );
        assert!(
            match_curion(re, &curion),
            "「動物 の 犬」が「動物」で match"
        );
    }

    #[test]
    fn test_filter_mode_s_key_is_typed_not_save() {
        // フィルタ入力モード中の `s` はフィルタテキストに入る (main.rs 側で save を抑止する想定)。
        // ui レイヤーの handle_key は filter_mode 中の `s` を普通の文字として処理する。
        let mut app = empty_app();
        app.set_tab(Tab::Collection);
        app.handle_key(KeyCode::Char('/')).unwrap();
        app.handle_key(KeyCode::Char('s')).unwrap();
        assert_eq!(app.filter_text, "s");
    }

    #[test]
    fn test_filter_mode_backspace() {
        let mut app = empty_app();
        app.set_tab(Tab::Collection);
        app.handle_key(KeyCode::Char('/')).unwrap();

        for c in ['A', 'B', 'C'] {
            app.handle_key(KeyCode::Char(c)).unwrap();
        }
        assert_eq!(app.filter_text, "ABC");

        app.handle_key(KeyCode::Backspace).unwrap();
        assert_eq!(app.filter_text, "AB", "Backspace で 1 文字削除");

        app.handle_key(KeyCode::Backspace).unwrap();
        app.handle_key(KeyCode::Backspace).unwrap();
        assert_eq!(app.filter_text, "", "Backspace で全部消える");

        // 空文字列ではコンパイル無効化
        assert!(app.compiled_filter.is_none());
        assert!(app.filter_error.is_none());
    }

    #[test]
    fn test_filter_mode_slash_only_on_collection_tab() {
        // Collection 以外のタブでは `/` キーは filter_mode を起動しない
        let mut app = empty_app();
        app.set_tab(Tab::Dashboard);
        app.handle_key(KeyCode::Char('/')).unwrap();
        assert!(
            !app.filter_mode,
            "Dashboard タブでは `/` でも filter_mode に入らない"
        );
    }

    #[test]
    fn test_filter_enter_keeps_filter_but_exits_input_mode() {
        // Enter は入力モードだけ抜ける。フィルタは維持。
        let mut app = empty_app();
        app.set_tab(Tab::Collection);
        app.handle_key(KeyCode::Char('/')).unwrap();
        app.handle_key(KeyCode::Char('R')).unwrap();
        app.handle_key(KeyCode::Char('A')).unwrap();
        assert!(app.compiled_filter.is_some());

        app.handle_key(KeyCode::Enter).unwrap();
        assert!(!app.filter_mode, "Enter で filter_mode = false");
        assert_eq!(app.filter_text, "RA", "Enter では filter_text は維持");
        assert!(
            app.compiled_filter.is_some(),
            "Enter では compiled_filter も維持"
        );
    }
}
