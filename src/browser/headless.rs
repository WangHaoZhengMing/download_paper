use anyhow::Result;
use chromiumoxide::handler::viewport::Viewport;
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures::StreamExt;
use std::time::Duration;

pub async fn launch_headless_get_page_browser(url: &str) -> Result<(Browser, Page)> {
    let viewport = Viewport {
        width: 1920,
        height: 1080,
        ..Default::default() // 其他属性使用默认值
    };
    let ua_string = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36";

    let config = BrowserConfig::builder()
        .viewport(viewport)
        .arg(format!("--user-agent={}", ua_string))
        .build()
        .map_err(|e| anyhow::Error::msg(e))?; // 将 String 错误转换为 anyhow Error

    let (browser, mut handler) = Browser::launch(config).await?;

    // 启动后台处理线程
    tokio::task::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() {
                break;
            }
        }
    });

    // 创建页面
    let page = browser.new_page("about:blank").await?;
    page.evaluate_on_new_document(
        "Object.defineProperty(navigator, 'webdriver', { get: () => undefined })",
    )
    .await?;

    page.goto(url).await?;

    println!("等待 1 秒让 JS 执行刷新...");
    tokio::time::sleep(Duration::from_secs(1)).await;

    // browser.close().await?;
    Ok((browser, page))
}
