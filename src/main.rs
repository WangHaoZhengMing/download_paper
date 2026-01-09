mod config;
mod core;
mod modules;
mod utils;
mod workflow;

use anyhow::Result;
use config::AppConfig;
use std::fs;
use utils::logger;
use workflow::pipeline;

#[tokio::main]
async fn main() -> Result<()> {
    logger::init();

    let app_config = AppConfig::load(None)?;

    for dir in &app_config.directories {
        fs::create_dir_all(dir)?;
    }

    pipeline::run(app_config).await
}
