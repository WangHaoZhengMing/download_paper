use anyhow::Result;
use chromiumoxide::{Browser, Page};
use futures::stream::{self, StreamExt};
use tokio::time::{Duration, sleep};
use tracing::{debug, info, warn, error};

use crate::config::AppConfig;
use crate::core::models::PaperInfo;
use crate::core::types::{ProcessResult, ProcessStats};
use crate::modules::browser::{get_or_open_page, BrowserPool, download_page};
use crate::modules::catalogue::fetch_paper_list;
use crate::modules::storage::persist_paper_locally;

async fn process_single_paper(
    paper_info: &PaperInfo,
    browser: &Browser,
    output_dir: &str,
) -> Result<ProcessResult> {
    let paper_page = get_or_open_page(browser, &paper_info.url, None).await?;

    debug!("开始处理试卷: {}", paper_info.title);
    let result: Result<ProcessResult> = async {
        let page_data = download_page(&paper_page).await.map_err(|e| {
            warn!("下载页面数据失败: {}", e);
            e
        })?;

        persist_paper_locally(&page_data, output_dir)?;
        info!("✅ 成功处理: {}", page_data.name);
        Ok(ProcessResult::Success)
    }
    .await;

    debug!("正在关闭试卷页面");
    if let Err(e) = paper_page.close().await {
        warn!("关闭试卷页面失败: {}，但继续处理", e);
    }
    result
}

pub async fn process_catalogue_page(
    page_number: i32,
    browser: &Browser,
) -> Result<Vec<PaperInfo>> {
    let catalogue_url = format!("https://zujuan.xkw.com/czkx/shijuan/jdcs/p{}", page_number);
    info!("📖 正在处理目录页 {}...", page_number);

    let catalogue_page = get_or_open_page(browser, &catalogue_url, None).await?;

    let result = async {
        let papers = fetch_paper_list(&catalogue_page).await?;
        info!("📄 在页面 {} 找到 {} 个试卷", page_number, papers.len());
        Ok(papers)
    }
    .await;

    debug!("正在关闭目录页");
    if let Err(e) = catalogue_page.close().await {
        warn!("关闭目录页失败: {}，但继续处理", e);
    }
    result
}

pub async fn run(app_config: AppConfig) -> Result<()> {
    let browser_pool = BrowserPool::new(app_config.debug_port, app_config.concurrency);

    info!("🚀 开始试卷下载流程...");
    info!("📊 页面范围: {} - {}", app_config.start_page, app_config.end_page);
    debug!("🔌 浏览器端口: {}", browser_pool.port());

    let (browser, _bootstrap_page) = browser_pool
        .connect_page(Some("https://tk-lpzx.xdf.cn/#/paperEnterList"), None)
        .await?;

    let tiku_page = get_or_open_page(
        &browser,
        "https://tk-lpzx.xdf.cn/#/paperEnterList",
        Some("试卷录入"),
    )
    .await?;
    // info!("{}", tiku_page.content().await?);
    let mut total = ProcessStats::default();

    for page_num in app_config.start_page..app_config.end_page {
        match process_catalogue_page(page_num, &browser).await {
            Ok(papers) => {
                if papers.is_empty() {
                    debug!("页面 {} 没有试卷，跳过", page_num);
                    continue;
                }
                let (stats, pending) = stream::iter(papers.into_iter())
                    .then(|mut paper| {
                        let tiku_page = tiku_page.clone();
                        async move {
                            match paper.check_paper_existence(&tiku_page).await {
                                Ok(true) => (ProcessResult::AlreadyExists, None),
                                Ok(false) => (ProcessResult::Success, Some(paper)),
                                Err(e) => {
                                    warn!("❌ 目录页检查失败 '{}': {}", paper.title, e);
                                    (ProcessResult::Failed, None)
                                }
                            }
                        }
                    })
                    .fold(
                        (ProcessStats::default(), Vec::new()),
                        |(mut stats, mut keep), (check_result, paper_opt)| async move {
                            match check_result {
                                ProcessResult::AlreadyExists => stats.add_result(&ProcessResult::AlreadyExists),
                                ProcessResult::Failed => stats.add_result(&ProcessResult::Failed),
                                ProcessResult::Success => {
                                    if let Some(p) = paper_opt {
                                        keep.push(p);
                                    }
                                }
                            }
                            (stats, keep)
                        },
                    )
                    .await;

                let stats_after_dl = stream::iter(pending.into_iter().map(|paper| {
                    let browser = browser.clone();
                    let output_dir = app_config.output_dir.clone();
                    async move {
                        let res = process_single_paper(&paper, &browser, &output_dir).await;
                        (paper.title, res)
                    }
                }))
                .buffer_unordered(app_config.concurrency)
                .fold(stats, |mut stats, (title, result)| async move {
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
                    stats
                })
                .await;

                total.success += stats_after_dl.success;
                total.exists += stats_after_dl.exists;
                total.failed += stats_after_dl.failed;
                info!(
                    "✅ 页面 {} 完成: 成功 {}，已存在 {}，失败 {}",
                    page_num, stats_after_dl.success, stats_after_dl.exists, stats_after_dl.failed
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
        "成功 {} 个，已存在 {} 个，失败 {} 个",
        total.success, total.exists, total.failed
    );

    Ok(())
}
 