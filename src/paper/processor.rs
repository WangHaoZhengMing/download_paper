use crate::browser::headless::launch_headless_get_page_browser;
use crate::{add_paper::PaperService};
use crate::download_paper::download_page;
use crate::model::PaperInfo;
use crate::paper::checker::check_paper_exists;
use crate::paper::types::ProcessResult;
use anyhow::{Result, anyhow};
use chromiumoxide::{Browser, Page};
use std::sync::Arc;
use tokio::time::sleep;
use tracing::{debug, error, info, warn};

/// 处理单个试卷
pub async fn process_single_paper(
    paper_browser: &Arc<Browser>, paper_info: &PaperInfo,tiku_page: &Page
) -> Result<ProcessResult> {
let current_page = paper_browser.new_page(paper_info.url.as_str()).await?;
    debug!("开始处理试卷: {}", paper_info.title);
    let result = async {
        const MAX_RETRIES: u32 = 3;
        let mut last_error = None;

        

        // 重试下载和保存流程
        for attempt in 1..=MAX_RETRIES {
            info!("📥 尝试处理试卷 (第 {}/{} 次): {}", attempt, MAX_RETRIES, paper_info.title);
            
            match try_process_once(&current_page, tiku_page).await {
                Ok(result) => {
                    match result {
                        ProcessResult::Success => {
                            info!("✅ 试卷处理成功！");
                            return Ok(ProcessResult::Success);
                        }
                        ProcessResult::AlreadyExists => {
                            return Ok(ProcessResult::AlreadyExists);
                        }
                        ProcessResult::Failed => {
                            warn!("⚠️ 第 {} 次处理失败", attempt);
                            if attempt < MAX_RETRIES {
                                let delay = attempt as u64 * 2;
                                warn!("⏳ {} 秒后重试...", delay);
                                sleep(tokio::time::Duration::from_secs(delay)).await;
                            } else {
                                last_error = Some(anyhow!("处理失败：已重试 {} 次", MAX_RETRIES));
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("❌ 第 {} 次处理出错: {}", attempt, e);
                    if attempt < MAX_RETRIES {
                        let delay = attempt as u64 * 2;
                        warn!("⏳ {} 秒后重试...", delay);
                        sleep(tokio::time::Duration::from_secs(delay)).await;
                    } else {
                        last_error = Some(anyhow!("处理失败：已重试 {} 次，最后一次错误: {}", MAX_RETRIES, e));
                    }
                }
            }
        }

        // 所有重试都失败
        error!("❌ 试卷处理最终失败，已重试 {} 次: {}", MAX_RETRIES, paper_info.title);
        Err(last_error.unwrap_or_else(|| anyhow!("处理失败：未知错误")))
    }
    .await;

    debug!("正在关闭试卷页面");
    if let Err(e) = current_page.close().await {
        warn!("关闭试卷页面失败: {}，但继续处理", e);
    } else {
        debug!("试卷页面已关闭");
    }
    // drop(paper_browser);
    result
}

/// 单次处理尝试
async fn try_process_once(
    paper_page: &Page,
    tiku_page: &Page,
) -> Result<ProcessResult> {
    // 下载页面数据
    debug!("正在下载页面数据");
    let page_data = download_page(paper_page).await.map_err(|e| {
        error!("下载页面数据失败: {}", e);
        e
    })?;
    debug!("页面数据下载成功: {}", page_data.name);

    // 检查是否已存在
    debug!("检查试卷是否已存在");
    let exists = check_paper_exists(tiku_page, &page_data.name)
        .await
        .map_err(|e| {
            error!("检查试卷是否存在时出错: {}", e);
            e
        })?;

    if exists {
        warn!("⚠️ 试卷已存在: {}", page_data.name);
        return Ok(ProcessResult::AlreadyExists);
    }

    // 保存新试卷
    debug!("开始保存新试卷");
    let mut question_page = page_data;
    let paper_service = PaperService::new(Arc::new(tiku_page.clone()), None);
    paper_service
        .save_new_paper(&mut question_page)
        .await
        .map_err(|e| {
            error!("保存新试卷失败: {}", e);
            e
        })?;
    info!("✅ 成功处理: {}", question_page.name);
    debug!("试卷处理完成");
    Ok(ProcessResult::Success)
}

