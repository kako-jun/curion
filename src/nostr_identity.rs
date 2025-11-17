use anyhow::{Context, Result};
use nostr_sdk::prelude::*;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Nostr アイデンティティ（keypair）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NostrIdentity {
    /// 秘密鍵（hex形式）
    secret_key: String,
    /// 公開鍵（hex形式）
    pub public_key: String,
}

impl NostrIdentity {
    /// 新しいkeypairを生成
    pub fn generate() -> Result<Self> {
        let keys = Keys::generate();
        Ok(Self {
            secret_key: keys.secret_key().to_secret_hex(),
            public_key: keys.public_key().to_hex(),
        })
    }

    /// 秘密鍵からkeypairを復元
    pub fn from_secret_key(secret_hex: &str) -> Result<Self> {
        let secret_key = SecretKey::from_hex(secret_hex)
            .context("Failed to parse secret key")?;
        let keys = Keys::new(secret_key);
        Ok(Self {
            secret_key: keys.secret_key().to_secret_hex(),
            public_key: keys.public_key().to_hex(),
        })
    }

    /// Keysオブジェクトを取得
    pub fn keys(&self) -> Result<Keys> {
        let secret_key = SecretKey::from_hex(&self.secret_key)
            .context("Failed to parse secret key")?;
        Ok(Keys::new(secret_key))
    }

    /// ファイルから読み込み
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read identity file: {}", path.as_ref().display()))?;

        let identity: NostrIdentity = serde_json::from_str(&content)
            .context("Failed to parse identity file")?;

        Ok(identity)
    }

    /// ファイルに保存
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        // ディレクトリが存在しない場合は作成
        if let Some(parent) = path.as_ref().parent() {
            fs::create_dir_all(parent)
                .context("Failed to create identity directory")?;
        }

        let json = serde_json::to_string_pretty(self)
            .context("Failed to serialize identity")?;

        fs::write(&path, json)
            .with_context(|| format!("Failed to write identity file: {}", path.as_ref().display()))?;

        Ok(())
    }
}

/// プロファイル管理
pub struct ProfileManager {
    profile_name: String,
    config_dir: PathBuf,
}

impl ProfileManager {
    /// 新しいプロファイルマネージャーを作成
    pub fn new(profile_name: Option<String>) -> Result<Self> {
        let config_dir = Self::get_config_directory()?;
        let profile_name = profile_name.unwrap_or_else(|| "default".to_string());

        Ok(Self {
            profile_name,
            config_dir,
        })
    }

    /// 設定ディレクトリを取得
    fn get_config_directory() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("Could not find home directory")?;
        Ok(home.join(".curion"))
    }

    /// プロファイル名を取得
    pub fn profile_name(&self) -> &str {
        &self.profile_name
    }

    /// セーブファイルのパスを取得
    pub fn save_path(&self) -> PathBuf {
        if self.profile_name == "default" {
            self.config_dir.join("save.json")
        } else {
            self.config_dir.join(format!("{}.json", self.profile_name))
        }
    }

    /// アイデンティティファイルのパスを取得
    pub fn identity_path(&self) -> PathBuf {
        if self.profile_name == "default" {
            self.config_dir.join("identity.json")
        } else {
            self.config_dir.join(format!("{}_identity.json", self.profile_name))
        }
    }

    /// アイデンティティを読み込みまたは生成
    pub fn load_or_generate_identity(&self) -> Result<NostrIdentity> {
        let identity_path = self.identity_path();

        if identity_path.exists() {
            NostrIdentity::load_from_file(&identity_path)
        } else {
            let identity = NostrIdentity::generate()?;
            identity.save_to_file(&identity_path)?;
            println!("✨ Generated new Nostr identity for profile '{}'", self.profile_name);
            println!("   Public key: {}", identity.public_key);
            Ok(identity)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identity_generation() {
        let identity = NostrIdentity::generate().unwrap();
        assert!(!identity.secret_key.is_empty());
        assert!(!identity.public_key.is_empty());
    }

    #[test]
    fn test_identity_roundtrip() {
        let identity1 = NostrIdentity::generate().unwrap();
        let identity2 = NostrIdentity::from_secret_key(&identity1.secret_key).unwrap();
        assert_eq!(identity1.public_key, identity2.public_key);
    }
}
