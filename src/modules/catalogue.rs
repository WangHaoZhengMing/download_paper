use anyhow::{anyhow, Result};
use chromiumoxide::Page;
use serde_json::Value;
use tracing::{debug, error};

use crate::core::models::PaperInfo;

/// 获取目录页的试卷列表
pub async fn fetch_paper_list(catalogue_page: &Page) -> Result<Vec<PaperInfo>> {
    let js_code = r#"
        () => {
            const elements = document.querySelectorAll("div.info-item.exam-info a.exam-name");
            return Array.from(elements).map(el => ({
                url: 'https://zujuan.xkw.com' + el.getAttribute('href'),
                title: el.innerText.trim()
            }));
        }
    "#;

    debug!("正在获取目录页的试卷列表");
    let response: Value = catalogue_page
        .evaluate(js_code)
        .await
        .map_err(|e| {
            error!("执行获取试卷列表脚本失败: {}", e);
            e
        })?
        .into_value()
        .map_err(|e| {
            error!("获取试卷列表结果失败: {}", e);
            anyhow!("获取试卷列表结果失败: {}", e)
        })?;

    let papers: Vec<PaperInfo> = serde_json::from_value(response).map_err(|e| {
        error!("解析试卷列表失败: {}", e);
        anyhow!("解析试卷列表失败: {}", e)
    })?;
    debug!("成功获取到 {} 个试卷", papers.len());

    Ok(papers)
}
