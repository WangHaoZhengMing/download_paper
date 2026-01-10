use anyhow::{Context, Result};
use chromiumoxide::{Browser, Page};
use futures::StreamExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::sleep;
use tracing::{debug, info, warn};

#[derive(Clone, Debug)]
pub struct BrowserPool {
    port: u16,
    semaphore: Arc<Semaphore>,
}

impl BrowserPool {
    pub fn new(port: u16, max_concurrent: usize) -> Self {
        Self {
            port,
            semaphore: Arc::new(Semaphore::new(max_concurrent.max(1))),
        }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn connect_page(
        &self,
        target_url: Option<&str>,
        target_title: Option<&str>,
    ) -> Result<(Browser, Page)> {
        let _permit = self.semaphore.acquire().await.expect("Semaphore closed");
        connect_to_browser_and_page(self.port, target_url, target_title).await
    }
}

pub async fn connect_to_browser_and_page(
    port: u16,
    target_url: Option<&str>,
    target_title: Option<&str>,
) -> Result<(Browser, Page)> {
    let browser_url = format!("http://localhost:{}", port);
    debug!("尝试连接到现有浏览器: {}", browser_url);

    let is_new_instance;

    
    let connect_result = Browser::connect(&browser_url).await;

    let (browser, mut handler) = match connect_result {
        Ok(res) => {
            info!("✓ 成功连接到端口 {} 的现有浏览器", port);
            is_new_instance = false;
            res
        }
        Err(_) => {
            warn!("无法连接到端口 {}，准备启动新的 Edge 实例...", port);
            is_new_instance = true;
            launch_edge_process(port, target_url)?;
            let mut retries = 20;
            let mut connected_browser = None;
            while retries > 0 {
                sleep(Duration::from_millis(500)).await;
                match Browser::connect(&browser_url).await {
                    Ok(res) => {
                        info!("✓ 新 Edge 启动成功并已连接");
                        connected_browser = Some(res);
                        break;
                    }
                    Err(_) => {
                        debug!("等待浏览器端口就绪... 剩余重试: {}", retries);
                        retries -= 1;
                    }
                }
            }
            connected_browser.ok_or_else(|| anyhow::anyhow!("启动 Edge 后连接超时"))?
        }
    };

    tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() {
                break;
            }
        }
    });

    if is_new_instance {
        info!("检测到新启动的浏览器实例，等待 10 秒供用户操作（如扫码登录）...");
        for i in (1..=10).rev() {
            if i % 2 == 0 {
                info!("等待中... 剩余 {} 秒", i);
            }
            sleep(Duration::from_secs(1)).await;
        }
        info!("等待结束，开始执行自动化任务");
    } else {
        debug!("复用现有实例，无需等待，立即执行");
    }

    let pages = browser.pages().await.context("获取页面列表失败")?;
    debug!("当前有 {} 个页面", pages.len());

    if let Some(url) = target_url {
        for p in pages.iter() {
            if let Ok(Some(page_url)) = p.url().await {
                if page_url.contains(url) {
                    info!("✓ 找到包含目标 URL 的页面");
                    let _ = p.activate().await;
                    return Ok((browser, p.clone()));
                }
            }
        }
    }

    if let Some(title) = target_title {
        for p in pages.iter() {
            if let Ok(Some(page_title)) = p.get_title().await {
                if page_title.contains(title) {
                    info!("✓ 找到目标页面: {}", page_title);
                    let _ = p.activate().await;
                    return Ok((browser, p.clone()));
                }
            }
        }
    }

    if let Some(url) = target_url {
        let page = browser.new_page(url).await?;
        return Ok((browser, page));
    }

    if let Some(first_page) = pages.first() {
        Ok((browser, first_page.clone()))
    } else {
        let page = browser.new_page("about:blank").await?;
        Ok((browser, page))
    }
}

/// 在已有浏览器中复用或新建页面：先按 URL，再按标题匹配
pub async fn get_or_open_page(
    browser: &Browser,
    target_url: &str,
    target_title: Option<&str>,
) -> Result<Page> {
    let pages = browser.pages().await.context("获取页面列表失败")?;
    debug!("当前有 {} 个页面", pages.len());

    for p in pages.iter() {
        if let Ok(Some(page_url)) = p.url().await {
            if page_url.contains(target_url) {
                info!("✓ 复用已打开的页面 (URL 匹配)");
                let _ = p.activate().await;
                return Ok(p.clone());
            }
        }
    }

    if let Some(title) = target_title {
        for p in pages.iter() {
            if let Ok(Some(page_title)) = p.get_title().await {
                if page_title.contains(title) {
                    info!("✓ 复用已打开的页面 (标题匹配)");
                    let _ = p.activate().await;
                    return Ok(p.clone());
                }
            }
        }
    }

    let page = browser.new_page(target_url).await?;
    Ok(page)
}

fn launch_edge_process(port: u16, url: Option<&str>) -> Result<()> {
    let user_profile = std::env::var("USERPROFILE").context("找不到 USERPROFILE")?;
    let base_user_data_dir = PathBuf::from(user_profile).join(r"AppData\\Local\\Microsoft\\Edge\\User Data");
    let profile_name = format!("Profile_{}", port);
    let user_data_dir = base_user_data_dir.join(profile_name);
    let edge_path = r"C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe";

    let mut args = vec![
        format!("--remote-debugging-port={}", port),
        format!("--user-data-dir={}", user_data_dir.to_string_lossy()),
        "--new-window".to_string(),
        "--no-first-run".to_string(),
        "--no-default-browser-check".to_string(),
    ];

    if let Some(target_url) = url {
        args.push(target_url.to_string());
    } else {
        args.push("about:blank".to_string());
    }

    Command::new(edge_path)
        .args(&args)
        .spawn()
        .context("启动 Edge 失败")?;

    Ok(())
}




#[tokio::test]
async fn test_connect() {
    let port = 2001;

    let (browser, page) = connect_to_browser_and_page(port, Some("www.bing.com"), None).await.unwrap();

}