use anyhow::Result;
use chromiumoxide::{Browser, Page};
use std::sync::Arc;
use tokio::sync::Semaphore;

use super::connect_to_browser_and_page;

/// 简单的浏览器连接池：只负责端口、并发控制和页面连接
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

    /// 获取一个页面句柄，内部用信号量控制并发
    pub async fn connect_page(
        &self,
        target_url: Option<&str>,
        target_title: Option<&str>,
    ) -> Result<(Browser, Page)> {
        let _permit = self.semaphore.acquire().await.expect("Semaphore closed");
        connect_to_browser_and_page(self.port, target_url, target_title).await
    }
}
