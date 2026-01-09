use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    #[serde(default = "default_start_page")]
    pub start_page: i32,
    #[serde(default = "default_end_page")]
    pub end_page: i32,
    #[serde(default = "default_debug_port")]
    pub debug_port: u16,
    #[serde(default = "default_delay_ms")]
    pub delay_ms: u64,
    #[serde(default = "default_directories")]
    pub directories: Vec<String>,
    #[serde(default = "default_tiku_title")]
    pub tiku_target_title: String,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

impl AppConfig {
    pub fn load(config_path: Option<&Path>) -> Result<Self> {
        let path = config_path.unwrap_or_else(|| Path::new("config.toml"));
        if path.exists() {
            let raw = fs::read_to_string(path)
                .with_context(|| format!("读取配置文件失败: {}", path.display()))?;
            let cfg: AppConfig = toml::from_str(&raw)
                .with_context(|| format!("解析配置文件失败: {}", path.display()))?;
            return Ok(cfg);
        }
        Ok(AppConfig::default())
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            start_page: default_start_page(),
            end_page: default_end_page(),
            debug_port: default_debug_port(),
            delay_ms: default_delay_ms(),
            directories: default_directories(),
            tiku_target_title: default_tiku_title(),
            concurrency: default_concurrency(),
        }
    }
}

fn default_start_page() -> i32 {
    58
}

fn default_end_page() -> i32 {
    466
}

fn default_debug_port() -> u16 {
    2001
}

fn default_delay_ms() -> u64 {
    1000
}

fn default_directories() -> Vec<String> {
    vec!["PDF".to_string(), "output_toml".to_string(), "other".to_string()]
}

fn default_tiku_title() -> String {
    "题库平台 | 录排中心".to_string()
}

fn default_concurrency() -> usize {
    4
}
