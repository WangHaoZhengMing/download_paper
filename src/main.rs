mod add_paper;
mod logger;
mod model;
mod tencent_cos;
mod download_paper;
mod bank_page_info;
mod ask_llm;

use anyhow::{Result, anyhow};
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use model::PaperInfo;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use tokio::time::sleep;
use tracing::{info, warn};
use urlencoding::encode;
use add_paper::save_new_paper;
use crate::download_paper::download_page;

// ============================================================================
// 类型定义和枚举
// ============================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
enum ProcessResult {
    Success,
    AlreadyExists,
    Failed,
}

// ============================================================================
// 辅助函数
// ============================================================================

/// 连接到浏览器并获取页面
async fn connect_to_browser_and_page(
    port: u16,
    target_url: Option<&str>,
    target_title: Option<&str>,
) -> Result<(Browser, Page)> {
    let browser_url = format!("http://localhost:{}", port);
    info!("正在连接到浏览器: {}", browser_url);

    let (browser, mut handler) = Browser::connect(&browser_url).await?;

    // 在后台处理浏览器事件
    tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() {
                break;
            }
        }
    });

    // 添加短暂延迟以等待浏览器状态同步
    sleep(tokio::time::Duration::from_millis(500)).await;

    let pages = browser.pages().await?;

    // 如果指定了目标标题，尝试查找匹配的页面
    if let Some(title) = target_title {
        for p in pages.iter() {
            if let Ok(Some(page_title)) = p.get_title().await {
                if page_title.contains(title) {
                    info!("✓ 找到目标页面: {}", page_title);
                    return Ok((browser, p.clone()));
                }
            }
        }
    }

    // 如果没有找到匹配的页面，创建新页面
    let new_page = if let Some(url) = target_url {
        let page = browser.new_page("about:blank").await?;
        page.goto(url).await?;
        info!("已导航到: {}", url);
        page
    } else {
        browser.new_page("about:blank").await?
    };

    Ok((browser, new_page))
}

/// 检查试卷是否已存在
async fn check_paper_exists(tiku_page: &Page, paper_title: &str) -> Result<bool> {
    let encoded_paper_name = encode(paper_title);
    let check_url = format!(
        "https://tps-tiku-api.staff.xdf.cn/paper/check/paperName?paperName={}&operationType=1&paperId=",
        encoded_paper_name
    );

    let check_js = format!(
        r#"
        async () => {{
            try {{
                const response = await fetch("{}", {{
                    method: "GET",
                    headers: {{
                        "Accept": "application/json, text/plain, */*"
                    }},
                    credentials: "include"
                }});
                const data = await response.json();
                return data;
            }} catch (err) {{
                return {{ error: err.toString() }};
            }}
        }}
        "#,
        check_url
    );

    let response: Value = tiku_page.evaluate(check_js.as_str()).await?.into_value()?;

    if let Some(error) = response.get("error") {
        return Err(anyhow!("API 请求失败: {}", error));
    }

    if let Some(data) = response.get("data") {
        if let Some(repeated) = data.get("repeated") {
            if repeated.as_bool().unwrap_or(false) {
                // 记录到重复文件
                let log_path = Path::new("other").join("重复.txt");
                if let Some(parent) = log_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                use std::fs::OpenOptions;
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)?;
                file.write_all(format!("{}\n", paper_title).as_bytes())?;
                return Ok(true);
            }
        }
    }

    Ok(false)
}

/// 获取目录页的试卷列表
async fn fetch_paper_list(catalogue_page: &Page) -> Result<Vec<PaperInfo>> {
    let js_code = r#"
        () => {
            const elements = document.querySelectorAll("div.info-item.exam-info a.exam-name");
            return Array.from(elements).map(el => ({
                url: 'https://zujuan.xkw.com' + el.getAttribute('href'),
                title: el.innerText.trim()
            }));
        }
    "#;

    let response: Value = catalogue_page.evaluate(js_code).await?.into_value()?;

    let papers: Vec<PaperInfo> = serde_json::from_value(response)?;

    Ok(papers)
}

/// 处理单个试卷
async fn process_single_paper(
    paper_info: &PaperInfo,
    port: u16,
    tiku_page: &Page,
) -> Result<ProcessResult> {
    let paper_browser = connect_to_browser_and_page(port, Some(&paper_info.url), None).await?;
    let (browser, paper_page) = paper_browser;

    let result = async {
        // 下载页面数据
        let page_data = download_page(&paper_page).await?;

        // 检查是否已存在
        let exists = check_paper_exists(tiku_page, &page_data.name).await?;

        if exists {
            warn!("⚠️ 试卷已存在: {}", page_data.name);
            return Ok(ProcessResult::AlreadyExists);
        }

        // 保存新试卷
        let mut question_page = page_data;
        save_new_paper(&mut question_page, tiku_page).await?;
        info!("✅ 成功处理: {}", question_page.name);
        Ok(ProcessResult::Success)
    }
    .await;

    // 清理资源 - 当变量离开作用域时会自动清理
    drop(paper_page);
    drop(browser);

    result
}

/// 处理单个目录页
async fn process_catalogue_page(page_number: i32, port: u16, tiku_page: &Page) -> Result<i32> {
    let catalogue_url = format!("https://zujuan.xkw.com/czkx/shijuan/jdcs/p{}", page_number);
    info!("📖 正在处理目录页 {}...", page_number);

    let (catalogue_browser, catalogue_page) =
        connect_to_browser_and_page(port, Some(&catalogue_url), None).await?;

    let result = async {
        // 获取试卷列表
        let papers = fetch_paper_list(&catalogue_page).await?;
        info!("📄 在页面 {} 找到 {} 个试卷", page_number, papers.len());

        if papers.is_empty() {
            return Ok(0);
        }

        // 并发处理所有试卷
        info!("⚡ 开始并发处理 {} 个试卷...", papers.len());

        let mut tasks = Vec::new();
        for paper in &papers {
            let paper_clone = paper.clone();
            let tiku_page_clone = tiku_page.clone();
            tasks.push(tokio::spawn(async move {
                process_single_paper(&paper_clone, port, &tiku_page_clone).await
            }));
        }

        // 等待所有任务完成
        let mut success_count = 0;
        for (idx, task) in tasks.into_iter().enumerate() {
            match task.await {
                Ok(Ok(ProcessResult::Success)) => {
                    success_count += 1;
                }
                Ok(Ok(ProcessResult::AlreadyExists)) => {
                    // 已存在，不计入成功数
                }
                Ok(Ok(ProcessResult::Failed)) => {
                    if let Some(paper) = papers.get(idx) {
                        warn!("❌ 处理失败: {}", paper.title);
                    }
                }
                Ok(Err(e)) => {
                    if let Some(paper) = papers.get(idx) {
                        warn!("❌ 处理 '{}' 时出错: {}", paper.title, e);
                    }
                }
                Err(e) => {
                    warn!("❌ 任务执行失败: {}", e);
                }
            }
        }

        Ok(success_count)
    }
    .await;

    // 清理资源 - 当变量离开作用域时会自动清理
    drop(catalogue_page);
    drop(catalogue_browser);

    result
}

// ============================================================================
// 主函数
// ============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    logger::init();

    // 确保必要的目录存在
    let directories = vec!["PDF", "output_toml", "other"];
    for dir in directories {
        fs::create_dir_all(dir)?;
    }

    // 配置参数
    let start_page = 58;
    let end_page = 466;
    let debug_port = 2001;
    let mut total_success = 0;

    info!("🚀 开始试卷下载流程...");
    info!("📊 页面范围: {} - {}", start_page, end_page);
    info!("🔌 浏览器端口: {}", debug_port);
    info!("{}", "=".repeat(60));

    // 连接到题库平台页面
    let (browser, tiku_page) =
        connect_to_browser_and_page(debug_port, None, Some("题库平台 | 录排中心")).await?;

    // 处理每个目录页
    for page_num in start_page..end_page {
        match process_catalogue_page(page_num, debug_port, &tiku_page).await {
            Ok(count) => {
                total_success += count;
                info!("✅ 页面 {} 完成: 处理了 {} 个试卷", page_num, count);
            }
            Err(e) => {
                warn!("❌ 页面 {} 失败: {}", page_num, e);
            }
        }

        // 延迟避免请求过快
        sleep(tokio::time::Duration::from_secs(1)).await;
        info!("{}", "=".repeat(60));
    }

    // 清理资源 - 当变量离开作用域时会自动清理
    drop(browser);

    info!("\n🎉 处理完成! 总共处理了 {} 个试卷", total_success);

    Ok(())
}
