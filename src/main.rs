mod add_paper;
mod model;
mod tencent_cos;
mod logger;

use model::{Question, QuestionPage};
use add_paper::save_new_paper;
use anyhow::Result;
use chromiumoxide::Browser;
use futures::StreamExt;
use tracing::{info, warn, debug};

#[tokio::main]
async fn main() -> Result<()> {
    logger::init();
    
    info!("正在连接到浏览器...");
    // 连接到已经在运行的浏览器 (CDP)
    // 连接到 Chrome 的调试端口 (默认 9222)
    // 如果使用不同的端口，请修改此 URL
    let (browser, mut handler) =
        Browser::connect("http://localhost:2001").await?;

    // 在后台处理浏览器事件
    tokio::spawn(async move {
        while let Some(h) = handler.next().await {
            if h.is_err() {
                break;
            }
        }
    });

    // 添加短暂延迟以等待浏览器状态同步
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let pages = browser.pages().await?;
    
    info!("\n=== 所有打开的页面 ===");
    info!("总共找到 {} 个页面", pages.len());
    
    let page = if pages.is_empty() {
        warn!("没有找到已打开的页面，正在创建新页面并导航到题库平台...");
        let new_page = browser.new_page("about:blank").await?;
        new_page.goto("https://tk-lpzx.xdf.cn/#/paperEnterList").await?;
        info!("已导航到题库平台，请在浏览器中登录（如需要）");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        new_page
    } else {
        for (i, p) in pages.iter().enumerate() {
            let title = p.get_title().await.ok().flatten().unwrap_or_else(|| "无标题".to_string());
            let url = p.url().await.ok().flatten().unwrap_or_else(|| "无URL".to_string());
            debug!("页面 {}: 标题={}, URL={}", i + 1, title, url);
        }
        info!("\n======================\n");
        
        // 查找标题为 "题库平台 | 录排中心" 的页面
        let mut target_page = None;
        for p in pages.iter() {
            if let Ok(Some(title)) = p.get_title().await {
                if title.contains("题库平台") && title.contains("录排中心") {
                    target_page = Some(p.clone());
                    info!("✓ 找到目标页面: {}", title);
                    break;
                }
            }
        }
        
        target_page.ok_or_else(|| anyhow::anyhow!("未找到标题为 '题库平台 | 录排中心' 的页面"))?
    };

    // 模拟试卷数据
    let mut question_page = QuestionPage {
        name: "测试试卷".to_string(),
        subject: "数学".to_string(),
        province: "北京".to_string(),
        grade: "高一".to_string(),
        year: "2025".to_string(),
        page_id: None,
        stemlist: vec![Question {
            origin: "题目来源".to_string(),
            stem: "题目内容".to_string(),
            origin_from_our_bank: vec![],
        }],
    };

    let paper_id = save_new_paper(&mut question_page, &page).await?;
    info!("最终结果: {:?}", paper_id);

    Ok(())
}
