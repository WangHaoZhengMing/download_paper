use chromiumoxide::{Browser, Page};
use futures::StreamExt; // 记得加上这个，解决 next() 报错
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, info, warn};
use anyhow::{Context, Result};

/// 连接到浏览器并获取页面
pub async fn connect_to_browser_and_page(
    port: u16,
    target_url: Option<&str>,
    target_title: Option<&str>,
) -> Result<(Browser, Page)> {
    let browser_url = format!("http://localhost:{}", port);
    debug!("尝试连接到现有浏览器: {}", browser_url);

    // 标志位：是否是新启动的实例
    let is_new_instance;

    // 1. 尝试连接现有的浏览器
    let connect_result = Browser::connect(&browser_url).await;

    let (browser, mut handler) = match connect_result {
        Ok(res) => {
            info!("✓ 成功连接到端口 {} 的现有浏览器", port);
            is_new_instance = false; // 复用旧实例
            res
        }
        Err(_) => {
            warn!("无法连接到端口 {}，准备启动新的 Edge 实例...", port);
            is_new_instance = true; // 标记为新实例

            // 2. 如果连接失败，手动启动 Edge 进程
            launch_edge_process(port, target_url)?;

            // 3. 循环尝试连接，最多等待 10 秒
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

    // 4. 在后台处理浏览器事件
    tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() {
                break;
            }
        }
    });

    // ========== 关键修改逻辑开始 ==========
    if is_new_instance {
        info!("检测到新启动的浏览器实例，等待 10 秒供用户操作（如扫码登录）...");
        // 倒计时提示，体验更好
        for i in (1..=10).rev() {
            if i % 2 == 0 { // 每2秒打印一次，避免刷屏
                info!("等待中... 剩余 {} 秒", i);
            }
            sleep(Duration::from_secs(1)).await;
        }
        info!("等待结束，开始执行自动化任务");
    } else {
        debug!("复用现有实例，无需等待，立即执行");
    }
    // ========== 关键修改逻辑结束 ==========

    // 5. 获取页面列表
    let pages = browser.pages().await.context("获取页面列表失败")?;
    debug!("当前有 {} 个页面", pages.len());

    // 6. 查找目标页面（逻辑保持不变）
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

    // 7. 处理 URL
    if let Some(url) = target_url {
        // 检查是否已经有页面打开了这个 URL
        for p in pages.iter() {
            if let Ok(Some(page_url)) = p.url().await {
                if page_url.contains(url) {
                    info!("✓ 找到包含目标 URL 的页面");
                    return Ok((browser, p.clone()));
                }
            }
        }
        // 没找到则新建
        let page = browser.new_page(url).await?;
        return Ok((browser, page));
    }

    // 兜底
    if let Some(first_page) = pages.first() {
        Ok((browser, first_page.clone()))
    } else {
        let page = browser.new_page("about:blank").await?;
        Ok((browser, page))
    }
}

fn launch_edge_process(port: u16, url: Option<&str>) -> Result<()> {
    // ... 这里保持你之前的 launch_edge_process 代码不变 ...
    // 为了完整性，简单写一下
    let user_profile = std::env::var("USERPROFILE").context("找不到 USERPROFILE")?;
    let base_user_data_dir = PathBuf::from(user_profile).join(r"AppData\Local\Microsoft\Edge\User Data");
    let profile_name = format!("Profile_{}", port);
    let user_data_dir = base_user_data_dir.join(profile_name);
    let edge_path = r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe";

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
