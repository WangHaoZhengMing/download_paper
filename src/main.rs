mod app;
mod adapters;
mod domain;
mod infra;
mod config;
mod logger;
mod model;
mod add_paper;
mod ask_llm;
mod bank_page_info;
mod browser;
mod download_paper;
mod services;
mod tencent_cos;

use anyhow::Result;
use app::config::AppConfig;
use app::logging;
use services::orchestrator;
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();

    let app_config = AppConfig::load(None)?;

    for dir in &app_config.directories {
        fs::create_dir_all(dir)?;
    }

    orchestrator::run(app_config).await
}
