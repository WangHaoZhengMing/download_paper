use anyhow::{anyhow, Result};
use chromiumoxide::Page;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use tracing::{debug, error, info};

use crate::core::models::QuestionPage;
use crate::utils::text::sanitize_filename;

/// 本地保存试卷 TOML（占位，可扩展上传逻辑）
pub fn persist_paper_locally(question_page: &QuestionPage, output_dir: &str) -> Result<()> {
    let output_dir = Path::new(output_dir);
    fs::create_dir_all(output_dir)?;
    let sanitized_name = sanitize_filename(&question_page.name);
    let toml_path = output_dir.join(format!("{}.toml", sanitized_name));
    let toml_content = toml::to_string(question_page)?;
    fs::write(toml_path, toml_content)?;
    Ok(())
}
