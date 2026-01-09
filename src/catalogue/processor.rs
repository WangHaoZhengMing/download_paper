
use std::sync::Arc;

use crate::{browser::headless::launch_headless_get_page_browser, catalogue::fetch_paper_list};
use crate::paper::processor::process_single_paper;
use crate::paper::types::ProcessResult;
use anyhow::Result;
use chromiumoxide::Page;
use tracing::{debug, error, info, warn};

/// 处理单个目录页
pub async fn process_catalogue_page(page_number: i32, port: u16, tiku_page: &Page) -> Result<i32> {
    let catalogue_url = format!("https://zujuan.xkw.com/czls/shijuan/bk/p{}", page_number);
    info!("正在deal 目录页{}",page_number);
    std::fs::write("output.txt", format!("📖 正在处理目录页 {}...", page_number))?;




    

    // 使用无头浏览器处理目录页（更轻量，资源占用更少）
    let (mut catalogue_browser, catalogue_page) = launch_headless_get_page_browser(&catalogue_url).await?;

    // 获取试卷列表
    debug!("正在获取目录页 {} 的试卷列表", page_number);
    let papers = fetch_paper_list(&catalogue_page).await.map_err(|e| {
        error!("获取目录页 {} 的试卷列表失败: {}", page_number, e);
        e
    })?;
    info!("📄 在页面 {} 找到 {} 个试卷", page_number, papers.len());
    debug!(
        "试卷列表: {:?}",
        papers.iter().map(|p| &p.title).collect::<Vec<_>>()
    );

    let success_count = if papers.is_empty() {
        debug!("页面 {} 没有试卷，跳过", page_number);
        0
    } else {
        // 并发处理所有试卷
        info!("⚡ 开始并发处理 {} 个试卷...", papers.len());
        debug!("启动 {} 个并发任务", papers.len());


        let paper_browser = launch_headless_get_page_browser( &papers.first().unwrap().url).await?;
        let (currnet_browser, _current_paper_page) = paper_browser;

        let paper_browser = Arc::new(currnet_browser);

        let mut tasks = Vec::new();
        for paper in &papers {
            let paper_clone = paper.clone();
            let tiku_page_clone = tiku_page.clone();
            let paper_browser2 = paper_browser.clone();
            tasks.push(tokio::spawn(async move {
                process_single_paper(&paper_browser2,&paper_clone,&tiku_page_clone).await
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
        drop(paper_browser);
        success_count
        
    };

    // 清理资源 - 显式关闭目录页和浏览器
    debug!("正在清理浏览器资源...");
    
    // 先关闭页面
    if let Err(e) = catalogue_page.close().await {
        warn!("关闭目录页失败: {}，但继续处理", e);
    } else {
        debug!("目录页已关闭");
    }
    
    // 等待一小段时间确保资源释放
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    
    // 关闭浏览器
    if let Err(e) = catalogue_browser.close().await {
        warn!("关闭浏览器失败: {}，但继续处理", e);
    } else {
        debug!("浏览器已关闭");
    }
    
    // 再次等待确保资源完全释放
    tokio::time::sleep(tokio::time::Duration::from_millis(300)).await;

    Ok(success_count)
}

