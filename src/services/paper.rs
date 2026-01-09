use anyhow::{anyhow, Result};
use chromiumoxide::{Browser, Page};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use tracing::{debug, error, info, warn};

use crate::browser::BrowserPool;
use crate::download_paper::download_page;
use crate::model::PaperInfo;
use crate::add_paper::save_new_paper;

use super::types::ProcessResult;

/// 检查试卷是否已存在
pub async fn check_paper_exists(tiku_page: &Page, paper_title: &str) -> Result<bool> {
    let safe_title_json = serde_json::to_string(paper_title).unwrap_or_else(|_| format!("\"{}\"", paper_title));

    let check_js = format!(
        r#"
        (async () => {{
            try {{
                const rawTitle = {0}; 
                const paperName = encodeURIComponent(rawTitle);
                const url = `https://tps-tiku-api.staff.xdf.cn/paper/check/paperName?paperName=${{paperName}}&operationType=1&paperId=`;
                
                const response = await fetch(url, {{
                    method: "GET",
                    headers: {{
                        "Accept": "application/json, text/plain, */*"
                    }},
                    credentials: "include"
                }});

                if (!response.ok) {{
                    return {{ error: `HTTP Error: ${{response.status}}` }};
                }}

                const data = await response.json();
                return data;
            }} catch (err) {{
                return {{ error: err.toString() }};
            }}
        }})()
        "#,
        safe_title_json
    );

    info!("检查试卷是否已存在: {}", paper_title);

    let response: Value = tiku_page
        .evaluate(check_js)
        .await
        .map_err(|e| {
            error!("执行检查脚本失败: {}", e);
            e
        })?
        .into_value()
        .map_err(|e| {
            error!("解析脚本返回值失败: {}", e);
            anyhow!("解析脚本返回值失败: {}", e)
        })?;

    if let Some(error) = response.get("error") {
        let err_msg = error.as_str().unwrap_or("未知错误");
        error!("API 请求逻辑失败: {}", err_msg);
        return Err(anyhow!("API 请求逻辑失败: {}", err_msg));
    }

    if let Some(data) = response.get("data") {
        if let Some(repeated) = data.get("repeated") {
            if repeated.as_bool().unwrap_or(false) {
                debug!("试卷已存在: {}", paper_title);
                let log_path = Path::new("other").join("重复.txt");
                if let Some(parent) = log_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_path) {
                    let _ = writeln!(file, "{}", paper_title);
                }
                debug!("已记录重复试卷到日志文件");
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// 处理单个试卷
pub async fn process_single_paper(
    paper_info: &PaperInfo,
    pool: &BrowserPool,
    tiku_page: &Page,
) -> Result<ProcessResult> {
    let paper_browser: (Browser, Page) = pool.connect_page(Some(&paper_info.url), None).await?;
    let (browser, paper_page) = paper_browser;

    debug!("开始处理试卷: {}", paper_info.title);
    let result: Result<ProcessResult> = async {
        let page_data = download_page(&paper_page).await.map_err(|e| {
            error!("下载页面数据失败: {}", e);
            e
        })?;
        debug!("页面数据下载成功: {}", page_data.name);

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

        let mut question_page = page_data;
        let _ = save_new_paper(&mut question_page, tiku_page).await?;
        info!("✅ 成功处理: {}", question_page.name);
        Ok(ProcessResult::Success)
    }
    .await;

    debug!("正在关闭试卷页面");
    if let Err(e) = paper_page.close().await {
        warn!("关闭试卷页面失败: {}，但继续处理", e);
    }
    drop(browser);

    result
}
