use anyhow::Result;
use chromiumoxide::Page;
use futures::stream::{self, StreamExt};
use tokio::time::{sleep, Duration};
use tracing::{debug, info, warn};

use crate::browser::BrowserPool;
use crate::config::AppConfig;
use crate::services::catalogue::fetch_paper_list;
use crate::services::paper::process_single_paper;
use crate::services::types::{ProcessResult, ProcessStats};

/// 处理单个目录页，返回统计
pub async fn process_catalogue_page(
    page_number: i32,
    pool: &BrowserPool,
    tiku_page: &Page,
    concurrency: usize,
) -> Result<ProcessStats> {
    let catalogue_url = format!("https://zujuan.xkw.com/czkx/shijuan/jdcs/p{}", page_number);
    info!("📖 正在处理目录页 {}...", page_number);

    let (catalogue_browser, catalogue_page) = pool.connect_page(Some(&catalogue_url), None).await?;

    let result = async {
        let papers = fetch_paper_list(&catalogue_page).await?;
        info!("📄 在页面 {} 找到 {} 个试卷", page_number, papers.len());

        if papers.is_empty() {
            debug!("页面 {} 没有试卷，跳过", page_number);
            return Ok(ProcessStats::default());
        }

        let mut stats = ProcessStats::default();
        let mut stream = stream::iter(papers.into_iter().map(|paper| {
            let pool = pool.clone();
            let tiku_page = tiku_page.clone();
            async move {
                let res = process_single_paper(&paper, &pool, &tiku_page).await;
                (paper.title, res)
            }
        }))
        .buffer_unordered(concurrency);

        while let Some((title, result)) = stream.next().await {
            match result {
                Ok(ProcessResult::Success) => stats.add_result(&ProcessResult::Success),
                Ok(ProcessResult::AlreadyExists) => stats.add_result(&ProcessResult::AlreadyExists),
                Ok(ProcessResult::Failed) => {
                    warn!("❌ 处理失败: {}", title);
                    stats.add_result(&ProcessResult::Failed);
                }
                Err(e) => {
                    warn!("❌ 处理 '{}' 时出错: {}", title, e);
                    stats.add_result(&ProcessResult::Failed);
                }
            }
        }

        Ok(stats)
    }
    .await;

    debug!("正在关闭目录页");
    if let Err(e) = catalogue_page.close().await {
        warn!("关闭目录页失败: {}，但继续处理", e);
    }
    drop(catalogue_browser);

    result
}

/// 入口：根据配置处理所有目录页
pub async fn run(app_config: AppConfig) -> Result<()> {
    let browser_pool = BrowserPool::new(app_config.debug_port, app_config.concurrency);

    info!("🚀 开始试卷下载流程...");
    info!("📊 页面范围: {} - {}", app_config.start_page, app_config.end_page);
    info!("🔌 浏览器端口: {}", browser_pool.port());

    let (browser, tiku_page) = browser_pool
        .connect_page(None, Some(&app_config.tiku_target_title))
        .await?;

    let mut total = ProcessStats::default();

    for page_num in app_config.start_page..app_config.end_page {
        match process_catalogue_page(
            page_num,
            &browser_pool,
            &tiku_page,
            app_config.concurrency,
        )
        .await
        {
            Ok(stats) => {
                total.success += stats.success;
                total.exists += stats.exists;
                total.failed += stats.failed;
                info!(
                    "✅ 页面 {} 完成: 成功 {}，已存在 {}，失败 {}",
                    page_num, stats.success, stats.exists, stats.failed
                );
            }
            Err(e) => {
                warn!("❌ 页面 {} 失败: {}", page_num, e);
            }
        }

        sleep(Duration::from_millis(app_config.delay_ms)).await;
        info!("{}", "=".repeat(60));
    }

    drop(browser);

    info!(
        "\n🎉 处理完成! 成功 {} 个，已存在 {} 个，失败 {} 个",
        total.success, total.exists, total.failed
    );

    Ok(())
}
