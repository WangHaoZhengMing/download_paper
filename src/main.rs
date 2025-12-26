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
use tracing::{debug, error, info, warn};
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
pub async fn connect_to_browser_and_page(
    port: u16,
    target_url: Option<&str>,
    target_title: Option<&str>,
) -> Result<(Browser, Page)> {
    let browser_url = format!("http://localhost:{}", port);
    info!("正在连接到浏览器: {}", browser_url);
    debug!("目标 URL: {:?}, 目标标题: {:?}", target_url, target_title);

    let (browser, mut handler) = Browser::connect(&browser_url).await
        .map_err(|e| {
            error!("连接浏览器失败: {}", e);
            e
        })?;
    debug!("浏览器连接成功");

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
    debug!("获取到 {} 个页面", pages.len());

    // 如果指定了目标标题，尝试查找匹配的页面
    if let Some(title) = target_title {
        debug!("正在查找标题包含 '{}' 的页面", title);
        for p in pages.iter() {
            if let Ok(Some(page_title)) = p.get_title().await {
                debug!("检查页面标题: {}", page_title);
                if page_title.contains(title) {
                    info!("✓ 找到目标页面: {}", page_title);
                    return Ok((browser, p.clone()));
                }
            }
        }
        debug!("未找到匹配的页面，将创建新页面");
    }

    // 如果没有找到匹配的页面，创建新页面
    let new_page = if let Some(url) = target_url {
        debug!("创建新页面并导航到: {}", url);
        let page = browser.new_page("about:blank").await
            .map_err(|e| {
                error!("创建新页面失败: {}", e);
                e
            })?;
        page.goto(url).await
            .map_err(|e| {
                error!("导航到 {} 失败: {}", url, e);
                e
            })?;
        info!("已导航到: {}", url);
        debug!("页面导航成功");
        page
    } else {
        debug!("创建空白页面");
        browser.new_page("about:blank").await
            .map_err(|e| {
                error!("创建空白页面失败: {}", e);
                e
            })?
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

    debug!("检查试卷是否已存在: {}", paper_title);
    let response: Value = tiku_page.evaluate(check_js.as_str()).await
        .map_err(|e| {
            error!("执行检查脚本失败: {}", e);
            e
        })?
        .into_value()
        .map_err(|e| {
            error!("获取检查结果失败: {}", e);
            anyhow!("获取检查结果失败: {}", e)
        })?;

    if let Some(error) = response.get("error") {
        error!("API 请求失败: {}", error);
        return Err(anyhow!("API 请求失败: {}", error));
    }

    if let Some(data) = response.get("data") {
        if let Some(repeated) = data.get("repeated") {
            if repeated.as_bool().unwrap_or(false) {
                debug!("试卷已存在: {}", paper_title);
                // 记录到重复文件
                let log_path = Path::new("other").join("重复.txt");
                if let Some(parent) = log_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| {
                            error!("创建重复日志目录失败: {}", e);
                            e
                        })?;
                }
                use std::fs::OpenOptions;
                let mut file = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log_path)
                    .map_err(|e| {
                        error!("打开重复日志文件失败: {}", e);
                        e
                    })?;
                file.write_all(format!("{}\n", paper_title).as_bytes())
                    .map_err(|e| {
                        error!("写入重复日志失败: {}", e);
                        e
                    })?;
                debug!("已记录重复试卷到日志文件");
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

    debug!("正在获取目录页的试卷列表");
    let response: Value = catalogue_page.evaluate(js_code).await
        .map_err(|e| {
            error!("执行获取试卷列表脚本失败: {}", e);
            e
        })?
        .into_value()
        .map_err(|e| {
            error!("获取试卷列表结果失败: {}", e);
            anyhow!("获取试卷列表结果失败: {}", e)
        })?;

    let papers: Vec<PaperInfo> = serde_json::from_value(response)
        .map_err(|e| {
            error!("解析试卷列表失败: {}", e);
            anyhow!("解析试卷列表失败: {}", e)
        })?;
    debug!("成功获取到 {} 个试卷", papers.len());

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

    debug!("开始处理试卷: {}", paper_info.title);
    let result = async {
        // 下载页面数据
        debug!("正在下载页面数据");
        let page_data = download_page(&paper_page).await
            .map_err(|e| {
                error!("下载页面数据失败: {}", e);
                e
            })?;
        debug!("页面数据下载成功: {}", page_data.name);

        // 检查是否已存在
        debug!("检查试卷是否已存在");
        let exists = check_paper_exists(tiku_page, &page_data.name).await
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
        save_new_paper(&mut question_page, tiku_page).await
            .map_err(|e| {
                error!("保存新试卷失败: {}", e);
                e
            })?;
        info!("✅ 成功处理: {}", question_page.name);
        debug!("试卷处理完成");
        Ok(ProcessResult::Success)
    }
    .await;

    // 清理资源 - 显式关闭页面
    debug!("正在关闭试卷页面");
    if let Err(e) = paper_page.close().await {
        warn!("关闭试卷页面失败: {}，但继续处理", e);
    } else {
        debug!("试卷页面已关闭");
    }
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
        debug!("正在获取目录页 {} 的试卷列表", page_number);
        let papers = fetch_paper_list(&catalogue_page).await
            .map_err(|e| {
                error!("获取目录页 {} 的试卷列表失败: {}", page_number, e);
                e
            })?;
        info!("📄 在页面 {} 找到 {} 个试卷", page_number, papers.len());
        debug!("试卷列表: {:?}", papers.iter().map(|p| &p.title).collect::<Vec<_>>());

        if papers.is_empty() {
            debug!("页面 {} 没有试卷，跳过", page_number);
            return Ok(0);
        }

        // 并发处理所有试卷
        info!("⚡ 开始并发处理 {} 个试卷...", papers.len());
        debug!("启动 {} 个并发任务", papers.len());

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

    // 清理资源 - 显式关闭目录页
    debug!("正在关闭目录页");
    if let Err(e) = catalogue_page.close().await {
        warn!("关闭目录页失败: {}，但继续处理", e);
    } else {
        debug!("目录页已关闭");
    }
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
