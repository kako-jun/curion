use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::player::GameState;

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
            fs::create_dir_all(&save_dir)
                .context("Failed to create save directory")?;
        }

        let save_path = save_dir.join("save.json");

        Ok(Self { save_path })
    }

    /// セーブディレクトリのパスを取得
    fn get_save_directory() -> Result<PathBuf> {
        // ホームディレクトリを取得
        let home = dirs::home_dir()
            .context("Could not find home directory")?;

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
        fs::write(&self.save_path, json)
            .context("Failed to write save file")?;

        Ok(())
    }

    /// ゲーム状態を読み込み
    pub fn load(&self) -> Result<GameState> {
        // ファイルが存在しない場合は新規作成
        if !self.save_path.exists() {
            return Ok(GameState::new());
        }

        // ファイルを読み込み
        let json = fs::read_to_string(&self.save_path)
            .context("Failed to read save file")?;

        // JSONをパース
        let serializable: SerializableGameState = serde_json::from_str(&json)
            .context("Failed to parse save file")?;

        // GameStateに変換
        let game_state = serializable.into();

        Ok(game_state)
    }

    /// セーブファイルが存在するか確認
    pub fn save_exists(&self) -> bool {
        self.save_path.exists()
    }

    /// セーブファイルのパスを取得
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
        }
    }
}

impl From<SerializableGameState> for GameState {
    fn from(serializable: SerializableGameState) -> Self {
        let mut game_state = GameState::new();
        game_state.player = serializable.player;

        // 実績の進捗を復元
        for progress in serializable.achievement_progress {
            if let Some(stored_progress) = game_state
                .achievement_manager
                .get_progress_mut(&progress.achievement_id)
            {
                *stored_progress = progress;
            }
        }

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
}
