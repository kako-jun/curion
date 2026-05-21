use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use crate::nostr_identity::ProfileManager;
use crate::player::GameState;
use crate::synthesis::{RecipeDatabase, SynthesisManager};

/// セーブデータマネージャー
pub struct SaveManager {
    save_path: PathBuf,
}

impl SaveManager {
    /// 新しいセーブマネージャーを作成
    pub fn new() -> Result<Self> {
        let save_dir = Self::get_save_directory()?;

        // ディレクトリが存在しない場合は作成
        if !save_dir.exists() {
            fs::create_dir_all(&save_dir).context("Failed to create save directory")?;
        }

        let save_path = save_dir.join("save.json");

        Ok(Self { save_path })
    }

    /// プロファイル対応のセーブマネージャーを作成
    pub fn new_with_profile(profile_manager: &ProfileManager) -> Result<Self> {
        let save_path = profile_manager.save_path();

        // ディレクトリが存在しない場合は作成
        if let Some(parent) = save_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).context("Failed to create save directory")?;
            }
        }

        Ok(Self { save_path })
    }

    /// セーブディレクトリのパスを取得
    fn get_save_directory() -> Result<PathBuf> {
        // ホームディレクトリを取得
        let home = dirs::home_dir().context("Could not find home directory")?;

        Ok(home.join(".curion"))
    }

    /// ゲーム状態を保存
    pub fn save(&self, game_state: &GameState) -> Result<()> {
        // SerializableGameStateに変換
        let serializable = SerializableGameState::from(game_state);

        // JSONに変換
        let json = serde_json::to_string_pretty(&serializable)
            .context("Failed to serialize game state")?;

        // ファイルに書き込み
        fs::write(&self.save_path, json).context("Failed to write save file")?;

        Ok(())
    }

    /// ゲーム状態を読み込み
    pub fn load(&self) -> Result<GameState> {
        // レシピデータベースをロード
        let synthesis_manager = Self::create_synthesis_manager()?;

        // ファイルが存在しない場合は新規作成
        if !self.save_path.exists() {
            return Ok(GameState::new(synthesis_manager));
        }

        // ファイルを読み込み
        let json = fs::read_to_string(&self.save_path).context("Failed to read save file")?;

        // JSONをパース
        let serializable: SerializableGameState =
            serde_json::from_str(&json).context("Failed to parse save file")?;

        // GameStateに変換
        let game_state = serializable.into_game_state(synthesis_manager);

        Ok(game_state)
    }

    /// 合成マネージャーを作成
    fn create_synthesis_manager() -> Result<SynthesisManager> {
        let recipe_db =
            RecipeDatabase::load_embedded().context("Failed to load recipe database")?;
        Ok(SynthesisManager::new(recipe_db))
    }

    /// セーブファイルのパスを取得
    #[cfg(test)]
    pub fn save_path(&self) -> &Path {
        &self.save_path
    }
}

impl Default for SaveManager {
    fn default() -> Self {
        Self::new().expect("Failed to create save manager")
    }
}

/// シリアライズ可能なゲーム状態
#[derive(Debug, Serialize, Deserialize)]
pub struct SerializableGameState {
    pub player: crate::player::Player,
    // achievement_managerは再構築するので保存しない
    // 進捗だけ保存する
    pub achievement_progress: Vec<crate::achievement::AchievementProgress>,
    // synthesis_managerの発見済みレシピ
    pub discovered_recipes: std::collections::HashMap<String, bool>,
    /// UI 言語 (Issue #63 Phase 1)。
    ///
    /// 旧セーブにはこのフィールドが無いため `#[serde(default)]` を付け、
    /// 復元時は [`crate::i18n::Language::default()`] = `Language::En`
    /// にフォールバックする。
    #[serde(default)]
    pub language: crate::i18n::Language,
}

impl From<&GameState> for SerializableGameState {
    fn from(game_state: &GameState) -> Self {
        // 全実績の進捗を収集
        let achievement_progress = game_state
            .achievement_manager
            .get_all_achievements()
            .iter()
            .filter_map(|achievement| {
                game_state
                    .achievement_manager
                    .get_progress(&achievement.id)
                    .cloned()
            })
            .collect();

        Self {
            player: game_state.player.clone(),
            achievement_progress,
            discovered_recipes: game_state.synthesis_manager.get_discovered_state(),
            language: game_state.language,
        }
    }
}

impl SerializableGameState {
    /// GameStateに変換
    fn into_game_state(self, synthesis_manager: SynthesisManager) -> GameState {
        let mut game_state = GameState::new(synthesis_manager);
        game_state.player = self.player;
        game_state.language = self.language;

        // 実績の進捗を復元
        for progress in self.achievement_progress {
            if let Some(stored_progress) = game_state
                .achievement_manager
                .get_progress_mut(&progress.achievement_id)
            {
                *stored_progress = progress;
            }
        }

        // 合成レシピの発見状態を復元
        game_state
            .synthesis_manager
            .set_discovered_state(self.discovered_recipes);

        game_state
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_save_manager_creation() {
        let manager = SaveManager::new().expect("Failed to create save manager");
        assert!(manager.save_path().to_str().is_some());
    }

    // ── Issue #63 Phase 1: language round-trip via SerializableGameState ──

    fn fresh_game_state() -> GameState {
        let recipe_db = RecipeDatabase::load_embedded().expect("Failed to load recipe database");
        let synthesis_manager = SynthesisManager::new(recipe_db);
        GameState::new(synthesis_manager)
    }

    fn round_trip(state: &GameState) -> GameState {
        let recipe_db = RecipeDatabase::load_embedded().expect("Failed to load recipe database");
        let synthesis_manager = SynthesisManager::new(recipe_db);
        let serializable = SerializableGameState::from(state);
        let json = serde_json::to_string(&serializable).expect("serialize");
        let parsed: SerializableGameState = serde_json::from_str(&json).expect("deserialize");
        parsed.into_game_state(synthesis_manager)
    }

    /// H-1: A GameState with `language = En` round-trips through JSON
    /// unchanged.
    #[test]
    fn serializable_round_trip_preserves_language_en() {
        let mut state = fresh_game_state();
        state.language = crate::i18n::Language::En;
        let restored = round_trip(&state);
        assert_eq!(restored.language, crate::i18n::Language::En);
    }

    /// H-2: A GameState with `language = Ja` round-trips through JSON
    /// unchanged.
    #[test]
    fn serializable_round_trip_preserves_language_ja() {
        let mut state = fresh_game_state();
        state.language = crate::i18n::Language::Ja;
        let restored = round_trip(&state);
        assert_eq!(restored.language, crate::i18n::Language::Ja);
    }

    /// H-3: A legacy save JSON (no `language` field) deserializes with
    /// `language = En` via `#[serde(default)]`. This is the migration path
    /// for pre-#63 save files.
    #[test]
    fn legacy_save_without_language_field_deserializes_as_english() {
        // Build a minimal SerializableGameState JSON without the `language`
        // field by serializing a fresh state and then stripping the key.
        let state = fresh_game_state();
        let serializable = SerializableGameState::from(&state);
        let mut value: serde_json::Value =
            serde_json::to_value(&serializable).expect("to_value");
        let obj = value.as_object_mut().expect("top-level object");
        assert!(
            obj.remove("language").is_some(),
            "language field should exist before stripping"
        );
        let legacy_json = serde_json::to_string(&value).expect("serialize stripped");
        let parsed: SerializableGameState =
            serde_json::from_str(&legacy_json).expect("legacy deserialize");
        assert_eq!(parsed.language, crate::i18n::Language::En);
    }

    /// H-4: A newly serialized SerializableGameState carries the `language`
    /// field in its JSON output (= older readers see the new field).
    #[test]
    fn serializable_json_contains_language_field_for_new_save() {
        let mut state = fresh_game_state();
        state.language = crate::i18n::Language::Ja;
        let serializable = SerializableGameState::from(&state);
        let json = serde_json::to_string(&serializable).expect("serialize");
        assert!(
            json.contains("\"language\""),
            "JSON should contain the language key, got: {json}"
        );
        assert!(
            json.contains("\"Ja\""),
            "JSON should contain the Ja variant tag, got: {json}"
        );
    }
}
