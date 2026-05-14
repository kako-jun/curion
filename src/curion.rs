use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// キュリオンのレアリティ
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Rarity {
    Common,
    Rare,
    Epic,
    Legendary,
}

impl Rarity {
    /// レアリティの確率を返す
    pub fn probability(&self) -> f64 {
        match self {
            Rarity::Common => 0.60,    // 60%
            Rarity::Rare => 0.30,      // 30%
            Rarity::Epic => 0.09,      // 9%
            Rarity::Legendary => 0.01, // 1%
        }
    }
}

/// キュリオンのカテゴリ
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Category {
    Animal,     // 動物
    Plant,      // 植物
    Color,      // 色
    Object,     // 物体
    Concept,    // 概念
    Element,    // 元素
    Food,       // 食べ物
    Phenomenon, // 現象
    Abstract,   // 抽象概念
}

impl Category {
    pub fn as_str(&self) -> &str {
        match self {
            Category::Animal => "動物",
            Category::Plant => "植物",
            Category::Color => "色",
            Category::Object => "物体",
            Category::Concept => "概念",
            Category::Element => "元素",
            Category::Food => "食べ物",
            Category::Phenomenon => "現象",
            Category::Abstract => "抽象",
        }
    }
}

/// キュリオン（興味を司る粒子）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Curion {
    /// 一意のID
    pub id: String,

    /// 生成元のGUID
    pub source_guid: Uuid,

    /// 名詞（例: "魚"、"赤色"、"本"）
    pub noun: String,

    /// カテゴリ
    pub category: Category,

    /// レアリティ
    pub rarity: Rarity,

    /// 興味度（0.0〜1.0）
    pub interest: f64,

    /// 美しさ（0.0〜1.0）
    pub beauty: f64,

    /// 取得日時
    pub acquired_at: DateTime<Utc>,
}

impl Curion {
    /// 新しいキュリオンを作成
    pub fn new(
        source_guid: Uuid,
        noun: String,
        category: Category,
        rarity: Rarity,
        interest: f64,
        beauty: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            source_guid,
            noun,
            category,
            rarity,
            interest,
            beauty,
            acquired_at: Utc::now(),
        }
    }

    /// キュリオンの表示用文字列
    pub fn display_name(&self) -> String {
        format!("{} の {}", self.category.as_str(), self.noun)
    }
}
